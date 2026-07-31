//! Restricted registration-phase API and immutable script declarations.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use mlua::{Function, Lua, RegistryKey, Table, Value, Variadic};

use crate::{DispatchPolicy, EventFilter, HandlerId};

/// Persistence behavior selected by a script or UI override.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivationMode {
    Always,
    Temporary,
}

/// Validated script metadata and default runtime limits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptConfig {
    pub name: String,
    pub activation: ActivationMode,
    pub default_dispatch: DispatchPolicy,
    pub task_timeout_ms: u64,
}

/// Event or chord source attached to a Lua callback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HandlerKind {
    Event {
        filter: EventFilter,
        debounce_ms: Option<u64>,
    },
    Hotkey {
        chord: String,
    },
    Schedule(ScheduleDeclaration),
}

/// Parsed schedule attached to a registered callback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScheduleDeclaration {
    OnceAfter {
        delay_ms: u64,
    },
    Every {
        interval_ms: u64,
    },
    Cron {
        expression: String,
        timezone: String,
    },
}

/// Public callback metadata without Lua registry internals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerDescriptor {
    pub id: HandlerId,
    pub kind: HandlerKind,
    pub dispatch: DispatchPolicy,
    pub queue_capacity: usize,
    pub max_parallel: usize,
}

#[derive(Debug)]
pub(crate) struct RegisteredHandler {
    pub descriptor: HandlerDescriptor,
    pub callback: RegistryKey,
}

/// Script-scoped folder watcher declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatchRegistration {
    pub root: PathBuf,
    pub recursive: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

/// Immutable result of executing a script's registration phase.
pub struct RegisteredScript {
    config: ScriptConfig,
    handlers: Vec<RegisteredHandler>,
    handler_descriptors: Vec<HandlerDescriptor>,
    watches: Vec<WatchRegistration>,
}

impl RegisteredScript {
    /// Returns validated script metadata.
    #[must_use]
    pub const fn config(&self) -> &ScriptConfig {
        &self.config
    }

    /// Returns callback metadata in registration order.
    #[must_use]
    pub fn handlers(&self) -> &[HandlerDescriptor] {
        &self.handler_descriptors
    }

    /// Returns folder watches in registration order.
    #[must_use]
    pub fn watches(&self) -> &[WatchRegistration] {
        &self.watches
    }

    pub(crate) fn callback(&self, handler_id: HandlerId) -> Option<&RegistryKey> {
        self.handlers
            .iter()
            .find(|handler| handler.descriptor.id == handler_id)
            .map(|handler| &handler.callback)
    }
}

impl std::fmt::Debug for RegisteredScript {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RegisteredScript")
            .field("config", &self.config)
            .field("handlers", &self.handler_descriptors)
            .field("watches", &self.watches)
            .finish()
    }
}

#[derive(Debug)]
struct RegistrationBuilder {
    open: bool,
    script_dir: PathBuf,
    config: ScriptConfig,
    configured: bool,
    handlers: Vec<RegisteredHandler>,
    watches: Vec<WatchRegistration>,
}

pub(crate) fn register_script(
    lua: &Lua,
    source: &str,
    script_path: &Path,
) -> mlua::Result<RegisteredScript> {
    let default_name = script_path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Lua automation")
        .to_owned();
    let state = Arc::new(Mutex::new(RegistrationBuilder {
        open: true,
        script_dir: script_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf(),
        config: ScriptConfig {
            name: default_name,
            activation: ActivationMode::Temporary,
            default_dispatch: DispatchPolicy::Queue,
            task_timeout_ms: 90_000,
        },
        configured: false,
        handlers: Vec::new(),
        watches: Vec::new(),
    }));

    install_registration_globals(lua, &state)?;
    let chunk_name = script_path.to_string_lossy();
    let execution = lua.load(source).set_name(chunk_name.as_ref()).exec();
    remove_registration_globals(lua)?;
    execution?;

    let mut builder = state
        .lock()
        .map_err(|_| registration_error("registration state is unavailable"))?;
    builder.open = false;
    let handlers = std::mem::take(&mut builder.handlers);
    let handler_descriptors = handlers
        .iter()
        .map(|handler| handler.descriptor.clone())
        .collect();
    Ok(RegisteredScript {
        config: builder.config.clone(),
        handlers,
        handler_descriptors,
        watches: std::mem::take(&mut builder.watches),
    })
}

fn install_registration_globals(
    lua: &Lua,
    state: &Arc<Mutex<RegistrationBuilder>>,
) -> mlua::Result<()> {
    let script = lua.create_table()?;
    let configure_state = Arc::clone(state);
    script.set(
        "configure",
        lua.create_function(move |_, table: Table| {
            let mut builder = lock_open(&configure_state)?;
            if builder.configured {
                return Err(registration_error(
                    "script.configure may only be called once",
                ));
            }
            if let Some(name) = table.get::<Option<String>>("name")? {
                if name.trim().is_empty() {
                    return Err(registration_error("script name must not be empty"));
                }
                builder.config.name = name;
            }
            if let Some(activation) = table.get::<Option<String>>("activation")? {
                builder.config.activation = parse_activation(&activation)?;
            }
            if let Some(dispatch) = table.get::<Option<String>>("default_dispatch")? {
                builder.config.default_dispatch = parse_dispatch(&dispatch)?;
            }
            if let Some(timeout) = table.get::<Option<Value>>("task_timeout")? {
                builder.config.task_timeout_ms = parse_duration(timeout)?;
            }
            builder.configured = true;
            Ok(())
        })?,
    )?;
    lua.globals().set("script", script)?;

    let on_state = Arc::clone(state);
    lua.globals().set(
        "on",
        lua.create_function(move |lua, args: Variadic<Value>| {
            let (name, options, callback) = parse_handler_args(lua, &args)?;
            let filter = EventFilter::parse(&name)
                .map_err(|_| registration_error("invalid event filter"))?;
            let mut builder = lock_open(&on_state)?;
            let defaults = builder.config.default_dispatch;
            let descriptor = handler_descriptor(
                HandlerKind::Event {
                    filter,
                    debounce_ms: option_duration(&options, "debounce")?,
                },
                &options,
                defaults,
            )?;
            let callback = lua.create_registry_value(callback)?;
            builder.handlers.push(RegisteredHandler {
                descriptor,
                callback,
            });
            Ok(())
        })?,
    )?;

    let hotkey_state = Arc::clone(state);
    lua.globals().set(
        "hotkey",
        lua.create_function(move |lua, (chord, callback): (String, Function)| {
            if chord.trim().is_empty() {
                return Err(registration_error("hotkey chord must not be empty"));
            }
            let mut builder = lock_open(&hotkey_state)?;
            let descriptor = HandlerDescriptor {
                id: HandlerId::new(),
                kind: HandlerKind::Hotkey { chord },
                dispatch: builder.config.default_dispatch,
                queue_capacity: 1_024,
                max_parallel: 4,
            };
            let callback = lua.create_registry_value(callback)?;
            builder.handlers.push(RegisteredHandler {
                descriptor,
                callback,
            });
            Ok(())
        })?,
    )?;

    let watch_state = Arc::clone(state);
    lua.globals().set(
        "watch",
        lua.create_function(move |_, table: Table| {
            let mut builder = lock_open(&watch_state)?;
            let root_text = table.get::<String>("root")?;
            let root = PathBuf::from(root_text);
            let root = if root.is_absolute() {
                root
            } else {
                builder.script_dir.join(root)
            };
            builder.watches.push(WatchRegistration {
                root,
                recursive: table.get::<Option<bool>>("recursive")?.unwrap_or(false),
                include: string_array(&table, "include")?,
                exclude: string_array(&table, "exclude")?,
            });
            Ok(())
        })?,
    )?;

    let schedule = lua.create_table()?;
    let once_state = Arc::clone(state);
    schedule.set(
        "once",
        lua.create_function(move |lua, (delay, callback): (Value, Function)| {
            register_schedule(
                lua,
                &once_state,
                ScheduleDeclaration::OnceAfter {
                    delay_ms: parse_duration(delay)?,
                },
                callback,
            )
        })?,
    )?;
    let every_state = Arc::clone(state);
    schedule.set(
        "every",
        lua.create_function(move |lua, (interval, callback): (Value, Function)| {
            let interval_ms = parse_duration(interval)?;
            if interval_ms == 0 {
                return Err(registration_error("schedule interval must be non-zero"));
            }
            register_schedule(
                lua,
                &every_state,
                ScheduleDeclaration::Every { interval_ms },
                callback,
            )
        })?,
    )?;
    let cron_state = Arc::clone(state);
    schedule.set(
        "cron",
        lua.create_function(
            move |lua, (expression, timezone, callback): (String, String, Function)| {
                crate::CronSchedule::parse(&expression, &timezone)
                    .map_err(|_| registration_error("invalid cron schedule"))?;
                register_schedule(
                    lua,
                    &cron_state,
                    ScheduleDeclaration::Cron {
                        expression,
                        timezone,
                    },
                    callback,
                )
            },
        )?,
    )?;
    lua.globals().set("schedule", schedule)?;
    Ok(())
}

fn remove_registration_globals(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    for name in ["script", "on", "hotkey", "watch", "schedule"] {
        globals.set(name, Value::Nil)?;
    }
    Ok(())
}

fn register_schedule(
    lua: &Lua,
    state: &Arc<Mutex<RegistrationBuilder>>,
    declaration: ScheduleDeclaration,
    callback: Function,
) -> mlua::Result<()> {
    let mut builder = lock_open(state)?;
    let descriptor = HandlerDescriptor {
        id: HandlerId::new(),
        kind: HandlerKind::Schedule(declaration),
        dispatch: DispatchPolicy::Queue,
        queue_capacity: 1_024,
        max_parallel: 1,
    };
    let callback = lua.create_registry_value(callback)?;
    builder.handlers.push(RegisteredHandler {
        descriptor,
        callback,
    });
    Ok(())
}

fn parse_handler_args(
    lua: &Lua,
    args: &Variadic<Value>,
) -> mlua::Result<(String, Table, Function)> {
    match args.as_slice() {
        [Value::String(name), Value::Function(callback)] => Ok((
            name.to_str()?.to_owned(),
            lua.create_table()?,
            callback.clone(),
        )),
        [
            Value::String(name),
            Value::Table(options),
            Value::Function(callback),
        ] => Ok((name.to_str()?.to_owned(), options.clone(), callback.clone())),
        _ => Err(registration_error(
            "on expects (event, callback) or (event, options, callback)",
        )),
    }
}

fn handler_descriptor(
    kind: HandlerKind,
    options: &Table,
    default_dispatch: DispatchPolicy,
) -> mlua::Result<HandlerDescriptor> {
    let dispatch = options
        .get::<Option<String>>("dispatch")?
        .map_or(Ok(default_dispatch), |value| parse_dispatch(&value))?;
    let queue_capacity = options
        .get::<Option<usize>>("queue_capacity")?
        .unwrap_or(1_024);
    let max_parallel = options.get::<Option<usize>>("max_parallel")?.unwrap_or(4);
    if queue_capacity == 0 || queue_capacity > 10_000 || max_parallel == 0 || max_parallel > 32 {
        return Err(registration_error("handler limits must be non-zero"));
    }
    Ok(HandlerDescriptor {
        id: HandlerId::new(),
        kind,
        dispatch,
        queue_capacity,
        max_parallel,
    })
}

fn parse_activation(value: &str) -> mlua::Result<ActivationMode> {
    match value {
        "always" => Ok(ActivationMode::Always),
        "temporary" => Ok(ActivationMode::Temporary),
        _ => Err(registration_error("invalid activation mode")),
    }
}

fn parse_dispatch(value: &str) -> mlua::Result<DispatchPolicy> {
    match value {
        "queue" => Ok(DispatchPolicy::Queue),
        "parallel" => Ok(DispatchPolicy::Parallel),
        "latest" => Ok(DispatchPolicy::Latest),
        "drop" => Ok(DispatchPolicy::Drop),
        _ => Err(registration_error("invalid dispatch mode")),
    }
}

fn option_duration(table: &Table, key: &str) -> mlua::Result<Option<u64>> {
    table
        .get::<Option<Value>>(key)?
        .map(parse_duration)
        .transpose()
}

fn parse_duration(value: Value) -> mlua::Result<u64> {
    match value {
        Value::Integer(value) => {
            u64::try_from(value).map_err(|_| registration_error("duration must be non-negative"))
        }
        Value::String(value) => parse_duration_text(value.to_str()?.as_ref()),
        _ => Err(registration_error("duration must be milliseconds or text")),
    }
}

fn parse_duration_text(value: &str) -> mlua::Result<u64> {
    for (suffix, multiplier) in [("ms", 1), ("s", 1_000), ("m", 60_000), ("h", 3_600_000)] {
        if let Some(number) = value.strip_suffix(suffix) {
            let number = number
                .parse::<u64>()
                .map_err(|_| registration_error("invalid duration"))?;
            return number
                .checked_mul(multiplier)
                .ok_or_else(|| registration_error("duration is too large"));
        }
    }
    Err(registration_error("duration suffix must be ms, s, m, or h"))
}

fn string_array(table: &Table, key: &str) -> mlua::Result<Vec<String>> {
    let Some(values) = table.get::<Option<Table>>(key)? else {
        return Ok(Vec::new());
    };
    values.sequence_values::<String>().collect()
}

fn lock_open(
    state: &Arc<Mutex<RegistrationBuilder>>,
) -> mlua::Result<std::sync::MutexGuard<'_, RegistrationBuilder>> {
    let builder = state
        .lock()
        .map_err(|_| registration_error("registration state is unavailable"))?;
    if !builder.open {
        return Err(registration_error("registration phase is closed"));
    }
    Ok(builder)
}

fn registration_error(message: &'static str) -> mlua::Error {
    mlua::Error::external(message)
}

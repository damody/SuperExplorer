//! Script registration, lifecycle, and Lua runtime coordination.

use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use mlua::{
    Function, HookTriggers, Lua, LuaOptions, LuaSerdeExt as _, StdLib, Table, Value, VmState,
};

use crate::{
    AutomationError, AutomationErrorKind, AutomationResult, RegisteredScript,
    registration::register_script,
};

/// One restricted Lua 5.4 VM. A script registry owns exactly one active VM.
pub struct LuaVm {
    lua: Lua,
    registration: Option<RegisteredScript>,
    timer: Option<Arc<dyn crate::TimerHost>>,
    limits: LuaResourceLimits,
    budget: ExecutionBudget,
    task_contexts: Arc<Mutex<BTreeMap<usize, crate::TaskContext>>>,
}

/// Per-VM resource limits constrained by application hard maxima.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LuaResourceLimits {
    pub memory_bytes: usize,
    pub continuous_cpu_ms: u64,
    pub hook_instruction_interval: u32,
}

impl Default for LuaResourceLimits {
    fn default() -> Self {
        Self {
            memory_bytes: 128 * 1024 * 1024,
            continuous_cpu_ms: 2_000,
            hook_instruction_interval: 10_000,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct ExecutionBudget {
    deadline: Arc<Mutex<Option<Instant>>>,
}

struct ExecutionBudgetGuard(ExecutionBudget);

struct TaskContextGuard {
    contexts: Arc<Mutex<BTreeMap<usize, crate::TaskContext>>>,
    key: usize,
}

impl Drop for TaskContextGuard {
    fn drop(&mut self) {
        self.contexts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.key);
    }
}

impl ExecutionBudgetGuard {
    fn begin(budget: &ExecutionBudget, duration_ms: u64) -> Self {
        budget.begin_slice(duration_ms);
        Self(budget.clone())
    }
}

impl Drop for ExecutionBudgetGuard {
    fn drop(&mut self) {
        self.0.clear();
    }
}

impl ExecutionBudget {
    fn begin_slice(&self, duration_ms: u64) {
        let mut deadline = self
            .deadline
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *deadline = Some(Instant::now() + Duration::from_millis(duration_ms));
    }

    fn clear(&self) {
        let mut deadline = self
            .deadline
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *deadline = None;
    }

    fn exceeded(&self) -> bool {
        self.deadline
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some_and(|deadline| Instant::now() >= deadline)
    }
}

impl LuaVm {
    /// Creates a VM with only the approved pure standard libraries.
    ///
    /// # Errors
    ///
    /// Returns a script error if Lua initialization or global restriction fails.
    pub fn new() -> AutomationResult<Self> {
        Self::new_with_limits(LuaResourceLimits::default())
    }

    /// Creates a VM with validated memory and continuous-execution limits.
    ///
    /// # Errors
    ///
    /// Returns an input error outside hard limits or a script error when Lua setup fails.
    pub fn new_with_limits(limits: LuaResourceLimits) -> AutomationResult<Self> {
        if limits.memory_bytes == 0
            || limits.memory_bytes > 512 * 1024 * 1024
            || limits.continuous_cpu_ms == 0
            || limits.continuous_cpu_ms > 10_000
            || limits.hook_instruction_interval == 0
        {
            return Err(AutomationError::new(
                AutomationErrorKind::InvalidInput,
                "lua.limits",
                false,
                "The Lua resource limits are invalid",
            ));
        }
        let libraries =
            StdLib::COROUTINE | StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8;
        let lua = Lua::new_with(libraries, LuaOptions::new()).map_err(|error| lua_error(&error))?;
        lua.set_memory_limit(limits.memory_bytes)
            .map_err(|error| lua_error(&error))?;
        let budget = ExecutionBudget::default();
        let hook_budget = budget.clone();
        lua.set_global_hook(
            HookTriggers::new().every_nth_instruction(limits.hook_instruction_interval),
            move |_, _| {
                if hook_budget.exceeded() {
                    Err(mlua::Error::external(
                        "continuous Lua execution limit exceeded",
                    ))
                } else {
                    Ok(VmState::Continue)
                }
            },
        )
        .map_err(|error| lua_error(&error))?;
        let globals = lua.globals();
        for name in ["dofile", "loadfile", "require", "print"] {
            globals
                .set(name, Value::Nil)
                .map_err(|error| lua_error(&error))?;
        }
        drop(globals);
        Ok(Self {
            lua,
            registration: None,
            timer: None,
            limits,
            budget,
            task_contexts: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    /// Compiles a chunk without running registration or host effects.
    ///
    /// # Errors
    ///
    /// Returns a privacy-safe script error when the chunk does not compile.
    pub fn validate(&self, source: &str, chunk_name: &str) -> AutomationResult<()> {
        self.lua
            .load(source)
            .set_name(chunk_name)
            .into_function()
            .map(|_| ())
            .map_err(|error| lua_error(&error))
    }

    /// Returns whether a global exists, for capability validation and documentation gates.
    ///
    /// # Errors
    ///
    /// Returns a script error if Lua cannot read the globals table.
    pub fn has_global(&self, name: &str) -> AutomationResult<bool> {
        self.lua
            .globals()
            .get::<Value>(name)
            .map(|value| !value.is_nil())
            .map_err(|error| lua_error(&error))
    }

    /// Executes the restricted registration phase and stores its immutable result.
    ///
    /// # Errors
    ///
    /// Returns a script error for invalid metadata, subscriptions, watch declarations,
    /// or registration-time Lua failures.
    pub fn register(
        &mut self,
        source: &str,
        script_path: &std::path::Path,
    ) -> AutomationResult<()> {
        if self.registration.is_some() {
            return Err(AutomationError::new(
                AutomationErrorKind::Script,
                "lua.register",
                false,
                "The Lua VM is already registered",
            ));
        }
        let registration =
            register_script(&self.lua, source, script_path).map_err(|error| lua_error(&error))?;
        self.registration = Some(registration);
        Ok(())
    }

    /// Installs coroutine-friendly runtime primitives backed by an injected timer.
    ///
    /// `spawn` creates an async child coroutine. Passing the current task table as
    /// its second argument preserves cwd and parent identity without global mutable context.
    ///
    /// # Errors
    ///
    /// Returns a script error if Lua cannot install the functions.
    pub fn install_async_basics(
        &mut self,
        timer: Arc<dyn crate::TimerHost>,
    ) -> AutomationResult<()> {
        let await_function = self
            .lua
            .create_function(|_, value: Value| Ok(value))
            .map_err(|error| lua_error(&error))?;
        self.lua
            .globals()
            .set("await", await_function)
            .map_err(|error| lua_error(&error))?;

        let sleep_timer = Arc::clone(&timer);
        let sleep_budget = self.budget.clone();
        let slice_ms = self.limits.continuous_cpu_ms;
        let sleep_function = self
            .lua
            .create_async_function(move |_, duration: Value| {
                let timer = Arc::clone(&sleep_timer);
                let budget = sleep_budget.clone();
                async move {
                    let duration_ms = parse_runtime_duration(duration)?;
                    timer
                        .sleep(duration_ms)
                        .await
                        .map_err(|error| mlua::Error::external(error.to_string()))?;
                    budget.begin_slice(slice_ms);
                    Ok(())
                }
            })
            .map_err(|error| lua_error(&error))?;
        self.lua
            .globals()
            .set("sleep", sleep_function)
            .map_err(|error| lua_error(&error))?;

        let spawn_function = self
            .lua
            .create_async_function(
                |lua, (callback, parent): (Function, Option<Table>)| async move {
                    let child = lua.create_table()?;
                    child.set("id", uuid::Uuid::new_v4().to_string())?;
                    if let Some(parent) = parent {
                        child.set("parent_id", parent.get::<Option<String>>("id")?)?;
                        child.set("cwd", parent.get::<Option<String>>("cwd")?)?;
                    }
                    callback.call_async::<()>(child).await
                },
            )
            .map_err(|error| lua_error(&error))?;
        self.lua
            .globals()
            .set("spawn", spawn_function)
            .map_err(|error| lua_error(&error))?;
        self.timer = Some(timer);
        Ok(())
    }

    /// Installs task-relative async `fs` functions backed by a confirmation-aware host.
    ///
    /// # Errors
    ///
    /// Returns a script error if the API table cannot be installed.
    pub fn install_file_api(&self, host: Arc<dyn crate::FileHost>) -> AutomationResult<()> {
        let fs = self.lua.create_table().map_err(|error| lua_error(&error))?;

        let read_host = Arc::clone(&host);
        let read_contexts = Arc::clone(&self.task_contexts);
        fs.set(
            "read_text",
            self.lua
                .create_async_function(move |lua, (path, options): (String, Option<Table>)| {
                    let host = Arc::clone(&read_host);
                    let contexts = Arc::clone(&read_contexts);
                    async move {
                        let task = current_task(&lua, &contexts)?;
                        let path = resolve_host_path(&task, &path, options.as_ref());
                        let bytes = host
                            .read(path)
                            .await
                            .map_err(|error| mlua::Error::external(error.to_string()))?;
                        String::from_utf8(bytes)
                            .map_err(|_| mlua::Error::external("file is not valid UTF-8"))
                    }
                })
                .map_err(|error| lua_error(&error))?,
        )
        .map_err(|error| lua_error(&error))?;

        for (name, mode) in [
            ("write_text", crate::FileWriteMode::AtomicReplace),
            ("append_text", crate::FileWriteMode::Append),
        ] {
            let write_host = Arc::clone(&host);
            let write_contexts = Arc::clone(&self.task_contexts);
            fs.set(
                name,
                self.lua
                    .create_async_function(
                        move |lua, (path, text, options): (String, String, Option<Table>)| {
                            let host = Arc::clone(&write_host);
                            let contexts = Arc::clone(&write_contexts);
                            async move {
                                let task = current_task(&lua, &contexts)?;
                                let path = resolve_host_path(&task, &path, options.as_ref());
                                let selected = options
                                    .as_ref()
                                    .and_then(|table| {
                                        table.get::<Option<String>>("mode").ok().flatten()
                                    })
                                    .map_or(mode, |value| parse_file_mode(&value).unwrap_or(mode));
                                host.write(path, text.into_bytes(), selected)
                                    .await
                                    .map(|path| path.to_string_lossy().into_owned())
                                    .map_err(|error| mlua::Error::external(error.to_string()))
                            }
                        },
                    )
                    .map_err(|error| lua_error(&error))?,
            )
            .map_err(|error| lua_error(&error))?;
        }

        let remove_host = host;
        let remove_contexts = Arc::clone(&self.task_contexts);
        fs.set(
            "remove",
            self.lua
                .create_async_function(move |lua, (path, options): (String, Option<Table>)| {
                    let host = Arc::clone(&remove_host);
                    let contexts = Arc::clone(&remove_contexts);
                    async move {
                        let task = current_task(&lua, &contexts)?;
                        let path = resolve_host_path(&task, &path, options.as_ref());
                        host.remove(task.script_id, path)
                            .await
                            .map_err(|error| mlua::Error::external(error.to_string()))
                    }
                })
                .map_err(|error| lua_error(&error))?,
        )
        .map_err(|error| lua_error(&error))?;
        self.lua
            .globals()
            .set("fs", fs)
            .map_err(|error| lua_error(&error))
    }

    /// Installs async clipboard, notification, summary, and safe logging APIs.
    ///
    /// # Errors
    ///
    /// Returns a script error if any runtime table cannot be installed.
    pub fn install_presentation_api(
        &self,
        clipboard: Arc<dyn crate::ClipboardHost>,
        ui: Arc<dyn crate::UiHost>,
        logger: Arc<dyn crate::AutomationLogger>,
    ) -> AutomationResult<()> {
        let clipboard_table = self.lua.create_table().map_err(|error| lua_error(&error))?;
        clipboard_table
            .set(
                "read_text",
                self.lua
                    .create_async_function(move |_, ()| {
                        let clipboard = Arc::clone(&clipboard);
                        async move {
                            clipboard
                                .read_text()
                                .await
                                .map_err(|error| mlua::Error::external(error.to_string()))
                        }
                    })
                    .map_err(|error| lua_error(&error))?,
            )
            .map_err(|error| lua_error(&error))?;
        self.lua
            .globals()
            .set("clipboard", clipboard_table)
            .map_err(|error| lua_error(&error))?;

        let ui_table = self.lua.create_table().map_err(|error| lua_error(&error))?;
        let notify_ui = Arc::clone(&ui);
        ui_table
            .set(
                "notify",
                self.lua
                    .create_async_function(move |_, (title, body): (String, Option<String>)| {
                        let ui = Arc::clone(&notify_ui);
                        async move {
                            ui.present(crate::HostEffect::Notify { title, body })
                                .await
                                .map(|_| ())
                                .map_err(|error| mlua::Error::external(error.to_string()))
                        }
                    })
                    .map_err(|error| lua_error(&error))?,
            )
            .map_err(|error| lua_error(&error))?;
        ui_table
            .set(
                "show_summary",
                self.lua
                    .create_async_function(move |_, (text, options): (String, Option<Table>)| {
                        let ui = Arc::clone(&ui);
                        let popup = options
                            .as_ref()
                            .and_then(|table| table.get::<Option<bool>>("popup").ok().flatten())
                            .unwrap_or(false);
                        async move {
                            ui.present(crate::HostEffect::ShowSummary { text, popup })
                                .await
                                .map(|_| ())
                                .map_err(|error| mlua::Error::external(error.to_string()))
                        }
                    })
                    .map_err(|error| lua_error(&error))?,
            )
            .map_err(|error| lua_error(&error))?;
        self.lua
            .globals()
            .set("ui", ui_table)
            .map_err(|error| lua_error(&error))?;

        let log_table = self.lua.create_table().map_err(|error| lua_error(&error))?;
        let log_contexts = Arc::clone(&self.task_contexts);
        log_table
            .set(
                "info",
                self.lua
                    .create_async_function(move |lua, operation: String| {
                        let logger = Arc::clone(&logger);
                        let contexts = Arc::clone(&log_contexts);
                        async move {
                            let task = current_task(&lua, &contexts)?;
                            logger
                                .log(crate::AutomationLogRecord {
                                    level: crate::AutomationLogLevel::Info,
                                    operation,
                                    correlation_id: Some(task.correlation_id),
                                    safe_fields: BTreeMap::new(),
                                })
                                .await
                                .map_err(|error| mlua::Error::external(error.to_string()))
                        }
                    })
                    .map_err(|error| lua_error(&error))?,
            )
            .map_err(|error| lua_error(&error))?;
        self.lua
            .globals()
            .set("log", log_table)
            .map_err(|error| lua_error(&error))
    }

    /// Installs `ai.summarize`, including optional atomic task-relative TXT output.
    ///
    /// # Errors
    ///
    /// Returns a script error if the AI table cannot be installed.
    pub fn install_ai_api(
        &self,
        client: Arc<dyn explorer_ai::AiClient>,
        files: Arc<dyn crate::FileHost>,
    ) -> AutomationResult<()> {
        let ai = self.lua.create_table().map_err(|error| lua_error(&error))?;
        let contexts = Arc::clone(&self.task_contexts);
        ai.set(
            "summarize",
            self.lua
                .create_async_function(move |lua, options: Table| {
                    let client = Arc::clone(&client);
                    let files = Arc::clone(&files);
                    let contexts = Arc::clone(&contexts);
                    async move {
                        let input = options.get::<String>("text")?;
                        let model = options
                            .get::<Option<String>>("model")?
                            .unwrap_or_else(|| explorer_ai::DEEPSEEK_DEFAULT_MODEL.into());
                        let system_prompt = options.get::<Option<String>>("system_prompt")?;
                        let output = options
                            .get::<Option<Table>>("output")?
                            .map(|table| {
                                let path = table.get::<String>("path")?;
                                let base = table.get::<Option<String>>("base")?;
                                Ok::<_, mlua::Error>((path, base))
                            })
                            .transpose()?;
                        let task = current_task(&lua, &contexts)?;
                        let cancellation = explorer_ai::AiCancellation::default();
                        if task.cancellation.is_cancelled() {
                            cancellation.cancel();
                        }
                        let response = client
                            .execute_cancellable(
                                explorer_ai::AiRequest {
                                    operation: explorer_ai::AiOperation::Summarize,
                                    provider: "deepseek".into(),
                                    model,
                                    input,
                                    system_prompt,
                                    timeout_ms: task.deadline_unix_ms.map_or(90_000, |deadline| {
                                        deadline.saturating_sub(task.created_unix_ms)
                                    }),
                                    correlation_id: task.correlation_id.as_uuid().to_string(),
                                },
                                cancellation,
                            )
                            .await
                            .map_err(|error| mlua::Error::external(error.to_string()))?;
                        if let Some((path, base)) = output {
                            let path = std::path::PathBuf::from(path);
                            let path = if path.is_absolute() {
                                path
                            } else {
                                base.map_or_else(|| task.cwd.clone(), std::path::PathBuf::from)
                                    .join(path)
                            };
                            files
                                .write(
                                    path,
                                    response.text.as_bytes().to_vec(),
                                    crate::FileWriteMode::AtomicReplace,
                                )
                                .await
                                .map_err(|error| mlua::Error::external(error.to_string()))?;
                        }
                        let result = lua.create_table()?;
                        result.set("provider", response.provider)?;
                        result.set("model", response.model)?;
                        result.set("text", response.text)?;
                        Ok(result)
                    }
                })
                .map_err(|error| lua_error(&error))?,
        )
        .map_err(|error| lua_error(&error))?;
        self.lua
            .globals()
            .set("ai", ai)
            .map_err(|error| lua_error(&error))
    }

    /// Installs direct executable and fixed-interpreter script process APIs.
    ///
    /// # Errors
    ///
    /// Returns a script error if the process table cannot be installed.
    pub fn install_process_api(
        &self,
        processes: Arc<dyn crate::ProcessHost>,
        files: Arc<dyn crate::FileHost>,
        ui: Arc<dyn crate::UiHost>,
    ) -> AutomationResult<()> {
        let process = self.lua.create_table().map_err(|error| lua_error(&error))?;
        let run_processes = Arc::clone(&processes);
        let run_contexts = Arc::clone(&self.task_contexts);
        process
            .set(
                "run",
                self.lua
                    .create_async_function(
                        move |lua, (executable, arguments, options): (String, Table, Option<Table>)| {
                            let processes = Arc::clone(&run_processes);
                            let contexts = Arc::clone(&run_contexts);
                            async move {
                                let task = current_task(&lua, &contexts)?;
                                let request = process_request(
                                    &task,
                                    std::path::PathBuf::from(executable),
                                    lua_arguments(&arguments)?,
                                    options.as_ref(),
                                );
                                let result = processes
                                    .run(request)
                                    .await
                                    .map_err(|error| mlua::Error::external(error.to_string()))?;
                                process_result_table(&lua, &result)
                            }
                        },
                    )
                    .map_err(|error| lua_error(&error))?,
            )
            .map_err(|error| lua_error(&error))?;

        let script_contexts = Arc::clone(&self.task_contexts);
        process
            .set(
                "run_script",
                self.lua
                    .create_async_function(
                        move |lua, (script, arguments, options): (String, Table, Option<Table>)| {
                            let processes = Arc::clone(&processes);
                            let files = Arc::clone(&files);
                            let ui = Arc::clone(&ui);
                            let contexts = Arc::clone(&script_contexts);
                            async move {
                                let task = current_task(&lua, &contexts)?;
                                let script_path =
                                    resolve_host_path(&task, &script, options.as_ref());
                                let source = files
                                    .read(script_path.clone())
                                    .await
                                    .map_err(|error| mlua::Error::external(error.to_string()))?;
                                let extension = script_path
                                    .extension()
                                    .and_then(std::ffi::OsStr::to_str)
                                    .unwrap_or_default();
                                let risk = crate::ProcessPolicy::scan_script(
                                    &String::from_utf8_lossy(&source),
                                    extension,
                                );
                                if risk != crate::ScriptDeletionRisk::NoDeletionDetected
                                    && !ui
                                        .present(crate::HostEffect::ConfirmDeletion {
                                            script_id: task.script_id,
                                            paths: vec![script_path.clone()],
                                        })
                                        .await
                                        .map_err(|error| mlua::Error::external(error.to_string()))?
                                {
                                    return Err(mlua::Error::external("DeletionDenied"));
                                }
                                let arguments = lua_arguments(&arguments)?;
                                let (executable, arguments) =
                                    crate::ProcessPolicy::script_command(&script_path, &arguments)
                                        .map_err(|error| {
                                            mlua::Error::external(error.to_string())
                                        })?;
                                let request =
                                    process_request(&task, executable, arguments, options.as_ref());
                                let result = processes
                                    .run_script(request)
                                    .await
                                    .map_err(|error| mlua::Error::external(error.to_string()))?;
                                process_result_table(&lua, &result)
                            }
                        },
                    )
                    .map_err(|error| lua_error(&error))?,
            )
            .map_err(|error| lua_error(&error))?;
        self.lua
            .globals()
            .set("process", process)
            .map_err(|error| lua_error(&error))
    }

    /// Returns the completed immutable registration, if any.
    #[must_use]
    pub const fn registration(&self) -> Option<&RegisteredScript> {
        self.registration.as_ref()
    }

    /// Invokes a registered callback with owned event data and immutable task metadata.
    ///
    /// # Errors
    ///
    /// Returns a script error for an unknown handler, serialization failure, or Lua failure.
    pub fn invoke_registered(
        &self,
        handler_id: crate::HandlerId,
        event: &crate::AutomationEvent,
        task: &crate::TaskContext,
    ) -> AutomationResult<()> {
        task.ensure_active(event.timestamp_unix_ms)?;
        let registration = self.registration.as_ref().ok_or_else(|| {
            AutomationError::new(
                AutomationErrorKind::Script,
                "lua.invoke",
                false,
                "The Lua VM is not registered",
            )
        })?;
        let callback_key = registration.callback(handler_id).ok_or_else(|| {
            AutomationError::new(
                AutomationErrorKind::Script,
                "lua.invoke",
                false,
                "The Lua handler is not registered",
            )
        })?;
        let callback = self
            .lua
            .registry_value::<Function>(callback_key)
            .map_err(|error| lua_error(&error))?;
        let event = self
            .lua
            .to_value(event)
            .map_err(|error| lua_error(&error))?;
        let task_table = self.lua.create_table().map_err(|error| lua_error(&error))?;
        task_table
            .set("id", task.id.as_uuid().to_string())
            .and_then(|()| task_table.set("cwd", task.cwd.to_string_lossy().as_ref()))
            .and_then(|()| task_table.set("deadline_unix_ms", task.deadline_unix_ms))
            .map_err(|error| lua_error(&error))?;
        let context_key = self.lua.current_thread().to_pointer() as usize;
        let _context_guard = self.enter_task_context(context_key, task);
        let _budget_guard =
            ExecutionBudgetGuard::begin(&self.budget, self.limits.continuous_cpu_ms);
        callback
            .call::<()>((event, task_table))
            .map_err(|error| lua_error(&error))
    }

    /// Invokes a registered handler as an asynchronous Lua coroutine.
    ///
    /// # Errors
    ///
    /// Returns cancellation, timeout, unknown-handler, serialization, or Lua errors.
    pub async fn invoke_registered_async(
        &self,
        handler_id: crate::HandlerId,
        event: &crate::AutomationEvent,
        task: &crate::TaskContext,
    ) -> AutomationResult<()> {
        let now_ms = self
            .timer
            .as_ref()
            .map_or(event.timestamp_unix_ms, |timer| timer.now_ms());
        task.ensure_active(now_ms)?;
        let registration = self.registration.as_ref().ok_or_else(|| {
            AutomationError::new(
                AutomationErrorKind::Script,
                "lua.invoke_async",
                false,
                "The Lua VM is not registered",
            )
        })?;
        let callback_key = registration.callback(handler_id).ok_or_else(|| {
            AutomationError::new(
                AutomationErrorKind::Script,
                "lua.invoke_async",
                false,
                "The Lua handler is not registered",
            )
        })?;
        let callback = self
            .lua
            .registry_value::<Function>(callback_key)
            .map_err(|error| lua_error(&error))?;
        let event_value = self
            .lua
            .to_value(event)
            .map_err(|error| lua_error(&error))?;
        let task_table = self.lua.create_table().map_err(|error| lua_error(&error))?;
        task_table
            .set("id", task.id.as_uuid().to_string())
            .and_then(|()| task_table.set("cwd", task.cwd.to_string_lossy().as_ref()))
            .and_then(|()| task_table.set("deadline_unix_ms", task.deadline_unix_ms))
            .map_err(|error| lua_error(&error))?;
        let thread = self
            .lua
            .create_thread(callback)
            .map_err(|error| lua_error(&error))?;
        let context_key = thread.to_pointer() as usize;
        let _context_guard = self.enter_task_context(context_key, task);
        let _budget_guard =
            ExecutionBudgetGuard::begin(&self.budget, self.limits.continuous_cpu_ms);
        thread
            .into_async::<()>((event_value, task_table))
            .map_err(|error| lua_error(&error))?
            .await
            .map_err(|error| lua_error(&error))?;
        let now_ms = self
            .timer
            .as_ref()
            .map_or(event.timestamp_unix_ms, |timer| timer.now_ms());
        task.ensure_active(now_ms)
    }

    fn enter_task_context(&self, key: usize, task: &crate::TaskContext) -> TaskContextGuard {
        self.task_contexts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, task.clone());
        TaskContextGuard {
            contexts: Arc::clone(&self.task_contexts),
            key,
        }
    }
}

impl fmt::Debug for LuaVm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("LuaVm").finish_non_exhaustive()
    }
}

fn lua_error(error: &mlua::Error) -> AutomationError {
    let safe_detail = match error {
        mlua::Error::SyntaxError {
            incomplete_input, ..
        } => format!("kind=syntax incomplete={incomplete_input}"),
        mlua::Error::MemoryError(_) => "kind=memory".to_owned(),
        mlua::Error::SafetyError(_) => "kind=safety".to_owned(),
        _ => "kind=runtime".to_owned(),
    };
    AutomationError::new(
        AutomationErrorKind::Script,
        "lua.runtime",
        false,
        "The Lua script could not be loaded or executed",
    )
    .with_safe_detail(safe_detail)
}

fn parse_runtime_duration(value: Value) -> mlua::Result<u64> {
    match value {
        Value::Integer(value) => {
            u64::try_from(value).map_err(|_| mlua::Error::external("duration must be non-negative"))
        }
        Value::String(value) => {
            let value = value.to_str()?;
            for (suffix, multiplier) in [("ms", 1), ("s", 1_000), ("m", 60_000), ("h", 3_600_000)] {
                if let Some(number) = value.strip_suffix(suffix) {
                    let number = number
                        .parse::<u64>()
                        .map_err(|_| mlua::Error::external("invalid duration"))?;
                    return number
                        .checked_mul(multiplier)
                        .ok_or_else(|| mlua::Error::external("duration is too large"));
                }
            }
            Err(mlua::Error::external("invalid duration suffix"))
        }
        _ => Err(mlua::Error::external("invalid duration")),
    }
}

fn current_task(
    lua: &Lua,
    contexts: &Arc<Mutex<BTreeMap<usize, crate::TaskContext>>>,
) -> mlua::Result<crate::TaskContext> {
    let key = lua.current_thread().to_pointer() as usize;
    contexts
        .lock()
        .map_err(|_| mlua::Error::external("task context is unavailable"))?
        .get(&key)
        .cloned()
        .ok_or_else(|| mlua::Error::external("host API requires a handler task"))
}

fn resolve_host_path(
    task: &crate::TaskContext,
    path: &str,
    options: Option<&Table>,
) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(path);
    if path.is_absolute() {
        return path;
    }
    let base = options
        .and_then(|table| table.get::<Option<String>>("base").ok().flatten())
        .map_or_else(|| task.cwd.clone(), std::path::PathBuf::from);
    base.join(path)
}

fn parse_file_mode(value: &str) -> Option<crate::FileWriteMode> {
    match value {
        "create_new" => Some(crate::FileWriteMode::CreateNew),
        "atomic_replace" => Some(crate::FileWriteMode::AtomicReplace),
        "append" => Some(crate::FileWriteMode::Append),
        _ => None,
    }
}

fn lua_arguments(table: &Table) -> mlua::Result<Vec<String>> {
    table.sequence_values::<String>().collect()
}

fn process_request(
    task: &crate::TaskContext,
    executable: std::path::PathBuf,
    arguments: Vec<String>,
    options: Option<&Table>,
) -> crate::ProcessRequest {
    let cwd = options
        .and_then(|table| table.get::<Option<String>>("cwd").ok().flatten())
        .map_or_else(|| task.cwd.clone(), std::path::PathBuf::from);
    let timeout_ms = options
        .and_then(|table| table.get::<Option<Value>>("timeout").ok().flatten())
        .and_then(|value| parse_runtime_duration(value).ok())
        .unwrap_or(30_000);
    crate::ProcessRequest {
        executable,
        arguments,
        cwd,
        timeout_ms,
        correlation_id: task.correlation_id,
    }
}

fn process_result_table(lua: &Lua, result: &crate::ProcessResult) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("exit_code", result.exit_code)?;
    table.set("stdout", lua.create_string(&result.stdout)?)?;
    table.set("stderr", lua.create_string(&result.stderr)?)?;
    table.set("stdout_truncated", result.stdout_truncated)?;
    table.set("stderr_truncated", result.stderr_truncated)?;
    Ok(table)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::Arc,
        task::{Context, Poll, Waker},
    };

    use crate::{ManualTimer, fakes::FakeFileHost};
    use explorer_ai::{AiResponse, AiUsage, FakeAiClient};

    use super::{LuaResourceLimits, LuaVm};

    #[test]
    fn restricted_vm_exposes_only_approved_standard_libraries() {
        let vm = LuaVm::new().expect("restricted VM");
        for allowed in ["coroutine", "table", "string", "math", "utf8"] {
            assert!(vm.has_global(allowed).expect("read allowed global"));
        }
        for denied in [
            "io", "os", "package", "debug", "require", "dofile", "loadfile", "print",
        ] {
            assert!(!vm.has_global(denied).expect("read denied global"));
        }
    }

    #[test]
    fn validation_compiles_without_running_chunk() {
        let vm = LuaVm::new().expect("restricted VM");
        vm.validate("error('must not run')", "safe.lua")
            .expect("compile only");
        let error = vm
            .validate("function broken(", "broken.lua")
            .expect_err("syntax error");
        assert_eq!(
            error.safe_detail.as_deref(),
            Some("kind=syntax incomplete=true")
        );
        assert!(!error.to_string().contains("broken"));
    }

    #[test]
    fn registration_globals_exist_only_while_loading() {
        let mut vm = LuaVm::new().expect("restricted VM");
        vm.register(
            r#"
                script.configure {
                    name = "Summary",
                    activation = "always",
                    default_dispatch = "queue",
                    task_timeout = "2m"
                }
                watch {
                    root = "D:\\Notes",
                    recursive = true,
                    include = { "**/*.txt" },
                    exclude = { "**/summary/**" }
                }
                on("fs.created", { debounce = "500ms" }, function() end)
                hotkey("Ctrl+Alt+S", function() end)
                schedule.once("1s", function() end)
                schedule.every("1m", function() end)
                schedule.cron("0 0 * * * * *", "Asia/Taipei", function() end)
            "#,
            std::path::Path::new(r"D:\Scripts\summary.lua"),
        )
        .expect("register");
        let registration = vm.registration().expect("registration");
        assert_eq!(registration.config().name, "Summary");
        assert_eq!(registration.config().task_timeout_ms, 120_000);
        assert_eq!(registration.handlers().len(), 5);
        assert_eq!(registration.watches().len(), 1);
        for removed in ["script", "on", "hotkey", "watch", "schedule"] {
            assert!(!vm.has_global(removed).expect("removed global"));
        }
    }

    #[test]
    fn registered_callback_receives_event_and_captured_task_cwd() {
        let mut vm = LuaVm::new().expect("restricted VM");
        vm.register(
            r#"
                on("fs.created", function(event, task)
                    observed = event.sequence .. ":" .. task.cwd
                end)
            "#,
            std::path::Path::new(r"D:\Scripts\observe.lua"),
        )
        .expect("register");
        let handler = vm.registration().expect("registration").handlers()[0].id;
        let task = crate::task::tests_support::task_for_runtime_test(handler, r"D:\A");
        let event = task.event.clone();
        vm.invoke_registered(handler, &event, &task)
            .expect("invoke callback");
        assert_eq!(
            vm.lua
                .globals()
                .get::<String>("observed")
                .expect("observed"),
            r"1:D:\A"
        );
    }

    #[test]
    fn async_handler_yields_for_sleep_and_spawned_child() {
        let timer = Arc::new(ManualTimer::at(1));
        let mut vm = LuaVm::new().expect("restricted VM");
        vm.install_async_basics(timer.clone()).expect("async API");
        vm.register(
            r#"
                on("fs.created", function(event, task)
                    await(sleep("10ms"))
                    await(spawn(function(child)
                        observed = task.cwd .. ":" .. child.parent_id
                    end, task))
                end)
            "#,
            std::path::Path::new(r"D:\Scripts\async.lua"),
        )
        .expect("register");
        let handler = vm.registration().expect("registration").handlers()[0].id;
        let task = crate::task::tests_support::task_for_runtime_test(handler, r"D:\A");
        let event = task.event.clone();
        let mut invocation = Box::pin(vm.invoke_registered_async(handler, &event, &task));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(invocation.as_mut().poll(&mut context).is_pending());
        let _ = timer.advance(10);
        assert_eq!(invocation.as_mut().poll(&mut context), Poll::Ready(Ok(())));
        drop(invocation);
        let observed = vm
            .lua
            .globals()
            .get::<String>("observed")
            .expect("observed");
        assert!(observed.starts_with(r"D:\A:"));
        assert!(observed.ends_with(&task.id.as_uuid().to_string()));
    }

    #[test]
    fn file_api_uses_each_invocations_captured_task_directory() {
        let files = Arc::new(FakeFileHost::default());
        let mut vm = LuaVm::new().expect("restricted VM");
        vm.install_async_basics(Arc::new(ManualTimer::at(1)))
            .expect("async API");
        vm.install_file_api(files.clone()).expect("file API");
        vm.register(
            r#"
                on("directory.entered", function(event, task)
                    await(fs.write_text("summary.txt", task.cwd))
                end)
            "#,
            std::path::Path::new(r"D:\Scripts\files.lua"),
        )
        .expect("register");
        let handler = vm.registration().expect("registration").handlers()[0].id;
        let task = crate::task::tests_support::task_for_runtime_test(handler, r"D:\NewFolder");
        let event = task.event.clone();
        let mut invocation = Box::pin(vm.invoke_registered_async(handler, &event, &task));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert_eq!(invocation.as_mut().poll(&mut context), Poll::Ready(Ok(())));
        assert_eq!(
            files
                .file(&std::path::PathBuf::from(r"D:\NewFolder\summary.txt"))
                .expect("file"),
            Some(br"D:\NewFolder".to_vec())
        );
    }

    #[test]
    fn lua_can_call_deepseek_contract_and_write_atomic_txt() {
        let ai = Arc::new(FakeAiClient::default());
        ai.push_response(Ok(AiResponse {
            provider: "deepseek".into(),
            model: "deepseek-v4-flash".into(),
            text: "摘要結果".into(),
            usage: AiUsage::default(),
        }))
        .expect("AI response");
        let files = Arc::new(FakeFileHost::default());
        let mut vm = LuaVm::new().expect("restricted VM");
        vm.install_async_basics(Arc::new(ManualTimer::at(1)))
            .expect("async API");
        vm.install_ai_api(ai, files.clone()).expect("AI API");
        vm.register(
            r#"
                on("hotkey.triggered", function(event, task)
                    local result = await(ai.summarize {
                        text = "long text",
                        output = { path = "summary.txt", base = task.cwd }
                    })
                    observed_summary = result.text
                end)
            "#,
            std::path::Path::new(r"D:\Scripts\ai.lua"),
        )
        .expect("register");
        let handler = vm.registration().expect("registration").handlers()[0].id;
        let task = crate::task::tests_support::task_for_runtime_test(handler, r"D:\Notes");
        let event = task.event.clone();
        let mut invocation = Box::pin(vm.invoke_registered_async(handler, &event, &task));
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert_eq!(invocation.as_mut().poll(&mut context), Poll::Ready(Ok(())));
        assert_eq!(
            files
                .file(&std::path::PathBuf::from(r"D:\Notes\summary.txt"))
                .expect("file"),
            Some("摘要結果".as_bytes().to_vec())
        );
    }

    #[test]
    fn watchdog_and_memory_limits_fail_only_the_offending_vm() {
        let limits = LuaResourceLimits {
            memory_bytes: 2 * 1024 * 1024,
            continuous_cpu_ms: 1,
            hook_instruction_interval: 100,
        };
        let mut runaway = LuaVm::new_with_limits(limits).expect("limited VM");
        runaway
            .register(
                r#"on("task.started", function() while true do end end)"#,
                std::path::Path::new(r"D:\Scripts\runaway.lua"),
            )
            .expect("register runaway");
        let handler = runaway.registration().expect("registration").handlers()[0].id;
        let task = crate::task::tests_support::task_for_runtime_test(handler, r"D:\A");
        let event = task.event.clone();
        assert_eq!(
            runaway
                .invoke_registered(handler, &event, &task)
                .expect_err("watchdog")
                .kind,
            crate::AutomationErrorKind::Script
        );

        let mut healthy = LuaVm::new().expect("healthy VM");
        healthy
            .register(
                r#"on("task.started", function() healthy_result = 42 end)"#,
                std::path::Path::new(r"D:\Scripts\healthy.lua"),
            )
            .expect("register healthy");
        let healthy_handler = healthy.registration().expect("registration").handlers()[0].id;
        let healthy_task =
            crate::task::tests_support::task_for_runtime_test(healthy_handler, r"D:\B");
        let healthy_event = healthy_task.event.clone();
        healthy
            .invoke_registered(healthy_handler, &healthy_event, &healthy_task)
            .expect("healthy callback");
        assert_eq!(
            healthy
                .lua
                .globals()
                .get::<i64>("healthy_result")
                .expect("result"),
            42
        );
    }

    #[test]
    fn hard_resource_maxima_are_validated() {
        let error = LuaVm::new_with_limits(LuaResourceLimits {
            memory_bytes: 513 * 1024 * 1024,
            ..LuaResourceLimits::default()
        })
        .expect_err("hard memory maximum");
        assert_eq!(error.kind, crate::AutomationErrorKind::InvalidInput);
    }
}

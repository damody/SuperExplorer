//! Script discovery and atomic activation lifecycle.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use uuid::Uuid;

use crate::{
    ActivationMode, AutomationError, AutomationErrorKind, AutomationResult, CancellationToken,
    LuaResourceLimits, LuaVm, ScriptId,
};

const SCRIPT_ID_NAMESPACE: Uuid = Uuid::from_u128(0x78f2_7dd5_a845_4f55_9ba4_10fc_91e9_77fe);

/// A path and stable identifier discovered below the configured scripts directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredScript {
    pub id: ScriptId,
    pub path: PathBuf,
}

/// User-visible lifecycle state independent of Lua internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptLifecycleState {
    Disabled,
    Enabled,
    ReloadError,
}

/// Public lifecycle metadata for one discovered script.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptLifecycle {
    pub script: DiscoveredScript,
    pub activation: ActivationMode,
    pub state: ScriptLifecycleState,
    pub diagnostic: Option<AutomationError>,
}

struct ManagedScript {
    lifecycle: ScriptLifecycle,
    vm: Option<LuaVm>,
    cancellation: CancellationToken,
}

/// Owns all script VMs and swaps fresh registrations atomically.
#[derive(Default)]
pub struct ScriptRegistry {
    scripts: BTreeMap<PathBuf, ManagedScript>,
    limits: LuaResourceLimits,
}

impl ScriptRegistry {
    /// Creates an empty registry using the supplied per-script limits.
    #[must_use]
    pub fn with_limits(limits: LuaResourceLimits) -> Self {
        Self {
            scripts: BTreeMap::new(),
            limits,
        }
    }

    /// Discovers and validates scripts. `always` scripts become active; temporary scripts do not.
    /// Invalid files remain visible with a safe diagnostic.
    ///
    /// # Errors
    ///
    /// Returns an I/O error only when the configured root cannot be traversed.
    pub fn discover_and_restore(&mut self, root: &Path) -> AutomationResult<Vec<ScriptLifecycle>> {
        let discovered = discover_lua_scripts(root)?;
        for script in discovered {
            let source = read_script(&script.path)?;
            match build_vm(&source, &script.path, self.limits) {
                Ok(vm) => {
                    let activation = vm
                        .registration()
                        .map_or(ActivationMode::Temporary, |value| value.config().activation);
                    let enabled = activation == ActivationMode::Always;
                    self.scripts.insert(
                        script.path.clone(),
                        ManagedScript {
                            lifecycle: ScriptLifecycle {
                                script,
                                activation,
                                state: if enabled {
                                    ScriptLifecycleState::Enabled
                                } else {
                                    ScriptLifecycleState::Disabled
                                },
                                diagnostic: None,
                            },
                            vm: enabled.then_some(vm),
                            cancellation: CancellationToken::default(),
                        },
                    );
                }
                Err(error) => {
                    self.scripts.insert(
                        script.path.clone(),
                        ManagedScript {
                            lifecycle: ScriptLifecycle {
                                script,
                                activation: ActivationMode::Temporary,
                                state: ScriptLifecycleState::ReloadError,
                                diagnostic: Some(error),
                            },
                            vm: None,
                            cancellation: CancellationToken::default(),
                        },
                    );
                }
            }
        }
        Ok(self.list())
    }

    /// Enables a discovered script from its current source.
    ///
    /// # Errors
    ///
    /// Returns a safe I/O or Lua registration error, leaving the prior state unchanged.
    pub fn enable(&mut self, path: &Path) -> AutomationResult<()> {
        let source = read_script(path)?;
        let vm = build_vm(&source, path, self.limits)?;
        self.replace_vm(path, vm, ScriptLifecycleState::Enabled);
        Ok(())
    }

    /// Validates a fresh VM and swaps it only after complete registration succeeds.
    ///
    /// # Errors
    ///
    /// Returns the fresh VM error. The active VM remains available on failure.
    pub fn reload(&mut self, path: &Path) -> AutomationResult<()> {
        let result = read_script(path).and_then(|source| build_vm(&source, path, self.limits));
        match result {
            Ok(vm) => {
                self.replace_vm(path, vm, ScriptLifecycleState::Enabled);
                Ok(())
            }
            Err(error) => {
                if let Some(managed) = self.scripts.get_mut(path) {
                    managed.lifecycle.state = ScriptLifecycleState::ReloadError;
                    managed.lifecycle.diagnostic = Some(error.clone());
                }
                Err(error)
            }
        }
    }

    /// Cancels all owned work and removes the VM without removing the source file.
    pub fn disable(&mut self, path: &Path) {
        if let Some(managed) = self.scripts.get_mut(path) {
            managed.cancellation.cancel();
            managed.vm = None;
            managed.cancellation = CancellationToken::default();
            managed.lifecycle.state = ScriptLifecycleState::Disabled;
            managed.lifecycle.diagnostic = None;
        }
    }

    /// Cancels and forgets one managed script, including its inactive lifecycle metadata.
    pub fn remove(&mut self, path: &Path) -> Option<ScriptLifecycle> {
        let mut managed = self.scripts.remove(path)?;
        managed.cancellation.cancel();
        managed.vm = None;
        Some(managed.lifecycle)
    }

    /// Cancels scripts in stable path order, then drops every VM and owned registration.
    pub fn shutdown(&mut self) {
        for managed in self.scripts.values_mut() {
            managed.cancellation.cancel();
            managed.vm = None;
            managed.lifecycle.state = ScriptLifecycleState::Disabled;
        }
    }

    /// Returns lifecycle snapshots in stable path order.
    #[must_use]
    pub fn list(&self) -> Vec<ScriptLifecycle> {
        self.scripts
            .values()
            .map(|managed| managed.lifecycle.clone())
            .collect()
    }

    /// Returns an active VM for dispatch composition.
    #[must_use]
    pub fn active_vm(&self, path: &Path) -> Option<&LuaVm> {
        self.scripts
            .get(path)
            .and_then(|managed| managed.vm.as_ref())
    }

    fn replace_vm(&mut self, path: &Path, vm: LuaVm, state: ScriptLifecycleState) {
        let activation = vm
            .registration()
            .map_or(ActivationMode::Temporary, |value| value.config().activation);
        let managed = self
            .scripts
            .entry(path.to_path_buf())
            .or_insert_with(|| ManagedScript {
                lifecycle: ScriptLifecycle {
                    script: discovered_script(path),
                    activation,
                    state: ScriptLifecycleState::Disabled,
                    diagnostic: None,
                },
                vm: None,
                cancellation: CancellationToken::default(),
            });
        managed.cancellation.cancel();
        managed.vm = Some(vm);
        managed.cancellation = CancellationToken::default();
        managed.lifecycle.activation = activation;
        managed.lifecycle.state = state;
        managed.lifecycle.diagnostic = None;
    }
}

/// Recursively discovers `.lua` files in deterministic path order.
///
/// # Errors
///
/// Returns a privacy-safe I/O error if traversal fails.
pub fn discover_lua_scripts(root: &Path) -> AutomationResult<Vec<DiscoveredScript>> {
    let mut paths = Vec::new();
    discover_into(root, &mut paths)?;
    paths.sort();
    Ok(paths
        .into_iter()
        .map(|path| discovered_script(&path))
        .collect())
}

fn discover_into(root: &Path, paths: &mut Vec<PathBuf>) -> AutomationResult<()> {
    let entries = fs::read_dir(root).map_err(|_| io_error("script.discover"))?;
    for entry in entries {
        let entry = entry.map_err(|_| io_error("script.discover"))?;
        let file_type = entry.file_type().map_err(|_| io_error("script.discover"))?;
        if file_type.is_dir() {
            discover_into(&entry.path(), paths)?;
        } else if file_type.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lua"))
        {
            paths.push(entry.path());
        }
    }
    Ok(())
}

fn discovered_script(path: &Path) -> DiscoveredScript {
    let stable_path = path.to_string_lossy().replace('/', "\\").to_lowercase();
    DiscoveredScript {
        id: ScriptId::from_uuid(Uuid::new_v5(&SCRIPT_ID_NAMESPACE, stable_path.as_bytes())),
        path: path.to_path_buf(),
    }
}

fn read_script(path: &Path) -> AutomationResult<String> {
    fs::read_to_string(path).map_err(|_| io_error("script.read"))
}

fn build_vm(source: &str, path: &Path, limits: LuaResourceLimits) -> AutomationResult<LuaVm> {
    let mut vm = LuaVm::new_with_limits(limits)?;
    vm.register(source, path)?;
    Ok(vm)
}

fn io_error(operation: &'static str) -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::FileSystem,
        operation,
        true,
        "The automation script could not be read",
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{ScriptLifecycleState, ScriptRegistry, discover_lua_scripts};

    #[test]
    fn discovery_is_recursive_sorted_and_identity_is_stable() {
        let root = tempdir().expect("tempdir");
        fs::create_dir(root.path().join("nested")).expect("nested");
        fs::write(root.path().join("z.lua"), "").expect("z");
        fs::write(root.path().join("nested/a.lua"), "").expect("a");
        fs::write(root.path().join("ignored.txt"), "").expect("ignored");
        let first = discover_lua_scripts(root.path()).expect("discovery");
        let second = discover_lua_scripts(root.path()).expect("discovery");
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert!(first[0].path < first[1].path);
    }

    #[test]
    fn restore_enables_always_but_not_temporary_scripts() {
        let root = tempdir().expect("tempdir");
        let always = root.path().join("always.lua");
        let temporary = root.path().join("temporary.lua");
        fs::write(&always, "script.configure { activation = 'always' }").expect("always");
        fs::write(&temporary, "script.configure { activation = 'temporary' }").expect("temporary");
        let mut registry = ScriptRegistry::default();
        let entries = registry.discover_and_restore(root.path()).expect("restore");
        assert_eq!(entries[0].state, ScriptLifecycleState::Enabled);
        assert_eq!(entries[1].state, ScriptLifecycleState::Disabled);
        assert!(registry.active_vm(&always).is_some());
        assert!(registry.active_vm(&temporary).is_none());
    }

    #[test]
    fn invalid_reload_preserves_active_vm_and_shutdown_drops_it() {
        let root = tempdir().expect("tempdir");
        let script = root.path().join("active.lua");
        fs::write(&script, "script.configure { activation = 'always' }").expect("script");
        let mut registry = ScriptRegistry::default();
        registry.discover_and_restore(root.path()).expect("restore");
        fs::write(&script, "function broken(").expect("invalid update");
        assert!(registry.reload(&script).is_err());
        assert!(registry.active_vm(&script).is_some());
        assert_eq!(registry.list()[0].state, ScriptLifecycleState::ReloadError);
        registry.shutdown();
        assert!(registry.active_vm(&script).is_none());
    }

    #[test]
    fn repeated_enable_reload_disable_releases_each_vm() {
        let root = tempdir().expect("tempdir");
        let script = root.path().join("repeat.lua");
        fs::write(&script, "script.configure { activation = 'temporary' }").expect("script");
        let mut registry = ScriptRegistry::default();
        registry
            .discover_and_restore(root.path())
            .expect("discover");
        for _ in 0..100 {
            registry.enable(&script).expect("enable");
            assert!(registry.active_vm(&script).is_some());
            registry.reload(&script).expect("reload");
            registry.disable(&script);
            assert!(registry.active_vm(&script).is_none());
        }
        registry.shutdown();
    }
}

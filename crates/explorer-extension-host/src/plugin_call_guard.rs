//! Durable native-call markers and the startup Safe Mode deny overlay.
#![allow(clippy::missing_errors_doc)]

use std::{
    collections::{BTreeMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use ring::rand::{SecureRandom as _, SystemRandom};
use serde::{Deserialize, Serialize};

use crate::NativeLifecycleErrorV1;

const MARKER_SCHEMA_V1: u32 = 1;
const MAX_MARKER_BYTES_V1: usize = 4 * 1024;
const MAX_MARKER_ENTRIES_V1: usize = 128;
const MAX_LAUNCH_NAMESPACES_V1: usize = 128;
const MAX_STARTUP_SCAN_DURATION_V1: Duration = Duration::from_secs(1);
const GLOBAL_INCIDENT_ID_V1: NativeSafeModeIncidentIdV1 = NativeSafeModeIncidentIdV1(0);
pub(crate) const MAX_NATIVE_CALL_TIMINGS_V1: usize = 128;

#[cfg(all(test, windows))]
type ReopenDeadNamespaceHookV1 = (PathBuf, Box<dyn FnOnce() + Send>);
#[cfg(all(test, windows))]
static REOPEN_DEAD_NAMESPACE_HOOK: Mutex<Option<ReopenDeadNamespaceHookV1>> = Mutex::new(None);

/// A native extension callback guarded by a durable marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeCallOperationV1 {
    /// A package/incarnation-scoped attempt written before `LoadLibrary`.
    LoadLibrary,
    /// `LoadLibrary` completed, but a typed post-map validation/admission gate rejected it.
    LoadRejectedResident,
    Registrar,
    JobProvider,
    BatchColumnProvider,
    VisualMeasure,
    VisualRender,
}

/// Opaque identifier for one recovered Safe Mode incident.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NativeSafeModeIncidentIdV1(u64);

/// The bounded, path-free class of a Safe Mode incident.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeSafeModeIncidentKindV1 {
    RegistrarInProgress,
    UnsafeMarkerState,
}

/// A safe, path-free recovered native-call incident.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeSafeModeIncidentV1 {
    RegistrarInProgress {
        incident_id: NativeSafeModeIncidentIdV1,
        package_id: String,
        sealed_manifest_digest: String,
        entrypoint_id: String,
        root_module_id: String,
        primary_interface_namespace: u32,
        primary_interface_value: u64,
        operation: NativeCallOperationV1,
    },
    UnsafeMarkerState {
        incident_id: NativeSafeModeIncidentIdV1,
    },
}

impl NativeSafeModeIncidentV1 {
    /// Returns the opaque identifier accepted by scoped confirmation.
    #[must_use]
    pub const fn incident_id(&self) -> NativeSafeModeIncidentIdV1 {
        match self {
            Self::RegistrarInProgress { incident_id, .. }
            | Self::UnsafeMarkerState { incident_id } => *incident_id,
        }
    }

    /// Returns the safe incident class.
    #[must_use]
    pub const fn kind(&self) -> NativeSafeModeIncidentKindV1 {
        match self {
            Self::RegistrarInProgress { .. } => NativeSafeModeIncidentKindV1::RegistrarInProgress,
            Self::UnsafeMarkerState { .. } => NativeSafeModeIncidentKindV1::UnsafeMarkerState,
        }
    }
}

/// Sanitized terminal class retained for bounded native-call timing diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeCallTerminalV1 {
    Accepted,
    PluginError,
    Incompatible,
    Panicked,
    MarkerFailure,
    SafeModeDenied,
}

/// A path-free native callback timing record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCallTimingV1 {
    pub package_id: String,
    /// Sealed registrar entrypoint or provider contribution identity.
    pub callback_id: String,
    pub primary_interface_namespace: u32,
    pub primary_interface_value: u64,
    pub operation: NativeCallOperationV1,
    pub elapsed: Duration,
    pub terminal: NativeCallTerminalV1,
    pub slow: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MarkerV1 {
    schema_version: u32,
    package_id: String,
    sealed_manifest_digest: String,
    entrypoint_id: String,
    root_module_id: String,
    primary_interface_namespace: u32,
    primary_interface_value: u64,
    operation: NativeCallOperationV1,
}

impl MarkerV1 {
    fn incident(&self, incident_id: u64) -> NativeSafeModeIncidentV1 {
        NativeSafeModeIncidentV1::RegistrarInProgress {
            incident_id: NativeSafeModeIncidentIdV1(incident_id),
            package_id: self.package_id.clone(),
            sealed_manifest_digest: self.sealed_manifest_digest.clone(),
            entrypoint_id: self.entrypoint_id.clone(),
            root_module_id: self.root_module_id.clone(),
            primary_interface_namespace: self.primary_interface_namespace,
            primary_interface_value: self.primary_interface_value,
            operation: self.operation,
        }
    }

    fn matches(&self, other: &Self) -> bool {
        self.package_id == other.package_id
            && self.sealed_manifest_digest == other.sealed_manifest_digest
            && self.entrypoint_id == other.entrypoint_id
            && self.root_module_id == other.root_module_id
            && self.primary_interface_namespace == other.primary_interface_namespace
            && self.primary_interface_value == other.primary_interface_value
            && self.operation == other.operation
    }
}

enum OverlayV1 {
    Clean,
    Global(GlobalEvidenceV1),
    Incidents(BTreeMap<u64, RecoveredMarkerV1>),
}

struct RecoveredMarkerV1 {
    marker: MarkerV1,
    namespace: Arc<RecoveredNamespaceV1>,
    #[cfg(windows)]
    file: Option<File>,
    #[cfg(not(windows))]
    path: PathBuf,
}

struct RecoveredNamespaceV1 {
    lease: DirectoryLeaseV1,
    #[cfg(windows)]
    _owner: File,
}

struct ScannedMarkerV1 {
    marker: MarkerV1,
    #[cfg(windows)]
    file: File,
    #[cfg(not(windows))]
    path: PathBuf,
}

enum GlobalEvidenceV1 {
    /// A state transition failed without a durable filesystem object to move.
    /// Confirmation must rescan rather than silently clearing this overlay.
    Rescan,
    /// An exact, no-follow handle for the root child that made marker state
    /// unsafe.  Confirmation moves this handle, never a reopened path.
    #[cfg(windows)]
    Handle {
        source: File,
        parent: Arc<DirectoryLeaseV1>,
        state_parent: Arc<DirectoryLeaseV1>,
        state_parent_path: PathBuf,
    },
    #[cfg(not(windows))]
    Path(PathBuf),
}

struct LaunchLeaseV1 {
    #[cfg(windows)]
    owner: Option<File>,
}

struct DirectoryLeaseV1 {
    #[cfg(windows)]
    handle: Option<File>,
}

struct MarkerFileV1 {
    file: Option<File>,
    #[cfg(not(windows))]
    path: PathBuf,
}

pub(crate) struct PluginCallGuardStoreV1 {
    root: PathBuf,
    launch: PathBuf,
    _application_state_lease: DirectoryLeaseV1,
    marker_root_lease: DirectoryLeaseV1,
    launch_directory_lease: DirectoryLeaseV1,
    launch_lease: LaunchLeaseV1,
    overlay: Mutex<OverlayV1>,
    io: Mutex<()>,
    next_id: AtomicU64,
    timings: Mutex<VecDeque<NativeCallTimingV1>>,
    slow_threshold: Duration,
}

impl PluginCallGuardStoreV1 {
    pub(crate) fn open(
        root: PathBuf,
        slow_threshold: Duration,
    ) -> Result<Arc<Self>, NativeLifecycleErrorV1> {
        validate_state_directory(&root)?;
        let application_state = root
            .parent()
            .ok_or(NativeLifecycleErrorV1::MarkerStateUnavailable)?;
        let application_state_lease = acquire_directory_lease(application_state)?;
        let marker_root_lease = acquire_directory_lease(&root)?;
        let (launch, launch_directory_lease, launch_lease) = create_launch_namespace(&root)?;
        let store = Arc::new(Self {
            root,
            launch,
            _application_state_lease: application_state_lease,
            marker_root_lease,
            launch_directory_lease,
            launch_lease,
            overlay: Mutex::new(OverlayV1::Clean),
            io: Mutex::new(()),
            next_id: AtomicU64::new(1),
            timings: Mutex::new(VecDeque::new()),
            slow_threshold,
        });
        store.scan()?;
        Ok(store)
    }

    pub(crate) fn begin(
        self: &Arc<Self>,
        marker: &MarkerV1,
    ) -> Result<PluginCallGuardV1, GuardErrorV1> {
        let _io = self.io.lock().map_err(|_| GuardErrorV1::Fault)?;
        {
            if !marker.is_valid() {
                self.set_global();
                return Err(GuardErrorV1::Fault);
            }
            let overlay = self.overlay.lock().map_err(|_| GuardErrorV1::Fault)?;
            match &*overlay {
                OverlayV1::Global(_) => return Err(GuardErrorV1::Denied),
                OverlayV1::Incidents(incidents)
                    if incidents.values().any(|saved| saved.marker.matches(marker)) =>
                {
                    return Err(GuardErrorV1::Denied);
                }
                _ => {}
            }
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let final_path = self.launch.join(marker_name(id));
        let bytes = serde_json::to_vec(marker).map_err(|_| GuardErrorV1::Fault)?;
        if bytes.len() > MAX_MARKER_BYTES_V1 {
            return Err(GuardErrorV1::Fault);
        }
        let write = (|| -> io::Result<MarkerFileV1> {
            let mut file = create_new_marker_file(&final_path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            sync_directory(&self.launch)?;
            Ok(MarkerFileV1 {
                file: Some(file),
                #[cfg(not(windows))]
                path: final_path.clone(),
            })
        })();
        let Ok(marker_file) = write else {
            self.set_global();
            return Err(GuardErrorV1::Fault);
        };
        Ok(PluginCallGuardV1 {
            store: Arc::clone(self),
            marker_file,
            marker: marker.clone(),
        })
    }

    /// Returns whether Safe Mode currently blocks this exact callback marker.
    /// This is used during startup preflight so a recovered incident prevents
    /// every registrar callback in the affected package admission transaction.
    pub(crate) fn denies(&self, marker: &MarkerV1) -> bool {
        let Ok(overlay) = self.overlay.lock() else {
            return true;
        };
        match &*overlay {
            OverlayV1::Global(_) => true,
            OverlayV1::Incidents(incidents) => {
                incidents.values().any(|saved| saved.marker.matches(marker))
            }
            OverlayV1::Clean => false,
        }
    }

    pub(crate) fn incidents(&self) -> Vec<NativeSafeModeIncidentV1> {
        let Ok(overlay) = self.overlay.lock() else {
            return Vec::new();
        };
        match &*overlay {
            OverlayV1::Incidents(incidents) => incidents
                .iter()
                .map(|(id, recovered)| recovered.marker.incident(*id))
                .collect(),
            OverlayV1::Global(_) => vec![NativeSafeModeIncidentV1::UnsafeMarkerState {
                incident_id: GLOBAL_INCIDENT_ID_V1,
            }],
            OverlayV1::Clean => Vec::new(),
        }
    }

    pub(crate) fn confirm(
        &self,
        incident_id: NativeSafeModeIncidentIdV1,
    ) -> Result<(), NativeLifecycleErrorV1> {
        let _io = self
            .io
            .lock()
            .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
        let mut overlay = self
            .overlay
            .lock()
            .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
        if matches!(*overlay, OverlayV1::Global(_)) {
            if incident_id != GLOBAL_INCIDENT_ID_V1 {
                return Err(NativeLifecycleErrorV1::SafeModeIncidentUnknown);
            }
            drop(overlay);
            return self.confirm_global_locked();
        }
        let mut marker = match &mut *overlay {
            OverlayV1::Incidents(incidents) => incidents
                .remove(&incident_id.0)
                .ok_or(NativeLifecycleErrorV1::SafeModeIncidentUnknown)?,
            OverlayV1::Clean => return Err(NativeLifecycleErrorV1::SafeModeIncidentUnknown),
            OverlayV1::Global(_) => unreachable!("global overlay handled above"),
        };
        drop(overlay);
        #[cfg(windows)]
        {
            let file = marker
                .file
                .take()
                .ok_or(NativeLifecycleErrorV1::MarkerStateUnavailable)?;
            if delete_file_handle(&file).is_err() {
                marker.file = Some(file);
                self.restore_scoped_incident(incident_id, marker)?;
                return Err(NativeLifecycleErrorV1::MarkerStateUnavailable);
            }
            drop(file);
        }
        #[cfg(not(windows))]
        fs::remove_file(&marker.path)
            .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
        if marker.namespace.lease.sync().is_err() {
            self.set_global();
            return Err(NativeLifecycleErrorV1::MarkerStateUnavailable);
        }
        Ok(())
    }

    fn restore_scoped_incident(
        &self,
        incident_id: NativeSafeModeIncidentIdV1,
        marker: RecoveredMarkerV1,
    ) -> Result<(), NativeLifecycleErrorV1> {
        let mut overlay = self
            .overlay
            .lock()
            .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
        let OverlayV1::Incidents(incidents) = &mut *overlay else {
            return Err(NativeLifecycleErrorV1::MarkerStateUnavailable);
        };
        incidents.insert(incident_id.0, marker);
        Ok(())
    }

    pub(crate) fn record_timing(
        &self,
        marker: &MarkerV1,
        elapsed: Duration,
        terminal: NativeCallTerminalV1,
    ) {
        if let Ok(mut timings) = self.timings.lock() {
            if timings.len() == MAX_NATIVE_CALL_TIMINGS_V1 {
                timings.pop_front();
            }
            timings.push_back(NativeCallTimingV1 {
                package_id: marker.package_id.clone(),
                callback_id: marker.entrypoint_id.clone(),
                primary_interface_namespace: marker.primary_interface_namespace,
                primary_interface_value: marker.primary_interface_value,
                operation: marker.operation,
                elapsed,
                terminal,
                slow: elapsed >= self.slow_threshold,
            });
        }
    }

    pub(crate) fn timings(&self) -> Vec<NativeCallTimingV1> {
        self.timings
            .lock()
            .map(|timings| timings.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn scan(&self) -> Result<(), NativeLifecycleErrorV1> {
        self.scan_until(Instant::now() + MAX_STARTUP_SCAN_DURATION_V1)
    }

    fn scan_until(&self, deadline: Instant) -> Result<(), NativeLifecycleErrorV1> {
        let mut incidents = BTreeMap::new();
        let mut incident_id = 1_u64;
        let mut unsafe_namespace = None;
        let mut namespace_count = 0_usize;
        for entry in
            fs::read_dir(&self.root).map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?
        {
            let Ok(entry) = entry else {
                unsafe_namespace = Some(self.root.clone());
                break;
            };
            let namespace = entry.path();
            if namespace == self.launch {
                continue;
            }
            namespace_count = namespace_count.saturating_add(1);
            if namespace_count > MAX_LAUNCH_NAMESPACES_V1 || Instant::now() > deadline {
                unsafe_namespace = Some(namespace);
                break;
            }
            if entry
                .file_type()
                .map_or(true, |kind| !kind.is_dir() || kind.is_symlink())
                || is_reparse(&namespace)
                || !valid_launch_name(entry.file_name().to_string_lossy().as_ref())
            {
                unsafe_namespace = Some(namespace);
                break;
            }
            // Pin the exact namespace before inspecting its owner.  Live
            // launches already deny DELETE, so only a dead namespace is
            // reopened with DELETE access after an identity comparison.
            let mut namespace_lease = acquire_directory_lease(&namespace)
                .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
            let Ok(owner) = namespace_owner_state(&namespace) else {
                unsafe_namespace = Some(namespace);
                break;
            };
            if matches!(owner, NamespaceOwnerStateV1::Live) {
                continue;
            }
            let NamespaceOwnerStateV1::Dead(owner) = owner else {
                unreachable!("live namespace handled above");
            };
            #[cfg(all(test, windows))]
            let mut owner = Some(owner);
            #[cfg(not(all(test, windows)))]
            let owner = Some(owner);
            #[cfg(all(test, windows))]
            prepare_reopen_dead_namespace_hook(&namespace, &mut owner);
            let Ok(namespace_lease) = reopen_dead_namespace(&namespace, &mut namespace_lease)
            else {
                unsafe_namespace = Some(namespace);
                break;
            };
            let Some(owner) = owner else {
                unsafe_namespace = Some(namespace);
                break;
            };
            let Ok(recovered) = scan_dead_namespace(&namespace, deadline) else {
                unsafe_namespace = Some(namespace);
                break;
            };
            if recovered.is_empty() {
                if cleanup_dead_namespace(owner, namespace_lease, &namespace).is_err() {
                    unsafe_namespace = Some(namespace);
                    break;
                }
                continue;
            }
            if incidents.len().saturating_add(recovered.len()) > MAX_MARKER_ENTRIES_V1 {
                unsafe_namespace = Some(namespace);
                break;
            }
            let recovered_namespace = Arc::new(RecoveredNamespaceV1 {
                lease: namespace_lease,
                #[cfg(windows)]
                _owner: owner,
            });
            for recovered in recovered {
                let id = incident_id;
                incident_id = incident_id.saturating_add(1);
                incidents.insert(
                    id,
                    RecoveredMarkerV1 {
                        marker: recovered.marker,
                        namespace: Arc::clone(&recovered_namespace),
                        #[cfg(windows)]
                        file: Some(recovered.file),
                        #[cfg(not(windows))]
                        path: recovered.path,
                    },
                );
            }
        }
        let mut overlay = self
            .overlay
            .lock()
            .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
        *overlay = if let Some(namespace) = unsafe_namespace {
            OverlayV1::Global(self.capture_global_evidence(&namespace))
        } else if incidents.is_empty() {
            OverlayV1::Clean
        } else {
            OverlayV1::Incidents(incidents)
        };
        Ok(())
    }

    fn set_global(&self) {
        if let Ok(mut overlay) = self.overlay.lock() {
            *overlay = OverlayV1::Global(GlobalEvidenceV1::Rescan);
        }
    }

    pub(crate) fn is_global(&self) -> bool {
        self.overlay
            .lock()
            .is_ok_and(|overlay| matches!(*overlay, OverlayV1::Global(_)))
    }

    fn confirm_global_locked(&self) -> Result<(), NativeLifecycleErrorV1> {
        let mut evidence = {
            let mut overlay = self
                .overlay
                .lock()
                .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
            match std::mem::replace(&mut *overlay, OverlayV1::Clean) {
                OverlayV1::Global(evidence) => evidence,
                other => {
                    *overlay = other;
                    return Err(NativeLifecycleErrorV1::SafeModeIncidentUnknown);
                }
            }
        };
        if let Err(error) = quarantine_global_evidence(&mut evidence) {
            if let Ok(mut overlay) = self.overlay.lock() {
                *overlay = OverlayV1::Global(evidence);
            }
            return Err(error);
        }
        if self.scan().is_err() {
            self.set_global();
            return Err(NativeLifecycleErrorV1::MarkerStateUnavailable);
        }
        if self.is_global() {
            return Err(NativeLifecycleErrorV1::MarkerStateUnavailable);
        }
        Ok(())
    }

    fn capture_global_evidence(&self, namespace: &Path) -> GlobalEvidenceV1 {
        #[cfg(windows)]
        {
            let Some(state_parent) = self.root.parent() else {
                return GlobalEvidenceV1::Rescan;
            };
            let Ok(source) = open_evidence_source(namespace) else {
                return GlobalEvidenceV1::Rescan;
            };
            let Ok(parent) = acquire_directory_lease(&self.root) else {
                return GlobalEvidenceV1::Rescan;
            };
            let state_parent_path = state_parent.to_path_buf();
            let Ok(state_parent) = acquire_directory_lease(state_parent) else {
                return GlobalEvidenceV1::Rescan;
            };
            GlobalEvidenceV1::Handle {
                source,
                parent: Arc::new(parent),
                state_parent: Arc::new(state_parent),
                state_parent_path,
            }
        }
        #[cfg(not(windows))]
        {
            GlobalEvidenceV1::Path(namespace.to_path_buf())
        }
    }
}

impl Drop for PluginCallGuardStoreV1 {
    fn drop(&mut self) {
        // A retained marker is deliberate crash evidence.  Only a namespace
        // containing its validated owner and nothing else is normal cleanup.
        let Ok(entries) = fs::read_dir(&self.launch) else {
            return;
        };
        let mut owner_only = true;
        let mut count = 0_usize;
        for entry in entries {
            let Ok(entry) = entry else {
                return;
            };
            count = count.saturating_add(1);
            if entry.file_name() != "owner.lease"
                || entry
                    .file_type()
                    .map_or(true, |kind| !kind.is_file() || kind.is_symlink())
                || is_reparse(&entry.path())
            {
                owner_only = false;
                break;
            }
        }
        if !owner_only || count != 1 {
            return;
        }
        #[cfg(windows)]
        {
            let Ok(mut launch) =
                reopen_dead_namespace(&self.launch, &mut self.launch_directory_lease)
            else {
                return;
            };
            let Some(owner) = self.launch_lease.owner.as_ref() else {
                return;
            };
            if delete_file_handle(owner).is_err() {
                return;
            }
            self.launch_lease.owner.take();
            if launch.delete_and_close().is_ok() {
                let _ = self.marker_root_lease.sync();
            }
        }
        #[cfg(not(windows))]
        {
            let _ = fs::remove_file(self.launch.join("owner.lease"));
            let _ = fs::remove_dir(&self.launch);
        }
    }
}

pub(crate) struct PluginCallGuardV1 {
    store: Arc<PluginCallGuardStoreV1>,
    marker_file: MarkerFileV1,
    marker: MarkerV1,
}
impl PluginCallGuardV1 {
    pub(crate) fn transition_operation(
        &mut self,
        operation: NativeCallOperationV1,
    ) -> Result<(), GuardErrorV1> {
        let _io = self.store.io.lock().map_err(|_| GuardErrorV1::Fault)?;
        self.marker.operation = operation;
        let bytes = serde_json::to_vec(&self.marker).map_err(|_| GuardErrorV1::Fault)?;
        let file = self
            .marker_file
            .file
            .as_mut()
            .ok_or(GuardErrorV1::Fault)?;
        file.seek(io::SeekFrom::Start(0))
            .and_then(|_| file.set_len(0))
            .and_then(|_| file.write_all(&bytes))
            .and_then(|_| file.sync_all())
            .and_then(|_| self.store.launch_directory_lease.sync())
            .map_err(|_| {
                self.store.set_global();
                GuardErrorV1::Fault
            })
    }

    pub(crate) fn clear(mut self) -> Result<(), GuardErrorV1> {
        let _io = self.store.io.lock().map_err(|_| GuardErrorV1::Fault)?;
        if delete_marker_file(&self.marker_file).is_err() {
            self.store.set_global();
            return Err(GuardErrorV1::Fault);
        }
        // Set disposition on the exact marker, close its final handle, then
        // flush the retained launch directory so a returned callback cannot
        // race its durable clear with process teardown.
        drop(self.marker_file.file.take());
        if self.store.launch_directory_lease.sync().is_err() {
            self.store.set_global();
            return Err(GuardErrorV1::Fault);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum GuardErrorV1 {
    Denied,
    Fault,
}

pub(crate) fn marker(
    package_id: &str,
    digest: &str,
    entrypoint: &str,
    root_module: &str,
    namespace: u32,
    value: u64,
) -> MarkerV1 {
    marker_with_operation(
        package_id,
        digest,
        entrypoint,
        root_module,
        namespace,
        value,
        NativeCallOperationV1::Registrar,
    )
}

pub(crate) fn marker_with_operation(
    package_id: &str,
    digest: &str,
    entrypoint: &str,
    root_module: &str,
    namespace: u32,
    value: u64,
    operation: NativeCallOperationV1,
) -> MarkerV1 {
    MarkerV1 {
        schema_version: MARKER_SCHEMA_V1,
        package_id: package_id.into(),
        sealed_manifest_digest: digest.into(),
        entrypoint_id: entrypoint.into(),
        root_module_id: root_module.into(),
        primary_interface_namespace: namespace,
        primary_interface_value: value,
        operation,
    }
}

fn marker_name(id: u64) -> String {
    format!("marker-{id:016x}.json")
}
fn parse_marker_name(name: &str) -> Option<u64> {
    name.strip_prefix("marker-")?
        .strip_suffix(".json")
        .filter(|hex| {
            hex.len() == 16
                && hex
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        })
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .filter(|id| *id != 0)
}

impl MarkerV1 {
    fn is_valid(&self) -> bool {
        self.schema_version == MARKER_SCHEMA_V1
            && valid_identity_component(&self.package_id)
            && valid_identity_component(&self.entrypoint_id)
            && valid_identity_component(&self.root_module_id)
            && self.sealed_manifest_digest.len() == 64
            && self
                .sealed_manifest_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            && self.primary_interface_namespace != 0
            && self.primary_interface_value != 0
    }
}

fn valid_identity_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn create_launch_namespace(
    root: &Path,
) -> Result<(PathBuf, DirectoryLeaseV1, LaunchLeaseV1), NativeLifecycleErrorV1> {
    let rng = SystemRandom::new();
    for _ in 0..MAX_MARKER_ENTRIES_V1 {
        let mut nonce = [0_u8; 16];
        rng.fill(&mut nonce)
            .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
        let name = format!("launch-{}", hex_lower(&nonce));
        let launch = root.join(name);
        match fs::create_dir(&launch) {
            Ok(()) => {
                // The directory handle is acquired before the owner file.  It
                // pins the exact newly-created namespace against replacement.
                let directory_lease = acquire_directory_lease(&launch)?;
                let lease = create_owner_lease(&launch)?;
                return Ok((launch, directory_lease, lease));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(NativeLifecycleErrorV1::MarkerStateUnavailable),
        }
    }
    Err(NativeLifecycleErrorV1::MarkerStateUnavailable)
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn valid_launch_name(name: &str) -> bool {
    name.strip_prefix("launch-").is_some_and(|nonce| {
        nonce.len() == 32
            && nonce
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    })
}

#[cfg(windows)]
fn create_new_marker_file(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const GENERIC_READ_WRITE_DELETE: u32 = 0xc001_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .access_mode(GENERIC_READ_WRITE_DELETE)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(windows))]
fn create_new_marker_file(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn delete_marker_file(marker: &MarkerFileV1) -> io::Result<()> {
    marker
        .file
        .as_ref()
        .ok_or_else(|| io::Error::other("marker handle already closed"))
        .and_then(delete_file_handle)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn delete_file_handle(file: &File) -> io::Result<()> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle as _;

    #[repr(C)]
    struct FileDispositionInfo {
        delete_file: i32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetFileInformationByHandle(
            file: *mut core::ffi::c_void,
            file_information_class: u32,
            file_information: *const core::ffi::c_void,
            buffer_size: u32,
        ) -> i32;
    }
    let disposition = FileDispositionInfo { delete_file: 1 };
    // SAFETY: the guard retains the exact handle opened with DELETE access.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            4,
            (&raw const disposition).cast(),
            u32::try_from(size_of::<FileDispositionInfo>())
                .map_err(|_| io::Error::other("disposition size"))?,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn delete_marker_file(marker: &MarkerFileV1) -> io::Result<()> {
    fs::remove_file(&marker.path)
}

#[cfg(windows)]
fn acquire_directory_lease(path: &Path) -> Result<DirectoryLeaseV1, NativeLifecycleErrorV1> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};

    const FILE_SHARE_READ_WRITE: u32 = 0x0000_0003;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let handle = OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(0xc000_0000)
        .share_mode(FILE_SHARE_READ_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
    let attributes = handle
        .metadata()
        .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?
        .file_attributes();
    if attributes & FILE_ATTRIBUTE_DIRECTORY == 0 || attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(NativeLifecycleErrorV1::MarkerStateUnavailable);
    }
    Ok(DirectoryLeaseV1 {
        handle: Some(handle),
    })
}

#[cfg(windows)]
fn acquire_removable_directory_lease(
    path: &Path,
) -> Result<DirectoryLeaseV1, NativeLifecycleErrorV1> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const GENERIC_READ_WRITE_DELETE: u32 = 0xc001_0000;
    const FILE_SHARE_READ_WRITE: u32 = 0x0000_0003;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let handle = OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(GENERIC_READ_WRITE_DELETE)
        .share_mode(FILE_SHARE_READ_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
    // Reuse the validator after opening; the retained handle's share mode
    // pins this exact child against replacement for its full lifetime.
    let attributes = {
        use std::os::windows::fs::MetadataExt as _;
        handle
            .metadata()
            .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?
            .file_attributes()
    };
    if attributes & 0x0000_0410 != 0x0000_0010 {
        return Err(NativeLifecycleErrorV1::MarkerStateUnavailable);
    }
    Ok(DirectoryLeaseV1 {
        handle: Some(handle),
    })
}

#[cfg(windows)]
fn reopen_dead_namespace(
    namespace: &Path,
    observer: &mut DirectoryLeaseV1,
) -> Result<DirectoryLeaseV1, ()> {
    let identity = observer.identity().map_err(|_| ())?;
    observer.close();
    #[cfg(all(test, windows))]
    run_reopen_dead_namespace_hook(namespace);
    let lease = acquire_removable_directory_lease(namespace).map_err(|_| ())?;
    (lease.identity().map_err(|_| ())? == identity)
        .then_some(lease)
        .ok_or(())
}

#[cfg(all(test, windows))]
fn run_reopen_dead_namespace_hook(namespace: &Path) {
    let Ok(mut hook) = REOPEN_DEAD_NAMESPACE_HOOK.lock() else {
        return;
    };
    let Some((target, _)) = hook.as_ref() else {
        return;
    };
    if target != namespace {
        return;
    }
    let Some((_, hook)) = hook.take() else {
        return;
    };
    hook();
}

#[cfg(all(test, windows))]
fn prepare_reopen_dead_namespace_hook(namespace: &Path, owner: &mut Option<File>) {
    let Ok(hook) = REOPEN_DEAD_NAMESPACE_HOOK.lock() else {
        return;
    };
    if hook.as_ref().is_some_and(|(target, _)| target == namespace) {
        owner.take();
    }
}

#[cfg(not(windows))]
fn acquire_directory_lease(_: &Path) -> Result<DirectoryLeaseV1, NativeLifecycleErrorV1> {
    Ok(DirectoryLeaseV1 {})
}

#[cfg(not(windows))]
fn acquire_removable_directory_lease(_: &Path) -> Result<DirectoryLeaseV1, NativeLifecycleErrorV1> {
    Ok(DirectoryLeaseV1 {})
}

#[cfg(not(windows))]
fn reopen_dead_namespace(_: &Path, _: &mut DirectoryLeaseV1) -> Result<DirectoryLeaseV1, ()> {
    Ok(DirectoryLeaseV1 {})
}

#[cfg(windows)]
fn create_owner_lease(launch: &Path) -> Result<LaunchLeaseV1, NativeLifecycleErrorV1> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};

    const GENERIC_READ_WRITE_DELETE: u32 = 0xc001_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let mut owner = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .access_mode(GENERIC_READ_WRITE_DELETE)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(launch.join("owner.lease"))
        .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
    let attributes = owner
        .metadata()
        .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?
        .file_attributes();
    if attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        return Err(NativeLifecycleErrorV1::MarkerStateUnavailable);
    }
    owner
        .write_all(b"v1")
        .and_then(|()| owner.sync_all())
        .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
    Ok(LaunchLeaseV1 { owner: Some(owner) })
}

#[cfg(not(windows))]
fn create_owner_lease(_: &Path) -> Result<LaunchLeaseV1, NativeLifecycleErrorV1> {
    Ok(LaunchLeaseV1 {})
}

#[cfg(windows)]
#[cfg(windows)]
enum NamespaceOwnerStateV1 {
    Live,
    Dead(File),
}

#[cfg(not(windows))]
enum NamespaceOwnerStateV1 {
    Live,
    Dead(()),
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn namespace_owner_state(namespace: &Path) -> Result<NamespaceOwnerStateV1, ()> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
    use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _};

    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_READ_WRITE_DELETE: u32 = 0xc001_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_ALL: u32 = 0x0000_0007;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReOpenFile(
            original_file: *mut core::ffi::c_void,
            desired_access: u32,
            share_mode: u32,
            flags_and_attributes: u32,
        ) -> *mut core::ffi::c_void;
    }

    // Read and validate the exact file object first.  ReOpenFile then probes
    // that same object rather than reopening the path after a TOCTOU window.
    let mut validated = OpenOptions::new()
        .read(true)
        .access_mode(GENERIC_READ)
        .share_mode(FILE_SHARE_ALL)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(namespace.join("owner.lease"))
        .map_err(|_| ())?;
    let metadata = validated.metadata().map_err(|_| ())?;
    if metadata.file_attributes() & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        return Err(());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut validated)
        .take(3)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes != b"v1" {
        return Err(());
    }
    // SAFETY: `validated` remains open for the call and the returned owned
    // handle is immediately wrapped in `File` exactly once.
    let raw = unsafe {
        ReOpenFile(
            validated.as_raw_handle(),
            GENERIC_READ_WRITE_DELETE,
            FILE_SHARE_READ,
            FILE_FLAG_OPEN_REPARSE_POINT,
        )
    };
    if raw.is_null() || raw == (-1_isize) as *mut core::ffi::c_void {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(32) {
            Ok(NamespaceOwnerStateV1::Live)
        } else {
            Err(())
        };
    }
    // SAFETY: `raw` is a successful owned handle from ReOpenFile.
    let probe = unsafe { File::from_raw_handle(raw) };
    Ok(NamespaceOwnerStateV1::Dead(probe))
}

#[cfg(not(windows))]
fn namespace_owner_state(_: &Path) -> Result<NamespaceOwnerStateV1, ()> {
    // The production lease is Windows-specific. Other platforms remain
    // fail-closed when stale namespaces are present.
    Ok(NamespaceOwnerStateV1::Dead(()))
}

fn scan_dead_namespace(namespace: &Path, deadline: Instant) -> Result<Vec<ScannedMarkerV1>, ()> {
    let mut saw_owner = false;
    let mut recovered = Vec::new();
    for entry in fs::read_dir(namespace).map_err(|_| ())? {
        if Instant::now() > deadline {
            return Err(());
        }
        let entry = entry.map_err(|_| ())?;
        let path = entry.path();
        if entry
            .file_type()
            .map_or(true, |kind| !kind.is_file() || kind.is_symlink())
            || is_reparse(&path)
        {
            return Err(());
        }
        let name = entry.file_name();
        let name = name.to_str().ok_or(())?;
        if name == "owner.lease" {
            saw_owner = true;
            continue;
        }
        let _marker_id = parse_marker_name(name).ok_or(())?;
        if recovered.len() == MAX_MARKER_ENTRIES_V1 {
            return Err(());
        }
        let (recovered_file, bytes) = open_recovered_marker(&path)?;
        if bytes.len() > MAX_MARKER_BYTES_V1 {
            return Err(());
        }
        let marker = serde_json::from_slice::<MarkerV1>(&bytes).map_err(|_| ())?;
        if !marker.is_valid() {
            return Err(());
        }
        recovered.push(ScannedMarkerV1 {
            marker,
            #[cfg(windows)]
            file: recovered_file,
            #[cfg(not(windows))]
            path,
        });
    }
    saw_owner.then_some(recovered).ok_or(())
}

#[cfg(windows)]
fn open_recovered_marker(path: &Path) -> Result<(File, Vec<u8>), ()> {
    use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};

    const GENERIC_READ_DELETE: u32 = 0x8001_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let file = OpenOptions::new()
        .read(true)
        .access_mode(GENERIC_READ_DELETE)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| ())?;
    let metadata = file.metadata().map_err(|_| ())?;
    if metadata.file_attributes() & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        return Err(());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut (&file))
        .take((MAX_MARKER_BYTES_V1 + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    Ok((file, bytes))
}

#[cfg(not(windows))]
fn open_recovered_marker(path: &Path) -> Result<((), Vec<u8>), ()> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|mut file| {
            Read::by_ref(&mut file)
                .take((MAX_MARKER_BYTES_V1 + 1) as u64)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| ())?;
    Ok(((), bytes))
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?
        .sync_all()
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
impl DirectoryLeaseV1 {
    fn handle(&self) -> io::Result<&File> {
        self.handle
            .as_ref()
            .ok_or_else(|| io::Error::other("directory lease already closed"))
    }

    fn sync(&self) -> io::Result<()> {
        self.handle()?.sync_all()
    }

    fn delete_and_close(&mut self) -> io::Result<()> {
        delete_file_handle(self.handle()?)?;
        self.handle.take();
        Ok(())
    }

    fn close(&mut self) {
        self.handle.take();
    }

    #[allow(unsafe_code)]
    fn identity(&self) -> io::Result<(u32, u64)> {
        use std::os::windows::io::AsRawHandle as _;

        #[repr(C)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        #[repr(C)]
        struct ByHandleFileInformation {
            attributes: u32,
            creation: FileTime,
            last_access: FileTime,
            last_write: FileTime,
            volume_serial: u32,
            file_size_high: u32,
            file_size_low: u32,
            number_of_links: u32,
            file_index_high: u32,
            file_index_low: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            #[link_name = "GetFileInformationByHandle"]
            fn get_file_information_by_handle(
                file: isize,
                information: *mut core::ffi::c_void,
            ) -> i32;
        }
        let mut information = std::mem::MaybeUninit::<ByHandleFileInformation>::uninit();
        // SAFETY: the directory lease retains a valid HANDLE and the output
        // points to initialized storage of the exact Win32 structure.
        if unsafe {
            get_file_information_by_handle(
                self.handle()?.as_raw_handle() as isize,
                information.as_mut_ptr().cast(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a successful API call wrote every structure field.
        let information = unsafe { information.assume_init() };
        Ok((
            information.volume_serial,
            (u64::from(information.file_index_high) << 32) | u64::from(information.file_index_low),
        ))
    }
}

#[cfg(not(windows))]
impl DirectoryLeaseV1 {
    fn sync(&self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
fn cleanup_dead_namespace(
    owner: File,
    mut namespace: DirectoryLeaseV1,
    namespace_path: &Path,
) -> io::Result<()> {
    // Both objects are exact no-follow handles.  Delete the owner first, then
    // the now-empty namespace; no path-relative removal is used.
    delete_file_handle(&owner)?;
    owner.sync_all()?;
    drop(owner);
    namespace.delete_and_close()?;
    namespace_path
        .parent()
        .ok_or_else(|| io::Error::other("namespace has no parent"))
        .and_then(sync_directory)
}

#[cfg(not(windows))]
fn cleanup_dead_namespace(_: (), _: DirectoryLeaseV1, namespace: &Path) -> io::Result<()> {
    fs::remove_dir(namespace)
}

#[cfg(windows)]
fn open_evidence_source(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const GENERIC_READ_WRITE_DELETE: u32 = 0xc001_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(GENERIC_READ_WRITE_DELETE)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn rename_handle_to_path(source: &File, destination: &Path) -> io::Result<()> {
    use std::{mem::size_of, os::windows::ffi::OsStrExt as _, os::windows::io::AsRawHandle as _};

    #[repr(C)]
    struct FileRenameInfoLayout {
        replace_if_exists: u8,
        root_directory: *mut core::ffi::c_void,
        file_name_length: u32,
        file_name: [u16; 1],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetFileInformationByHandle(
            file: *mut core::ffi::c_void,
            file_information_class: u32,
            file_information: *const core::ffi::c_void,
            buffer_size: u32,
        ) -> i32;
    }

    let name: Vec<u16> = destination.as_os_str().encode_wide().collect();
    let name_bytes = name
        .len()
        .checked_mul(size_of::<u16>())
        .ok_or_else(|| io::Error::other("quarantine name too long"))?;
    let prefix = std::mem::offset_of!(FileRenameInfoLayout, file_name);
    let length = prefix
        .checked_add(name_bytes)
        .ok_or_else(|| io::Error::other("rename buffer too large"))?;
    let mut buffer = vec![0_u8; length];
    // SAFETY: `buffer` is sized for the C prefix and UTF-16 tail; unaligned
    // writes are intentional because the Win32 API accepts a byte buffer.
    unsafe {
        std::ptr::write_unaligned(
            buffer
                .as_mut_ptr()
                .add(std::mem::offset_of!(FileRenameInfoLayout, root_directory))
                .cast::<*mut core::ffi::c_void>(),
            std::ptr::null_mut(),
        );
        std::ptr::write_unaligned(
            buffer
                .as_mut_ptr()
                .add(std::mem::offset_of!(FileRenameInfoLayout, file_name_length))
                .cast::<u32>(),
            u32::try_from(name_bytes).map_err(|_| io::Error::other("quarantine name too long"))?,
        );
        std::ptr::copy_nonoverlapping(
            name.as_ptr().cast::<u8>(),
            buffer.as_mut_ptr().add(prefix),
            name_bytes,
        );
    }
    // SAFETY: source and destination are retained handles; the buffer remains
    // live throughout this synchronous Win32 call.
    if unsafe {
        SetFileInformationByHandle(
            source.as_raw_handle(),
            3,
            buffer.as_ptr().cast(),
            u32::try_from(buffer.len()).map_err(|_| io::Error::other("rename buffer too large"))?,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn create_quarantine_directory(
    state_parent_path: &Path,
) -> Result<(PathBuf, DirectoryLeaseV1), NativeLifecycleErrorV1> {
    let rng = SystemRandom::new();
    for _ in 0..MAX_MARKER_ENTRIES_V1 {
        let mut nonce = [0_u8; 16];
        rng.fill(&mut nonce)
            .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
        let path = state_parent_path.join(format!("native-call-quarantine-{}", hex_lower(&nonce)));
        match fs::create_dir(&path) {
            Ok(()) => {
                let lease = acquire_directory_lease(&path)?;
                return Ok((path, lease));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(NativeLifecycleErrorV1::MarkerStateUnavailable),
        }
    }
    Err(NativeLifecycleErrorV1::MarkerStateUnavailable)
}

#[cfg(windows)]
fn quarantine_global_evidence(
    evidence: &mut GlobalEvidenceV1,
) -> Result<(), NativeLifecycleErrorV1> {
    let GlobalEvidenceV1::Handle {
        source,
        parent,
        state_parent,
        state_parent_path,
    } = evidence
    else {
        return Ok(());
    };
    let (quarantine_path, quarantine) = create_quarantine_directory(state_parent_path)?;
    let rng = SystemRandom::new();
    let mut nonce = [0_u8; 16];
    rng.fill(&mut nonce)
        .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
    let target = quarantine_path.join(format!("evidence-{}", hex_lower(&nonce)));
    rename_handle_to_path(source, &target)
        .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
    quarantine
        .sync()
        .and_then(|()| parent.sync())
        .and_then(|()| state_parent.sync())
        .map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)
}

#[cfg(not(windows))]
fn quarantine_global_evidence(
    evidence: &mut GlobalEvidenceV1,
) -> Result<(), NativeLifecycleErrorV1> {
    match evidence {
        GlobalEvidenceV1::Path(path) => {
            let quarantine = path.with_extension("quarantined");
            fs::rename(path, quarantine).map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)
        }
        GlobalEvidenceV1::Rescan => Ok(()),
    }
}

pub(crate) fn validate_application_state_dir(
    application_state_dir: &Path,
) -> Result<(), NativeLifecycleErrorV1> {
    validate_state_directory(application_state_dir)
}

fn validate_state_directory(root: &Path) -> Result<(), NativeLifecycleErrorV1> {
    if !root.is_absolute() {
        return Err(NativeLifecycleErrorV1::MarkerStateUnavailable);
    }
    fs::create_dir_all(root).map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
    let metadata =
        fs::symlink_metadata(root).map_err(|_| NativeLifecycleErrorV1::MarkerStateUnavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse(root) {
        return Err(NativeLifecycleErrorV1::MarkerStateUnavailable);
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    fs::symlink_metadata(path).map_or(true, |metadata| metadata.file_attributes() & 0x400 != 0)
}
#[cfg(not(windows))]
fn is_reparse(_: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker_one() -> MarkerV1 {
        marker(
            "pkg",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "entry",
            "root",
            0x5345_0001,
            9,
        )
    }
    fn store(directory: &Path) -> Arc<PluginCallGuardStoreV1> {
        PluginCallGuardStoreV1::open(directory.to_path_buf(), Duration::ZERO).expect("store")
    }

    #[test]
    fn marker_is_present_before_callback_and_cleared_after_return() {
        let directory = tempfile::tempdir().expect("directory");
        let store = store(directory.path());
        let permit = store.begin(&marker_one()).expect("begin");
        assert_eq!(fs::read_dir(&store.launch).expect("entries").count(), 2);
        permit.clear().expect("clear");
        assert_eq!(fs::read_dir(&store.launch).expect("entries").count(), 1);
    }

    #[test]
    fn load_attempt_is_durable_before_mapping_and_registered_clear_is_atomic() {
        let temporary = tempfile::tempdir().expect("temporary state");
        let root = temporary.path().join("markers");
        fs::create_dir(&root).expect("marker root");
        let store = store(&root);
        let marker = marker_with_operation(
            "example.package",
            &"a".repeat(64),
            "native",
            "root-contract-v1",
            0x5345_0001,
            1,
            NativeCallOperationV1::LoadLibrary,
        );
        let mut guard = store.begin(&marker).expect("durable load attempt");
        let marker_files = fs::read_dir(&store.launch)
            .expect("launch directory")
            .filter_map(Result::ok)
            .filter(|entry| parse_marker_name(&entry.file_name().to_string_lossy()).is_some())
            .count();
        assert_eq!(marker_files, 1, "attempt must exist before mapping");
        guard
            .transition_operation(NativeCallOperationV1::LoadRejectedResident)
            .expect("typed rejection transition");
        let terminal = fs::read_dir(&store.launch)
            .expect("launch directory")
            .filter_map(Result::ok)
            .find(|entry| parse_marker_name(&entry.file_name().to_string_lossy()).is_some())
            .map(|entry| fs::read_to_string(entry.path()).expect("marker bytes"))
            .expect("terminal marker");
        assert!(terminal.contains("load_rejected_resident"));
        guard.clear().expect("registered clear");
        assert!(store.incidents().is_empty());
        assert_eq!(
            fs::read_dir(&store.launch)
                .expect("launch directory")
                .filter_map(Result::ok)
                .filter(|entry| parse_marker_name(&entry.file_name().to_string_lossy()).is_some())
                .count(),
            0
        );
    }

    #[test]
    fn abnormal_load_attempt_is_resuppressed_on_the_next_start() {
        const CHILD: &str = "SUPEREXPLORER_ABORT_DURING_LOAD_ATTEMPT";
        const ROOT: &str = "SUPEREXPLORER_LOAD_ATTEMPT_ROOT";
        if std::env::var_os(CHILD).is_some() {
            let root = PathBuf::from(std::env::var_os(ROOT).expect("child root"));
            let store = store(&root);
            let marker = marker_with_operation(
                "example.package",
                &"b".repeat(64),
                "native",
                "root-contract-v1",
                0x5345_0001,
                1,
                NativeCallOperationV1::LoadLibrary,
            );
            let _attempt = store.begin(&marker).expect("durable child attempt");
            std::process::abort();
        }
        let temporary = tempfile::tempdir().expect("temporary state");
        let root = temporary.path().join("markers");
        fs::create_dir(&root).expect("marker root");
        let status = std::process::Command::new(std::env::current_exe().expect("test exe"))
            .arg("plugin_call_guard::tests::abnormal_load_attempt_is_resuppressed_on_the_next_start")
            .arg("--exact")
            .env(CHILD, "1")
            .env(ROOT, &root)
            .status()
            .expect("child process");
        assert!(!status.success(), "child must terminate abnormally");
        let restarted = store(&root);
        let marker = marker_with_operation(
            "example.package",
            &"b".repeat(64),
            "native",
            "root-contract-v1",
            0x5345_0001,
            1,
            NativeCallOperationV1::LoadLibrary,
        );
        assert!(restarted.denies(&marker));
        assert!(restarted.incidents().iter().any(|incident| matches!(
            incident,
            NativeSafeModeIncidentV1::RegistrarInProgress {
                operation: NativeCallOperationV1::LoadLibrary,
                ..
            }
        )));
    }

    #[test]
    fn typed_error_and_translated_panic_terminals_clear_markers() {
        let directory = tempfile::tempdir().expect("directory");
        let store = store(directory.path());
        store
            .begin(&marker_one())
            .expect("typed error guard")
            .clear()
            .expect("clear");
        store
            .begin(&marker_one())
            .expect("panic guard")
            .clear()
            .expect("clear");
        assert_eq!(fs::read_dir(&store.launch).expect("entries").count(), 1);
    }

    #[test]
    fn create_or_clear_failure_fails_closed() {
        let directory = tempfile::tempdir().expect("directory");
        let primary = store(directory.path());
        fs::write(primary.launch.join(marker_name(1)), b"residue").expect("marker residue");
        assert!(matches!(
            primary.begin(&marker_one()),
            Err(GuardErrorV1::Fault)
        ));
        assert!(matches!(
            primary.begin(&marker_one()),
            Err(GuardErrorV1::Denied)
        ));

        let second = tempfile::tempdir().expect("second");
        let store = store(second.path());
        let permit = store.begin(&marker_one()).expect("begin");
        assert!(fs::remove_file(store.launch.join(marker_name(1))).is_err());
        permit.clear().expect("handle clear");
    }

    #[test]
    fn recovered_incident_denies_matching_call_until_confirmed() {
        let directory = tempfile::tempdir().expect("directory");
        let original = store(directory.path());
        let residue = original.begin(&marker_one()).expect("residue");
        drop(residue);
        drop(original);
        let recovered = store(directory.path());
        let incidents = recovered.incidents();
        assert_eq!(incidents.len(), 1);
        assert!(matches!(
            recovered.begin(&marker_one()),
            Err(GuardErrorV1::Denied)
        ));
        recovered
            .confirm(incidents[0].incident_id())
            .expect("confirm");
        let permit = recovered.begin(&marker_one()).expect("confirmed marker");
        assert!(recovered.launch.join(marker_name(1)).is_file());
        permit.clear().expect("clear");
    }

    #[cfg(windows)]
    #[test]
    fn live_launch_namespace_is_ignored_by_a_concurrent_scanner() {
        let directory = tempfile::tempdir().expect("directory");
        let first = store(directory.path());
        let first_guard = first.begin(&marker_one()).expect("first guard");
        let second = store(directory.path());
        assert!(second.incidents().is_empty());
        second
            .begin(&marker_one())
            .expect("independent launch guard")
            .clear()
            .expect("clear");
        first_guard.clear().expect("first clear");
    }

    #[cfg(windows)]
    #[test]
    fn scanner_rejects_a_dead_namespace_path_swap_as_global_state() {
        let root = tempfile::tempdir().expect("root");
        let namespace = root.path().join("launch-0123456789abcdef0123456789abcdef");
        fs::create_dir(&namespace).expect("namespace");
        fs::write(namespace.join("owner.lease"), b"v1").expect("owner");
        let blocked = acquire_directory_lease(&namespace).expect("pinned observer");
        assert!(fs::rename(&namespace, root.path().join("blocked-swap")).is_err());
        drop(blocked);

        let moved = root.path().join("moved-dead-namespace");
        let replacement = namespace.clone();
        *REOPEN_DEAD_NAMESPACE_HOOK.lock().expect("hook") = Some((
            namespace,
            Box::new(move || {
                fs::rename(&replacement, moved).expect("swap old namespace");
                fs::create_dir(&replacement).expect("replacement namespace");
                fs::write(replacement.join("owner.lease"), b"v1").expect("replacement owner");
            }),
        ));
        let recovered = store(root.path());
        assert!(recovered.is_global());
    }

    #[cfg(windows)]
    #[test]
    fn owner_reparse_and_invalid_owner_fail_closed() {
        use std::process::Command;

        fn junction(link: &Path, target: &Path) {
            let status = Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .output()
                .expect("junction command");
            assert!(status.status.success(), "junction creation failed");
        }

        let root = tempfile::tempdir().expect("root");
        let namespace = root.path().join("launch-0123456789abcdef0123456789abcdef");
        fs::create_dir(&namespace).expect("namespace");
        fs::write(namespace.join("owner.lease"), b"v2").expect("invalid owner");
        assert!(matches!(
            store(root.path()).incidents()[0].kind(),
            NativeSafeModeIncidentKindV1::UnsafeMarkerState
        ));

        let root = tempfile::tempdir().expect("root");
        let namespace = root.path().join("launch-0123456789abcdef0123456789abcdef");
        fs::create_dir(&namespace).expect("namespace");
        let target = tempfile::tempdir().expect("target");
        junction(&namespace.join("owner.lease"), target.path());
        assert!(matches!(
            store(root.path()).incidents()[0].kind(),
            NativeSafeModeIncidentKindV1::UnsafeMarkerState
        ));
    }

    #[test]
    fn normal_open_drop_cleans_empty_launch_namespaces() {
        let root = tempfile::tempdir().expect("root");
        for _ in 0..4 {
            let store = store(root.path());
            store
                .begin(&marker_one())
                .expect("begin")
                .clear()
                .expect("clear");
            drop(store);
            assert_eq!(fs::read_dir(root.path()).expect("root entries").count(), 0);
        }
    }

    #[cfg(windows)]
    #[test]
    fn startup_removes_a_dead_empty_namespace() {
        let root = tempfile::tempdir().expect("root");
        let dead = root.path().join("launch-0123456789abcdef0123456789abcdef");
        fs::create_dir(&dead).expect("dead namespace");
        fs::write(dead.join("owner.lease"), b"v1").expect("dead owner");
        let recovered = store(root.path());
        assert!(!dead.exists());
        assert!(recovered.incidents().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn overflow_quarantines_the_current_namespace_then_recovers_scoped_incidents() {
        let root = tempfile::tempdir().expect("root");
        let bytes = serde_json::to_vec(&marker_one()).expect("marker");
        for id in 0..=MAX_LAUNCH_NAMESPACES_V1 {
            let namespace = root.path().join(format!("launch-{id:032x}"));
            fs::create_dir(&namespace).expect("namespace");
            fs::write(namespace.join("owner.lease"), b"v1").expect("owner");
            fs::write(namespace.join(marker_name(1)), &bytes).expect("marker");
        }
        let recovered = store(root.path());
        assert!(recovered.is_global());
        let global = recovered.incidents();
        recovered
            .confirm(global[0].incident_id())
            .expect("quarantine overflow namespace");
        assert!(!recovered.is_global());
        assert_eq!(recovered.incidents().len(), MAX_LAUNCH_NAMESPACES_V1);
        for incident in recovered.incidents() {
            recovered
                .confirm(incident.incident_id())
                .expect("confirm scoped incident");
        }
        drop(recovered);
        let next = store(root.path());
        assert!(!next.is_global());
        assert!(next.incidents().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn deadline_quarantines_a_concrete_namespace_and_never_false_succeeds() {
        let root = tempfile::tempdir().expect("root");
        let store = store(root.path());
        let namespace = root.path().join("launch-0123456789abcdef0123456789abcdef");
        fs::create_dir(&namespace).expect("namespace");
        fs::write(namespace.join("owner.lease"), b"v1").expect("owner");
        fs::write(
            namespace.join(marker_name(1)),
            serde_json::to_vec(&marker_one()).expect("marker"),
        )
        .expect("marker");
        store
            .scan_until(Instant::now())
            .expect("deadline becomes global overlay");
        assert!(store.is_global());
        let global = store.incidents();
        assert_eq!(global.len(), 1);
        store
            .confirm(global[0].incident_id())
            .expect("quarantine deadline namespace");
        assert!(!store.is_global());
    }

    #[test]
    fn corrupt_oversized_and_overflow_residue_deny_globally() {
        let directory = tempfile::tempdir().expect("directory");
        fs::write(directory.path().join(marker_name(1)), b"not-json").expect("residue");
        assert!(matches!(
            store(directory.path()).begin(&marker_one()),
            Err(GuardErrorV1::Denied)
        ));
        let directory = tempfile::tempdir().expect("directory");
        fs::write(
            directory.path().join(marker_name(1)),
            vec![b'x'; MAX_MARKER_BYTES_V1 + 1],
        )
        .expect("residue");
        assert!(matches!(
            store(directory.path()).begin(&marker_one()),
            Err(GuardErrorV1::Denied)
        ));
        let directory = tempfile::tempdir().expect("directory");
        let bytes = serde_json::to_vec(&marker_one()).expect("marker");
        for id in 1..=(MAX_MARKER_ENTRIES_V1 + 1) {
            fs::write(directory.path().join(marker_name(id as u64)), &bytes).expect("residue");
        }
        assert!(matches!(
            store(directory.path()).begin(&marker_one()),
            Err(GuardErrorV1::Denied)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn reparse_equivalent_residue_denies_globally() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("directory");
        let target = tempfile::NamedTempFile::new().expect("target");
        symlink(target.path(), directory.path().join(marker_name(1))).expect("symlink");
        assert!(matches!(
            store(directory.path()).begin(&marker_one()),
            Err(GuardErrorV1::Denied)
        ));
    }

    #[test]
    fn timing_is_bounded_and_zero_threshold_is_deterministically_slow() {
        let directory = tempfile::tempdir().expect("directory");
        let store = store(directory.path());
        for _ in 0..=MAX_NATIVE_CALL_TIMINGS_V1 {
            store.record_timing(
                &marker_one(),
                Duration::ZERO,
                NativeCallTerminalV1::Accepted,
            );
        }
        let timings = store.timings();
        assert_eq!(timings.len(), MAX_NATIVE_CALL_TIMINGS_V1);
        assert!(timings.iter().all(|timing| timing.slow));
    }

    #[test]
    fn provider_timing_is_bounded_path_free_and_interface_scoped() {
        let directory = tempfile::tempdir().expect("directory");
        let store = store(directory.path());
        let marker = marker_with_operation(
            "provider-package",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "provider-contribution",
            "provider",
            0x5345_0001,
            77,
            NativeCallOperationV1::JobProvider,
        );
        for _ in 0..=MAX_NATIVE_CALL_TIMINGS_V1 {
            store.record_timing(&marker, Duration::ZERO, NativeCallTerminalV1::Accepted);
        }
        let timings = store.timings();
        assert_eq!(timings.len(), MAX_NATIVE_CALL_TIMINGS_V1);
        assert!(timings.iter().all(|timing| {
            timing.package_id == "provider-package"
                && timing.callback_id == "provider-contribution"
                && timing.primary_interface_namespace == 0x5345_0001
                && timing.primary_interface_value == 77
                && timing.operation == NativeCallOperationV1::JobProvider
                && timing.terminal == NativeCallTerminalV1::Accepted
                && timing.slow
        }));
    }

    #[test]
    fn global_confirmation_quarantines_evidence_and_next_startup_is_clean() {
        let state = tempfile::tempdir().expect("state");
        let root = state.path().join("markers");
        fs::create_dir(&root).expect("root");
        fs::write(root.join(marker_name(1)), b"corrupt").expect("residue");
        let recovered = store(&root);
        let incidents = recovered.incidents();
        assert_eq!(incidents.len(), 1);
        assert_eq!(
            incidents[0].kind(),
            NativeSafeModeIncidentKindV1::UnsafeMarkerState
        );
        recovered
            .confirm(incidents[0].incident_id())
            .expect("quarantine corrupt evidence");
        assert!(!recovered.is_global());
        drop(recovered);
        let next = store(&root);
        assert!(next.incidents().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn reparse_roots_and_children_are_rejected() {
        use std::process::Command;

        fn junction(link: &Path, target: &Path) {
            let status = Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .output()
                .expect("junction command");
            assert!(status.status.success(), "junction creation failed");
        }

        let state = tempfile::tempdir().expect("state");
        let target = tempfile::tempdir().expect("target");
        let reparse_root = state.path().join("reparse-root");
        junction(&reparse_root, target.path());
        assert!(matches!(
            PluginCallGuardStoreV1::open(reparse_root, Duration::ZERO),
            Err(NativeLifecycleErrorV1::MarkerStateUnavailable)
        ));

        let marker_root = state.path().join("markers");
        fs::create_dir(&marker_root).expect("marker root");
        let child_target = tempfile::tempdir().expect("child target");
        junction(&marker_root.join(marker_name(1)), child_target.path());
        assert!(matches!(
            store(&marker_root).begin(&marker_one()),
            Err(GuardErrorV1::Denied)
        ));
    }
}

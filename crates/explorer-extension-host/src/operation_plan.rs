//! Host-owned validation and execution for extension-authored data-only plans.

use explorer_extension_api::{
    FileIdentityV1, MAX_OPERATION_STEPS_V1, OperationKindV1, OperationOutcomeV1, OperationPlanV1,
    OperationPreviewV1, OperationTerminalV1,
};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Default)]
pub struct OperationCancellationV1(Arc<AtomicBool>);
impl OperationCancellationV1 {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OperationPlanErrorV1 {
    #[error("operation plan exceeds the step limit")]
    TooManySteps,
    #[error("operation path is unsafe")]
    UnsafePath,
    #[error("operation target name is invalid on Windows")]
    InvalidWindowsName,
    #[error("operation targets collide case-insensitively")]
    DuplicateTarget,
    #[error("operation target already exists")]
    TargetExists,
    #[error("operation kind is unsupported")]
    UnsupportedKind,
    #[error("filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug)]
enum UndoStep {
    RemoveEmpty(PathBuf),
    Rename { from: PathBuf, to: PathBuf },
}

pub struct HostOperationPlanEngineV1 {
    root: PathBuf,
    undo: Vec<Vec<UndoStep>>,
}
impl HostOperationPlanEngineV1 {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            undo: Vec::new(),
        }
    }
    fn resolve(&self, value: &str) -> Result<PathBuf, OperationPlanErrorV1> {
        let path = Path::new(value);
        if value.is_empty()
            || path.is_absolute()
            || path.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::Prefix(_) | Component::RootDir
                )
            })
        {
            return Err(OperationPlanErrorV1::UnsafePath);
        }
        for component in path.components() {
            let Component::Normal(name) = component else {
                continue;
            };
            let name = name.to_string_lossy();
            if name.ends_with(['.', ' '])
                || name.chars().any(|c| c < ' ' || "<>:\"/\\|?*".contains(c))
            {
                return Err(OperationPlanErrorV1::InvalidWindowsName);
            }
            let stem = name
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            if matches!(
                stem.as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            ) {
                return Err(OperationPlanErrorV1::InvalidWindowsName);
            }
        }
        Ok(self.root.join(path))
    }
    pub fn preview(
        &self,
        plan: &OperationPlanV1,
    ) -> Result<OperationPreviewV1, OperationPlanErrorV1> {
        if plan.steps.len() > MAX_OPERATION_STEPS_V1 {
            return Err(OperationPlanErrorV1::TooManySteps);
        }
        let mut targets = std::collections::BTreeSet::new();
        for step in &plan.steps {
            if step.kind != OperationKindV1::CREATE_DIRECTORY
                && step.kind != OperationKindV1::RENAME
            {
                return Err(OperationPlanErrorV1::UnsupportedKind);
            }
            let destination = self.resolve(step.destination.as_str())?;
            if !targets.insert(step.destination.to_lowercase()) {
                return Err(OperationPlanErrorV1::DuplicateTarget);
            }
            if destination.exists() {
                return Err(OperationPlanErrorV1::TargetExists);
            }
            if let Some(s) = step.source.as_ref().into_option() {
                self.resolve(s.as_str())?;
            }
        }
        Ok(OperationPreviewV1 {
            terminal_if_committed: OperationTerminalV1::COMPLETED,
            step_count: plan.steps.len() as u32,
            requires_confirmation: plan.confirmation_threshold > 0
                && plan.steps.len() as u32 > plan.confirmation_threshold,
            summary: format!("{} operation(s): {}", plan.steps.len(), plan.title).into(),
        })
    }
    pub fn execute(
        &mut self,
        plan: &OperationPlanV1,
        confirmed: bool,
        cancel: &OperationCancellationV1,
    ) -> Result<OperationOutcomeV1, OperationPlanErrorV1> {
        let preview = self.preview(plan)?;
        if preview.requires_confirmation && !confirmed {
            return Ok(outcome(
                OperationTerminalV1::REJECTED,
                0,
                None,
                None,
                "confirmation required",
            ));
        }
        let mut undo = Vec::new();
        for (index, step) in plan.steps.iter().enumerate() {
            if cancel.is_cancelled() {
                let token = self.store_undo(plan.undo_requested, undo);
                return Ok(outcome(
                    if index == 0 {
                        OperationTerminalV1::CANCELLED
                    } else {
                        OperationTerminalV1::PARTIAL
                    },
                    index,
                    None,
                    token,
                    "cancelled",
                ));
            }
            let destination = self.resolve(step.destination.as_str())?;
            let result = if step.kind == OperationKindV1::CREATE_DIRECTORY {
                fs::create_dir(&destination).map(|()| undo.push(UndoStep::RemoveEmpty(destination)))
            } else if step.kind == OperationKindV1::RENAME {
                let source_value = step
                    .source
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::UnsafePath)?;
                let source = self.resolve(source_value.as_str())?;
                if let Some(expected) = step.expected_source.as_ref().into_option() {
                    if identity(&source)? != *expected {
                        return Ok(outcome(
                            OperationTerminalV1::CONFLICT,
                            index,
                            Some(index),
                            self.store_undo(plan.undo_requested, undo),
                            "source identity changed",
                        ));
                    }
                }
                fs::rename(&source, &destination).map(|()| {
                    undo.push(UndoStep::Rename {
                        from: destination,
                        to: source,
                    })
                })
            } else {
                return Err(OperationPlanErrorV1::UnsupportedKind);
            };
            if let Err(error) = result {
                return Ok(outcome(
                    OperationTerminalV1::PARTIAL,
                    index,
                    Some(index),
                    self.store_undo(plan.undo_requested, undo),
                    &error.to_string(),
                ));
            }
        }
        let token = self.store_undo(plan.undo_requested, undo);
        Ok(outcome(
            OperationTerminalV1::COMPLETED,
            plan.steps.len(),
            None,
            token,
            "completed",
        ))
    }
    fn store_undo(&mut self, requested: bool, steps: Vec<UndoStep>) -> Option<String> {
        if !requested || steps.is_empty() {
            return None;
        }
        self.undo.push(steps);
        Some(format!("undo:{}", self.undo.len() - 1))
    }
    pub fn undo(&mut self, token: &str) -> OperationOutcomeV1 {
        let Some(index) = token
            .strip_prefix("undo:")
            .and_then(|v| v.parse::<usize>().ok())
        else {
            return outcome(
                OperationTerminalV1::REJECTED,
                0,
                None,
                None,
                "invalid undo token",
            );
        };
        let Some(steps) = self.undo.get_mut(index) else {
            return outcome(
                OperationTerminalV1::REJECTED,
                0,
                None,
                None,
                "expired undo token",
            );
        };
        let mut completed = 0;
        while let Some(step) = steps.pop() {
            let result = match step {
                UndoStep::RemoveEmpty(path) => fs::remove_dir(path),
                UndoStep::Rename { from, to } => fs::rename(from, to),
            };
            if result.is_err() {
                return outcome(
                    OperationTerminalV1::PARTIAL,
                    completed,
                    Some(completed),
                    None,
                    "undo stopped conservatively",
                );
            }
            completed += 1;
        }
        outcome(
            OperationTerminalV1::COMPLETED,
            completed,
            None,
            None,
            "undone",
        )
    }
}

fn outcome(
    t: OperationTerminalV1,
    completed: usize,
    failed: Option<usize>,
    token: Option<String>,
    detail: &str,
) -> OperationOutcomeV1 {
    OperationOutcomeV1 {
        terminal: t,
        completed_steps: completed as u32,
        failed_step: failed.map(|v| v as u32).into(),
        undo_token: token.map(Into::into).into(),
        detail: detail.into(),
    }
}
pub fn identity(path: &Path) -> Result<FileIdentityV1, std::io::Error> {
    let metadata = fs::metadata(path)?;
    let modified_ticks = metadata
        .modified()
        .ok()
        .and_then(|v| v.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |v| v.as_nanos() as i64);
    #[cfg(windows)]
    let (volume_serial, file_id_low) = file_identity_windows(path)?;
    #[cfg(not(windows))]
    let (volume_serial, file_id_low) = (0, 0);
    Ok(FileIdentityV1 {
        volume_serial,
        file_id_low,
        file_id_high: 0,
        length: metadata.len(),
        modified_ticks,
    })
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn file_identity_windows(path: &Path) -> Result<(u64, u64), std::io::Error> {
    use std::{fs::File, os::windows::io::AsRawHandle};
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    struct Info {
        attributes: u32,
        created: FileTime,
        accessed: FileTime,
        written: FileTime,
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetFileInformationByHandle"]
        fn get_file_information(handle: isize, info: *mut std::ffi::c_void) -> i32;
    }
    let file = File::open(path)?;
    let mut info = std::mem::MaybeUninit::<Info>::uninit();
    // SAFETY: the handle is live for this call and `info` is valid writable storage.
    if unsafe { get_file_information(file.as_raw_handle() as isize, info.as_mut_ptr().cast()) } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the successful Win32 call initialized the complete structure.
    let info = unsafe { info.assume_init() };
    Ok((
        u64::from(info.volume),
        u64::from(info.index_high) << 32 | u64::from(info.index_low),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use abi_stable::std_types::ROption;
    use explorer_extension_api::OperationStepV1;
    use tempfile::TempDir;
    #[test]
    fn partial_cancel_and_conservative_undo_are_truthful() {
        let root = TempDir::new().unwrap();
        let mut engine = HostOperationPlanEngineV1::new(root.path().to_owned());
        let plan = OperationPlanV1 {
            title: "folders".into(),
            confirmation_threshold: 0,
            undo_requested: true,
            steps: vec!["a", "b"]
                .into_iter()
                .map(|p| OperationStepV1 {
                    kind: OperationKindV1::CREATE_DIRECTORY,
                    source: ROption::RNone,
                    destination: p.into(),
                    expected_source: ROption::RNone,
                })
                .collect::<Vec<_>>()
                .into(),
        };
        let result = engine
            .execute(&plan, true, &OperationCancellationV1::default())
            .unwrap();
        assert_eq!(result.terminal, OperationTerminalV1::COMPLETED);
        fs::write(root.path().join("b/x"), b"user").unwrap();
        let undone = engine.undo(result.undo_token.as_ref().unwrap());
        assert_eq!(undone.terminal, OperationTerminalV1::PARTIAL);
        assert!(root.path().join("b/x").exists());
    }
    #[test]
    fn traversal_is_rejected() {
        let root = TempDir::new().unwrap();
        let engine = HostOperationPlanEngineV1::new(root.path().to_owned());
        let plan = OperationPlanV1 {
            title: "bad".into(),
            confirmation_threshold: 0,
            undo_requested: false,
            steps: vec![OperationStepV1 {
                kind: OperationKindV1::CREATE_DIRECTORY,
                source: ROption::RNone,
                destination: "../escape".into(),
                expected_source: ROption::RNone,
            }]
            .into(),
        };
        assert!(matches!(
            engine.preview(&plan),
            Err(OperationPlanErrorV1::UnsafePath)
        ));
    }
}

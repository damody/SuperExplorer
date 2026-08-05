//! Host-owned validation and execution for extension-authored data-only plans.

use explorer_extension_api::{
    FileIdentityV1, MAX_OPERATION_STEPS_V1, OperationConflictV1, OperationKindV1,
    OperationObjectHandleV1, OperationOutcomeV1, OperationPermissionV1, OperationPlanV1,
    OperationPreviewStepV1, OperationPreviewV1, OperationProgressV1, OperationTerminalV1,
};
use ring::rand::{SecureRandom as _, SystemRandom};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::runtime_authority::{AuthorityAdapterV1, AuthorityEnvelopeV1, RuntimeAuthorityV1};

const SECOND_CONFIRMATION_STEP_THRESHOLD_V1: usize = 1_000;
static NEXT_OPERATION_JOURNAL_ID_V1: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

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
    #[error("operation plan runtime authority is missing, stale, or revoked")]
    Unauthorized,
    #[error("operation plan exceeds the step limit")]
    TooManySteps,
    #[error("operation plan root or object handle is unknown, forged, or stale")]
    InvalidHandle,
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
    #[error("secure operation handle generation failed")]
    RandomUnavailable,
}

#[derive(Clone, Debug)]
enum UndoStep {
    RemoveEmpty {
        path: PathBuf,
        identity: FileIdentityV1,
    },
    RemoveCreatedFile {
        path: PathBuf,
        identity: FileIdentityV1,
    },
    Rename {
        from: PathBuf,
        to: PathBuf,
    },
}

#[derive(Clone, Debug)]
struct AuthorizedObjectV1 {
    path: PathBuf,
    identity: Option<FileIdentityV1>,
    is_directory: bool,
}

pub struct HostOperationPlanEngineV1 {
    root: PathBuf,
    root_handle: OperationObjectHandleV1,
    objects: BTreeMap<OperationObjectHandleV1, AuthorizedObjectV1>,
    undo: Vec<Vec<UndoStep>>,
    authority: OperationPlanAuthorityV1,
}

/// Host-private execution request. Public extension ABI never contains model
/// or Shell objects; opaque handles are resolved into these only after preview.
#[derive(Clone, Debug)]
pub(crate) enum HostMappedOperationRequestV1 {
    File(Vec<explorer_model::FileOperationRequest>),
    Extract {
        archive: explorer_model::ItemDescriptor,
        destination: explorer_model::LocationDescriptor,
    },
    ArchiveMutation {
        archive: explorer_model::ItemDescriptor,
    },
}

/// Opaque use-time grant for host-owned operation-plan validation and commit.
#[derive(Clone)]
pub struct OperationPlanAuthorityV1 {
    runtime: Arc<RuntimeAuthorityV1>,
    envelope: AuthorityEnvelopeV1,
}

impl std::fmt::Debug for OperationPlanAuthorityV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OperationPlanAuthorityV1")
            .finish_non_exhaustive()
    }
}

impl OperationPlanAuthorityV1 {
    pub(crate) fn from_host(
        runtime: Arc<RuntimeAuthorityV1>,
        envelope: AuthorityEnvelopeV1,
    ) -> Self {
        Self { runtime, envelope }
    }

    fn revalidate(&self) -> Result<(), OperationPlanErrorV1> {
        self.runtime
            .revalidate(&self.envelope, AuthorityAdapterV1::OperationPlan)
            .map(|_| ())
            .map_err(|_| OperationPlanErrorV1::Unauthorized)
    }
}

impl HostOperationPlanEngineV1 {
    pub fn new(
        root: PathBuf,
        authority: OperationPlanAuthorityV1,
    ) -> Result<Self, OperationPlanErrorV1> {
        authority.revalidate()?;
        let root = fs::canonicalize(root)?;
        if !root.is_dir() {
            return Err(OperationPlanErrorV1::InvalidHandle);
        }
        let root_handle = mint_object_handle(authority.envelope.location_generation())?;
        let mut objects = BTreeMap::new();
        objects.insert(
            root_handle,
            AuthorizedObjectV1 {
                path: root.clone(),
                identity: None,
                is_directory: true,
            },
        );
        Ok(Self {
            root,
            root_handle,
            objects,
            undo: Vec::new(),
            authority,
        })
    }

    #[must_use]
    pub const fn root_handle(&self) -> OperationObjectHandleV1 {
        self.root_handle
    }

    /// Mints an opaque source/destination authorization for an existing object
    /// below this engine's sealed root. The relative path never crosses ABI.
    pub fn authorize_existing(
        &mut self,
        relative: &Path,
    ) -> Result<OperationObjectHandleV1, OperationPlanErrorV1> {
        self.authority.revalidate()?;
        let lexical = self.resolve_relative(relative)?;
        let canonical = fs::canonicalize(lexical)?;
        if !canonical.starts_with(&self.root) {
            return Err(OperationPlanErrorV1::UnsafePath);
        }
        let metadata = fs::metadata(&canonical)?;
        let handle = mint_object_handle(self.root_handle.generation)?;
        self.objects.insert(
            handle,
            AuthorizedObjectV1 {
                path: canonical.clone(),
                identity: if metadata.is_file() {
                    Some(identity(&canonical)?)
                } else {
                    None
                },
                is_directory: metadata.is_dir(),
            },
        );
        Ok(handle)
    }

    fn resolve_relative(&self, path: &Path) -> Result<PathBuf, OperationPlanErrorV1> {
        let value = path.to_string_lossy();
        let path = Path::new(value.as_ref());
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

    fn object(
        &self,
        handle: OperationObjectHandleV1,
    ) -> Result<&AuthorizedObjectV1, OperationPlanErrorV1> {
        if !handle.is_valid() || handle.generation != self.root_handle.generation {
            return Err(OperationPlanErrorV1::InvalidHandle);
        }
        self.objects
            .get(&handle)
            .ok_or(OperationPlanErrorV1::InvalidHandle)
    }

    fn destination(
        &self,
        parent: OperationObjectHandleV1,
        name: &str,
    ) -> Result<PathBuf, OperationPlanErrorV1> {
        validate_basename(name)?;
        let parent = self.object(parent)?;
        if !parent.is_directory {
            return Err(OperationPlanErrorV1::InvalidHandle);
        }
        Ok(parent.path.join(name))
    }

    pub(crate) fn map_to_host_requests(
        &self,
        plan: &OperationPlanV1,
    ) -> Result<Vec<HostMappedOperationRequestV1>, OperationPlanErrorV1> {
        let _ = self.preview(plan)?;
        plan.steps
            .iter()
            .map(|step| self.map_step_to_host_request(step))
            .collect()
    }

    fn map_step_to_host_request(
        &self,
        step: &explorer_extension_api::OperationStepV1,
    ) -> Result<HostMappedOperationRequestV1, OperationPlanErrorV1> {
        use explorer_model::{
            FileOperationFlags, FileOperationKind, FileOperationRequest, ItemDescriptor,
            LocationDescriptor, ShellItemId,
        };
        let item =
            |handle: OperationObjectHandleV1| -> Result<ItemDescriptor, OperationPlanErrorV1> {
                let object = self.object(handle)?;
                let mut id = handle.token.to_vec();
                id.extend_from_slice(&handle.generation.to_le_bytes());
                Ok(ItemDescriptor {
                    id: ShellItemId::from_provider_bytes(id)
                        .ok_or(OperationPlanErrorV1::InvalidHandle)?,
                    location: LocationDescriptor::file_system(object.path.clone()),
                })
            };
        let flags = FileOperationFlags::default();
        match step.kind {
            OperationKindV1::CREATE_DIRECTORY => {
                let parent = *step
                    .destination_parent
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::InvalidHandle)?;
                let name = step
                    .destination_name
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::InvalidWindowsName)?;
                Ok(HostMappedOperationRequestV1::File(vec![
                    FileOperationRequest {
                        kind: FileOperationKind::CreateFolder {
                            parent: LocationDescriptor::file_system(
                                self.object(parent)?.path.clone(),
                            ),
                            name: name.to_string(),
                        },
                        flags,
                    },
                ]))
            }
            OperationKindV1::RENAME => {
                let source_handle = *step
                    .source
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::InvalidHandle)?;
                let source = item(source_handle)?;
                let name = step
                    .destination_name
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::InvalidWindowsName)?
                    .to_string();
                let parent = *step
                    .destination_parent
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::InvalidHandle)?;
                let parent_path = self.object(parent)?.path.clone();
                let source_path = self.object(source_handle)?.path.clone();
                if source_path.parent() == Some(parent_path.as_path()) {
                    Ok(HostMappedOperationRequestV1::File(vec![
                        FileOperationRequest {
                            kind: FileOperationKind::Rename {
                                item: source,
                                new_name: name,
                            },
                            flags,
                        },
                    ]))
                } else {
                    let source_name = source_path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .ok_or(OperationPlanErrorV1::InvalidWindowsName)?
                        .to_owned();
                    let mut id = source_handle.token.to_vec();
                    id.extend_from_slice(name.as_bytes());
                    Ok(HostMappedOperationRequestV1::File(vec![
                        FileOperationRequest {
                            kind: FileOperationKind::Move {
                                items: vec![source],
                                destination: LocationDescriptor::file_system(parent_path.clone()),
                            },
                            flags,
                        },
                        FileOperationRequest {
                            kind: FileOperationKind::Rename {
                                item: ItemDescriptor {
                                    id: ShellItemId::from_provider_bytes(id)
                                        .ok_or(OperationPlanErrorV1::InvalidHandle)?,
                                    location: LocationDescriptor::file_system(
                                        parent_path.join(source_name),
                                    ),
                                },
                                new_name: name,
                            },
                            flags,
                        },
                    ]))
                }
            }
            OperationKindV1::COPY | OperationKindV1::MOVE => {
                let source_handle = *step
                    .source
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::InvalidHandle)?;
                let source = item(source_handle)?;
                let source_name = self
                    .object(source_handle)?
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or(OperationPlanErrorV1::InvalidWindowsName)?
                    .to_owned();
                let parent = *step
                    .destination_parent
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::InvalidHandle)?;
                let parent_path = self.object(parent)?.path.clone();
                let destination = LocationDescriptor::file_system(parent_path.clone());
                let kind = if step.kind == OperationKindV1::COPY {
                    FileOperationKind::Copy {
                        items: vec![source],
                        destination,
                    }
                } else {
                    FileOperationKind::Move {
                        items: vec![source],
                        destination,
                    }
                };
                let mut requests = vec![FileOperationRequest { kind, flags }];
                let destination_name = step
                    .destination_name
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::InvalidWindowsName)?
                    .to_string();
                if destination_name != source_name {
                    let mut id = source_handle.token.to_vec();
                    id.extend_from_slice(destination_name.as_bytes());
                    requests.push(FileOperationRequest {
                        kind: FileOperationKind::Rename {
                            item: ItemDescriptor {
                                id: ShellItemId::from_provider_bytes(id)
                                    .ok_or(OperationPlanErrorV1::InvalidHandle)?,
                                location: LocationDescriptor::file_system(
                                    parent_path.join(source_name),
                                ),
                            },
                            new_name: destination_name,
                        },
                        flags,
                    });
                }
                Ok(HostMappedOperationRequestV1::File(requests))
            }
            OperationKindV1::DELETE => {
                let source = item(
                    *step
                        .source
                        .as_ref()
                        .into_option()
                        .ok_or(OperationPlanErrorV1::InvalidHandle)?,
                )?;
                Ok(HostMappedOperationRequestV1::File(vec![
                    FileOperationRequest {
                        kind: FileOperationKind::PermanentDelete {
                            items: vec![source],
                            confirmed: true,
                        },
                        flags,
                    },
                ]))
            }
            OperationKindV1::EXTRACT => {
                let archive = item(
                    *step
                        .source
                        .as_ref()
                        .into_option()
                        .ok_or(OperationPlanErrorV1::InvalidHandle)?,
                )?;
                let parent = *step
                    .destination_parent
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::InvalidHandle)?;
                Ok(HostMappedOperationRequestV1::Extract {
                    archive,
                    destination: LocationDescriptor::file_system(self.object(parent)?.path.clone()),
                })
            }
            OperationKindV1::ARCHIVE_MUTATION => {
                Ok(HostMappedOperationRequestV1::ArchiveMutation {
                    archive: item(
                        *step
                            .source
                            .as_ref()
                            .into_option()
                            .ok_or(OperationPlanErrorV1::InvalidHandle)?,
                    )?,
                })
            }
            _ => Err(OperationPlanErrorV1::UnsupportedKind),
        }
    }
    pub fn preview(
        &self,
        plan: &OperationPlanV1,
    ) -> Result<OperationPreviewV1, OperationPlanErrorV1> {
        self.authority.revalidate()?;
        if plan.root != self.root_handle {
            return Err(OperationPlanErrorV1::InvalidHandle);
        }
        if plan.steps.len() > MAX_OPERATION_STEPS_V1 {
            return Err(OperationPlanErrorV1::TooManySteps);
        }
        let mut targets = BTreeSet::new();
        let mut preview_steps = Vec::with_capacity(plan.steps.len());
        let mut warnings = Vec::new();
        let mut irreversible_reasons = Vec::new();
        let mut total_bytes = 0_u64;
        let mut blocked = false;
        for step in &plan.steps {
            if !known_operation_kind(step.kind) || !valid_step_shape(step) {
                return Err(OperationPlanErrorV1::UnsupportedKind);
            }
            let source = step
                .source
                .as_ref()
                .into_option()
                .map(|handle| self.object(*handle))
                .transpose()?;
            let source_display_name = source
                .and_then(|object| object.path.file_name())
                .map(|name| name.to_string_lossy().into_owned());
            let source_identity_changed = source.is_some_and(|object| {
                identity(&object.path).ok().as_ref() != object.identity.as_ref()
                    || step
                        .expected_source
                        .as_ref()
                        .into_option()
                        .is_some_and(|expected| {
                            identity(&object.path).ok().as_ref() != Some(expected)
                        })
            });
            let estimated_bytes = source
                .and_then(|object| fs::metadata(&object.path).ok())
                .map_or(0, |metadata| metadata.len());
            total_bytes = total_bytes.saturating_add(estimated_bytes);

            let destination = match (
                step.destination_parent.as_ref().into_option(),
                step.destination_name.as_ref().into_option(),
            ) {
                (Some(parent), Some(name)) => Some(self.destination(*parent, name.as_str())?),
                (Some(parent), None) if step.kind == OperationKindV1::EXTRACT => {
                    Some(self.object(*parent)?.path.clone())
                }
                (None, None) => None,
                _ => return Err(OperationPlanErrorV1::UnsupportedKind),
            };
            if let (Some(parent), Some(name)) = (
                step.destination_parent.as_ref().into_option(),
                step.destination_name.as_ref().into_option(),
            ) && !targets.insert((*parent, name.to_lowercase()))
            {
                return Err(OperationPlanErrorV1::DuplicateTarget);
            }
            let destination_display_name = destination
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned());
            let conflict = if source_identity_changed {
                OperationConflictV1::SOURCE_CHANGED
            } else if destination
                .as_ref()
                .is_some_and(|path| path.exists() && step.kind != OperationKindV1::EXTRACT)
            {
                OperationConflictV1::TARGET_EXISTS
            } else {
                OperationConflictV1::NONE
            };
            let permission = operation_permission(source, destination.as_deref());
            blocked |= conflict != OperationConflictV1::NONE
                || permission == OperationPermissionV1::DENIED;
            let irreversible_reason = if matches!(
                step.kind,
                OperationKindV1::DELETE | OperationKindV1::ARCHIVE_MUTATION
            ) {
                Some(if step.kind == OperationKindV1::DELETE {
                    "delete requires a host backup to be reversible"
                } else {
                    "archive mutation requires a verified whole-container backup"
                })
            } else {
                None
            };
            let warning = if permission == OperationPermissionV1::UNKNOWN {
                Some("permission could not be fully established until commit")
            } else if conflict != OperationConflictV1::NONE {
                Some("operation conflict must be resolved before commit")
            } else {
                None
            };
            if let Some(value) = warning {
                warnings.push(value.into());
            }
            if let Some(value) = irreversible_reason {
                irreversible_reasons.push(value.into());
            }
            preview_steps.push(OperationPreviewStepV1 {
                kind: step.kind,
                source_display_name: source_display_name.map(Into::into).into(),
                destination_display_name: destination_display_name.map(Into::into).into(),
                permission,
                conflict,
                estimated_items: 1,
                estimated_bytes,
                reversible: irreversible_reason.is_none(),
                warning: warning.map(Into::into).into(),
                irreversible_reason: irreversible_reason.map(Into::into).into(),
            });
        }
        let examples = preview_steps
            .iter()
            .filter_map(|step| step.destination_display_name.as_ref().into_option())
            .take(3)
            .map(AsRef::<str>::as_ref)
            .collect::<Vec<_>>()
            .join(", ");
        Ok(OperationPreviewV1 {
            terminal_if_committed: if blocked {
                OperationTerminalV1::REJECTED
            } else {
                OperationTerminalV1::COMPLETED
            },
            step_count: plan.steps.len() as u32,
            requires_confirmation: plan.steps.len() > SECOND_CONFIRMATION_STEP_THRESHOLD_V1
                || (plan.confirmation_threshold > 0
                    && plan.steps.len() as u32 > plan.confirmation_threshold),
            summary: format!(
                "{} operation(s): {}; examples: {}",
                plan.steps.len(),
                plan.title,
                examples
            )
            .into(),
            estimated_items: plan.steps.len() as u64,
            estimated_bytes: total_bytes,
            warnings: warnings.into(),
            irreversible_reasons: irreversible_reasons.into(),
            steps: preview_steps.into(),
        })
    }
    pub fn execute(
        &mut self,
        plan: &OperationPlanV1,
        confirmed: bool,
        cancel: &OperationCancellationV1,
    ) -> Result<OperationOutcomeV1, OperationPlanErrorV1> {
        self.execute_with_progress(plan, confirmed, cancel, |_| {})
    }

    pub fn execute_with_progress<F>(
        &mut self,
        plan: &OperationPlanV1,
        confirmed: bool,
        cancel: &OperationCancellationV1,
        mut progress: F,
    ) -> Result<OperationOutcomeV1, OperationPlanErrorV1>
    where
        F: FnMut(OperationProgressV1),
    {
        let preview = self.preview(plan)?;
        // Resolve every public opaque step into the existing host/model
        // operation request vocabulary before any mutation is attempted.
        let mapped_requests = self.map_to_host_requests(plan)?;
        debug_assert_eq!(mapped_requests.len(), plan.steps.len());
        debug_assert!(mapped_requests.iter().all(|request| match request {
            HostMappedOperationRequestV1::File(requests) => !requests.is_empty(),
            HostMappedOperationRequestV1::Extract {
                archive,
                destination,
            } => archive.location.path().is_some() && destination.path().is_some(),
            HostMappedOperationRequestV1::ArchiveMutation { archive } => {
                archive.location.path().is_some()
            }
        }));
        if preview.terminal_if_committed != OperationTerminalV1::COMPLETED {
            return Ok(execution_outcome(
                plan.steps.len(),
                OperationTerminalV1::CONFLICT,
                0,
                Some(0),
                None,
                "preview contains an unresolved permission or identity conflict",
            ));
        }
        if preview.requires_confirmation && !confirmed {
            return Ok(execution_outcome(
                plan.steps.len(),
                OperationTerminalV1::REJECTED,
                0,
                None,
                None,
                "confirmation required",
            ));
        }
        let mut undo = Vec::new();
        for (index, step) in plan.steps.iter().enumerate() {
            self.authority.revalidate()?;
            if cancel.is_cancelled() {
                let token = self.store_undo(plan.undo_requested, undo);
                return Ok(execution_outcome(
                    plan.steps.len(),
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
            let destination = match (
                step.destination_parent.as_ref().into_option(),
                step.destination_name.as_ref().into_option(),
            ) {
                (Some(parent), Some(name)) => self.destination(*parent, name.as_str())?,
                _ => {
                    return Ok(execution_outcome(
                        plan.steps.len(),
                        OperationTerminalV1::REJECTED,
                        index,
                        Some(index),
                        self.store_undo(plan.undo_requested, undo),
                        "operation kind requires a specialized host executor",
                    ));
                }
            };
            self.authority.revalidate()?;
            let result = if step.kind == OperationKindV1::CREATE_DIRECTORY {
                fs::create_dir(&destination).and_then(|()| {
                    let created_identity = identity(&destination)?;
                    undo.push(UndoStep::RemoveEmpty {
                        path: destination,
                        identity: created_identity,
                    });
                    Ok(())
                })
            } else if matches!(step.kind, OperationKindV1::RENAME | OperationKindV1::MOVE) {
                let source_handle = step
                    .source
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::InvalidHandle)?;
                let source_object = self.object(*source_handle)?;
                let source = source_object.path.clone();
                if identity(&source).ok().as_ref() != source_object.identity.as_ref() {
                    return Ok(execution_outcome(
                        plan.steps.len(),
                        OperationTerminalV1::CONFLICT,
                        index,
                        Some(index),
                        self.store_undo(plan.undo_requested, undo),
                        "source identity changed",
                    ));
                }
                if let Some(expected) = step.expected_source.as_ref().into_option() {
                    if identity(&source)? != *expected {
                        return Ok(execution_outcome(
                            plan.steps.len(),
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
            } else if step.kind == OperationKindV1::COPY {
                let source_handle = step
                    .source
                    .as_ref()
                    .into_option()
                    .ok_or(OperationPlanErrorV1::InvalidHandle)?;
                let source_object = self.object(*source_handle)?;
                let source = source_object.path.clone();
                if source_object.is_directory
                    || identity(&source).ok().as_ref() != source_object.identity.as_ref()
                {
                    return Ok(execution_outcome(
                        plan.steps.len(),
                        OperationTerminalV1::CONFLICT,
                        index,
                        Some(index),
                        self.store_undo(plan.undo_requested, undo),
                        "copy source is unsupported or changed",
                    ));
                }
                fs::copy(&source, &destination).and_then(|_| {
                    let copied_identity = identity(&destination)?;
                    undo.push(UndoStep::RemoveCreatedFile {
                        path: destination,
                        identity: copied_identity,
                    });
                    Ok(())
                })
            } else {
                return Ok(execution_outcome(
                    plan.steps.len(),
                    OperationTerminalV1::REJECTED,
                    index,
                    Some(index),
                    self.store_undo(plan.undo_requested, undo),
                    "operation kind requires a specialized host executor",
                ));
            };
            if let Err(error) = result {
                return Ok(execution_outcome(
                    plan.steps.len(),
                    OperationTerminalV1::PARTIAL,
                    index,
                    Some(index),
                    self.store_undo(plan.undo_requested, undo),
                    &error.to_string(),
                ));
            }
            progress(OperationProgressV1 {
                completed_steps: (index + 1) as u32,
                failed_steps: 0,
                unattempted_steps: (plan.steps.len() - index - 1) as u32,
                current_step: Some(index as u32).into(),
            });
        }
        let token = self.store_undo(plan.undo_requested, undo);
        Ok(execution_outcome(
            plan.steps.len(),
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
        if self.authority.revalidate().is_err() {
            return outcome(
                OperationTerminalV1::REJECTED,
                0,
                None,
                None,
                "operation authority revoked",
            );
        }
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
        let pending = std::mem::take(steps);
        let total = pending.len();
        let mut completed = 0;
        let mut not_reverted = 0;
        for (position, step) in pending.into_iter().rev().enumerate() {
            if self.authority.revalidate().is_err() {
                let mut result = outcome(
                    OperationTerminalV1::PARTIAL,
                    completed,
                    Some(completed),
                    None,
                    "operation authority revoked",
                );
                result.reverted_steps = completed as u32;
                result.not_reverted_steps = (total - position) as u32;
                return result;
            }
            let result = match step {
                UndoStep::RemoveEmpty {
                    path,
                    identity: expected,
                } => match identity(&path) {
                    Ok(actual) if actual == expected => fs::remove_dir(path),
                    Ok(_) => Err(std::io::Error::other("created directory identity changed")),
                    Err(error) => Err(error),
                },
                UndoStep::RemoveCreatedFile {
                    path,
                    identity: expected,
                } => match identity(&path) {
                    Ok(actual) if actual == expected => fs::remove_file(path),
                    Ok(_) => Err(std::io::Error::other("created file identity changed")),
                    Err(error) => Err(error),
                },
                UndoStep::Rename { from, to } => fs::rename(from, to),
            };
            if result.is_err() {
                not_reverted += 1;
            } else {
                completed += 1;
            }
        }
        let mut result = outcome(
            if not_reverted == 0 {
                OperationTerminalV1::COMPLETED
            } else {
                OperationTerminalV1::PARTIAL
            },
            completed,
            (not_reverted != 0).then_some(completed),
            None,
            if not_reverted == 0 {
                "undone"
            } else {
                "undo completed conservatively; changed or non-empty items were retained"
            },
        );
        result.reverted_steps = completed as u32;
        result.not_reverted_steps = not_reverted as u32;
        result
    }
}

fn mint_object_handle(generation: u64) -> Result<OperationObjectHandleV1, OperationPlanErrorV1> {
    if generation == 0 {
        return Err(OperationPlanErrorV1::InvalidHandle);
    }
    let mut token = [0_u8; 16];
    SystemRandom::new()
        .fill(&mut token)
        .map_err(|_| OperationPlanErrorV1::RandomUnavailable)?;
    if token == [0; 16] {
        return Err(OperationPlanErrorV1::RandomUnavailable);
    }
    Ok(OperationObjectHandleV1::new(token, generation))
}

fn known_operation_kind(kind: OperationKindV1) -> bool {
    matches!(
        kind,
        OperationKindV1::CREATE_DIRECTORY
            | OperationKindV1::RENAME
            | OperationKindV1::COPY
            | OperationKindV1::MOVE
            | OperationKindV1::DELETE
            | OperationKindV1::EXTRACT
            | OperationKindV1::ARCHIVE_MUTATION
    )
}

fn valid_step_shape(step: &explorer_extension_api::OperationStepV1) -> bool {
    let source = step.source.is_some();
    let parent = step.destination_parent.is_some();
    let name = step.destination_name.is_some();
    match step.kind {
        OperationKindV1::CREATE_DIRECTORY => !source && parent && name,
        OperationKindV1::RENAME | OperationKindV1::COPY | OperationKindV1::MOVE => {
            source && parent && name
        }
        OperationKindV1::DELETE | OperationKindV1::ARCHIVE_MUTATION => source && !parent && !name,
        OperationKindV1::EXTRACT => source && parent && !name,
        _ => false,
    }
}

fn validate_basename(name: &str) -> Result<(), OperationPlanErrorV1> {
    let path = Path::new(name);
    if name.is_empty()
        || path.is_absolute()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|character| character < ' ' || "<>:\"/\\|?*".contains(character))
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
    Ok(())
}

fn operation_permission(
    source: Option<&AuthorizedObjectV1>,
    destination: Option<&Path>,
) -> OperationPermissionV1 {
    let mut unknown = false;
    for path in source
        .map(|object| object.path.as_path())
        .into_iter()
        .chain(destination.and_then(Path::parent))
    {
        match fs::metadata(path) {
            Ok(metadata) if metadata.permissions().readonly() => {
                return OperationPermissionV1::DENIED;
            }
            Ok(_) => {}
            Err(_) => unknown = true,
        }
    }
    if unknown {
        OperationPermissionV1::UNKNOWN
    } else {
        OperationPermissionV1::ALLOWED
    }
}

fn execution_outcome(
    total: usize,
    terminal: OperationTerminalV1,
    completed: usize,
    failed: Option<usize>,
    token: Option<String>,
    detail: &str,
) -> OperationOutcomeV1 {
    let mut value = outcome(terminal, completed, failed, token, detail);
    let journal_sequence = NEXT_OPERATION_JOURNAL_ID_V1.fetch_add(1, Ordering::Relaxed);
    value.journal_id = Some(format!("operation:{journal_sequence}").into()).into();
    if completed == 0
        && matches!(
            terminal,
            OperationTerminalV1::CONFLICT | OperationTerminalV1::REJECTED
        )
    {
        value.attempted_steps = 0;
        value.failed_steps = 0;
    }
    value.unattempted_steps = total.saturating_sub(value.attempted_steps as usize) as u32;
    value
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
        attempted_steps: (completed + usize::from(failed.is_some())) as u32,
        completed_steps: completed as u32,
        failed_steps: u32::from(failed.is_some()),
        unattempted_steps: 0,
        reverted_steps: 0,
        not_reverted_steps: 0,
        failed_step: failed.map(|v| v as u32).into(),
        undo_token: token.map(Into::into).into(),
        journal_id: Default::default(),
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
    use std::{
        fs::OpenOptions,
        os::windows::{fs::OpenOptionsExt as _, io::AsRawHandle},
    };
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
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
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
    use crate::runtime_authority::AuthorityClaimsV1;
    use abi_stable::std_types::ROption;
    use explorer_extension_api::OperationStepV1;
    use tempfile::TempDir;

    fn operation_authority() -> OperationPlanAuthorityV1 {
        let runtime = Arc::new(RuntimeAuthorityV1::new().unwrap());
        let envelope = runtime
            .issue(AuthorityClaimsV1 {
                package_id: "operation-test".into(),
                feature_id: "operations".into(),
                interface_id: "plan".into(),
                incarnation: 1,
                capability: "operations.submit".into(),
                authorized_root_sha256: "a".repeat(64),
                location_generation: 1,
                item_generation: 1,
                refresh_generation: 1,
                container_generation: 1,
                job_generation: 1,
            })
            .unwrap();
        OperationPlanAuthorityV1::from_host(runtime, envelope)
    }

    fn engine(root: &TempDir) -> HostOperationPlanEngineV1 {
        HostOperationPlanEngineV1::new(root.path().to_owned(), operation_authority()).unwrap()
    }

    fn create_step(parent: OperationObjectHandleV1, name: &str) -> OperationStepV1 {
        OperationStepV1 {
            kind: OperationKindV1::CREATE_DIRECTORY,
            source: ROption::RNone,
            destination_parent: ROption::RSome(parent),
            destination_name: ROption::RSome(name.into()),
            expected_source: ROption::RNone,
        }
    }

    fn plan(
        root: OperationObjectHandleV1,
        title: &str,
        steps: Vec<OperationStepV1>,
        undo_requested: bool,
    ) -> OperationPlanV1 {
        OperationPlanV1 {
            title: title.into(),
            root,
            confirmation_threshold: 0,
            undo_requested,
            steps: steps.into(),
        }
    }

    #[test]
    fn partial_cancel_and_conservative_undo_are_truthful() {
        let root = TempDir::new().unwrap();
        let mut engine = engine(&root);
        let root_handle = engine.root_handle();
        let plan = plan(
            root_handle,
            "folders",
            vec![create_step(root_handle, "a"), create_step(root_handle, "b")],
            true,
        );
        let result = engine
            .execute(&plan, true, &OperationCancellationV1::default())
            .unwrap();
        assert_eq!(result.terminal, OperationTerminalV1::COMPLETED);
        fs::write(root.path().join("b/x"), b"user").unwrap();
        let undone = engine.undo(result.undo_token.as_ref().unwrap());
        assert_eq!(undone.terminal, OperationTerminalV1::PARTIAL);
        assert_eq!(undone.reverted_steps, 1);
        assert_eq!(undone.not_reverted_steps, 1);
        assert!(root.path().join("b/x").exists());
        let repeated = engine.undo(result.undo_token.as_ref().unwrap());
        assert_eq!(repeated.terminal, OperationTerminalV1::COMPLETED);
        assert_eq!(repeated.reverted_steps, 0);
        assert!(root.path().join("b/x").exists());
    }
    #[test]
    fn traversal_is_rejected() {
        let root = TempDir::new().unwrap();
        let engine = engine(&root);
        let plan = plan(
            engine.root_handle(),
            "bad",
            vec![create_step(engine.root_handle(), "../escape")],
            false,
        );
        assert!(matches!(
            engine.preview(&plan),
            Err(OperationPlanErrorV1::InvalidWindowsName)
        ));
    }

    #[test]
    fn feature_revoke_after_preview_rejects_commit_without_mutation() {
        let root = TempDir::new().unwrap();
        let authority = operation_authority();
        let runtime = Arc::clone(&authority.runtime);
        let mut engine = HostOperationPlanEngineV1::new(root.path().to_owned(), authority).unwrap();
        let plan = plan(
            engine.root_handle(),
            "revoked",
            vec![create_step(engine.root_handle(), "must-not-exist")],
            false,
        );

        assert!(engine.preview(&plan).is_ok());
        assert_eq!(
            runtime.revoke_feature("operation-test", "operations"),
            Ok(1)
        );
        assert!(matches!(
            engine.execute(&plan, true, &OperationCancellationV1::default()),
            Err(OperationPlanErrorV1::Unauthorized)
        ));
        assert!(!root.path().join("must-not-exist").exists());
    }

    #[test]
    fn more_than_one_thousand_steps_always_require_confirmation_and_show_examples() {
        let root = TempDir::new().unwrap();
        let mut engine = engine(&root);
        let root_handle = engine.root_handle();
        let plan = plan(
            root_handle,
            "bulk folders",
            (0..=SECOND_CONFIRMATION_STEP_THRESHOLD_V1)
                .map(|index| create_step(root_handle, &format!("folder-{index:04}")))
                .collect(),
            false,
        );

        let preview = engine.preview(&plan).unwrap();
        assert!(preview.requires_confirmation);
        assert!(preview.summary.contains("folder-0000"));
        assert!(preview.summary.contains("folder-0001"));
        assert!(preview.summary.contains("folder-0002"));

        let outcome = engine
            .execute(&plan, false, &OperationCancellationV1::default())
            .unwrap();
        assert_eq!(outcome.terminal, OperationTerminalV1::REJECTED);
        assert!(!root.path().join("folder-0000").exists());
    }

    #[test]
    fn external_conflict_after_preview_is_rejected_without_authorized_mutation() {
        let root = TempDir::new().unwrap();
        let mut engine = engine(&root);
        let plan = plan(
            engine.root_handle(),
            "external conflict",
            vec![create_step(engine.root_handle(), "occupied")],
            false,
        );

        assert!(engine.preview(&plan).is_ok());
        fs::create_dir(root.path().join("occupied")).unwrap();
        fs::write(root.path().join("occupied/external.txt"), b"external").unwrap();

        let outcome = engine
            .execute(&plan, true, &OperationCancellationV1::default())
            .unwrap();
        assert_eq!(outcome.terminal, OperationTerminalV1::CONFLICT);
        assert_eq!(
            fs::read(root.path().join("occupied/external.txt")).unwrap(),
            b"external"
        );
    }

    #[test]
    fn opaque_handles_and_rich_preview_cover_every_typed_kind() {
        let root = TempDir::new().unwrap();
        fs::write(root.path().join("source.txt"), b"source").unwrap();
        fs::write(root.path().join("archive.7z"), b"archive").unwrap();
        let mut engine = engine(&root);
        let source = engine.authorize_existing(Path::new("source.txt")).unwrap();
        let archive = engine.authorize_existing(Path::new("archive.7z")).unwrap();
        let root_handle = engine.root_handle();
        let steps = vec![
            create_step(root_handle, "created"),
            OperationStepV1 {
                kind: OperationKindV1::RENAME,
                source: ROption::RSome(source),
                destination_parent: ROption::RSome(root_handle),
                destination_name: ROption::RSome("renamed.txt".into()),
                expected_source: ROption::RNone,
            },
            OperationStepV1 {
                kind: OperationKindV1::COPY,
                source: ROption::RSome(source),
                destination_parent: ROption::RSome(root_handle),
                destination_name: ROption::RSome("copied.txt".into()),
                expected_source: ROption::RNone,
            },
            OperationStepV1 {
                kind: OperationKindV1::MOVE,
                source: ROption::RSome(source),
                destination_parent: ROption::RSome(root_handle),
                destination_name: ROption::RSome("moved.txt".into()),
                expected_source: ROption::RNone,
            },
            OperationStepV1 {
                kind: OperationKindV1::DELETE,
                source: ROption::RSome(source),
                destination_parent: ROption::RNone,
                destination_name: ROption::RNone,
                expected_source: ROption::RNone,
            },
            OperationStepV1 {
                kind: OperationKindV1::EXTRACT,
                source: ROption::RSome(archive),
                destination_parent: ROption::RSome(root_handle),
                destination_name: ROption::RNone,
                expected_source: ROption::RNone,
            },
            OperationStepV1 {
                kind: OperationKindV1::ARCHIVE_MUTATION,
                source: ROption::RSome(archive),
                destination_parent: ROption::RNone,
                destination_name: ROption::RNone,
                expected_source: ROption::RNone,
            },
        ];
        let all_kinds_plan = plan(root_handle, "all kinds", steps, true);
        let preview = engine.preview(&all_kinds_plan).unwrap();
        assert_eq!(preview.steps.len(), 7);
        assert_eq!(preview.estimated_items, 7);
        assert!(preview.estimated_bytes >= 6);
        assert_eq!(preview.irreversible_reasons.len(), 2);
        assert!(
            preview
                .steps
                .iter()
                .all(|step| step.permission == OperationPermissionV1::ALLOWED)
        );
        let mapped = engine.map_to_host_requests(&all_kinds_plan).unwrap();
        assert_eq!(mapped.len(), 7);
        assert!(matches!(mapped[0], HostMappedOperationRequestV1::File(_)));
        assert!(matches!(
            mapped[5],
            HostMappedOperationRequestV1::Extract { .. }
        ));
        assert!(matches!(
            mapped[6],
            HostMappedOperationRequestV1::ArchiveMutation { .. }
        ));
        let mapped_payload_count = mapped
            .iter()
            .map(|request| match request {
                HostMappedOperationRequestV1::File(requests) => requests.len(),
                HostMappedOperationRequestV1::Extract {
                    archive,
                    destination,
                } => usize::from(archive.location == destination.clone()) + 1,
                HostMappedOperationRequestV1::ArchiveMutation { archive } => {
                    usize::from(archive.location.path().is_some())
                }
            })
            .sum::<usize>();
        assert!(mapped_payload_count >= 7);

        let forged = OperationObjectHandleV1::new([7; 16], root_handle.generation);
        let forged_plan = plan(
            root_handle,
            "forged",
            vec![create_step(forged, "nope")],
            false,
        );
        assert!(matches!(
            engine.preview(&forged_plan),
            Err(OperationPlanErrorV1::InvalidHandle)
        ));
    }

    #[test]
    fn preview_exposes_permission_conflict_warning_and_irreversible_reason() {
        let root = TempDir::new().unwrap();
        fs::create_dir(root.path().join("readonly")).unwrap();
        let mut permissions = fs::metadata(root.path().join("readonly"))
            .unwrap()
            .permissions();
        permissions.set_readonly(true);
        fs::set_permissions(root.path().join("readonly"), permissions.clone()).unwrap();
        let mut engine = engine(&root);
        let readonly = engine.authorize_existing(Path::new("readonly")).unwrap();
        let denied = engine
            .preview(&plan(
                engine.root_handle(),
                "denied",
                vec![create_step(readonly, "child")],
                false,
            ))
            .unwrap();
        assert_eq!(denied.terminal_if_committed, OperationTerminalV1::REJECTED);
        assert_eq!(denied.steps[0].permission, OperationPermissionV1::DENIED);

        permissions.set_readonly(false);
        fs::set_permissions(root.path().join("readonly"), permissions).unwrap();
        fs::create_dir(root.path().join("occupied")).unwrap();
        let conflict = engine
            .preview(&plan(
                engine.root_handle(),
                "conflict",
                vec![create_step(engine.root_handle(), "occupied")],
                false,
            ))
            .unwrap();
        assert_eq!(
            conflict.steps[0].conflict,
            OperationConflictV1::TARGET_EXISTS
        );
        assert!(!conflict.warnings.is_empty());

        fs::write(root.path().join("delete.txt"), b"delete").unwrap();
        let source = engine.authorize_existing(Path::new("delete.txt")).unwrap();
        let delete = OperationStepV1 {
            kind: OperationKindV1::DELETE,
            source: ROption::RSome(source),
            destination_parent: ROption::RNone,
            destination_name: ROption::RNone,
            expected_source: ROption::RNone,
        };
        let irreversible = engine
            .preview(&plan(engine.root_handle(), "delete", vec![delete], false))
            .unwrap();
        assert!(!irreversible.irreversible_reasons.is_empty());
        assert!(!irreversible.steps[0].reversible);
    }

    #[test]
    fn progress_cancel_failure_and_terminal_counts_are_exact_and_bounded() {
        let root = TempDir::new().unwrap();
        let mut engine = engine(&root);
        let root_handle = engine.root_handle();
        let bulk = plan(
            root_handle,
            "cancel",
            vec![
                create_step(root_handle, "one"),
                create_step(root_handle, "two"),
                create_step(root_handle, "three"),
            ],
            true,
        );
        let cancellation = OperationCancellationV1::default();
        let cancel_from_progress = cancellation.clone();
        let mut progress_events = Vec::new();
        let cancelled = engine
            .execute_with_progress(&bulk, true, &cancellation, |progress| {
                progress_events.push(progress);
                cancel_from_progress.cancel();
            })
            .unwrap();
        assert_eq!(progress_events.len(), 1);
        assert_eq!(cancelled.terminal, OperationTerminalV1::PARTIAL);
        assert_eq!(cancelled.attempted_steps, 1);
        assert_eq!(cancelled.completed_steps, 1);
        assert_eq!(cancelled.failed_steps, 0);
        assert_eq!(cancelled.unattempted_steps, 2);
        assert!(cancelled.journal_id.is_some());
        assert!(!root.path().join("two").exists());

        fs::create_dir(root.path().join("vanishing-parent")).unwrap();
        let parent = engine
            .authorize_existing(Path::new("vanishing-parent"))
            .unwrap();
        fs::remove_dir(root.path().join("vanishing-parent")).unwrap();
        let failure = plan(
            root_handle,
            "failure",
            vec![create_step(parent, "child")],
            false,
        );
        let failed = engine
            .execute(&failure, true, &OperationCancellationV1::default())
            .unwrap();
        assert_eq!(failed.terminal, OperationTerminalV1::PARTIAL);
        assert_eq!(failed.attempted_steps, 1);
        assert_eq!(failed.completed_steps, 0);
        assert_eq!(failed.failed_steps, 1);
        assert_eq!(failed.unattempted_steps, 0);
    }

    #[test]
    fn terminal_closes_synchronous_progress_and_repeat_undo_is_idempotent() {
        let root = TempDir::new().unwrap();
        let mut engine = engine(&root);
        let root_handle = engine.root_handle();
        let operation = plan(
            root_handle,
            "terminal barrier",
            vec![create_step(root_handle, "created")],
            true,
        );
        let callback_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let callback_probe = Arc::clone(&callback_count);
        let terminal = engine
            .execute_with_progress(
                &operation,
                true,
                &OperationCancellationV1::default(),
                move |_| {
                    callback_probe.fetch_add(1, Ordering::Relaxed);
                },
            )
            .unwrap();
        assert_eq!(terminal.terminal, OperationTerminalV1::COMPLETED);
        assert_eq!(callback_count.load(Ordering::Relaxed), 1);
        let token = terminal.undo_token.as_ref().unwrap().to_string();
        assert_eq!(engine.undo(&token).reverted_steps, 1);
        assert_eq!(engine.undo(&token).reverted_steps, 0);
        assert_eq!(callback_count.load(Ordering::Relaxed), 1);
    }
}

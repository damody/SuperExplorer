#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
//! Pure application state with no GPUI, filesystem, or COM dependencies.
#![allow(
    clippy::must_use_candidate,
    reason = "the model exposes many inexpensive constructors and state queries; requiring every caller to consume them would add noise without preventing resource loss"
)]

mod context_menu;
mod domain;
mod drag_drop;
mod lock_recovery;
mod namespace;
mod navigation;
mod operation;
mod preview;
mod protocol;
mod session;
mod thumbnail;
mod window;

use explorer_common::LifecyclePhase;

pub use context_menu::{
    ContextMenuHostCommand, ContextMenuInvocationProfile, ContextMenuOutcome, ContextMenuRequest,
    ContextMenuSession, ContextMenuSessionState, MenuPoint,
};
pub use domain::{
    CancellationRegistration, CancellationSignalReport, CancellationToken, Generation,
    LocationDescriptor, LocationDescriptorValidationError, MAX_LOCATION_DESCRIPTOR_BYTES,
    RequestContext, RequestRejection, ShellItemId, SyntheticRoot, TabId,
};
pub use drag_drop::{
    AutoScrollDirection, DragButton, DragEffect, DragModifiers, DragSession, DragSessionState,
    DropTargetKind, DropTargetSnapshot, default_filesystem_drop_effect,
    filesystem_drop_destination_is_valid, negotiate_effect, negotiate_filesystem_drop_effect,
};
pub use explorer_common::RequestId;
pub use lock_recovery::{
    DeleteLockKind, LockOwner, LockOwnerApplicationType, LockOwnerCloseOutcome,
    LockOwnerCloseRequest, LockOwnerCloseResult, LockOwnerCloseTerminal, LockOwnerDiscoveryRequest,
    LockOwnerDiscoveryTerminal, LockOwnerEligibility, LockOwnerIdentity,
};
pub use namespace::{
    DynamicColumnDescriptor, HomeAggregationState, NamespaceAvailability, NamespaceCapabilities,
    NamespaceCommand, NamespaceItem, NamespaceRoot, PropertyKey, PropertyValue,
    QuickAccessMutation, QuickAccessPin, QuickAccessPins, RecentItems, RecentNamespaceItem,
    ShellIdentity, UnavailableReason, aggregate_home, aggregate_home_state,
    namespace_command_enabled,
};
pub use navigation::{
    AddressBarMode, AddressBarState, BreadcrumbIconHint, BreadcrumbMenuItem, BreadcrumbSegment,
    BreadcrumbSegmentId, DetailsColumnWidths, DirectorySnapshot, DirectoryState, DriveAvailability,
    DriveKind, DriveMetadata, FileEntry, FileEntryMetadata, HistoryEntry, MenuFocusMovement,
    NavigationHistory, PresentationChange, SelectionModel, SortColumn, SortDescriptor,
    SortDirection, TabRequestScopes, TabSearchState, TabState, TabViewState, ViewAnchor, ViewMode,
    ViewSettings, default_icon_size_for_mode, effective_icon_size,
};
pub use operation::{
    JournalEntry, JournalInverse, JournalPreimage, JournalValidation, OperationCenterState,
    OperationJournal, OperationPhase, OperationRecord, OperationStateError, RenameCommitTrigger,
    RenameEditorState, WindowsFileNameError, validate_windows_file_name,
};
pub use preview::{
    PreviewDeadlinePolicy, PreviewEligibility, PreviewFallback, PreviewHandlerIdentity,
    PreviewHostBounds, PreviewHostCommand, PreviewHostError, PreviewHostTerminal,
    PreviewInitializationMode, PreviewLifecycle, PreviewOperation, PreviewRegistrationSource,
    PreviewRequestIdentity, PreviewSelection, PreviewTransitionError,
};
pub use protocol::{
    BaseIconClass, BaseIconKey, BreadcrumbTerminal, ClipboardMode, ClipboardState,
    ConflictDecision, DataTransferRequest, DirectoryDelta, ExplorerCommand, ExplorerEvent,
    ExplorerService, ExplorerServiceError, FileOperationFlags, FileOperationKind,
    FileOperationRequest, IconInvalidationEpochs, ItemDescriptor, LocationMetadata,
    OpenDisposition, OperationItemOutcome, OperationItemResult, OperationProgress,
    OperationTerminal, SearchBackend, SearchInput, SearchSourcePhase, SearchSourceStatus,
    SearchTerminal, ShellContextMenuTarget, ShellIconFallbackReason, ShellIconKey,
    ShellIconPayload, ShellIconPayloadError, ShellIconTheme, ShellNewItemDescriptor,
    ShellNewItemRecipe, ShellNewValidationError, TerminalLedger, TerminalViolation,
    TransferEffects, base_icon_key, classify_base_icon,
};
pub use session::{
    PersistedColumn, PersistedColumnWidths, PersistedHistoryEntry, PersistedQuickAccessPin,
    PersistedRect, PersistedSessionEnvelope, PersistedSessionPayload, PersistedSort,
    PersistedSortDirection, PersistedTab, PersistedViewMode, PersistedViewSettings,
    PersistedWindowPlacement, RestorePlan, SESSION_SCHEMA_VERSION, SessionLoadOutcome,
    SessionLoadSource, SessionProvenance, SessionResetScope, SessionStore, SessionStoreError,
    SessionValidationError,
};
pub use thumbnail::{
    ThumbnailConsumer, ThumbnailFallbackReason, ThumbnailMode, ThumbnailPixelError,
    ThumbnailPixels, ThumbnailPriority, ThumbnailProviderOutcome, ThumbnailRequest,
    ThumbnailRequestKey, ThumbnailSource, ThumbnailStatus, ThumbnailTerminal, ThumbnailViewport,
    normalize_thumbnail_provider_outcome, view_mode_thumbnail_policy,
};
pub use window::{
    ExplorerWindowState, TabCloseOutcome, TabPresentationSnapshot, TabStateInvariantError,
    WindowEventOutcome, WindowInvariantError,
};

/// The smallest composition state used while the workspace is bootstrapped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspaceModel {
    lifecycle: LifecyclePhase,
}

impl WorkspaceModel {
    /// Creates the initial model before services are started.
    pub const fn new() -> Self {
        Self {
            lifecycle: LifecyclePhase::Created,
        }
    }

    /// Returns the current process lifecycle visible to the model.
    pub const fn lifecycle(&self) -> LifecyclePhase {
        self.lifecycle
    }
}

impl Default for WorkspaceModel {
    fn default() -> Self {
        Self::new()
    }
}

//! Owned command/event protocol shared by UI coordination, fakes, and Windows adapters.

use std::{collections::HashSet, fmt};

use explorer_common::{ExplorerError, RequestId};

use crate::{
    BreadcrumbMenuItem, BreadcrumbSegment, BreadcrumbSegmentId, FileEntry, Generation,
    LocationDescriptor, RequestContext, ShellItemId, TabId,
};

/// Service endpoint status represented without platform-specific channel errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExplorerServiceError {
    Overloaded,
    Disconnected,
    Internal,
}

/// Thread-safe owned protocol boundary implemented by production and deterministic services.
pub trait ExplorerService: Send + Sync {
    /// Queues one owned command without blocking.
    ///
    /// # Errors
    ///
    /// Returns overload, disconnect, or internal endpoint status without losing ownership safety.
    fn submit(&self, command: ExplorerCommand) -> Result<(), ExplorerServiceError>;
    /// Receives at most one pending event without blocking.
    ///
    /// # Errors
    ///
    /// Returns disconnect or internal endpoint status; an empty live queue is `Ok(None)`.
    fn try_recv(&self) -> Result<Option<ExplorerEvent>, ExplorerServiceError>;

    /// Returns a bounded Host-owned snapshot. Implementations may perform local IPC, so callers
    /// must invoke this from a background worker rather than the UI thread.
    fn cache_telemetry_snapshot(&self) -> crate::CacheTelemetrySnapshotV1 {
        crate::CacheTelemetrySnapshotV1::default()
    }
}

/// Stable identity plus a resolvable descriptor for one operation item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemDescriptor {
    pub id: ShellItemId,
    pub location: LocationDescriptor,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShellIconTheme {
    Light,
    Dark,
    HighContrast,
}

/// Reusable base-icon class. Classes that may embed per-item artwork retain stable identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum BaseIconClass {
    Folder,
    Extension(String),
    ExtensionlessFile,
    Identity(ShellItemId),
}

/// Cache identity for an icon that may be shared by many directory rows.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BaseIconKey {
    pub class: BaseIconClass,
    pub size_bucket: u16,
    pub dpi: u16,
    pub theme: ShellIconTheme,
    pub association_epoch: u64,
}

/// Association and per-item presentation invalidation are deliberately independent of navigation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IconInvalidationEpochs {
    association: u64,
    overlay: u64,
}

impl IconInvalidationEpochs {
    pub const fn association(self) -> u64 {
        self.association
    }

    pub const fn overlay(self) -> u64 {
        self.overlay
    }

    pub fn advance_association(&mut self) -> u64 {
        self.association = self.association.saturating_add(1);
        self.association
    }

    pub fn advance_overlay(&mut self) -> u64 {
        self.overlay = self.overlay.saturating_add(1);
        self.overlay
    }

    pub fn advance_overlay_past(&mut self, observed: u64) -> u64 {
        self.overlay = self.overlay.max(observed).saturating_add(1);
        self.overlay
    }
}

/// Classifies a row without filesystem or Shell I/O.
pub fn classify_base_icon(entry: &FileEntry) -> BaseIconClass {
    let identity_specific_location = matches!(
        entry.location,
        LocationDescriptor::ShellNamespace(_)
            | LocationDescriptor::ParsingName(_)
            | LocationDescriptor::KnownFolder(_)
    );
    let filesystem_root = entry
        .location
        .path()
        .is_some_and(|path| path.parent().is_none());
    if identity_specific_location || filesystem_root {
        return BaseIconClass::Identity(entry.id.clone());
    }
    if entry.is_container {
        return BaseIconClass::Folder;
    }
    let extension = entry
        .location
        .path()
        .and_then(std::path::Path::extension)
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .or_else(|| {
            let LocationDescriptor::Virtual(location) = &entry.location else {
                return None;
            };
            std::path::Path::new(location.components.last()?)
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .map(str::to_ascii_lowercase)
        });
    match extension.as_deref() {
        Some("exe" | "dll" | "ico" | "lnk" | "url" | "cpl") => {
            BaseIconClass::Identity(entry.id.clone())
        }
        Some(extension) if !extension.is_empty() => BaseIconClass::Extension(extension.to_owned()),
        _ => BaseIconClass::ExtensionlessFile,
    }
}

pub fn base_icon_key(
    entry: &FileEntry,
    size_bucket: u16,
    dpi: u16,
    theme: ShellIconTheme,
    association_epoch: u64,
) -> BaseIconKey {
    BaseIconKey {
        class: classify_base_icon(entry),
        size_bucket,
        dpi,
        theme,
        association_epoch,
    }
}

/// Complete cache identity for an icon request. It is Send-owned and contains
/// no COM interfaces or Win32 handles.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ShellIconKey {
    pub item_id: Option<ShellItemId>,
    pub location: LocationDescriptor,
    pub size_bucket: u16,
    pub dpi: u16,
    pub theme: ShellIconTheme,
    pub association_generation: u64,
    /// Shell overlay state generation (for example `TortoiseGit` or `OneDrive` badges). It is
    /// independent from file-association changes so either source invalidates only stale pixels.
    pub overlay_generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellIconFallbackReason {
    ShellUnavailable,
    UnsupportedItem,
    InvalidBitmap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellIconPayloadError {
    ZeroDimension,
    InvalidStride,
    InvalidBufferLength,
}

/// Validated BC7 block rows owned by the Host and suitable for direct GPU upload.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompressedRasterKind {
    Icon,
    Thumbnail,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Bc7RasterPayload {
    pub kind: CompressedRasterKind,
    pub width: u32,
    pub height: u32,
    pub padded_width: u32,
    pub padded_height: u32,
    pub row_pitch: u32,
    pub blocks: Vec<u8>,
}

impl Bc7RasterPayload {
    pub fn validate(&self, maximum_bytes: usize) -> bool {
        let Some(padded_width) = self.width.checked_add(3).map(|value| value & !3) else {
            return false;
        };
        let Some(padded_height) = self.height.checked_add(3).map(|value| value & !3) else {
            return false;
        };
        let Some(row_pitch) = (padded_width / 4).checked_mul(16) else {
            return false;
        };
        let expected = usize::try_from(row_pitch).ok().and_then(|pitch| {
            usize::try_from(padded_height / 4)
                .ok()
                .and_then(|rows| pitch.checked_mul(rows))
        });
        self.width > 0
            && self.height > 0
            && self.padded_width == padded_width
            && self.padded_height == padded_height
            && self.row_pitch == row_pitch
            && expected == Some(self.blocks.len())
            && self.blocks.len() <= maximum_bytes
    }
}

/// Owned RGBA8 pixels crossing the STA boundary. No apartment-affine value is
/// allowed in this payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellIconPayload {
    pub key: ShellIconKey,
    pub width: u16,
    pub height: u16,
    pub stride: u32,
    pub rgba: Vec<u8>,
    pub bc7: Option<Bc7RasterPayload>,
    pub fallback_reason: Option<ShellIconFallbackReason>,
}

impl ShellIconPayload {
    /// Creates a validated tightly described Shell icon bitmap payload.
    ///
    /// # Errors
    ///
    /// Returns an error for zero dimensions, an undersized stride, or a byte buffer whose
    /// length does not exactly match `stride * height`.
    pub fn new(
        key: ShellIconKey,
        width: u16,
        height: u16,
        stride: u32,
        rgba: Vec<u8>,
        fallback_reason: Option<ShellIconFallbackReason>,
    ) -> Result<Self, ShellIconPayloadError> {
        if width == 0 || height == 0 {
            return Err(ShellIconPayloadError::ZeroDimension);
        }
        if stride < u32::from(width) * 4 {
            return Err(ShellIconPayloadError::InvalidStride);
        }
        if rgba.len() != stride as usize * usize::from(height) {
            return Err(ShellIconPayloadError::InvalidBufferLength);
        }
        Ok(Self {
            key,
            width,
            height,
            stride,
            rgba,
            bc7: None,
            fallback_reason,
        })
    }

    /// Builds an icon payload from a validated BC7 raster.
    ///
    /// # Errors
    ///
    /// Returns [`ShellIconPayloadError`] when dimensions or encoded bytes do
    /// not satisfy the bounded raster contract.
    pub fn new_bc7(
        key: ShellIconKey,
        raster: Bc7RasterPayload,
        fallback_reason: Option<ShellIconFallbackReason>,
    ) -> Result<Self, ShellIconPayloadError> {
        if !raster.validate(64 * 1024 * 1024) {
            return Err(ShellIconPayloadError::InvalidBufferLength);
        }
        let width =
            u16::try_from(raster.width).map_err(|_| ShellIconPayloadError::ZeroDimension)?;
        let height =
            u16::try_from(raster.height).map_err(|_| ShellIconPayloadError::ZeroDimension)?;
        Ok(Self {
            key,
            width,
            height,
            stride: 0,
            rgba: Vec::new(),
            bc7: Some(raster),
            fallback_reason,
        })
    }
}

/// How an activated item should be opened.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenDisposition {
    CurrentTab,
    NewTab,
    DefaultApplication,
}

/// Collision behavior explicitly chosen before a destructive native operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictDecision {
    Prompt,
    Skip,
    Replace,
    KeepBoth,
}

/// Cross-operation flags independent from `IFileOperation` bit values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileOperationFlags {
    pub allow_undo: bool,
    pub require_confirmation: bool,
    pub conflict: ConflictDecision,
}

impl Default for FileOperationFlags {
    fn default() -> Self {
        Self {
            allow_undo: true,
            require_confirmation: false,
            conflict: ConflictDecision::Prompt,
        }
    }
}

/// Typed file-operation kind submitted to the Shell STA.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileOperationKind {
    CreateFolder {
        parent: LocationDescriptor,
        name: String,
    },
    CreateItem {
        parent: LocationDescriptor,
        name: String,
        recipe: ShellNewItemRecipe,
    },
    Rename {
        item: ItemDescriptor,
        new_name: String,
    },
    SetUnixMode {
        item: ItemDescriptor,
        mode: u32,
    },
    Copy {
        items: Vec<ItemDescriptor>,
        destination: LocationDescriptor,
    },
    Move {
        items: Vec<ItemDescriptor>,
        destination: LocationDescriptor,
    },
    RecycleDelete {
        items: Vec<ItemDescriptor>,
    },
    PermanentDelete {
        items: Vec<ItemDescriptor>,
        confirmed: bool,
    },
    CreateShortcut {
        items: Vec<ItemDescriptor>,
    },
}

/// Safe owned subset of `ShellNew` recipes. Arbitrary registry handlers and commands are never
/// represented across the process boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellNewItemRecipe {
    Folder,
    EmptyFile,
    Data(Vec<u8>),
    TemplateFile(std::path::PathBuf),
}

impl ShellNewItemRecipe {
    pub const MAXIMUM_DATA_BYTES: usize = 64 * 1024;

    /// Verifies that the recipe can be executed through the safe Shell New boundary.
    ///
    /// # Errors
    ///
    /// Returns [`ShellNewValidationError::OversizedData`] for an oversized inline payload or
    /// [`ShellNewValidationError::UntrustedTemplate`] for a non-absolute template path.
    pub fn validate(&self) -> Result<(), ShellNewValidationError> {
        match self {
            Self::Folder | Self::EmptyFile => Ok(()),
            Self::Data(data) if data.len() <= Self::MAXIMUM_DATA_BYTES => Ok(()),
            Self::Data(_) => Err(ShellNewValidationError::OversizedData),
            Self::TemplateFile(path) if path.is_absolute() => Ok(()),
            Self::TemplateFile(_) => Err(ShellNewValidationError::UntrustedTemplate),
        }
    }
}

/// Presentation and creation metadata for one safe entry in the Explorer-like New menu.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellNewItemDescriptor {
    pub stable_id: String,
    pub display_name: String,
    pub extension: Option<String>,
    pub default_stem: String,
    pub recipe: ShellNewItemRecipe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellNewValidationError {
    EmptyIdentity,
    OversizedText,
    InvalidExtension,
    OversizedData,
    UntrustedTemplate,
}

impl ShellNewItemDescriptor {
    /// Validates menu metadata and its associated safe creation recipe.
    ///
    /// # Errors
    ///
    /// Returns [`ShellNewValidationError`] when identity or display text is invalid, an
    /// extension is unsafe, or the nested recipe violates its size or trust boundary.
    pub fn validate(&self) -> Result<(), ShellNewValidationError> {
        if self.stable_id.trim().is_empty() {
            return Err(ShellNewValidationError::EmptyIdentity);
        }
        if self.stable_id.len() > 128
            || self.display_name.is_empty()
            || self.display_name.len() > 512
            || self.default_stem.is_empty()
            || self.default_stem.len() > 512
        {
            return Err(ShellNewValidationError::OversizedText);
        }
        if let Some(extension) = &self.extension
            && (extension.len() > 32
                || !extension.starts_with('.')
                || extension[1..].is_empty()
                || extension[1..]
                    .chars()
                    .any(|character| !character.is_alphanumeric()))
        {
            return Err(ShellNewValidationError::InvalidExtension);
        }
        self.recipe.validate()
    }
}

/// Complete native file-operation request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileOperationRequest {
    pub kind: FileOperationKind,
    pub flags: FileOperationFlags,
}

/// Per-item native result retained even when the overall operation is partial.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationItemOutcome {
    pub item: Option<ItemDescriptor>,
    pub destination: Option<LocationDescriptor>,
    pub result: OperationItemResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationItemResult {
    Succeeded,
    Skipped,
    Cancelled,
    Partial(ExplorerError),
    Failed(ExplorerError),
}

/// Shell context-menu selection contract without native handles or interfaces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellContextMenuTarget {
    Background {
        parent: LocationDescriptor,
    },
    Items {
        parent: LocationDescriptor,
        items: Vec<ItemDescriptor>,
    },
}

/// User search text. Debug output is redacted because queries may contain private names.
#[derive(Clone, Eq, PartialEq)]
pub struct SearchInput(String);

impl SearchInput {
    /// Captures text for the dedicated search parser boundary.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrows the text only for parsing; services must consume a validated AST instead.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SearchInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SearchInput")
            .field("utf8_byte_count", &self.0.len())
            .finish()
    }
}

/// Clipboard or drag request represented without apartment-affine `IDataObject` values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataTransferRequest {
    Copy {
        items: Vec<ItemDescriptor>,
    },
    Cut {
        items: Vec<ItemDescriptor>,
    },
    Paste {
        destination: LocationDescriptor,
        conflict: ConflictDecision,
    },
    BeginDrag {
        items: Vec<ItemDescriptor>,
        allowed_effects: TransferEffects,
        button: crate::DragButton,
    },
    DropExternal {
        sources: Vec<LocationDescriptor>,
        destination: LocationDescriptor,
        effect: crate::DragEffect,
        conflict: ConflictDecision,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardMode {
    Copy,
    Cut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransferEffects {
    pub copy: bool,
    pub move_item: bool,
    pub link: bool,
}

impl TransferEffects {
    pub const COPY: Self = Self {
        copy: true,
        move_item: false,
        link: false,
    };
    pub const MOVE: Self = Self {
        copy: false,
        move_item: true,
        link: false,
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardState {
    None {
        generation: u64,
    },
    Owned {
        mode: ClipboardMode,
        items: Vec<ItemDescriptor>,
        effects: TransferEffects,
        generation: u64,
    },
    External {
        effects: TransferEffects,
        item_count: Option<usize>,
        generation: u64,
    },
    Unsupported {
        error: ExplorerError,
        generation: u64,
    },
}

impl Default for ClipboardState {
    fn default() -> Self {
        Self::None { generation: 0 }
    }
}

/// Commands accepted by the application service boundary.
#[derive(Clone, Debug)]
pub enum ExplorerCommand {
    Navigate {
        context: RequestContext,
        location: LocationDescriptor,
    },
    Refresh {
        context: RequestContext,
        location: LocationDescriptor,
    },
    ResolveAncestry {
        context: RequestContext,
        location: LocationDescriptor,
    },
    EnumerateChildContainers {
        context: RequestContext,
        parent: LocationDescriptor,
        segment_id: BreadcrumbSegmentId,
        menu_generation: u64,
    },
    OpenItem {
        context: RequestContext,
        item: ItemDescriptor,
        disposition: OpenDisposition,
    },
    Cancel {
        request_id: RequestId,
    },
    ExecuteFileOperation {
        context: RequestContext,
        request: FileOperationRequest,
    },
    ShowContextMenu {
        context: RequestContext,
        request: crate::ContextMenuRequest,
    },
    StartSearch {
        context: RequestContext,
        location: LocationDescriptor,
        input: SearchInput,
    },
    DataTransfer {
        context: RequestContext,
        request: DataTransferRequest,
    },
    LoadShellIcon {
        context: RequestContext,
        key: ShellIconKey,
    },
    LoadThumbnail {
        context: RequestContext,
        key: crate::ThumbnailRequestKey,
        location: LocationDescriptor,
        cache_only: bool,
    },
    ClearThumbnailCache {
        context: RequestContext,
    },
    PreviewHost {
        context: RequestContext,
        command: crate::PreviewHostCommand,
    },
    DiscoverLockOwners {
        context: RequestContext,
        request: crate::LockOwnerDiscoveryRequest,
    },
    CloseLockOwners {
        context: RequestContext,
        request: crate::LockOwnerCloseRequest,
    },
}

impl ExplorerCommand {
    /// Returns the new request context, excluding a cancellation command for an existing request.
    pub const fn context(&self) -> Option<&RequestContext> {
        match self {
            Self::Navigate { context, .. }
            | Self::Refresh { context, .. }
            | Self::ResolveAncestry { context, .. }
            | Self::EnumerateChildContainers { context, .. }
            | Self::OpenItem { context, .. }
            | Self::ExecuteFileOperation { context, .. }
            | Self::ShowContextMenu { context, .. }
            | Self::StartSearch { context, .. }
            | Self::DataTransfer { context, .. }
            | Self::LoadShellIcon { context, .. }
            | Self::LoadThumbnail { context, .. }
            | Self::ClearThumbnailCache { context }
            | Self::PreviewHost { context, .. }
            | Self::DiscoverLockOwners { context, .. }
            | Self::CloseLockOwners { context, .. } => Some(context),
            Self::Cancel { .. } => None,
        }
    }
}

/// Resolved display/capability metadata published before directory enumeration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocationMetadata {
    pub descriptor: LocationDescriptor,
    pub display_title: String,
    pub can_go_up: bool,
    pub can_write: bool,
}

/// Watcher or reconciliation mutation expressed with stable identities.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "directory deltas keep owned FileEntry values inline on the bounded batch boundary"
)]
pub enum DirectoryDelta {
    Upsert(FileEntry),
    Remove(ShellItemId),
    Overflow,
}

/// Coalescible operation progress payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationProgress {
    pub completed_items: usize,
    pub total_items: usize,
    pub completed_bytes: u64,
    pub total_bytes: Option<u64>,
    pub phase: TransferProgressPhase,
    pub current_item: Option<String>,
}

/// User-visible phase of a file transfer. Terminal completion remains represented by
/// `OperationTerminal`, so an in-flight progress event never implies success.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferProgressPhase {
    Preparing,
    Transferring,
    Finalizing,
}

/// Exactly-one terminal operation result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationTerminal {
    Finished,
    Cancelled,
    Partial { outcomes: Vec<OperationItemOutcome> },
    Failed(ExplorerError),
}

/// Exactly-one terminal search result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchTerminal {
    Finished,
    Cancelled,
    Partial(ExplorerError),
    Failed(ExplorerError),
}

/// Exactly-one breadcrumb request result. Partial failures retain already delivered batches.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BreadcrumbTerminal {
    Finished,
    Empty,
    Cancelled,
    Partial(ExplorerError),
    Failed(ExplorerError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchBackend {
    Everything,
    LocalIndex,
    WindowsIndex,
    FileSystemFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchSourcePhase {
    Indexed,
    Active,
    Complete,
    Partial,
    Unavailable,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchSourceStatus {
    pub backend: SearchBackend,
    pub phase: SearchSourcePhase,
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApkInstallStatus {
    Started,
    Succeeded,
    Failed { message: String },
    Cancelled,
    TimedOut,
}

impl ApkInstallStatus {
    pub const fn is_terminal(&self) -> bool {
        !matches!(self, Self::Started)
    }
}

pub fn normalize_apk_notice_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect()
}

/// Events emitted by fake and Windows service implementations.
#[derive(Clone, Debug)]
pub enum ExplorerEvent {
    LocationResolved {
        context: RequestContext,
        metadata: LocationMetadata,
    },
    DirectoryBatch {
        context: RequestContext,
        entries: Vec<FileEntry>,
    },
    AncestryBatch {
        context: RequestContext,
        segments: Vec<BreadcrumbSegment>,
    },
    AncestryFinished {
        context: RequestContext,
        outcome: BreadcrumbTerminal,
    },
    ChildContainersBatch {
        context: RequestContext,
        segment_id: BreadcrumbSegmentId,
        menu_generation: u64,
        children: Vec<BreadcrumbMenuItem>,
    },
    ChildContainersFinished {
        context: RequestContext,
        segment_id: BreadcrumbSegmentId,
        menu_generation: u64,
        outcome: BreadcrumbTerminal,
    },
    DirectoryChanged {
        tab_id: TabId,
        generation: Generation,
        changes: Vec<DirectoryDelta>,
    },
    OperationProgress {
        context: RequestContext,
        progress: OperationProgress,
    },
    SearchBatch {
        context: RequestContext,
        source: SearchBackend,
        entries: Vec<FileEntry>,
    },
    SearchStatus {
        context: RequestContext,
        status: SearchSourceStatus,
    },
    DirectoryFinished {
        context: RequestContext,
    },
    OperationFinished {
        context: RequestContext,
        outcome: OperationTerminal,
    },
    ClipboardChanged {
        state: ClipboardState,
    },
    ContextMenuFinished {
        context: RequestContext,
        outcome: crate::ContextMenuOutcome,
    },
    ApkInstallStatus {
        context: RequestContext,
        apk_name: String,
        device_name: String,
        serial: String,
        status: ApkInstallStatus,
    },
    SearchFinished {
        context: RequestContext,
        outcome: SearchTerminal,
    },
    ShellIconLoaded {
        context: RequestContext,
        payload: ShellIconPayload,
    },
    ShellIconFailed {
        context: RequestContext,
        key: ShellIconKey,
        reason: ShellIconFallbackReason,
    },
    ThumbnailFinished {
        context: RequestContext,
        key: crate::ThumbnailRequestKey,
        outcome: crate::ThumbnailTerminal,
    },
    ThumbnailCacheCleared {
        context: RequestContext,
        success: bool,
    },
    PreviewHostFinished {
        context: RequestContext,
        outcome: crate::PreviewHostTerminal,
    },
    LockOwnersDiscovered {
        context: RequestContext,
        outcome: crate::LockOwnerDiscoveryTerminal,
    },
    LockOwnersClosed {
        context: RequestContext,
        outcome: crate::LockOwnerCloseTerminal,
    },
    Failed {
        context: RequestContext,
        error: ExplorerError,
    },
}

impl ExplorerEvent {
    /// Returns request correlation for request-scoped events.
    pub const fn context(&self) -> Option<&RequestContext> {
        match self {
            Self::LocationResolved { context, .. }
            | Self::DirectoryBatch { context, .. }
            | Self::AncestryBatch { context, .. }
            | Self::AncestryFinished { context, .. }
            | Self::ChildContainersBatch { context, .. }
            | Self::ChildContainersFinished { context, .. }
            | Self::OperationProgress { context, .. }
            | Self::SearchBatch { context, .. }
            | Self::SearchStatus { context, .. }
            | Self::DirectoryFinished { context }
            | Self::OperationFinished { context, .. }
            | Self::ContextMenuFinished { context, .. }
            | Self::ApkInstallStatus { context, .. }
            | Self::SearchFinished { context, .. }
            | Self::ShellIconLoaded { context, .. }
            | Self::ShellIconFailed { context, .. }
            | Self::ThumbnailFinished { context, .. }
            | Self::ThumbnailCacheCleared { context, .. }
            | Self::PreviewHostFinished { context, .. }
            | Self::LockOwnersDiscovered { context, .. }
            | Self::LockOwnersClosed { context, .. }
            | Self::Failed { context, .. } => Some(context),
            Self::DirectoryChanged { .. } | Self::ClipboardChanged { .. } => None,
        }
    }

    /// Returns whether this event completes its request.
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::ApkInstallStatus { status, .. } if status.is_terminal())
            || matches!(
                self,
                Self::DirectoryFinished { .. }
                    | Self::AncestryFinished { .. }
                    | Self::ChildContainersFinished { .. }
                    | Self::OperationFinished { .. }
                    | Self::ContextMenuFinished { .. }
                    | Self::SearchFinished { .. }
                    | Self::ShellIconLoaded { .. }
                    | Self::ShellIconFailed { .. }
                    | Self::ThumbnailFinished { .. }
                    | Self::ThumbnailCacheCleared { .. }
                    | Self::PreviewHostFinished { .. }
                    | Self::LockOwnersDiscovered { .. }
                    | Self::LockOwnersClosed { .. }
                    | Self::Failed { .. }
            )
    }
}

/// Tracks request starts and rejects missing/duplicate terminal semantics.
#[derive(Debug, Default)]
pub struct TerminalLedger {
    active: HashSet<RequestId>,
    completed: HashSet<RequestId>,
}

impl TerminalLedger {
    /// Registers one newly dispatched request command.
    ///
    /// # Errors
    ///
    /// Rejects duplicate active or already-completed request identities.
    pub fn register(&mut self, command: &ExplorerCommand) -> Result<(), TerminalViolation> {
        let Some(context) = command.context() else {
            return Ok(());
        };
        if self.active.contains(&context.request_id) || self.completed.contains(&context.request_id)
        {
            return Err(TerminalViolation::DuplicateRequest(context.request_id));
        }
        self.active.insert(context.request_id);
        Ok(())
    }

    /// Records a terminal event and removes its outstanding request.
    ///
    /// # Errors
    ///
    /// Rejects non-terminal calls and unknown or duplicate terminal events.
    pub fn record_terminal(&mut self, event: &ExplorerEvent) -> Result<(), TerminalViolation> {
        if !event.is_terminal() {
            return Err(TerminalViolation::NotTerminal);
        }
        let context = event.context().ok_or(TerminalViolation::NotTerminal)?;
        if !self.active.remove(&context.request_id) {
            return Err(TerminalViolation::UnknownOrDuplicateTerminal(
                context.request_id,
            ));
        }
        self.completed.insert(context.request_id);
        Ok(())
    }

    /// Validates and records either an intermediate or terminal event.
    ///
    /// # Errors
    ///
    /// Rejects events for unknown requests and duplicate terminal events.
    pub fn record_event(&mut self, event: &ExplorerEvent) -> Result<(), TerminalViolation> {
        if event.is_terminal() {
            return self.record_terminal(event);
        }
        let Some(context) = event.context() else {
            return Ok(());
        };
        if self.active.contains(&context.request_id) {
            Ok(())
        } else {
            Err(TerminalViolation::UnknownEvent(context.request_id))
        }
    }

    /// Verifies that shutdown or a test left no request without a terminal event.
    ///
    /// # Errors
    ///
    /// Reports the number of requests still awaiting a terminal event.
    pub fn verify_drained(&self) -> Result<(), TerminalViolation> {
        if self.active.is_empty() {
            Ok(())
        } else {
            Err(TerminalViolation::Outstanding(self.active.len()))
        }
    }
}

/// Protocol lifecycle invariant violation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalViolation {
    DuplicateRequest(RequestId),
    UnknownEvent(RequestId),
    UnknownOrDuplicateTerminal(RequestId),
    NotTerminal,
    Outstanding(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon_entry(id: u8, path: &str, is_container: bool) -> FileEntry {
        FileEntry {
            id: ShellItemId::from_provider_bytes([id]).expect("identity"),
            display_name: path.to_owned(),
            location: LocationDescriptor::file_system(path),
            is_container,
            metadata: crate::FileEntryMetadata::default(),
        }
    }

    #[test]
    fn base_icon_classifier_shares_normal_classes_and_preserves_special_identity() {
        let jpg_upper = icon_entry(1, r"C:\fixture\one.JPG", false);
        let jpg_lower = icon_entry(2, r"C:\fixture\two.jpg", false);
        assert_eq!(
            classify_base_icon(&jpg_upper),
            classify_base_icon(&jpg_lower)
        );
        assert_eq!(
            classify_base_icon(&icon_entry(3, r"C:\fixture\a", true)),
            BaseIconClass::Folder
        );
        assert_eq!(
            classify_base_icon(&icon_entry(4, r"C:\fixture\README", false)),
            BaseIconClass::ExtensionlessFile
        );

        for (id, extension) in ["exe", "dll", "ico", "lnk", "url", "cpl"]
            .into_iter()
            .enumerate()
        {
            let entry = icon_entry(
                u8::try_from(id + 10).expect("small fixture id"),
                &format!(r"C:\fixture\special.{extension}"),
                false,
            );
            assert!(matches!(
                classify_base_icon(&entry),
                BaseIconClass::Identity(_)
            ));
        }

        let drive = icon_entry(30, r"C:\", true);
        assert!(matches!(
            classify_base_icon(&drive),
            BaseIconClass::Identity(_)
        ));
        for (id, location) in [
            LocationDescriptor::KnownFolder([1; 16]),
            LocationDescriptor::ShellNamespace(vec![1, 2, 3]),
            LocationDescriptor::ParsingName("shell:Downloads".to_owned()),
        ]
        .into_iter()
        .enumerate()
        {
            let mut entry = icon_entry(u8::try_from(id + 40).expect("small fixture id"), "x", true);
            entry.location = location;
            assert!(matches!(
                classify_base_icon(&entry),
                BaseIconClass::Identity(_)
            ));
        }

        let virtual_location = |entry_id, components: Vec<&str>| {
            LocationDescriptor::try_virtual(
                "archive",
                [7; 16],
                1,
                Some(entry_id),
                components.into_iter().map(str::to_owned).collect(),
            )
            .expect("virtual location")
        };
        let mut folder = icon_entry(60, "folder", true);
        folder.location = virtual_location(1, vec!["nested"]);
        assert_eq!(classify_base_icon(&folder), BaseIconClass::Folder);
        let mut text = icon_entry(61, "hello.TXT", false);
        text.location = virtual_location(2, vec!["nested", "hello.TXT"]);
        assert_eq!(
            classify_base_icon(&text),
            BaseIconClass::Extension("txt".to_owned())
        );
    }

    #[test]
    fn icon_epochs_are_independent_from_each_other_and_navigation() {
        let mut epochs = IconInvalidationEpochs::default();
        let baseline = epochs;
        let entry = icon_entry(70, r"C:\fixture\one.txt", false);
        let before_navigation = base_icon_key(&entry, 20, 96, ShellIconTheme::Light, 4);
        let after_navigation = base_icon_key(&entry, 20, 96, ShellIconTheme::Light, 4);
        assert_eq!(before_navigation, after_navigation);
        assert_ne!(
            before_navigation,
            base_icon_key(&entry, 20, 96, ShellIconTheme::Light, 5)
        );
        assert_eq!(epochs.advance_overlay(), 1);
        assert_eq!(epochs.association(), baseline.association());
        assert_eq!(epochs.advance_association(), 1);
        assert_eq!(epochs.overlay(), 1);
    }

    #[test]
    fn overlay_refresh_advances_past_every_observed_item_epoch() {
        let mut epochs = IconInvalidationEpochs::default();
        assert_eq!(epochs.advance_overlay_past(41), 42);
        assert_eq!(epochs.overlay(), 42);
        assert_eq!(epochs.association(), 0);
        assert_eq!(epochs.advance_overlay_past(7), 43);
        assert_eq!(epochs.association(), 0);
    }

    fn context() -> RequestContext {
        RequestContext::new(TabId::new(), Generation::new(1))
    }

    #[test]
    fn search_debug_never_exposes_query_text() {
        let input = SearchInput::new("private customer name");
        assert_eq!(input.as_str(), "private customer name");
        assert!(!format!("{input:?}").contains("private customer name"));
    }

    #[test]
    fn apk_install_status_distinguishes_started_and_every_terminal() {
        assert!(!ApkInstallStatus::Started.is_terminal());
        assert!(ApkInstallStatus::Succeeded.is_terminal());
        assert!(
            ApkInstallStatus::Failed {
                message: "x".to_owned()
            }
            .is_terminal()
        );
        assert!(ApkInstallStatus::Cancelled.is_terminal());
        assert!(ApkInstallStatus::TimedOut.is_terminal());
        assert_eq!(normalize_apk_notice_text("qq\n9.apk", 6), "qq9.ap");
    }

    #[test]
    fn apk_install_event_terminal_semantics_follow_its_status() {
        let context = context();
        let make = |status| ExplorerEvent::ApkInstallStatus {
            context: context.clone(),
            apk_name: "qq9.3.55.apk".to_owned(),
            device_name: "Pixel 9".to_owned(),
            serial: "emulator-5554".to_owned(),
            status,
        };
        assert!(!make(ApkInstallStatus::Started).is_terminal());
        assert!(make(ApkInstallStatus::Succeeded).is_terminal());
    }

    #[test]
    fn shell_icon_payload_rejects_invalid_dimensions_stride_and_buffer() {
        let key = ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(r"D:\"),
            size_bucket: 20,
            dpi: 168,
            theme: ShellIconTheme::Light,
            association_generation: 1,
            overlay_generation: 1,
        };
        assert_eq!(
            ShellIconPayload::new(key.clone(), 0, 20, 80, vec![], None),
            Err(ShellIconPayloadError::ZeroDimension)
        );
        assert_eq!(
            ShellIconPayload::new(key.clone(), 20, 20, 79, vec![0; 1_580], None),
            Err(ShellIconPayloadError::InvalidStride)
        );
        assert_eq!(
            ShellIconPayload::new(key, 20, 20, 80, vec![0; 1_599], None),
            Err(ShellIconPayloadError::InvalidBufferLength)
        );
    }

    #[test]
    fn bc7_payload_rejects_incomplete_rows_and_preserves_kind() {
        let valid = Bc7RasterPayload {
            kind: CompressedRasterKind::Icon,
            width: 5,
            height: 7,
            padded_width: 8,
            padded_height: 8,
            row_pitch: 32,
            blocks: vec![0; 64],
        };
        assert!(valid.validate(64));
        let mut truncated = valid.clone();
        truncated.blocks.pop();
        assert!(!truncated.validate(64));
        assert_eq!(valid.kind, CompressedRasterKind::Icon);
    }

    #[test]
    fn breadcrumb_commands_batches_and_terminals_share_one_correlation_contract() {
        let ancestry_context = context();
        let menu_context =
            RequestContext::new(ancestry_context.tab_id, ancestry_context.generation);
        let ancestry = ExplorerCommand::ResolveAncestry {
            context: ancestry_context.clone(),
            location: LocationDescriptor::file_system(r"D:\fixture\nested"),
        };
        let menu = ExplorerCommand::EnumerateChildContainers {
            context: menu_context.clone(),
            parent: LocationDescriptor::file_system(r"D:\fixture"),
            segment_id: BreadcrumbSegmentId(7),
            menu_generation: 3,
        };
        let mut ledger = TerminalLedger::default();
        ledger.register(&ancestry).expect("ancestry registers");
        ledger.register(&menu).expect("menu registers");
        ledger
            .record_event(&ExplorerEvent::AncestryBatch {
                context: ancestry_context.clone(),
                segments: Vec::new(),
            })
            .expect("ancestry batch is intermediate");
        ledger
            .record_event(&ExplorerEvent::ChildContainersBatch {
                context: menu_context.clone(),
                segment_id: BreadcrumbSegmentId(7),
                menu_generation: 3,
                children: Vec::new(),
            })
            .expect("menu batch is intermediate");
        ledger
            .record_event(&ExplorerEvent::AncestryFinished {
                context: ancestry_context,
                outcome: BreadcrumbTerminal::Finished,
            })
            .expect("ancestry terminal");
        ledger
            .record_event(&ExplorerEvent::ChildContainersFinished {
                context: menu_context,
                segment_id: BreadcrumbSegmentId(7),
                menu_generation: 3,
                outcome: BreadcrumbTerminal::Empty,
            })
            .expect("menu terminal");
        assert_eq!(ledger.verify_drained(), Ok(()));
    }

    #[test]
    fn shell_icon_key_changes_for_dpi_theme_association_and_overlay_generation() {
        let base = ShellIconKey {
            item_id: None,
            location: LocationDescriptor::file_system(r"D:\"),
            size_bucket: 20,
            dpi: 96,
            theme: ShellIconTheme::Light,
            association_generation: 0,
            overlay_generation: 0,
        };
        let mut variants = HashSet::new();
        variants.insert(base.clone());
        variants.insert(ShellIconKey {
            dpi: 144,
            ..base.clone()
        });
        variants.insert(ShellIconKey {
            theme: ShellIconTheme::Dark,
            ..base.clone()
        });
        variants.insert(ShellIconKey {
            association_generation: 1,
            ..base.clone()
        });
        variants.insert(ShellIconKey {
            overlay_generation: 1,
            ..base
        });
        assert_eq!(variants.len(), 5);
    }

    #[test]
    fn shell_icon_request_uses_the_shared_cooperative_cancellation_context() {
        let context = context();
        let command = ExplorerCommand::LoadShellIcon {
            context: context.clone(),
            key: ShellIconKey {
                item_id: None,
                location: LocationDescriptor::file_system(r"D:\"),
                size_bucket: 32,
                dpi: 168,
                theme: ShellIconTheme::Light,
                association_generation: 0,
                overlay_generation: 0,
            },
        };
        context.cancellation.cancel();
        assert!(command.context().unwrap().cancellation.is_cancelled());
    }

    #[test]
    fn terminal_ledger_requires_exactly_one_terminal_event() {
        let context = context();
        let command = ExplorerCommand::Navigate {
            context: context.clone(),
            location: LocationDescriptor::file_system(r"C:\fixture"),
        };
        let mut ledger = TerminalLedger::default();
        assert_eq!(ledger.register(&command), Ok(()));
        assert_eq!(
            ledger.verify_drained(),
            Err(TerminalViolation::Outstanding(1))
        );
        let event = ExplorerEvent::DirectoryFinished {
            context: context.clone(),
        };
        assert_eq!(ledger.record_terminal(&event), Ok(()));
        assert_eq!(ledger.verify_drained(), Ok(()));
        assert_eq!(
            ledger.record_terminal(&event),
            Err(TerminalViolation::UnknownOrDuplicateTerminal(
                context.request_id
            ))
        );
    }

    #[test]
    fn cancellation_command_does_not_register_a_second_request() {
        let mut ledger = TerminalLedger::default();
        assert_eq!(
            ledger.register(&ExplorerCommand::Cancel {
                request_id: RequestId::new()
            }),
            Ok(())
        );
        assert_eq!(ledger.verify_drained(), Ok(()));
    }

    #[test]
    fn shell_new_recipes_are_bounded_owned_and_reject_unsafe_descriptors() {
        let valid = ShellNewItemDescriptor {
            stable_id: ".txt".to_owned(),
            display_name: "Text Document".to_owned(),
            extension: Some(".txt".to_owned()),
            default_stem: "New Text Document".to_owned(),
            recipe: ShellNewItemRecipe::Data(vec![1, 2, 3]),
        };
        assert_eq!(valid.validate(), Ok(()));
        let cloned = valid.clone();
        let ShellNewItemRecipe::Data(mut copied_bytes) = cloned.recipe else {
            panic!("owned data recipe");
        };
        copied_bytes[0] = 9;
        assert!(matches!(valid.recipe, ShellNewItemRecipe::Data(ref data) if data[0] == 1));

        let oversized = ShellNewItemDescriptor {
            recipe: ShellNewItemRecipe::Data(vec![0; ShellNewItemRecipe::MAXIMUM_DATA_BYTES + 1]),
            ..valid.clone()
        };
        assert_eq!(
            oversized.validate(),
            Err(ShellNewValidationError::OversizedData)
        );
        let relative_template = ShellNewItemDescriptor {
            recipe: ShellNewItemRecipe::TemplateFile("template.dotx".into()),
            ..valid.clone()
        };
        assert_eq!(
            relative_template.validate(),
            Err(ShellNewValidationError::UntrustedTemplate)
        );
        for invalid in [
            ShellNewItemDescriptor {
                stable_id: String::new(),
                ..valid.clone()
            },
            ShellNewItemDescriptor {
                extension: Some("txt".to_owned()),
                ..valid.clone()
            },
            ShellNewItemDescriptor {
                extension: Some(".bad\\name".to_owned()),
                ..valid
            },
        ] {
            assert!(invalid.validate().is_err());
        }
    }
}

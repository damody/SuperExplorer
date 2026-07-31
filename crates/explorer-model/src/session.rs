//! Versioned, bounded, I/O-free session persistence contracts.

use std::{collections::HashSet, fmt};

use explorer_common::RoadmapLimits;
use serde::{Deserialize, Serialize};

use crate::{
    DetailsColumnWidths, ExplorerWindowState, HistoryEntry, LocationDescriptor, SortColumn,
    SortDescriptor, SortDirection, TabId, ViewMode, ViewSettings,
};

/// Current durable session schema.
pub const SESSION_SCHEMA_VERSION: u16 = 1;
const MAX_PROVENANCE_BYTES: usize = 256;
const MAX_DISPLAY_TITLE_BYTES: usize = 4 * 1024;
const MAX_PIN_NAME_BYTES: usize = 4 * 1024;
const MAX_WINDOW_DIMENSION: i32 = 100_000;
const MIN_DPI: u32 = 48;
const MAX_DPI: u32 = 960;

/// Build and host provenance used to diagnose migration without storing user identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionProvenance {
    pub app_version: String,
    pub app_revision: String,
    pub windows_build: String,
}

/// Signed logical rectangle persisted independently from a live monitor or HWND.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedRect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

/// Main-window placement needed to reconstruct a reachable window.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedWindowPlacement {
    pub normal_bounds: PersistedRect,
    pub source_work_area: PersistedRect,
    pub source_dpi: u32,
    pub maximized: bool,
}

/// Stable persisted view modes; enum names are an explicit schema contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedViewMode {
    ExtraLargeIcons,
    LargeIcons,
    MediumIcons,
    SmallIcons,
    List,
    Details,
    Tiles,
    Content,
}

/// Stable persisted column identities.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedColumn {
    Name,
    DateModified,
    Type,
    Size,
    DateCreated,
    Authors,
    Tags,
    Title,
}

/// Stable persisted sort direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedSortDirection {
    Ascending,
    Descending,
}

/// Persisted sort descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedSort {
    pub column: PersistedColumn,
    pub direction: PersistedSortDirection,
}

/// Persisted widths for the current four Details columns.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedColumnWidths {
    pub name: u16,
    pub date_modified: u16,
    pub item_type: u16,
    pub size: u16,
    #[serde(
        default = "default_optional_column_width",
        skip_serializing_if = "is_default_optional_column_width"
    )]
    pub date_created: u16,
    #[serde(
        default = "default_optional_column_width",
        skip_serializing_if = "is_default_optional_column_width"
    )]
    pub authors: u16,
    #[serde(
        default = "default_optional_column_width",
        skip_serializing_if = "is_default_optional_column_width"
    )]
    pub tags: u16,
    #[serde(
        default = "default_title_column_width",
        skip_serializing_if = "is_default_title_column_width"
    )]
    pub title: u16,
}

const fn default_optional_column_width() -> u16 {
    150
}
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip predicates receive references"
)]
fn is_default_optional_column_width(value: &u16) -> bool {
    *value == default_optional_column_width()
}
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip predicates receive references"
)]
fn is_default_title_column_width(value: &u16) -> bool {
    *value == default_title_column_width()
}
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip predicates receive references"
)]
fn is_default_column_visibility(value: &u16) -> bool {
    *value == default_column_visibility()
}
const fn default_title_column_width() -> u16 {
    180
}
const fn default_column_visibility() -> u16 {
    0b1111
}

impl Default for PersistedColumnWidths {
    fn default() -> Self {
        Self {
            name: 280,
            date_modified: 180,
            item_type: 160,
            size: 120,
            date_created: 150,
            authors: 150,
            tags: 150,
            title: 180,
        }
    }
}

/// Explicit durable mapping of tab-local view settings.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent Explorer View menu toggles are intentionally serialized as explicit schema fields"
)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedViewSettings {
    pub mode: PersistedViewMode,
    pub details_pane: bool,
    pub preview_pane: bool,
    pub item_check_boxes: bool,
    pub file_name_extensions: bool,
    pub hidden_items: bool,
    pub compact_view: bool,
    #[serde(default)]
    pub always_show_icons: bool,
    pub sort: PersistedSort,
    pub group_by: Option<PersistedColumn>,
    pub details_column_order: Vec<PersistedColumn>,
    pub details_columns: PersistedColumnWidths,
    #[serde(
        default = "default_column_visibility",
        skip_serializing_if = "is_default_column_visibility"
    )]
    pub details_column_visibility: u16,
    pub details_pane_width: u16,
    pub preview_pane_width: u16,
}

impl Default for PersistedViewSettings {
    fn default() -> Self {
        Self {
            mode: PersistedViewMode::Details,
            details_pane: false,
            preview_pane: false,
            item_check_boxes: false,
            file_name_extensions: false,
            hidden_items: false,
            compact_view: false,
            always_show_icons: false,
            sort: PersistedSort {
                column: PersistedColumn::Name,
                direction: PersistedSortDirection::Ascending,
            },
            group_by: None,
            details_column_order: PersistedColumn::ALL.to_vec(),
            details_columns: PersistedColumnWidths::default(),
            details_column_visibility: default_column_visibility(),
            details_pane_width: 320,
            preview_pane_width: 360,
        }
    }
}

/// One reconstructible history location. Runtime selection and editor state are absent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedHistoryEntry {
    pub location: LocationDescriptor,
    pub display_title: String,
    pub anchor_item: Option<crate::ShellItemId>,
    pub anchor_offset_logical_pixels: i32,
}

/// Durable state for one tab.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedTab {
    pub tab_id: TabId,
    pub current: PersistedHistoryEntry,
    pub back: Vec<PersistedHistoryEntry>,
    pub forward: Vec<PersistedHistoryEntry>,
    pub view_settings: PersistedViewSettings,
}

/// Durable Quick Access entry with stable explicit order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedQuickAccessPin {
    pub location: LocationDescriptor,
    pub display_name: String,
    pub order: u32,
}

/// Coherent session data covered by one envelope checksum.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedSessionPayload {
    pub restore_enabled: bool,
    pub window: PersistedWindowPlacement,
    pub tabs: Vec<PersistedTab>,
    pub active_tab_id: TabId,
    pub quick_access: Vec<PersistedQuickAccessPin>,
}

/// Versioned top-level envelope stored atomically by the app adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedSessionEnvelope {
    pub schema_version: u16,
    pub checksum: u64,
    pub write_generation: u64,
    pub provenance: SessionProvenance,
    pub payload: PersistedSessionPayload,
}

/// A fully validated plan safe for asynchronous location reconstruction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestorePlan {
    pub window: PersistedWindowPlacement,
    pub tabs: Vec<PersistedTab>,
    pub active_tab_id: TabId,
    pub quick_access: Vec<PersistedQuickAccessPin>,
}

/// Origin selected by a session store after validation and recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionLoadSource {
    Current,
    LastKnownGood,
    Defaults,
}

/// Privacy-safe result of loading durable state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionLoadOutcome {
    pub source: SessionLoadSource,
    pub envelope: Option<PersistedSessionEnvelope>,
    pub rejected_artifacts: usize,
    pub migration_performed: bool,
}

/// User-visible reset scopes. None imply deleting unrelated application data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionResetScope {
    Session,
    ViewSettings,
    QuickAccess,
    AllRoadmapState,
}

/// Platform-neutral persistence boundary used by coordinators and model tests.
pub trait SessionStore: Send + Sync {
    /// Loads current, last-known-good, or default state.
    ///
    /// # Errors
    ///
    /// Returns a privacy-safe storage error when the backing store cannot be inspected.
    fn load(&self) -> Result<SessionLoadOutcome, SessionStoreError>;

    /// Atomically persists one already validated envelope.
    ///
    /// # Errors
    ///
    /// Returns a validation, access, capacity, or I/O error without exposing a path.
    fn save(&self, envelope: &PersistedSessionEnvelope) -> Result<(), SessionStoreError>;

    /// Clears or rewrites only the selected roadmap-owned state.
    ///
    /// # Errors
    ///
    /// Returns a privacy-safe error if the scoped reset cannot complete.
    fn reset(&self, scope: SessionResetScope) -> Result<(), SessionStoreError>;
}

/// Store error without paths, content, or operating-system user identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionStoreError {
    Unavailable(String),
    AccessDenied,
    StorageFull,
    InvalidSnapshot(String),
    Io(String),
}

impl fmt::Display for SessionStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(reason) => write!(formatter, "session store unavailable: {reason}"),
            Self::AccessDenied => formatter.write_str("session store access denied"),
            Self::StorageFull => formatter.write_str("session store is full"),
            Self::InvalidSnapshot(reason) => {
                write!(formatter, "invalid session snapshot: {reason}")
            }
            Self::Io(operation) => {
                write!(formatter, "session storage operation failed: {operation}")
            }
        }
    }
}

impl std::error::Error for SessionStoreError {}

impl PersistedSessionEnvelope {
    /// Projects durable runtime state and computes the current schema checksum.
    ///
    /// # Errors
    ///
    /// Returns a validation or serialization error when runtime state cannot form a bounded
    /// reconstructible snapshot.
    pub fn project(
        window: &ExplorerWindowState,
        placement: PersistedWindowPlacement,
        quick_access: &[PersistedQuickAccessPin],
        restore_enabled: bool,
        write_generation: u64,
        provenance: SessionProvenance,
        limits: RoadmapLimits,
    ) -> Result<Self, SessionValidationError> {
        let tabs = window
            .tabs()
            .iter()
            .map(|tab| {
                let current = tab.history.current().ok_or_else(|| {
                    SessionValidationError::Invariant("tab has no current history entry".to_owned())
                })?;
                Ok(PersistedTab {
                    tab_id: tab.id,
                    current: PersistedHistoryEntry::from(current),
                    back: tab
                        .history
                        .back_entries()
                        .iter()
                        .map(PersistedHistoryEntry::from)
                        .collect(),
                    forward: tab
                        .history
                        .forward_entries()
                        .iter()
                        .map(PersistedHistoryEntry::from)
                        .collect(),
                    view_settings: PersistedViewSettings::from(tab.view.settings),
                })
            })
            .collect::<Result<Vec<_>, SessionValidationError>>()?;
        let payload = PersistedSessionPayload {
            restore_enabled,
            window: placement,
            tabs,
            active_tab_id: window.active_tab_id(),
            quick_access: quick_access.to_vec(),
        };
        Self::new(write_generation, provenance, payload, limits)
    }

    /// Creates and validates a current-version envelope.
    ///
    /// # Errors
    ///
    /// Returns a named invariant, bound, schema, or serialization error.
    pub fn new(
        write_generation: u64,
        provenance: SessionProvenance,
        payload: PersistedSessionPayload,
        limits: RoadmapLimits,
    ) -> Result<Self, SessionValidationError> {
        let mut envelope = Self {
            schema_version: SESSION_SCHEMA_VERSION,
            checksum: 0,
            write_generation,
            provenance,
            payload,
        };
        envelope.validate_without_checksum(limits)?;
        envelope.checksum = envelope.calculate_checksum()?;
        Ok(envelope)
    }

    /// Encodes a validated deterministic pretty-JSON snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when validation, checksum verification, or serialization fails.
    pub fn encode_pretty(&self, limits: RoadmapLimits) -> Result<Vec<u8>, SessionValidationError> {
        self.validate(limits)?;
        let bytes = serde_json::to_vec_pretty(self).map_err(SessionValidationError::json)?;
        if bytes.len() > limits.max_state_payload_bytes {
            return Err(SessionValidationError::PayloadTooLarge {
                bytes: bytes.len(),
                maximum: limits.max_state_payload_bytes,
            });
        }
        Ok(bytes)
    }

    /// Decodes and validates a complete current-version snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized, malformed, unsupported, corrupt, or invalid state.
    pub fn decode(bytes: &[u8], limits: RoadmapLimits) -> Result<Self, SessionValidationError> {
        if bytes.len() > limits.max_state_payload_bytes {
            return Err(SessionValidationError::PayloadTooLarge {
                bytes: bytes.len(),
                maximum: limits.max_state_payload_bytes,
            });
        }
        let envelope: Self = serde_json::from_slice(bytes).map_err(SessionValidationError::json)?;
        envelope.validate(limits)?;
        Ok(envelope)
    }

    /// Decodes the current schema or applies one registered prior-version migration.
    ///
    /// # Errors
    ///
    /// Returns an error for oversized, malformed, unsupported, corrupt, or invalid state.
    pub fn decode_or_migrate(
        bytes: &[u8],
        limits: RoadmapLimits,
    ) -> Result<(Self, bool), SessionValidationError> {
        if bytes.len() > limits.max_state_payload_bytes {
            return Err(SessionValidationError::PayloadTooLarge {
                bytes: bytes.len(),
                maximum: limits.max_state_payload_bytes,
            });
        }
        let header: SessionSchemaHeader =
            serde_json::from_slice(bytes).map_err(SessionValidationError::json)?;
        match header.schema_version {
            SESSION_SCHEMA_VERSION => Self::decode(bytes, limits).map(|value| (value, false)),
            0 => {
                let legacy: LegacySessionV0 =
                    serde_json::from_slice(bytes).map_err(SessionValidationError::json)?;
                if legacy.schema_version != 0 {
                    return Err(SessionValidationError::UnsupportedSchema(
                        legacy.schema_version,
                    ));
                }
                let migrated = Self::new(
                    legacy.write_generation.saturating_add(1),
                    legacy.provenance,
                    legacy.payload,
                    limits,
                )?;
                Ok((migrated, true))
            }
            version => Err(SessionValidationError::UnsupportedSchema(version)),
        }
    }

    /// Converts validated durable state into an owned restore plan.
    ///
    /// # Errors
    ///
    /// Returns an error when the envelope no longer satisfies current bounds or checksum.
    pub fn restore_plan(
        &self,
        limits: RoadmapLimits,
    ) -> Result<RestorePlan, SessionValidationError> {
        self.validate(limits)?;
        Ok(RestorePlan {
            window: self.payload.window,
            tabs: self.payload.tabs.clone(),
            active_tab_id: self.payload.active_tab_id,
            quick_access: self.payload.quick_access.clone(),
        })
    }

    fn validate(&self, limits: RoadmapLimits) -> Result<(), SessionValidationError> {
        self.validate_without_checksum(limits)?;
        let actual = self.calculate_checksum()?;
        if self.checksum != actual {
            return Err(SessionValidationError::ChecksumMismatch {
                expected: self.checksum,
                actual,
            });
        }
        Ok(())
    }

    fn validate_without_checksum(
        &self,
        limits: RoadmapLimits,
    ) -> Result<(), SessionValidationError> {
        limits
            .validate()
            .map_err(|error| SessionValidationError::Limits(error.to_string()))?;
        if self.schema_version != SESSION_SCHEMA_VERSION {
            return Err(SessionValidationError::UnsupportedSchema(
                self.schema_version,
            ));
        }
        validate_provenance(&self.provenance)?;
        validate_rect(self.payload.window.normal_bounds, "window.normal_bounds")?;
        validate_rect(
            self.payload.window.source_work_area,
            "window.source_work_area",
        )?;
        if !(MIN_DPI..=MAX_DPI).contains(&self.payload.window.source_dpi) {
            return Err(SessionValidationError::InvalidField {
                field: "window.source_dpi".to_owned(),
                reason: "DPI is outside the supported reconstruction range".to_owned(),
            });
        }
        if self.payload.tabs.is_empty() {
            return Err(SessionValidationError::Invariant(
                "session must contain at least one tab".to_owned(),
            ));
        }
        if self.payload.tabs.len() > limits.max_tabs {
            return Err(SessionValidationError::BoundExceeded {
                field: "tabs".to_owned(),
                value: self.payload.tabs.len(),
                maximum: limits.max_tabs,
            });
        }
        let mut tab_ids = HashSet::new();
        for (index, tab) in self.payload.tabs.iter().enumerate() {
            if !tab_ids.insert(tab.tab_id) {
                return Err(SessionValidationError::Invariant(
                    "session contains duplicate tab identities".to_owned(),
                ));
            }
            validate_history_entry(&tab.current, &format!("tabs[{index}].current"), limits)?;
            validate_history(&tab.back, &format!("tabs[{index}].back"), limits)?;
            validate_history(&tab.forward, &format!("tabs[{index}].forward"), limits)?;
            validate_view_settings(
                &tab.view_settings,
                &format!("tabs[{index}].view_settings"),
                limits,
            )?;
        }
        if !tab_ids.contains(&self.payload.active_tab_id) {
            return Err(SessionValidationError::Invariant(
                "active tab identity is not present".to_owned(),
            ));
        }
        let mut pin_identities = HashSet::new();
        let mut pin_orders = HashSet::new();
        for (index, pin) in self.payload.quick_access.iter().enumerate() {
            validate_location(
                &pin.location,
                &format!("quick_access[{index}].location"),
                limits,
            )?;
            validate_text(
                &pin.display_name,
                &format!("quick_access[{index}].display_name"),
                MAX_PIN_NAME_BYTES,
            )?;
            if !pin_identities.insert(pin.location.clone()) || !pin_orders.insert(pin.order) {
                return Err(SessionValidationError::Invariant(
                    "Quick Access contains duplicate location or order".to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn calculate_checksum(&self) -> Result<u64, SessionValidationError> {
        let bytes = serde_json::to_vec(&(
            self.schema_version,
            self.write_generation,
            &self.provenance,
            &self.payload,
        ))
        .map_err(SessionValidationError::json)?;
        Ok(fnv1a64(&bytes))
    }
}

impl RestorePlan {
    /// Resolves saved locations, drops stale history entries, and creates tab shells in saved order.
    ///
    /// The resolver may return a canonical replacement entry. A missing current location walks
    /// filesystem ancestors before falling back to the configured start location.
    ///
    /// # Errors
    ///
    /// Returns a tab invariant error if reconstructed identities are empty, duplicate, or invalid.
    pub fn resolve_window(
        &self,
        configured_start: HistoryEntry,
        mut resolve: impl FnMut(&LocationDescriptor) -> Option<HistoryEntry>,
    ) -> Result<ExplorerWindowState, crate::TabStateInvariantError> {
        let mut tabs = Vec::with_capacity(self.tabs.len());
        for persisted in &self.tabs {
            let mut current = resolve_with_ancestors(&persisted.current.location, &mut resolve)
                .unwrap_or_else(|| configured_start.clone());
            apply_saved_presentation(&persisted.current, &mut current);
            let back = persisted
                .back
                .iter()
                .filter_map(|saved| {
                    let mut entry = resolve(&saved.location)?;
                    apply_saved_presentation(saved, &mut entry);
                    Some(entry)
                })
                .collect();
            let forward = persisted
                .forward
                .iter()
                .filter_map(|saved| {
                    let mut entry = resolve(&saved.location)?;
                    apply_saved_presentation(saved, &mut entry);
                    Some(entry)
                })
                .collect();
            let history = crate::NavigationHistory::from_resolved_parts(back, current, forward);
            if let Some(tab) = crate::TabState::from_restored(
                persisted.tab_id,
                history,
                persisted.view_settings.to_runtime(),
            ) {
                tabs.push(tab);
            }
        }
        if tabs.is_empty() {
            tabs.push(crate::TabState::new(configured_start.clone()));
        }
        let active = tabs
            .iter()
            .find(|tab| tab.id == self.active_tab_id)
            .map_or(tabs[0].id, |tab| tab.id);
        ExplorerWindowState::from_restored_tabs(tabs, active, configured_start)
    }
}

impl PersistedViewSettings {
    /// Converts explicit schema fields into the runtime view representation.
    pub fn to_runtime(&self) -> ViewSettings {
        ViewSettings {
            mode: self.mode.into(),
            icon_size: crate::default_icon_size_for_mode(self.mode.into()),
            details_pane: self.details_pane,
            preview_pane: self.preview_pane,
            item_check_boxes: self.item_check_boxes,
            file_name_extensions: self.file_name_extensions,
            hidden_items: self.hidden_items,
            compact_view: self.compact_view,
            always_show_icons: self.always_show_icons,
            sort: self.sort.into(),
            details_columns: self.details_columns.into(),
            details_column_visibility: self.details_column_visibility | SortColumn::Name.bit(),
            details_pane_width: self.details_pane_width,
            preview_pane_width: self.preview_pane_width,
        }
    }
}

impl From<PersistedViewMode> for ViewMode {
    fn from(value: PersistedViewMode) -> Self {
        match value {
            PersistedViewMode::ExtraLargeIcons => Self::ExtraLargeIcons,
            PersistedViewMode::LargeIcons => Self::LargeIcons,
            PersistedViewMode::MediumIcons => Self::MediumIcons,
            PersistedViewMode::SmallIcons => Self::SmallIcons,
            PersistedViewMode::List => Self::List,
            PersistedViewMode::Details => Self::Details,
            PersistedViewMode::Tiles => Self::Tiles,
            PersistedViewMode::Content => Self::Content,
        }
    }
}

impl From<PersistedSort> for SortDescriptor {
    fn from(value: PersistedSort) -> Self {
        Self {
            column: match value.column {
                PersistedColumn::Name => SortColumn::Name,
                PersistedColumn::DateModified => SortColumn::DateModified,
                PersistedColumn::Type => SortColumn::Type,
                PersistedColumn::Size => SortColumn::Size,
                PersistedColumn::DateCreated => SortColumn::DateCreated,
                PersistedColumn::Authors => SortColumn::Authors,
                PersistedColumn::Tags => SortColumn::Tags,
                PersistedColumn::Title => SortColumn::Title,
            },
            direction: match value.direction {
                PersistedSortDirection::Ascending => SortDirection::Ascending,
                PersistedSortDirection::Descending => SortDirection::Descending,
            },
        }
    }
}

impl From<PersistedColumnWidths> for DetailsColumnWidths {
    fn from(value: PersistedColumnWidths) -> Self {
        Self {
            name: value.name,
            date_modified: value.date_modified,
            item_type: value.item_type,
            size: value.size,
            date_created: value.date_created,
            authors: value.authors,
            tags: value.tags,
            title: value.title,
        }
    }
}

fn resolve_with_ancestors(
    location: &LocationDescriptor,
    resolve: &mut impl FnMut(&LocationDescriptor) -> Option<HistoryEntry>,
) -> Option<HistoryEntry> {
    if let Some(entry) = resolve(location) {
        return Some(entry);
    }
    let mut path = location.path()?.parent();
    while let Some(ancestor) = path {
        let descriptor = LocationDescriptor::file_system(ancestor.to_path_buf());
        if let Some(entry) = resolve(&descriptor) {
            return Some(entry);
        }
        path = ancestor.parent();
    }
    None
}

fn apply_saved_presentation(saved: &PersistedHistoryEntry, resolved: &mut HistoryEntry) {
    if resolved.location != saved.location {
        return;
    }
    resolved.display_title.clone_from(&saved.display_title);
    resolved.view_anchor.item.clone_from(&saved.anchor_item);
    resolved.view_anchor.offset_logical_pixels = saved.anchor_offset_logical_pixels;
}

#[derive(Deserialize)]
struct SessionSchemaHeader {
    schema_version: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySessionV0 {
    schema_version: u16,
    write_generation: u64,
    provenance: SessionProvenance,
    payload: PersistedSessionPayload,
}

impl From<&HistoryEntry> for PersistedHistoryEntry {
    fn from(entry: &HistoryEntry) -> Self {
        Self {
            location: entry.location.clone(),
            display_title: entry.display_title.clone(),
            anchor_item: entry.view_anchor.item.clone(),
            anchor_offset_logical_pixels: entry.view_anchor.offset_logical_pixels,
        }
    }
}

impl From<ViewSettings> for PersistedViewSettings {
    fn from(settings: ViewSettings) -> Self {
        Self {
            mode: settings.mode.into(),
            details_pane: settings.details_pane,
            preview_pane: settings.preview_pane,
            item_check_boxes: settings.item_check_boxes,
            file_name_extensions: settings.file_name_extensions,
            hidden_items: settings.hidden_items,
            compact_view: settings.compact_view,
            always_show_icons: settings.always_show_icons,
            sort: settings.sort.into(),
            group_by: None,
            details_column_order: PersistedColumn::ALL.to_vec(),
            details_columns: settings.details_columns.into(),
            details_column_visibility: settings.details_column_visibility,
            details_pane_width: settings.details_pane_width,
            preview_pane_width: settings.preview_pane_width,
        }
    }
}

impl PersistedColumn {
    const ALL: [Self; 8] = [
        Self::Name,
        Self::DateModified,
        Self::Type,
        Self::Size,
        Self::DateCreated,
        Self::Authors,
        Self::Tags,
        Self::Title,
    ];
}

impl From<ViewMode> for PersistedViewMode {
    fn from(value: ViewMode) -> Self {
        match value {
            ViewMode::ExtraLargeIcons => Self::ExtraLargeIcons,
            ViewMode::LargeIcons => Self::LargeIcons,
            ViewMode::MediumIcons => Self::MediumIcons,
            ViewMode::SmallIcons => Self::SmallIcons,
            ViewMode::List => Self::List,
            ViewMode::Details => Self::Details,
            ViewMode::Tiles => Self::Tiles,
            ViewMode::Content => Self::Content,
        }
    }
}

impl From<SortDescriptor> for PersistedSort {
    fn from(value: SortDescriptor) -> Self {
        Self {
            column: match value.column {
                SortColumn::Name => PersistedColumn::Name,
                SortColumn::DateModified => PersistedColumn::DateModified,
                SortColumn::Type => PersistedColumn::Type,
                SortColumn::Size => PersistedColumn::Size,
                SortColumn::DateCreated => PersistedColumn::DateCreated,
                SortColumn::Authors => PersistedColumn::Authors,
                SortColumn::Tags => PersistedColumn::Tags,
                SortColumn::Title => PersistedColumn::Title,
            },
            direction: match value.direction {
                SortDirection::Ascending => PersistedSortDirection::Ascending,
                SortDirection::Descending => PersistedSortDirection::Descending,
            },
        }
    }
}

impl From<DetailsColumnWidths> for PersistedColumnWidths {
    fn from(value: DetailsColumnWidths) -> Self {
        Self {
            name: value.name,
            date_modified: value.date_modified,
            item_type: value.item_type,
            size: value.size,
            date_created: value.date_created,
            authors: value.authors,
            tags: value.tags,
            title: value.title,
        }
    }
}

fn validate_provenance(value: &SessionProvenance) -> Result<(), SessionValidationError> {
    validate_text(
        &value.app_version,
        "provenance.app_version",
        MAX_PROVENANCE_BYTES,
    )?;
    validate_text(
        &value.app_revision,
        "provenance.app_revision",
        MAX_PROVENANCE_BYTES,
    )?;
    validate_text(
        &value.windows_build,
        "provenance.windows_build",
        MAX_PROVENANCE_BYTES,
    )
}

fn validate_rect(value: PersistedRect, field: &str) -> Result<(), SessionValidationError> {
    if value.width <= 0
        || value.height <= 0
        || value.width > MAX_WINDOW_DIMENSION
        || value.height > MAX_WINDOW_DIMENSION
    {
        return Err(SessionValidationError::InvalidField {
            field: field.to_owned(),
            reason: "rectangle dimensions are non-positive or excessive".to_owned(),
        });
    }
    Ok(())
}

fn validate_history(
    entries: &[PersistedHistoryEntry],
    field: &str,
    limits: RoadmapLimits,
) -> Result<(), SessionValidationError> {
    if entries.len() > limits.max_history_entries_per_tab {
        return Err(SessionValidationError::BoundExceeded {
            field: field.to_owned(),
            value: entries.len(),
            maximum: limits.max_history_entries_per_tab,
        });
    }
    for (index, entry) in entries.iter().enumerate() {
        validate_history_entry(entry, &format!("{field}[{index}]"), limits)?;
    }
    Ok(())
}

fn validate_history_entry(
    entry: &PersistedHistoryEntry,
    field: &str,
    limits: RoadmapLimits,
) -> Result<(), SessionValidationError> {
    validate_location(&entry.location, &format!("{field}.location"), limits)?;
    validate_text(
        &entry.display_title,
        &format!("{field}.display_title"),
        MAX_DISPLAY_TITLE_BYTES,
    )?;
    if let Some(item) = &entry.anchor_item
        && item.provider_bytes().len() > limits.max_location_descriptor_bytes
    {
        return Err(SessionValidationError::BoundExceeded {
            field: format!("{field}.anchor_item"),
            value: item.provider_bytes().len(),
            maximum: limits.max_location_descriptor_bytes,
        });
    }
    Ok(())
}

fn validate_location(
    location: &LocationDescriptor,
    field: &str,
    limits: RoadmapLimits,
) -> Result<(), SessionValidationError> {
    location
        .validate()
        .map_err(|error| SessionValidationError::InvalidField {
            field: field.to_owned(),
            reason: error.to_string(),
        })?;
    if location.encoded_payload_len() > limits.max_location_descriptor_bytes {
        return Err(SessionValidationError::BoundExceeded {
            field: field.to_owned(),
            value: location.encoded_payload_len(),
            maximum: limits.max_location_descriptor_bytes,
        });
    }
    Ok(())
}

fn validate_view_settings(
    settings: &PersistedViewSettings,
    field: &str,
    limits: RoadmapLimits,
) -> Result<(), SessionValidationError> {
    let widths = [
        settings.details_columns.name,
        settings.details_columns.date_modified,
        settings.details_columns.item_type,
        settings.details_columns.size,
        settings.details_columns.date_created,
        settings.details_columns.authors,
        settings.details_columns.tags,
        settings.details_columns.title,
        settings.details_pane_width,
        settings.preview_pane_width,
    ];
    if let Some(width) = widths
        .into_iter()
        .find(|width| *width == 0 || *width > limits.max_column_width)
    {
        return Err(SessionValidationError::InvalidField {
            field: field.to_owned(),
            reason: format!("column or pane width {width} is outside configured bounds"),
        });
    }
    let order_len = settings.details_column_order.len();
    if !matches!(order_len, 4 | 8)
        || settings.details_column_order.len() > limits.max_columns_per_tab
        || settings
            .details_column_order
            .iter()
            .copied()
            .collect::<HashSet<_>>()
            .len()
            != order_len
    {
        return Err(SessionValidationError::InvalidField {
            field: format!("{field}.details_column_order"),
            reason: "column order must contain every supported column exactly once".to_owned(),
        });
    }
    let known_mask = (1_u16 << PersistedColumn::ALL.len()) - 1;
    if settings.details_column_visibility & !known_mask != 0
        || settings.details_column_visibility & 1 == 0
    {
        return Err(SessionValidationError::InvalidField {
            field: format!("{field}.details_column_visibility"),
            reason: "column visibility must contain Name and no unknown bits".to_owned(),
        });
    }
    Ok(())
}

fn validate_text(value: &str, field: &str, maximum: usize) -> Result<(), SessionValidationError> {
    if value.is_empty() {
        return Err(SessionValidationError::InvalidField {
            field: field.to_owned(),
            reason: "value must not be empty".to_owned(),
        });
    }
    if value.len() > maximum {
        return Err(SessionValidationError::BoundExceeded {
            field: field.to_owned(),
            value: value.len(),
            maximum,
        });
    }
    Ok(())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Failure to decode, migrate, validate, or checksum durable session state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionValidationError {
    UnsupportedSchema(u16),
    PayloadTooLarge {
        bytes: usize,
        maximum: usize,
    },
    ChecksumMismatch {
        expected: u64,
        actual: u64,
    },
    BoundExceeded {
        field: String,
        value: usize,
        maximum: usize,
    },
    InvalidField {
        field: String,
        reason: String,
    },
    Invariant(String),
    Limits(String),
    Json(String),
}

impl SessionValidationError {
    #[allow(
        clippy::needless_pass_by_value,
        reason = "serde Result::map_err supplies an owned error and this constructor is used directly"
    )]
    fn json(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

impl fmt::Display for SessionValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported session schema {version}")
            }
            Self::PayloadTooLarge { bytes, maximum } => {
                write!(formatter, "session payload {bytes} exceeds {maximum} bytes")
            }
            Self::ChecksumMismatch { expected, actual } => write!(
                formatter,
                "session checksum mismatch: expected {expected:016x}, actual {actual:016x}"
            ),
            Self::BoundExceeded {
                field,
                value,
                maximum,
            } => write!(
                formatter,
                "session field {field} value {value} exceeds {maximum}"
            ),
            Self::InvalidField { field, reason } => {
                write!(formatter, "invalid session field {field}: {reason}")
            }
            Self::Invariant(reason) => write!(formatter, "invalid session invariant: {reason}"),
            Self::Limits(reason) => write!(formatter, "invalid roadmap limits: {reason}"),
            Self::Json(reason) => write!(formatter, "invalid session JSON: {reason}"),
        }
    }
}

impl std::error::Error for SessionValidationError {}

#[cfg(test)]
mod tests {
    use explorer_common::RoadmapLimits;

    use super::*;
    use crate::{HistoryEntry, LocationDescriptor};

    fn provenance() -> SessionProvenance {
        SessionProvenance {
            app_version: "0.1.0".to_owned(),
            app_revision: "fixture".to_owned(),
            windows_build: "26200".to_owned(),
        }
    }

    fn placement() -> PersistedWindowPlacement {
        PersistedWindowPlacement {
            normal_bounds: PersistedRect {
                left: 100,
                top: 100,
                width: 1120,
                height: 720,
            },
            source_work_area: PersistedRect {
                left: 0,
                top: 0,
                width: 2560,
                height: 1440,
            },
            source_dpi: 168,
            maximized: false,
        }
    }

    fn projected() -> PersistedSessionEnvelope {
        let initial = HistoryEntry::new(LocationDescriptor::file_system(r"D:\fixture"), "fixture");
        let mut window = ExplorerWindowState::new(initial);
        let _ = window.new_tab();
        PersistedSessionEnvelope::project(
            &window,
            placement(),
            &[PersistedQuickAccessPin {
                location: LocationDescriptor::synthetic(crate::SyntheticRoot::Home),
                display_name: "Home".to_owned(),
                order: 0,
            }],
            true,
            7,
            provenance(),
            RoadmapLimits::default(),
        )
        .expect("project fixture")
    }

    #[test]
    fn projection_excludes_transient_state_and_round_trips_deterministically() {
        let envelope = projected();
        let first = envelope
            .encode_pretty(RoadmapLimits::default())
            .expect("encode");
        let decoded =
            PersistedSessionEnvelope::decode(&first, RoadmapLimits::default()).expect("decode");
        let second = decoded
            .encode_pretty(RoadmapLimits::default())
            .expect("reencode");
        assert_eq!(first, second);
        assert_eq!(decoded, envelope);
        let text = String::from_utf8(first).expect("JSON is UTF-8");
        for forbidden in [
            "selection",
            "clipboard",
            "rename",
            "operation",
            "search_results",
        ] {
            assert!(!text.contains(forbidden));
        }
    }

    #[test]
    fn checksum_schema_unknown_fields_and_enum_versions_are_rejected() {
        let envelope = projected();
        let bytes = envelope
            .encode_pretty(RoadmapLimits::default())
            .expect("encode");
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).expect("value");
        value["write_generation"] = serde_json::json!(99);
        let tampered = serde_json::to_vec(&value).expect("tampered");
        assert!(matches!(
            PersistedSessionEnvelope::decode(&tampered, RoadmapLimits::default()),
            Err(SessionValidationError::ChecksumMismatch { .. })
        ));

        value["schema_version"] = serde_json::json!(999);
        let unsupported = serde_json::to_vec(&value).expect("unsupported");
        assert!(matches!(
            PersistedSessionEnvelope::decode(&unsupported, RoadmapLimits::default()),
            Err(SessionValidationError::UnsupportedSchema(999))
        ));

        let mut unknown: serde_json::Value = serde_json::from_slice(&bytes).expect("unknown");
        unknown["future_field"] = serde_json::json!(true);
        assert!(
            PersistedSessionEnvelope::decode(
                &serde_json::to_vec(&unknown).expect("unknown bytes"),
                RoadmapLimits::default()
            )
            .is_err()
        );

        let mut invalid_enum: serde_json::Value = serde_json::from_slice(&bytes).expect("enum");
        invalid_enum["payload"]["tabs"][0]["view_settings"]["mode"] =
            serde_json::json!("future_hologram");
        assert!(
            PersistedSessionEnvelope::decode(
                &serde_json::to_vec(&invalid_enum).expect("enum bytes"),
                RoadmapLimits::default()
            )
            .is_err()
        );
    }

    #[test]
    fn validation_rejects_tab_history_location_column_window_and_payload_bounds() {
        let limits = RoadmapLimits::default();
        let mut envelope = projected();
        envelope.payload.tabs.clear();
        assert!(matches!(
            PersistedSessionEnvelope::new(
                envelope.write_generation,
                envelope.provenance,
                envelope.payload,
                limits
            ),
            Err(SessionValidationError::Invariant(_))
        ));

        let mut envelope = projected();
        envelope.payload.window.normal_bounds.width = 0;
        assert!(
            PersistedSessionEnvelope::new(
                envelope.write_generation,
                envelope.provenance,
                envelope.payload,
                limits
            )
            .is_err()
        );

        let bytes = vec![b'x'; limits.max_state_payload_bytes + 1];
        assert!(matches!(
            PersistedSessionEnvelope::decode(&bytes, limits),
            Err(SessionValidationError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn restore_plan_is_owned_and_preserves_tab_order_active_identity_and_pins() {
        let envelope = projected();
        let plan = envelope
            .restore_plan(RoadmapLimits::default())
            .expect("restore plan");
        assert_eq!(plan.tabs.len(), 2);
        assert_eq!(plan.active_tab_id, plan.tabs[1].tab_id);
        assert_eq!(plan.quick_access[0].order, 0);
        assert_eq!(plan.window, placement());
    }

    #[test]
    fn checked_in_current_and_prior_golden_fixtures_are_supported() {
        let current = include_bytes!("fixtures/session_v1.json");
        let (decoded, migrated) =
            PersistedSessionEnvelope::decode_or_migrate(current, RoadmapLimits::default())
                .expect("current golden fixture");
        assert!(!migrated);
        let reencoded = decoded
            .encode_pretty(RoadmapLimits::default())
            .expect("golden encode");
        assert_eq!(
            PersistedSessionEnvelope::decode(&reencoded, RoadmapLimits::default())
                .expect("deterministic golden reparse"),
            decoded
        );

        let prior = include_bytes!("fixtures/session_v0.json");
        let (decoded, migrated) =
            PersistedSessionEnvelope::decode_or_migrate(prior, RoadmapLimits::default())
                .expect("prior golden fixture");
        assert!(migrated);
        assert_eq!(decoded.schema_version, SESSION_SCHEMA_VERSION);
        assert_eq!(decoded.write_generation, 8);
        assert_eq!(decoded.payload.tabs.len(), 2);
    }
}

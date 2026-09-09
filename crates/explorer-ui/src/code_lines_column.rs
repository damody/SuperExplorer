//! UI boundary for the public-SDK Rust tokei Code lines example.
//!
//! The application owns bounded file I/O and plugin dispatch. This module owns
//! only copied requests/results and the host-side Details-column projection.

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
};

pub use explorer_extension_ui_api::{CellRenderContextV1, CellRenderPlanV1};
use explorer_model::{
    ColumnAlignment, ColumnApplicability, ColumnCost, ColumnDescriptor, ColumnId,
    ColumnSortSemantics, ColumnValueType, LocationDescriptor, RequestContext, ShellItemId,
};

const DIRECTORY_VALUE_CACHE_LIMIT: usize = 64;

pub const CODE_LINES_COLUMN_PACKAGE_ID: &str = "rust-tokei";
pub const CODE_LINES_COLUMN_ID: &str = "code-lines";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CodeLinesDisplayMode {
    #[default]
    CodeOnly,
    WithCommentAndBlank,
}

impl CodeLinesDisplayMode {
    pub const fn shows_detail(self) -> bool {
        matches!(self, Self::WithCommentAndBlank)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLinesColumnConfigV1 {
    pub descriptor: ColumnDescriptor,
    pub display: CodeLinesDisplayMode,
    /// Folder Options package that owns this runtime contribution. This is
    /// host-minted while loading the one example and is never plugin input.
    pub option_package_id: String,
    pub folder_admission: FolderAdmissionPolicyV1,
}

impl Default for CodeLinesColumnConfigV1 {
    fn default() -> Self {
        Self {
            descriptor: code_lines_column_descriptor(),
            display: CodeLinesDisplayMode::default(),
            option_package_id: "rust-tokei-code-lines-column".to_owned(),
            folder_admission: FolderAdmissionPolicyV1 {
                max_file_count: Some(999),
                max_folder_count: None,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FolderAdmissionPolicyV1 {
    pub max_file_count: Option<u64>,
    pub max_folder_count: Option<u64>,
}

impl FolderAdmissionPolicyV1 {
    pub const fn requires_directory_facts(self) -> bool {
        self.max_file_count.is_some() || self.max_folder_count.is_some()
    }

    pub fn evaluate(
        self,
        facts: crate::folder_size_column::DirectoryFactsV1,
    ) -> FolderAdmissionOutcomeV1 {
        if self
            .max_file_count
            .is_some_and(|maximum| facts.file_count > maximum)
            || self
                .max_folder_count
                .is_some_and(|maximum| facts.folder_count > maximum)
        {
            FolderAdmissionOutcomeV1::OverLimit
        } else {
            FolderAdmissionOutcomeV1::Admitted
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FolderAdmissionOutcomeV1 {
    Admitted,
    OverLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FolderAdmissionStateV1 {
    Pending,
    Unavailable,
    OverLimit,
}

impl FolderAdmissionStateV1 {
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Pending => "status-waiting-file-count",
            Self::Unavailable | Self::OverLimit => "status-file-count-pending-label",
        }
    }

    pub const fn reason_key(self) -> &'static str {
        match self {
            Self::Pending => "status-waiting-file-count",
            Self::Unavailable => "status-file-count-limit",
            Self::OverLimit => "status-file-count-over-limit",
        }
    }

    pub fn label(self, catalog: explorer_i18n::Catalog) -> String {
        catalog.t(self.label_key())
    }

    pub fn reason(self, catalog: explorer_i18n::Catalog) -> String {
        catalog.t(self.reason_key())
    }

    pub const fn is_limit(self) -> bool {
        matches!(self, Self::Unavailable | Self::OverLimit)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLinesRequestV1 {
    pub context: RequestContext,
    pub item_id: ShellItemId,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLinesValueV1 {
    pub language: String,
    pub code: u64,
    pub comments: u64,
    pub blanks: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLinesResultV1 {
    pub context: RequestContext,
    pub item_id: ShellItemId,
    pub value: Option<CodeLinesValueV1>,
    pub error: Option<String>,
}

pub trait CodeLinesRuntimePortV1: Send + Sync {
    fn config(&self) -> CodeLinesColumnConfigV1;
    fn submit_code_lines_requests(&self, requests: Vec<CodeLinesRequestV1>);
    fn cancel_code_lines_context(&self, context: &RequestContext);
    /// Invalidates only values whose items belong directly to this directory.
    /// In-flight work admitted before the refresh must not repopulate it.
    fn invalidate_directory_cache(&self, directory: &std::path::Path);
    fn drain_code_lines_results(&self) -> Vec<CodeLinesResultV1>;
    /// Moves completed asynchronous render plans into the host cache. Returns
    /// true only when GPUI needs another frame to consume a newly-ready plan.
    fn drain_render_results(&self) -> bool {
        false
    }
    fn render_cell(&self, context: CellRenderContextV1) -> CellRenderPlanV1;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLinesColumnVisuals {
    pub config: CodeLinesColumnConfigV1,
    /// Values belong to the current tab. A watcher generation bump on the same
    /// directory keeps them; navigation to another tab or location starts empty.
    pub context: Option<RequestContext>,
    pub values: HashMap<ShellItemId, CodeLinesValueV1>,
    pub errors: HashMap<ShellItemId, String>,
    pub admissions: HashMap<ShellItemId, FolderAdmissionStateV1>,
    location_key: Option<String>,
    directory_cache: HashMap<String, CodeLinesDirectorySnapshotV1>,
    directory_lru: VecDeque<String>,
    item_cache: HashMap<ShellItemId, CodeLinesValueV1>,
    item_error_cache: HashMap<ShellItemId, String>,
    item_lru: VecDeque<ShellItemId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CodeLinesDirectorySnapshotV1 {
    values: HashMap<ShellItemId, CodeLinesValueV1>,
    errors: HashMap<ShellItemId, String>,
    admissions: HashMap<ShellItemId, FolderAdmissionStateV1>,
}

impl CodeLinesColumnVisuals {
    pub fn new(config: CodeLinesColumnConfigV1) -> Self {
        Self {
            config,
            context: None,
            values: HashMap::new(),
            errors: HashMap::new(),
            admissions: HashMap::new(),
            location_key: None,
            directory_cache: HashMap::new(),
            directory_lru: VecDeque::new(),
            item_cache: HashMap::new(),
            item_error_cache: HashMap::new(),
            item_lru: VecDeque::new(),
        }
    }

    /// Starts a new host-owned request context. Same-tab generation bumps keep
    /// values; a different tab starts empty.
    pub fn begin_context(&mut self, context: RequestContext) -> bool {
        if self.context.as_ref().is_some_and(|current| {
            current.tab_id == context.tab_id && current.generation == context.generation
        }) {
            return false;
        }
        let same_tab = self
            .context
            .as_ref()
            .is_some_and(|current| current.tab_id == context.tab_id);
        self.context = Some(context);
        if !same_tab {
            self.values.clear();
            self.errors.clear();
            self.admissions.clear();
        }
        true
    }

    pub fn store_current_directory(&mut self) {
        let Some(key) = self.location_key.clone() else {
            return;
        };
        if self.values.is_empty() && self.errors.is_empty() && self.admissions.is_empty() {
            return;
        }
        self.remember_directory(
            key,
            CodeLinesDirectorySnapshotV1 {
                values: self.values.clone(),
                errors: self.errors.clone(),
                admissions: self.admissions.clone(),
            },
        );
    }

    pub fn activate_location(&mut self, location: Option<&LocationDescriptor>) -> bool {
        let new_key = location.map(crate::folder_size_column::directory_identity_key);
        if self.location_key.as_ref() == new_key.as_ref() {
            return false;
        }
        let had_location = self.location_key.is_some();
        self.store_current_directory();
        self.location_key = new_key.clone();
        if let Some(key) = new_key {
            if let Some(snapshot) = self.directory_cache.get(&key).cloned() {
                self.values = snapshot.values;
                self.errors = snapshot.errors;
                self.admissions = snapshot.admissions;
            } else if had_location {
                self.values.clear();
                self.errors.clear();
                self.admissions.clear();
            }
        } else if had_location {
            self.values.clear();
            self.errors.clear();
            self.admissions.clear();
        }
        true
    }

    pub fn hydrate_items(&mut self, item_ids: impl IntoIterator<Item = ShellItemId>) -> bool {
        let mut changed = false;
        for item_id in item_ids {
            if let Some(cached) = self.item_cache.get(&item_id).cloned()
                && self.values.insert(item_id.clone(), cached).is_none()
            {
                changed = true;
            }
            if let Some(cached) = self.item_error_cache.get(&item_id).cloned()
                && self.errors.insert(item_id, cached).is_none()
            {
                changed = true;
            }
        }
        changed
    }

    pub fn remember_value(&mut self, item_id: ShellItemId, value: CodeLinesValueV1) {
        self.touch_item(&item_id);
        self.item_error_cache.remove(&item_id);
        self.item_cache.insert(item_id, value);
    }

    pub fn remember_error(&mut self, item_id: ShellItemId, error: String) {
        self.touch_item(&item_id);
        self.item_cache.remove(&item_id);
        self.item_error_cache.insert(item_id, error);
    }

    pub fn clear_directory_cache(&mut self) {
        if let Some(key) = self.location_key.as_ref() {
            self.directory_cache.remove(key);
            self.directory_lru.retain(|cached| cached != key);
        }
        let item_ids = self
            .values
            .keys()
            .chain(self.errors.keys())
            .cloned()
            .collect::<Vec<_>>();
        for item_id in item_ids {
            self.item_cache.remove(&item_id);
            self.item_error_cache.remove(&item_id);
            self.item_lru.retain(|cached| cached != &item_id);
        }
    }

    fn touch_item(&mut self, item_id: &ShellItemId) {
        if self.item_cache.contains_key(item_id) || self.item_error_cache.contains_key(item_id) {
            self.item_lru.retain(|cached| cached != item_id);
        } else {
            while self.item_lru.len() >= 8_192 {
                if let Some(oldest) = self.item_lru.pop_front() {
                    self.item_cache.remove(&oldest);
                    self.item_error_cache.remove(&oldest);
                } else {
                    break;
                }
            }
        }
        self.item_lru.push_back(item_id.clone());
    }

    fn remember_directory(&mut self, key: String, snapshot: CodeLinesDirectorySnapshotV1) {
        if self.directory_cache.contains_key(&key) {
            self.directory_lru.retain(|cached| cached != &key);
        } else {
            while self.directory_lru.len() >= DIRECTORY_VALUE_CACHE_LIMIT {
                if let Some(oldest) = self.directory_lru.pop_front() {
                    self.directory_cache.remove(&oldest);
                } else {
                    break;
                }
            }
        }
        self.directory_lru.push_back(key.clone());
        self.directory_cache.insert(key, snapshot);
    }

    pub fn set_admission(
        &mut self,
        item_id: ShellItemId,
        state: Option<FolderAdmissionStateV1>,
    ) -> bool {
        match state {
            Some(state) => self.admissions.insert(item_id, state) != Some(state),
            None => self.admissions.remove(&item_id).is_some(),
        }
    }

    pub fn presentation_error_for(
        &self,
        item_id: &ShellItemId,
        catalog: explorer_i18n::Catalog,
    ) -> Option<String> {
        self.errors.get(item_id).cloned().or_else(|| {
            self.admissions
                .get(item_id)
                .map(|state| state.reason(catalog))
        })
    }

    pub fn exact_sort_values(&self) -> HashMap<ShellItemId, Option<u64>> {
        self.values
            .iter()
            .map(|(id, value)| (id.clone(), Some(value.code)))
            .collect()
    }

    pub fn maximum_value(&self) -> u64 {
        self.values
            .values()
            .map(|value| value.code)
            .max()
            .unwrap_or(0)
    }
}

pub type CodeLinesRuntimeHandleV1 = Arc<dyn CodeLinesRuntimePortV1>;

pub fn code_lines_column_descriptor() -> ColumnDescriptor {
    ColumnDescriptor {
        id: ColumnId::Extension {
            package_id: CODE_LINES_COLUMN_PACKAGE_ID.to_owned(),
            column_id: CODE_LINES_COLUMN_ID.to_owned(),
        },
        display_name: "Main code lines".to_owned(),
        value_type: ColumnValueType::Integer,
        default_width: 168,
        minimum_width: 104,
        maximum_width: 360,
        alignment: ColumnAlignment::End,
        applicability: ColumnApplicability::AllEntries,
        file_systems: explorer_model::ColumnFileSystems::LOCAL,
        sort_semantics: ColumnSortSemantics::Integer,
        cost: ColumnCost::BackgroundBatch,
    }
}

pub fn lock_owner_column_descriptor() -> ColumnDescriptor {
    ColumnDescriptor {
        id: ColumnId::Extension {
            package_id: "rust-lock-owner".to_owned(),
            column_id: "owners".to_owned(),
        },
        display_name: "Lock owners".to_owned(),
        value_type: ColumnValueType::Integer,
        default_width: 220,
        minimum_width: 120,
        maximum_width: 480,
        alignment: ColumnAlignment::Start,
        applicability: ColumnApplicability::Files,
        file_systems: explorer_model::ColumnFileSystems::LOCAL,
        sort_semantics: ColumnSortSemantics::Integer,
        cost: ColumnCost::BackgroundBatch,
    }
}

pub fn is_supported_code_lines_descriptor(descriptor: &ColumnDescriptor) -> bool {
    let code_lines_extension = matches!(
        &descriptor.id,
        ColumnId::Extension { column_id, .. } if column_id == CODE_LINES_COLUMN_ID
    );
    (code_lines_extension || descriptor.id == lock_owner_column_descriptor().id)
        && descriptor.value_type == ColumnValueType::Integer
        && descriptor.sort_semantics == ColumnSortSemantics::Integer
        && descriptor.validate().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use explorer_model::{Generation, TabId};

    #[test]
    fn descriptor_uses_exact_integer_background_batch_semantics() {
        let descriptor = code_lines_column_descriptor();
        assert!(is_supported_code_lines_descriptor(&descriptor));
        assert_eq!(descriptor.cost, ColumnCost::BackgroundBatch);
        assert_eq!(descriptor.applicability, ColumnApplicability::AllEntries);
        assert_eq!(descriptor.display_name, "Main code lines");
    }

    #[test]
    fn folder_admission_is_inclusive_and_requires_every_declared_limit() {
        let policy = FolderAdmissionPolicyV1 {
            max_file_count: Some(999),
            max_folder_count: Some(3),
        };
        let facts = |file_count, folder_count| crate::folder_size_column::DirectoryFactsV1 {
            mft_generation: 7,
            file_count,
            folder_count,
        };
        assert_eq!(
            policy.evaluate(facts(999, 3)),
            FolderAdmissionOutcomeV1::Admitted
        );
        assert_eq!(
            policy.evaluate(facts(1_000, 3)),
            FolderAdmissionOutcomeV1::OverLimit
        );
        assert_eq!(
            policy.evaluate(facts(999, 4)),
            FolderAdmissionOutcomeV1::OverLimit
        );
        let zh = explorer_i18n::Catalog::new(explorer_i18n::AppLocale::ZhTw);
        assert_eq!(
            FolderAdmissionStateV1::Pending.label(zh),
            "等待 File Count…"
        );
        assert_eq!(
            FolderAdmissionStateV1::Pending.reason(zh),
            "等待 File Count…"
        );
        assert!(!FolderAdmissionStateV1::Pending.is_limit());
        assert_eq!(FolderAdmissionStateV1::OverLimit.label(zh), "Limit");
        assert_eq!(
            FolderAdmissionStateV1::OverLimit.reason(zh),
            "File Count 超過限制，因此未啟動"
        );
        assert!(FolderAdmissionStateV1::OverLimit.is_limit());
        assert_eq!(FolderAdmissionStateV1::Unavailable.label(zh), "Limit");
        assert_eq!(
            FolderAdmissionStateV1::Unavailable.reason(zh),
            "依賴 File Count，因此未啟動"
        );
        assert!(FolderAdmissionStateV1::Unavailable.is_limit());
    }

    #[test]
    fn same_tab_refresh_generation_keeps_code_line_values() {
        let item = ShellItemId::from_provider_bytes([7]).unwrap();
        let first = RequestContext::new(TabId::new(), Generation::new(1));
        let mut visuals = CodeLinesColumnVisuals {
            context: Some(first.clone()),
            values: HashMap::from([(
                item.clone(),
                CodeLinesValueV1 {
                    language: "Rust".to_owned(),
                    code: 12,
                    comments: 1,
                    blanks: 1,
                    total: 14,
                },
            )]),
            ..CodeLinesColumnVisuals::new(CodeLinesColumnConfigV1::default())
        };

        assert!(visuals.begin_context(RequestContext::new(first.tab_id, Generation::new(2))));
        assert_eq!(visuals.values.get(&item).map(|value| value.code), Some(12));
        assert_eq!(visuals.exact_sort_values().get(&item), Some(&Some(12)));
    }

    #[test]
    fn switching_tabs_clears_code_line_values() {
        let item = ShellItemId::from_provider_bytes([7]).unwrap();
        let first = RequestContext::new(TabId::new(), Generation::new(1));
        let mut visuals = CodeLinesColumnVisuals {
            context: Some(first),
            values: HashMap::from([(
                item.clone(),
                CodeLinesValueV1 {
                    language: "Rust".to_owned(),
                    code: 12,
                    comments: 1,
                    blanks: 1,
                    total: 14,
                },
            )]),
            ..CodeLinesColumnVisuals::new(CodeLinesColumnConfigV1::default())
        };

        assert!(visuals.begin_context(RequestContext::new(TabId::new(), Generation::new(1))));
        assert!(visuals.values.is_empty());
        assert_eq!(visuals.exact_sort_values().get(&item), None);
    }

    #[test]
    fn returning_to_a_directory_restores_cached_code_lines() {
        let tab = TabId::new();
        let home = LocationDescriptor::file_system(r"C:\Users\Damody");
        let other = LocationDescriptor::file_system(r"C:\Temp");
        let item = ShellItemId::from_provider_bytes([7]).unwrap();
        let mut visuals = CodeLinesColumnVisuals {
            context: Some(RequestContext::new(tab, Generation::new(1))),
            values: HashMap::from([(
                item.clone(),
                CodeLinesValueV1 {
                    language: "Markdown".to_owned(),
                    code: 14,
                    comments: 0,
                    blanks: 0,
                    total: 14,
                },
            )]),
            ..CodeLinesColumnVisuals::new(CodeLinesColumnConfigV1::default())
        };
        visuals.activate_location(Some(&home));
        visuals.begin_context(RequestContext::new(tab, Generation::new(2)));
        visuals.activate_location(Some(&other));
        assert!(visuals.values.is_empty());
        visuals.begin_context(RequestContext::new(tab, Generation::new(3)));
        visuals.activate_location(Some(&home));
        assert_eq!(visuals.values.get(&item).map(|value| value.code), Some(14));
    }

    #[test]
    fn a_new_request_id_in_the_same_generation_preserves_values() {
        let item = ShellItemId::from_provider_bytes([8]).unwrap();
        let first = RequestContext::new(TabId::new(), Generation::new(3));
        let mut visuals = CodeLinesColumnVisuals {
            context: Some(first.clone()),
            values: HashMap::from([(
                item.clone(),
                CodeLinesValueV1 {
                    language: "Rust".to_owned(),
                    code: 4,
                    comments: 0,
                    blanks: 0,
                    total: 4,
                },
            )]),
            ..CodeLinesColumnVisuals::new(CodeLinesColumnConfigV1::default())
        };

        assert!(!visuals.begin_context(RequestContext::new(first.tab_id, first.generation)));
        assert_eq!(visuals.values.get(&item).map(|value| value.code), Some(4));
    }
}

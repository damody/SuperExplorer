//! Production process composition root.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs,
    hash::{Hash, Hasher},
    io::Read as _,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Error};
use explorer_common::{DiagnosticsSession, ErrorSeverity, RoadmapLimits};
use explorer_model::SessionStore as _;
use explorer_shell_win::ShellStaHandle;
use explorer_ui::{
    ExplorerRoot, UiTokens, initial_window_options, window_options_with_placement,
    window_options_with_size,
};
use gpui::AppContext as _;

use crate::windows_prerequisites::initialize_dpi_awareness;
use crate::{system_theme::high_contrast_tokens, visual_fixture::VisualFixtureConfig};

const SHELL_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

const FOLDER_SIZE_CONTRIBUTION_ID_V1: &str = "folder-size";
const FOLDER_SIZE_RENDERER_CONTRIBUTION_ID_V1: &str = "folder-size-renderer";
const SIZE_MAP_VIEW_CONTRIBUTION_ID_V1: &str = "size-map";
const SIZE_MAP_REQUEST_QUEUE_CAP_V1: usize = 1_024;
const SIZE_MAP_TREE_DELTA_BATCH_CAP_V1: usize = 256;
const CODE_LINES_CONTRIBUTION_ID_V1: &str = "rust-tokei:code-lines";
const LUA_CODE_LINES_CONTRIBUTION_ID_V1: &str = "lua-tokei:column";
const LUA_CODE_LINES_RENDERER_CONTRIBUTION_ID_V1: &str = "lua-tokei:renderer";
const SEVEN_Z_RESOURCE_CONTRIBUTION_ID_V1: &str = "rust-7z:resource";
const CODE_LINES_RENDERER_CONTRIBUTION_ID_V1: &str = "rust-tokei:code-lines-renderer";
const LOCK_OWNER_CONTRIBUTION_ID_V1: &str = "rust-lock-owner:owners";
const LOCK_OWNER_RENDERER_CONTRIBUTION_ID_V1: &str = "rust-lock-owner:owners-renderer";
const CODE_LINES_BATCH_ITEMS_V1: usize = 128;
const SIZE_MAP_VISIBLE_NODE_LIMIT_V1: u32 = 10_000;
const DIRECT_RENDER_QUEUE_CAP_V1: usize = 256;
const DIRECT_RENDER_CACHE_CAP_V1: usize = 512;
const SIZE_MAP_RENDER_QUEUE_CAP_V1: usize = 8;
static NEXT_SIZE_MAP_RUNTIME_INCARNATION_V1: AtomicU64 = AtomicU64::new(1);
const SIZE_MAP_RENDER_CACHE_CAP_V1: usize = 4;
static MFT_BUDGET_CONFIGURATION_V1: OnceLock<
    Arc<(
        Mutex<Option<crate::mft_query::MftCacheBudgetLimitsV1>>,
        Condvar,
    )>,
> = OnceLock::new();
static MFT_BUDGET_CONFIGURATION_PENDING_V1: AtomicBool = AtomicBool::new(true);

pub(crate) fn mft_budget_configuration_pending_v1() -> bool {
    MFT_BUDGET_CONFIGURATION_PENDING_V1.load(Ordering::Acquire)
}

fn mft_diagnostics_match_limits(
    diagnostics: &crate::mft_query::MftCacheDiagnosticsV1,
    limits: crate::mft_query::MftCacheBudgetLimitsV1,
) -> bool {
    let mib = 1024 * 1024;
    diagnostics.limit_bytes == u64::from(limits.lru_mb) * mib
        && diagnostics.persisted_index_limit_bytes
            == Some(u64::from(limits.persisted_index_mb) * mib)
        && diagnostics.volume_index_limit_bytes == Some(u64::from(limits.volume_index_mb) * mib)
        && diagnostics.file_data_limit_bytes == Some(u64::from(limits.file_data_mb) * mib)
        && diagnostics.aggregate_limit_bytes == Some(u64::from(limits.aggregate_mb) * mib)
}

fn configure_mft_budget_snapshot(limits: crate::mft_query::MftCacheBudgetLimitsV1) {
    let state = MFT_BUDGET_CONFIGURATION_V1
        .get_or_init(|| {
            let state = Arc::new((Mutex::new(None), Condvar::new()));
            let worker_state = Arc::clone(&state);
            let _worker = std::thread::Builder::new()
                .name("mft-budget-reconnect".to_owned())
                .spawn(move || {
                    loop {
                        let (lock, ready) = &*worker_state;
                        let desired = {
                            let mut desired = lock
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            while desired.is_none() {
                                desired = ready
                                    .wait(desired)
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                            }
                            desired.expect("checked above")
                        };
                        if crate::mft_query::query_diagnostics().is_ok_and(|diagnostics| {
                            mft_diagnostics_match_limits(&diagnostics, desired)
                        }) {
                            MFT_BUDGET_CONFIGURATION_PENDING_V1.store(false, Ordering::Release);
                            let guard = lock
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            let _ = ready
                                .wait_timeout(guard, Duration::from_secs(2))
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            continue;
                        }
                        MFT_BUDGET_CONFIGURATION_PENDING_V1.store(true, Ordering::Release);
                        let applied = crate::mft_query::set_cache_budgets(desired)
                            .is_ok_and(|effective| effective == desired);
                        if applied {
                            MFT_BUDGET_CONFIGURATION_PENDING_V1.store(false, Ordering::Release);
                        }
                        let desired_guard = lock
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let _ = ready
                            .wait_timeout(
                                desired_guard,
                                if applied {
                                    Duration::from_secs(2)
                                } else {
                                    Duration::from_millis(250)
                                },
                            )
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        MFT_BUDGET_CONFIGURATION_PENDING_V1.store(true, Ordering::Release);
                    }
                });
            state
        })
        .clone();
    let (lock, ready) = &*state;
    *lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(limits);
    MFT_BUDGET_CONFIGURATION_PENDING_V1.store(true, Ordering::Release);
    ready.notify_one();
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CellRenderKeyV1(Vec<u8>);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SizeMapRenderKeyV1(Vec<u8>);

fn append_bytes(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u64).to_le_bytes());
    target.extend_from_slice(value);
}

fn append_id(target: &mut Vec<u8>, value: explorer_extension_ui_api::StableIdV1) {
    target.extend_from_slice(&value.namespace.into_raw().to_le_bytes());
    target.extend_from_slice(&value.value.to_le_bytes());
}

fn append_option_u64(target: &mut Vec<u8>, value: Option<u64>) {
    match value {
        Some(value) => {
            target.push(1);
            target.extend_from_slice(&value.to_le_bytes());
        }
        None => target.push(0),
    }
}

fn append_plugin_value(target: &mut Vec<u8>, value: &explorer_extension_ui_api::PluginValueV1) {
    target.extend_from_slice(&value.kind.into_raw().to_le_bytes());
    target.extend_from_slice(&value.reserved.to_le_bytes());
    target.extend_from_slice(&value.integer.to_le_bytes());
    target.extend_from_slice(&value.float.to_bits().to_le_bytes());
    append_bytes(target, value.text.as_bytes());
    append_bytes(target, value.payload.as_slice());
    append_id(target, value.opaque_schema);
    target.extend_from_slice(&value.opaque_schema_version.to_le_bytes());
    target.extend_from_slice(&value.reserved_tail.to_le_bytes());
}

fn append_option_plugin_value(
    target: &mut Vec<u8>,
    value: Option<&explorer_extension_ui_api::PluginValueV1>,
) {
    match value {
        Some(value) => {
            target.push(1);
            append_plugin_value(target, value);
        }
        None => target.push(0),
    }
}

fn append_color(target: &mut Vec<u8>, color: explorer_extension_ui_api::CellColorV1) {
    target.extend_from_slice(&[color.red, color.green, color.blue, color.alpha]);
}

fn append_theme(target: &mut Vec<u8>, theme: explorer_extension_ui_api::CellThemeV1) {
    append_color(target, theme.foreground);
    append_color(target, theme.muted_foreground);
    append_color(target, theme.background);
    append_color(target, theme.selection_background);
    append_color(target, theme.accent);
}

fn cell_render_key(context: &explorer_extension_ui_api::CellRenderContextV1) -> CellRenderKeyV1 {
    let mut bytes = Vec::new();
    let value = context.value.clone().into_option();
    append_option_plugin_value(&mut bytes, value.as_ref());
    append_option_u64(&mut bytes, context.exact_bytes.clone().into_option());
    match context.aggregate.clone().into_option() {
        Some(aggregate) => {
            bytes.push(1);
            let aggregate_value = aggregate.largest_sibling_value.clone().into_option();
            append_option_plugin_value(&mut bytes, aggregate_value.as_ref());
            append_option_u64(&mut bytes, aggregate.largest_sibling_bytes.into_option());
        }
        None => bytes.push(0),
    }
    bytes.extend_from_slice(&[
        u8::from(context.loading),
        u8::from(context.selected),
        u8::from(context.hovered),
    ]);
    match context.error.clone().into_option() {
        Some(error) => {
            bytes.push(1);
            append_bytes(&mut bytes, error.as_bytes());
        }
        None => bytes.push(0),
    }
    bytes.extend_from_slice(&context.dpi_milli.to_le_bytes());
    append_theme(&mut bytes, context.theme);
    append_bytes(&mut bytes, context.settings.as_bytes());
    append_id(&mut bytes, context.item_id);
    bytes.extend_from_slice(&context.request_generation.to_le_bytes());
    bytes.extend_from_slice(&context.render_generation.to_le_bytes());
    CellRenderKeyV1(bytes)
}

fn revision_for(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish().max(1)
}

fn size_map_snapshot_bytes(context: &explorer_extension_ui_api::SizeMapRenderContextV1) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&context.snapshot.location_generation.to_le_bytes());
    bytes.extend_from_slice(&context.snapshot.refresh_generation.to_le_bytes());
    bytes.extend_from_slice(&context.snapshot.render_revision.to_le_bytes());
    bytes.extend_from_slice(&(context.nodes.len() as u64).to_le_bytes());
    for node in &context.nodes {
        append_id(&mut bytes, node.node_id);
        match node.parent_id.clone().into_option() {
            Some(parent_id) => {
                bytes.push(1);
                append_id(&mut bytes, parent_id);
            }
            None => bytes.push(0),
        }
        append_bytes(&mut bytes, node.name.as_bytes());
        bytes.extend_from_slice(&node.kind.into_raw().to_le_bytes());
        append_option_u64(&mut bytes, node.exact_bytes.clone().into_option());
        bytes.extend_from_slice(&node.status.into_raw().to_le_bytes());
    }
    bytes.extend_from_slice(&context.viewport.width_milli.to_le_bytes());
    bytes.extend_from_slice(&context.viewport.height_milli.to_le_bytes());
    bytes.extend_from_slice(&context.viewport.dpi_milli.to_le_bytes());
    append_theme(&mut bytes, context.theme);
    bytes.extend_from_slice(&(context.selected_node_ids.len() as u64).to_le_bytes());
    for selected in &context.selected_node_ids {
        append_id(&mut bytes, *selected);
    }
    append_bytes(&mut bytes, context.settings.as_bytes());
    bytes
}

fn size_map_node_id(
    item_id: &explorer_model::ShellItemId,
) -> explorer_extension_ui_api::StableIdV1 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    b"superexplorer:size-map-node:v1".hash(&mut hasher);
    item_id.provider_bytes().hash(&mut hasher);
    explorer_extension_ui_api::StableIdV1::new(
        explorer_extension_ui_api::EXTENSION_ID_NAMESPACE_V1,
        hasher.finish().max(1),
    )
}

fn partition_size_map_projection(
    nodes: &[explorer_ui::size_map_view::SizeMapNodeV1],
) -> (Vec<usize>, Vec<usize>) {
    const MAX_INDIVIDUAL_SIZE_MAP_NODES_V1: usize = 255;
    let mut roots = nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| node.parent_item_id.is_none().then_some(index))
        .collect::<Vec<_>>();
    // Retain the most useful visible siblings first. The stable sort keeps
    // deterministic source order for equal totals, while zero-byte siblings
    // naturally move into the accessible `Other` tail.
    roots.sort_by(|left, right| {
        nodes[*right]
            .exact_bytes
            .unwrap_or_default()
            .cmp(&nodes[*left].exact_bytes.unwrap_or_default())
    });
    let mut projected = roots
        .into_iter()
        .take(MAX_INDIVIDUAL_SIZE_MAP_NODES_V1)
        .collect::<Vec<_>>();
    let mut projected_ids = projected
        .iter()
        .map(|index| nodes[*index].item_id.clone())
        .collect::<HashSet<_>>();
    // `recursive_nodes_for` emits descendants parent-before-child. Admit a
    // descendant only when its parent is already projected, so the public
    // hierarchy can never contain an orphan.
    for (index, node) in nodes.iter().enumerate() {
        if projected.len() == MAX_INDIVIDUAL_SIZE_MAP_NODES_V1 {
            break;
        }
        if node
            .parent_item_id
            .as_ref()
            .is_some_and(|parent| projected_ids.contains(parent))
        {
            projected.push(index);
            projected_ids.insert(node.item_id.clone());
        }
    }
    let projected_indexes = projected.iter().copied().collect::<HashSet<_>>();
    let omitted = (0..nodes.len())
        .filter(|index| !projected_indexes.contains(index))
        .collect();
    (projected, omitted)
}

fn aggregate_size_map_items(
    nodes: &[explorer_ui::size_map_view::SizeMapNodeV1],
    omitted: &[usize],
) -> Vec<explorer_ui::size_map_view::SizeMapAggregateItemV1> {
    omitted
        .iter()
        .map(|index| {
            let node = &nodes[*index];
            explorer_ui::size_map_view::SizeMapAggregateItemV1 {
                item_id: node.item_id.clone(),
                label: node.display_name.clone(),
                detail: if node.partial {
                    "Partial".to_owned()
                } else if let Some(error) = &node.error {
                    format!("Failed: {error}")
                } else if let Some(bytes) = node.exact_bytes {
                    format!("{bytes} bytes. Complete")
                } else {
                    "Unavailable".to_owned()
                },
            }
        })
        .collect()
}

fn size_map_node_status_v1(
    node: &explorer_ui::size_map_view::SizeMapNodeV1,
) -> explorer_extension_ui_api::SizeMapNodeStatusV1 {
    use explorer_extension_ui_api::SizeMapNodeStatusV1;
    let diagnostic = node.error.as_deref().unwrap_or_default();
    if diagnostic.starts_with("cancelled:") {
        SizeMapNodeStatusV1::CANCELLED
    } else if diagnostic.starts_with("resource-limited:")
        || diagnostic.contains("resource limit")
        || diagnostic.contains("depth limit")
    {
        SizeMapNodeStatusV1::RESOURCE_LIMITED
    } else if diagnostic.starts_with("unavailable:") {
        SizeMapNodeStatusV1::UNAVAILABLE
    } else if node.partial {
        SizeMapNodeStatusV1::PARTIAL
    } else if node.error.is_some() {
        SizeMapNodeStatusV1::FAILED
    } else if node.exact_bytes.is_some() {
        SizeMapNodeStatusV1::COMPLETE
    } else {
        SizeMapNodeStatusV1::UNAVAILABLE
    }
}

fn append_size_map_host_scope(
    target: &mut Vec<u8>,
    request_context: &explorer_model::RequestContext,
    item_ids: &[explorer_model::ShellItemId],
) {
    // This scope remains host-local. The plugin only receives its opaque
    // revision, never a tab ID, request ID, filesystem location, or raw Shell
    // item identity. It nevertheless makes a result valid for exactly one
    // navigation/refresh context and one ordered set of real Shell items.
    let mut scope = std::collections::hash_map::DefaultHasher::new();
    b"superexplorer:size-map-render-scope:v1".hash(&mut scope);
    request_context.request_id.hash(&mut scope);
    request_context.tab_id.hash(&mut scope);
    request_context.generation.hash(&mut scope);
    for item_id in item_ids {
        item_id.provider_bytes().hash(&mut scope);
    }
    target.extend_from_slice(&scope.finish().to_le_bytes());
}

fn size_map_render_key(
    context: &mut explorer_extension_ui_api::SizeMapRenderContextV1,
    request_context: &explorer_model::RequestContext,
    item_ids: &[explorer_model::ShellItemId],
    package_incarnation: u64,
) -> SizeMapRenderKeyV1 {
    context.snapshot.render_revision = 1;
    let mut revision_input = size_map_snapshot_bytes(context);
    append_size_map_host_scope(&mut revision_input, request_context, item_ids);
    revision_input.extend_from_slice(&package_incarnation.to_le_bytes());
    context.snapshot.render_revision = revision_for(&revision_input);

    let mut key = size_map_snapshot_bytes(context);
    append_size_map_host_scope(&mut key, request_context, item_ids);
    key.extend_from_slice(&package_incarnation.to_le_bytes());
    SizeMapRenderKeyV1(key)
}

/// Bounded bridge between GPUI and one retained direct-DLL renderer. The GPUI
/// caller only polls a cache and uses `try_send`; the plugin callback (and its
/// durable call marker) always run on this worker thread.
struct AsyncCellRendererV1 {
    requests: mpsc::SyncSender<explorer_extension_ui_api::CellRenderContextV1>,
    results: Mutex<mpsc::Receiver<(CellRenderKeyV1, explorer_extension_ui_api::CellRenderPlanV1)>>,
    pending: Mutex<HashSet<CellRenderKeyV1>>,
    cache: Mutex<HashMap<CellRenderKeyV1, explorer_extension_ui_api::CellRenderPlanV1>>,
}

impl AsyncCellRendererV1 {
    fn start(
        mut renderer: explorer_extension_host::SinglePluginVisualRenderRuntimeV1,
        contribution_id: &'static str,
    ) -> Result<Self, Error> {
        let (request_tx, request_rx) = mpsc::sync_channel::<
            explorer_extension_ui_api::CellRenderContextV1,
        >(DIRECT_RENDER_QUEUE_CAP_V1);
        // At most the bounded request capacity can be outstanding, so this
        // receiver cannot grow without bound. Unlike a sync reply queue it
        // guarantees every pending key receives a terminal response.
        let (result_tx, result_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name(format!("plugin-render-{contribution_id}"))
            .spawn(move || {
                while let Ok(context) = request_rx.recv() {
                    let key = cell_render_key(&context);
                    let fallback_color = context.theme.muted_foreground;
                    let plan = renderer
                        .render(contribution_id, context)
                        .unwrap_or_else(|error| {
                            explorer_extension_ui_api::CellRenderPlanV1::text_only(
                                format!("Renderer unavailable: {error}"),
                                fallback_color,
                            )
                        });
                    if result_tx.send((key, plan)).is_err() {
                        return;
                    }
                }
            })
            .context("failed to start direct plugin render worker")?;
        Ok(Self {
            requests: request_tx,
            results: Mutex::new(result_rx),
            pending: Mutex::new(HashSet::new()),
            cache: Mutex::new(HashMap::new()),
        })
    }

    fn drain_ready(&self) -> bool {
        let mut changed = false;
        if let Ok(results) = self.results.lock() {
            while let Ok((ready_key, plan)) = results.try_recv() {
                if let Ok(mut pending) = self.pending.lock() {
                    pending.remove(&ready_key);
                }
                if let Ok(mut cache) = self.cache.lock() {
                    if cache.len() >= DIRECT_RENDER_CACHE_CAP_V1 {
                        cache.clear();
                    }
                    cache.insert(ready_key, plan);
                    changed = true;
                }
            }
        }
        changed
    }

    fn render_or_enqueue(
        &self,
        context: explorer_extension_ui_api::CellRenderContextV1,
        unavailable_label: &'static str,
    ) -> explorer_extension_ui_api::CellRenderPlanV1 {
        let key = cell_render_key(&context);
        let _ = self.drain_ready();
        if let Ok(cache) = self.cache.lock()
            && let Some(plan) = cache.get(&key)
        {
            return plan.clone();
        }
        let should_enqueue = self
            .pending
            .lock()
            .map(|mut pending| pending.insert(key.clone()))
            .unwrap_or(false);
        if should_enqueue && self.requests.try_send(context.clone()).is_err() {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&key);
            }
        }
        explorer_extension_ui_api::CellRenderPlanV1::text_only(
            unavailable_label,
            context.theme.muted_foreground,
        )
    }
}

struct SizeMapRenderRequestV1 {
    key: SizeMapRenderKeyV1,
    context: explorer_extension_ui_api::SizeMapRenderContextV1,
    mappings: HashMap<explorer_extension_ui_api::StableIdV1, SizeMapProjectionV1>,
    width: f32,
    height: f32,
}

enum SizeMapProjectionV1 {
    Item(
        explorer_ui::size_map_view::SizeMapInteractionTargetV1,
        String,
    ),
    Aggregate(Vec<explorer_ui::size_map_view::SizeMapAggregateItemV1>),
}

struct AsyncSizeMapRendererV1 {
    requests: mpsc::SyncSender<SizeMapRenderRequestV1>,
    results: Mutex<
        mpsc::Receiver<(
            SizeMapRenderKeyV1,
            explorer_ui::size_map_view::SizeMapRenderPlanV1,
        )>,
    >,
    pending: Mutex<HashSet<SizeMapRenderKeyV1>>,
    cache: Mutex<HashMap<SizeMapRenderKeyV1, explorer_ui::size_map_view::SizeMapRenderPlanV1>>,
}

impl AsyncSizeMapRendererV1 {
    fn start(
        mut renderer: explorer_extension_host::SinglePluginSizeMapViewRuntimeV1,
    ) -> Result<Self, Error> {
        let (request_tx, request_rx) =
            mpsc::sync_channel::<SizeMapRenderRequestV1>(SIZE_MAP_RENDER_QUEUE_CAP_V1);
        let (result_tx, result_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("plugin-render-size-map".to_owned())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    let SizeMapRenderRequestV1 {
                        key,
                        context,
                        mappings,
                        width,
                        height,
                    } = request;
                    let snapshot = context.snapshot;
                    let plan = match renderer.render(SIZE_MAP_VIEW_CONTRIBUTION_ID_V1, context) {
                        Ok(plan) if plan.snapshot == snapshot => {
                            project_size_map_plan(plan, mappings, width, height)
                        }
                        Ok(_) => {
                            size_map_render_fallback("Size Map renderer returned a stale plan")
                        }
                        Err(error) => size_map_render_fallback(&format!(
                            "Size Map renderer unavailable: {error}"
                        )),
                    };
                    if result_tx.send((key, plan)).is_err() {
                        return;
                    }
                }
            })
            .context("failed to start Size Map plugin render worker")?;
        Ok(Self {
            requests: request_tx,
            results: Mutex::new(result_rx),
            pending: Mutex::new(HashSet::new()),
            cache: Mutex::new(HashMap::new()),
        })
    }

    fn drain_ready(&self) -> bool {
        let mut changed = false;
        if let Ok(results) = self.results.lock() {
            while let Ok((key, plan)) = results.try_recv() {
                if let Ok(mut pending) = self.pending.lock() {
                    pending.remove(&key);
                }
                if let Ok(mut cache) = self.cache.lock() {
                    if cache.len() >= SIZE_MAP_RENDER_CACHE_CAP_V1 {
                        cache.clear();
                    }
                    cache.insert(key, plan);
                    changed = true;
                }
            }
        }
        changed
    }

    fn render_or_enqueue(
        &self,
        request: SizeMapRenderRequestV1,
    ) -> explorer_ui::size_map_view::SizeMapRenderPlanV1 {
        let _ = self.drain_ready();
        if let Ok(cache) = self.cache.lock()
            && let Some(plan) = cache.get(&request.key)
        {
            return plan.clone();
        }
        let key = request.key.clone();
        let should_enqueue = self
            .pending
            .lock()
            .map(|mut pending| pending.insert(key.clone()))
            .unwrap_or(false);
        if should_enqueue && self.requests.try_send(request).is_err() {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&key);
            }
        }
        size_map_render_fallback("Loading Size Map")
    }
}

fn size_map_render_fallback(status: &str) -> explorer_ui::size_map_view::SizeMapRenderPlanV1 {
    explorer_ui::size_map_view::SizeMapRenderPlanV1 {
        snapshot: None,
        rectangles: Vec::new(),
        status: Some(status.to_owned()),
        available: false,
    }
}

fn project_size_map_plan(
    plan: explorer_extension_ui_api::SizeMapRenderPlanV1,
    mappings: HashMap<explorer_extension_ui_api::StableIdV1, SizeMapProjectionV1>,
    width: f32,
    height: f32,
) -> explorer_ui::size_map_view::SizeMapRenderPlanV1 {
    let snapshot = plan.snapshot;
    explorer_ui::size_map_view::SizeMapRenderPlanV1 {
        snapshot: Some(snapshot),
        rectangles: plan
            .rectangles
            .into_iter()
            .filter_map(|rectangle| {
                let mapping = mappings.get(&rectangle.node_id)?;
                let (item_id, interaction_target, status, aggregate_items) = match mapping {
                    SizeMapProjectionV1::Item(target, status) => (
                        Some(target.item_id.clone()),
                        Some(target.clone()),
                        status.clone(),
                        Vec::new(),
                    ),
                    SizeMapProjectionV1::Aggregate(items) => {
                        (None, None, "Aggregated".to_owned(), items.clone())
                    }
                };
                Some(explorer_ui::size_map_view::SizeMapRectangleV1 {
                    node_id: Some(rectangle.node_id),
                    item_id,
                    interaction_target,
                    x: width * rectangle.x_millionths as f32 / 1_000_000.0,
                    y: height * rectangle.y_millionths as f32 / 1_000_000.0,
                    width: width * rectangle.width_millionths as f32 / 1_000_000.0,
                    height: height * rectangle.height_millionths as f32 / 1_000_000.0,
                    label: rectangle.label.into_string(),
                    detail: rectangle.detail.into_string(),
                    color: explorer_ui::theme::Rgba8 {
                        red: rectangle.color.red,
                        green: rectangle.color.green,
                        blue: rectangle.color.blue,
                        alpha: rectangle.color.alpha,
                    },
                    status,
                    aggregate_items,
                })
            })
            .collect(),
        status: (!plan.status.is_empty()).then(|| plan.status.into_string()),
        available: true,
    }
}

/// App-owned boundary for projecting runtime-ready extension batches into the
/// current list model. The application, rather than the host transport, owns
/// stable item identities and therefore is the only layer allowed to drain,
/// apply, and acknowledge ready work.
trait ApplicationExtensionReadyProjectorV1 {
    fn project_ready(
        &mut self,
        pump: &mut explorer_extension_host::ExtensionJobUiPumpV1,
        runtime: &Arc<explorer_extension_host::ExtensionJobRuntimeV1>,
        ingress: &explorer_extension_host::ExtensionJobUiIngressV1,
    ) -> Result<usize, explorer_extension_host::ExtensionJobUiPumpErrorV1>;
}

/// Deliberately preserves ready work until the dynamic-column model installs
/// its identity-aware projector. It must not consume a signal merely to make
/// an incomplete composition path appear live.
struct DeferredApplicationExtensionReadyProjectorV1;

impl ApplicationExtensionReadyProjectorV1 for DeferredApplicationExtensionReadyProjectorV1 {
    fn project_ready(
        &mut self,
        _pump: &mut explorer_extension_host::ExtensionJobUiPumpV1,
        _runtime: &Arc<explorer_extension_host::ExtensionJobRuntimeV1>,
        _ingress: &explorer_extension_host::ExtensionJobUiIngressV1,
    ) -> Result<usize, explorer_extension_host::ExtensionJobUiPumpErrorV1> {
        Ok(0)
    }
}

/// GPUI-thread composition of the host's unique UI inbox and its !Send
/// invalidation batcher. The UI crate sees only the host-agnostic poll trait;
/// this app layer additionally owns the ready-projector callback.
struct ApplicationExtensionUiPumpV1 {
    pump: explorer_extension_host::ExtensionJobUiPumpV1,
    runtime: Arc<explorer_extension_host::ExtensionJobRuntimeV1>,
    ingress: explorer_extension_host::ExtensionJobUiIngressV1,
    ready_projector: Box<dyn ApplicationExtensionReadyProjectorV1>,
}

impl ApplicationExtensionUiPumpV1 {
    fn new(
        inbox: explorer_extension_host::ExtensionJobUiInboxV1,
        ingress: explorer_extension_host::ExtensionJobUiIngressV1,
    ) -> Option<Self> {
        Self::with_ready_projector(
            inbox,
            ingress,
            Box::new(DeferredApplicationExtensionReadyProjectorV1),
        )
    }

    fn with_ready_projector(
        inbox: explorer_extension_host::ExtensionJobUiInboxV1,
        ingress: explorer_extension_host::ExtensionJobUiIngressV1,
        ready_projector: Box<dyn ApplicationExtensionReadyProjectorV1>,
    ) -> Option<Self> {
        if !ingress.is_for_runtime(inbox.runtime()) {
            return None;
        }
        let config = explorer_extension_host::UiInvalidationBatcherConfigV1::try_new(
            Duration::from_millis(20),
            explorer_extension_host::MAX_UI_INVALIDATION_SCOPES_V1,
        )
        .ok()?;
        let runtime = Arc::clone(inbox.runtime());
        Some(Self {
            pump: explorer_extension_host::ExtensionJobUiPumpV1::new(inbox, config),
            runtime,
            ingress,
            ready_projector,
        })
    }

    #[cfg(test)]
    fn set_ready_projector(
        &mut self,
        ready_projector: Box<dyn ApplicationExtensionReadyProjectorV1>,
    ) {
        self.ready_projector = ready_projector;
    }
}

impl explorer_ui::ExtensionUiPumpPortV1 for ApplicationExtensionUiPumpV1 {
    fn poll_due(&mut self, now: Instant) -> bool {
        // The current app has no dynamic-column identity model yet. Its
        // deferred projector leaves ready signals intact; task 5 installs the
        // concrete callback that drains, atomically applies, and then notifies
        // through this same app-owned seam.
        let _ = self
            .ready_projector
            .project_ready(&mut self.pump, &self.runtime, &self.ingress);
        let _ = self.pump.poll_applied(1_024);
        let _ = self.pump.next_deadline();
        matches!(self.pump.drain_due(now), Ok(Some(_)))
    }
}

/// One-process bridge for the single P0 folder-size example. The measure
/// object lives exclusively on its worker thread; the renderer is serialized
/// on the GPUI caller thread through this narrow host-owned mutex.
struct ApplicationVisualColumnRuntimeV1 {
    pending: Arc<(Mutex<PendingFolderSizeWorkV1>, Condvar)>,
    results: Mutex<mpsc::Receiver<explorer_ui::folder_size_column::FolderSizeResultV1>>,
    renderer: Option<AsyncCellRendererV1>,
    backend_status: Arc<AtomicU8>,
    backend_active: Arc<AtomicBool>,
    directory_facts_active: AtomicBool,
    directory_facts_focus: crate::mft_focus::FocusWindowReporterV1,
}

const HOST_EXTENSION_COLUMN_CACHE_CAPACITY_V1: usize = 16_384;
static HOST_EXTENSION_MEMORY_LIMIT_BYTES_V1: AtomicU64 = AtomicU64::new(32 * 1024 * 1024);
static HOST_EXTENSION_DISK_LIMIT_BYTES_V1: AtomicU64 = AtomicU64::new(256 * 1024 * 1024);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct HostExtensionColumnCacheKeyV1 {
    canonical_path: PathBuf,
    modified_seconds: u64,
    modified_nanos: u32,
}

fn unix_seconds_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn host_extension_column_cache_key(path: &Path) -> Option<HostExtensionColumnCacheKeyV1> {
    let canonical_path = fs::canonicalize(path).ok()?;
    let modified = fs::metadata(&canonical_path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?;
    Some(HostExtensionColumnCacheKeyV1 {
        canonical_path,
        modified_seconds: modified.as_secs(),
        modified_nanos: modified.subsec_nanos(),
    })
}

#[derive(Debug)]
struct HostExtensionColumnCacheV1<T> {
    /// Entries keyed by canonical path + the directory mtime observed when
    /// the value was produced. The value tuple is
    /// `(directory, directory_epoch, cached_at_seconds, value)`.
    values: HashMap<HostExtensionColumnCacheKeyV1, (PathBuf, u64, u64, T)>,
    directory_epochs: HashMap<PathBuf, u64>,
    persistent_namespace: Option<&'static str>,
    active_root: Option<PathBuf>,
    active_depth: usize,
    /// When `Some`, a lookup whose mtime no longer matches may still reuse an
    /// in-memory entry whose mtime is newer than the current one (or older,
    /// see `get`) as long as it was written within this many seconds.
    telemetry: Arc<HostExtensionCacheTrackerV1>,
}

#[derive(Debug, Default)]
struct HostExtensionCacheTrackerV1 {
    bytes: AtomicU64,
    entries: AtomicU64,
}

static HOST_EXTENSION_CACHE_TRACKERS_V1: OnceLock<
    Mutex<Vec<std::sync::Weak<HostExtensionCacheTrackerV1>>>,
> = OnceLock::new();

fn register_host_extension_cache_tracker_v1() -> Arc<HostExtensionCacheTrackerV1> {
    let tracker = Arc::new(HostExtensionCacheTrackerV1::default());
    if let Ok(mut trackers) = HOST_EXTENSION_CACHE_TRACKERS_V1
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
    {
        trackers.retain(|candidate| candidate.strong_count() != 0);
        trackers.push(Arc::downgrade(&tracker));
    }
    tracker
}

pub(crate) fn host_extension_cache_telemetry_v1() -> (u64, u64) {
    let Some(trackers) = HOST_EXTENSION_CACHE_TRACKERS_V1.get() else {
        return (0, 0);
    };
    let Ok(mut trackers) = trackers.lock() else {
        return (0, 0);
    };
    let mut bytes = 0_u64;
    let mut entries = 0_u64;
    trackers.retain(|candidate| {
        let Some(tracker) = candidate.upgrade() else {
            return false;
        };
        bytes = bytes.saturating_add(tracker.bytes.load(Ordering::Acquire));
        entries = entries.saturating_add(tracker.entries.load(Ordering::Acquire));
        true
    });
    (bytes, entries)
}

pub(crate) fn host_extension_cache_limit_v1() -> u64 {
    HOST_EXTENSION_MEMORY_LIMIT_BYTES_V1.load(Ordering::Acquire)
}

pub(crate) fn host_extension_persistent_cache_limit_v1() -> u64 {
    HOST_EXTENSION_DISK_LIMIT_BYTES_V1.load(Ordering::Acquire)
}

pub(crate) fn host_extension_persistent_cache_telemetry_v1() -> (u64, u64) {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return (0, 0);
    };
    let root = PathBuf::from(local_app_data)
        .join("SuperExplorer")
        .join("data-column-cache");
    let Ok(metadata) = fs::symlink_metadata(&root) else {
        return (0, 0);
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return (0, 0);
    }
    let mut bytes = 0_u64;
    let mut entries = 0_u64;
    let mut visited = 0_usize;
    let mut pending = vec![root];
    while let Some(directory) = pending.pop() {
        visited = visited.saturating_add(1);
        if visited > 100_000 {
            break;
        }
        let Ok(children) = fs::read_dir(directory) else {
            continue;
        };
        for child in children.flatten() {
            let Ok(metadata) = fs::symlink_metadata(child.path()) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(child.path());
            } else if metadata.is_file() {
                bytes = bytes.saturating_add(metadata.len());
                entries = entries.saturating_add(1);
            }
        }
    }
    (bytes, entries)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HostExtensionColumnCacheAdmissionV1 {
    key: HostExtensionColumnCacheKeyV1,
    directory: PathBuf,
    directory_epoch: u64,
}

impl<T> Default for HostExtensionColumnCacheV1<T> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            directory_epochs: HashMap::new(),
            persistent_namespace: None,
            active_root: None,
            active_depth: 3,
            telemetry: register_host_extension_cache_tracker_v1(),
        }
    }
}

trait HostExtensionColumnCacheValueV1: Clone {
    fn encode_cache_value(&self) -> serde_json::Value;
    fn decode_cache_value(value: &serde_json::Value) -> Option<Self>;
}

impl<T: HostExtensionColumnCacheValueV1> HostExtensionColumnCacheV1<T> {
    fn entry_bytes(key: &HostExtensionColumnCacheKeyV1, directory: &Path, value: &T) -> u64 {
        let value_bytes = serde_json::to_vec(&value.encode_cache_value())
            .map_or(0_u64, |bytes| bytes.len().try_into().unwrap_or(u64::MAX));
        (key.canonical_path.to_string_lossy().len() as u64)
            .saturating_add(directory.to_string_lossy().len() as u64)
            .saturating_add(value_bytes)
            .saturating_add(64)
    }

    fn trim_memory_budget(&mut self) {
        let limit = HOST_EXTENSION_MEMORY_LIMIT_BYTES_V1.load(Ordering::Acquire);
        while self
            .values
            .iter()
            .fold(0_u64, |total, (key, (directory, _, _, value))| {
                total.saturating_add(Self::entry_bytes(key, directory, value))
            })
            > limit
        {
            let Some(oldest_key) = self
                .values
                .iter()
                .min_by_key(|(_, (_, _, cached_at, _))| *cached_at)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.values.remove(&oldest_key);
        }
    }

    fn refresh_telemetry(&self) {
        let bytes = self
            .values
            .iter()
            .fold(0_u64, |total, (key, (directory, _, _, value))| {
                let value_bytes = serde_json::to_vec(&value.encode_cache_value())
                    .map_or(0_u64, |bytes| bytes.len().try_into().unwrap_or(u64::MAX));
                let key_bytes = key.canonical_path.to_string_lossy().len() as u64;
                let directory_bytes = directory.to_string_lossy().len() as u64;
                total
                    .saturating_add(key_bytes)
                    .saturating_add(directory_bytes)
                    .saturating_add(value_bytes)
                    .saturating_add(64)
            });
        self.telemetry.bytes.store(bytes, Ordering::Release);
        self.telemetry.entries.store(
            self.values.len().try_into().unwrap_or(u64::MAX),
            Ordering::Release,
        );
    }

    fn persistent(namespace: &'static str) -> Self {
        Self {
            persistent_namespace: Some(namespace),
            ..Self::default()
        }
    }

    fn admission(&self, path: &Path) -> Option<HostExtensionColumnCacheAdmissionV1> {
        let key = host_extension_column_cache_key(path)?;
        if let Some(root) = self.active_root.as_deref()
            && !path_is_within_depth(&key.canonical_path, root, self.active_depth)
        {
            return None;
        }
        let directory = key.canonical_path.parent()?.to_path_buf();
        let directory_epoch = self.directory_epochs.get(&directory).copied().unwrap_or(0);
        Some(HostExtensionColumnCacheAdmissionV1 {
            key,
            directory,
            directory_epoch,
        })
    }

    fn retain_window(&mut self, root: &Path, max_depth: usize) {
        let Ok(root) = fs::canonicalize(root) else {
            return;
        };
        if self.active_root.as_deref() == Some(root.as_path()) && self.active_depth == max_depth {
            return;
        }
        self.active_root = Some(root.clone());
        self.active_depth = max_depth;
        self.values
            .retain(|key, _| path_is_within_depth(&key.canonical_path, &root, max_depth));
        self.directory_epochs
            .retain(|directory, _| path_is_within_depth(directory, &root, max_depth));
        self.prune_persistent_window(&root, max_depth);
        self.refresh_telemetry();
    }

    fn prune_persistent_window(&self, root: &Path, max_depth: usize) {
        let Some(namespace) = self.persistent_namespace else {
            return;
        };
        let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let directory = PathBuf::from(local_app_data)
            .join("SuperExplorer")
            .join("data-column-cache")
            .join("v1")
            .join(namespace);
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let keep = fs::symlink_metadata(&path).is_ok_and(|metadata| {
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() <= 16 * 1024
            }) && fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|record| record.get("path")?.as_str().map(PathBuf::from))
                .is_some_and(|cached| path_is_within_depth(&cached, root, max_depth));
            if !keep {
                let _ = fs::remove_file(path);
            }
        }
    }

    fn get(&self, admission: &HostExtensionColumnCacheAdmissionV1) -> Option<T> {
        // Exact mtime match: the directory has not changed since the value was
        // produced, so the entry is unconditionally fresh.
        if let Some((directory, epoch, _, value)) = self.values.get(&admission.key)
            && directory == &admission.directory
            && *epoch == admission.directory_epoch
        {
            return Some(value.clone());
        }
        self.read_persistent(admission)
    }

    fn insert(&mut self, admission: HostExtensionColumnCacheAdmissionV1, value: T) -> bool {
        if self
            .directory_epochs
            .get(&admission.directory)
            .copied()
            .unwrap_or(0)
            != admission.directory_epoch
        {
            return false;
        }
        if self.values.len() >= HOST_EXTENSION_COLUMN_CACHE_CAPACITY_V1
            && !self.values.contains_key(&admission.key)
        {
            self.values.clear();
        }
        self.values.insert(
            admission.key.clone(),
            (
                admission.directory.clone(),
                admission.directory_epoch,
                unix_seconds_now(),
                value,
            ),
        );
        self.trim_memory_budget();
        if let Some((_, _, _, value)) = self.values.get(&admission.key) {
            self.write_persistent(&admission, value);
        }
        self.refresh_telemetry();
        true
    }

    fn invalidate_directory(&mut self, directory: &Path) {
        let Ok(directory) = fs::canonicalize(directory) else {
            return;
        };
        let epoch = self.directory_epochs.entry(directory.clone()).or_insert(0);
        *epoch = epoch.wrapping_add(1);
        self.values
            .retain(|_, (scope, _, _, _)| scope != &directory);
        self.refresh_telemetry();
    }

    fn persistent_path(&self, key: &HostExtensionColumnCacheKeyV1) -> Option<PathBuf> {
        let namespace = self.persistent_namespace?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        Some(
            root.join("SuperExplorer")
                .join("data-column-cache")
                .join("v1")
                .join(namespace)
                .join(format!("{:016x}.json", hasher.finish())),
        )
    }

    fn read_persistent(&self, admission: &HostExtensionColumnCacheAdmissionV1) -> Option<T> {
        let path = self.persistent_path(&admission.key)?;
        let metadata = fs::symlink_metadata(&path).ok()?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 16 * 1024 {
            return None;
        }
        let record: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
        (record.get("schema")?.as_u64()? == 1
            && record.get("path")?.as_str()? == admission.key.canonical_path.to_string_lossy()
            && record.get("modified_seconds")?.as_u64()? == admission.key.modified_seconds
            && record.get("modified_nanos")?.as_u64()? == u64::from(admission.key.modified_nanos))
        .then(|| T::decode_cache_value(record.get("value")?))?
    }

    fn write_persistent(&self, admission: &HostExtensionColumnCacheAdmissionV1, value: &T) {
        let Some(destination) = self.persistent_path(&admission.key) else {
            return;
        };
        let Some(directory) = destination.parent() else {
            return;
        };
        if fs::create_dir_all(directory).is_err() {
            return;
        }
        let record = serde_json::json!({
            "schema": 1,
            "path": admission.key.canonical_path.to_string_lossy(),
            "modified_seconds": admission.key.modified_seconds,
            "modified_nanos": admission.key.modified_nanos,
            "value": value.encode_cache_value(),
        });
        let Ok(bytes) = serde_json::to_vec(&record) else {
            return;
        };
        if bytes.len() > 16 * 1024 {
            return;
        }
        let temporary = destination.with_extension(format!("{}.tmp", std::process::id()));
        if fs::write(&temporary, bytes).is_ok() {
            let _ = fs::remove_file(&destination);
            if fs::rename(&temporary, &destination).is_err() {
                let _ = fs::remove_file(temporary);
            }
        }
    }
}

fn configure_host_extension_cache_budgets(memory_mb: u32, disk_mb: u32) {
    HOST_EXTENSION_MEMORY_LIMIT_BYTES_V1
        .store(u64::from(memory_mb) * 1024 * 1024, Ordering::Release);
    let disk_limit = u64::from(disk_mb) * 1024 * 1024;
    HOST_EXTENSION_DISK_LIMIT_BYTES_V1.store(disk_limit, Ordering::Release);
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let root = PathBuf::from(local_app_data)
        .join("SuperExplorer")
        .join("data-column-cache");
    let Ok(namespaces) = fs::read_dir(root) else {
        return;
    };
    let mut files = Vec::new();
    let mut pending = namespaces
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
                files.push((modified, metadata.len(), path));
            }
        }
    }
    files.sort_by_key(|(modified, _, _)| *modified);
    let mut total = files
        .iter()
        .fold(0_u64, |sum, (_, bytes, _)| sum.saturating_add(*bytes));
    for (_, bytes, path) in files {
        if total <= disk_limit {
            break;
        }
        if fs::remove_file(path).is_ok() {
            total = total.saturating_sub(bytes);
        }
    }
}

fn path_is_within_depth(path: &Path, root: &Path, max_depth: usize) -> bool {
    path.strip_prefix(root)
        .is_ok_and(|relative| relative.components().count() <= max_depth)
}

fn request_cache_root<'a>(paths: impl Iterator<Item = &'a Path>) -> Option<PathBuf> {
    paths.filter_map(Path::parent).next().map(Path::to_path_buf)
}

impl HostExtensionColumnCacheValueV1 for u64 {
    fn encode_cache_value(&self) -> serde_json::Value {
        (*self).into()
    }

    fn decode_cache_value(value: &serde_json::Value) -> Option<Self> {
        value.as_u64()
    }
}

#[derive(Default)]
struct PendingFolderSizeWorkV1 {
    requests: Option<Vec<explorer_ui::folder_size_column::FolderSizeRequestV1>>,
    in_flight: HashMap<FolderSizeWorkIdentityV1, explorer_model::RequestId>,
    cancelled: HashSet<explorer_model::RequestId>,
    stopped: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct FolderSizeWorkIdentityV1 {
    tab_id: explorer_model::TabId,
    generation: explorer_model::Generation,
    item_id: explorer_model::ShellItemId,
    path: PathBuf,
}

impl From<&explorer_ui::folder_size_column::FolderSizeRequestV1> for FolderSizeWorkIdentityV1 {
    fn from(request: &explorer_ui::folder_size_column::FolderSizeRequestV1) -> Self {
        Self {
            tab_id: request.context.tab_id,
            generation: request.context.generation,
            item_id: request.item_id.clone(),
            path: request.path.clone(),
        }
    }
}

fn enqueue_folder_size_requests(
    state: &mut PendingFolderSizeWorkV1,
    requests: Vec<explorer_ui::folder_size_column::FolderSizeRequestV1>,
) {
    let pending = state.requests.get_or_insert_with(Vec::new);
    for request in requests {
        let identity = FolderSizeWorkIdentityV1::from(&request);
        if !state.in_flight.contains_key(&identity)
            && !pending
                .iter()
                .any(|queued| FolderSizeWorkIdentityV1::from(queued) == identity)
        {
            pending.push(request);
        }
    }
}

fn take_folder_size_batch(
    state: &mut PendingFolderSizeWorkV1,
    limit: usize,
) -> Vec<explorer_ui::folder_size_column::FolderSizeRequestV1> {
    let Some(pending) = state.requests.as_mut() else {
        return Vec::new();
    };
    let Some(first) = pending.first() else {
        state.requests = None;
        return Vec::new();
    };
    let context = first.context.clone();
    let mut batch = Vec::with_capacity(limit.min(pending.len()));
    let mut index = 0;
    while index < pending.len() && batch.len() < limit {
        if pending[index].context == context {
            batch.push(pending.remove(index));
        } else {
            index += 1;
        }
    }
    if pending.is_empty() {
        state.requests = None;
    }
    state.in_flight.extend(batch.iter().map(|request| {
        (
            FolderSizeWorkIdentityV1::from(request),
            request.context.request_id,
        )
    }));
    batch
}

fn finish_folder_size_request(
    pending: &(Mutex<PendingFolderSizeWorkV1>, Condvar),
    request: &explorer_ui::folder_size_column::FolderSizeRequestV1,
) {
    let (lock, _) = pending;
    let mut state = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let identity = FolderSizeWorkIdentityV1::from(request);
    if state.in_flight.get(&identity) == Some(&request.context.request_id) {
        state.in_flight.remove(&identity);
    }
}

fn folder_size_request_cancelled(
    pending: &(Mutex<PendingFolderSizeWorkV1>, Condvar),
    request: &explorer_ui::folder_size_column::FolderSizeRequestV1,
) -> bool {
    pending
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .cancelled
        .contains(&request.context.request_id)
}

fn cancel_folder_size_context(
    state: &mut PendingFolderSizeWorkV1,
    context: &explorer_model::RequestContext,
) {
    state.cancelled.insert(context.request_id);
    if let Some(requests) = state.requests.as_mut() {
        requests.retain(|request| request.context.request_id != context.request_id);
        if requests.is_empty() {
            state.requests = None;
        }
    }
    state
        .in_flight
        .retain(|_, request_id| *request_id != context.request_id);
}

fn publish_mft_folder_result_v1(
    pending: &(Mutex<PendingFolderSizeWorkV1>, Condvar),
    backend_status: &AtomicU8,
    snapshot_service: &Arc<Mutex<crate::folder_size_service::FolderSizeServiceV1>>,
    result_tx: &mpsc::SyncSender<explorer_ui::folder_size_column::FolderSizeResultV1>,
    request: explorer_ui::folder_size_column::FolderSizeRequestV1,
    started: Instant,
    measured: Result<crate::mft_query::FolderAggregateQueryV1, String>,
) -> bool {
    finish_folder_size_request(pending, &request);
    if folder_size_request_cancelled(pending, &request) {
        return true;
    }
    let measured = measured.and_then(|aggregate| {
        (!aggregate.partial)
            .then_some((
                aggregate.logical_bytes,
                Some(explorer_ui::folder_size_column::DirectoryFactsV1 {
                    mft_generation: aggregate.generation,
                    file_count: aggregate.file_count,
                    folder_count: aggregate.directory_count.saturating_sub(1),
                }),
            ))
            .ok_or_else(|| {
                format!(
                    "MFT Service returned partial aggregate (generation={}, logical_bytes={}, file_count={}, directory_count={})",
                    aggregate.generation,
                    aggregate.logical_bytes,
                    aggregate.file_count,
                    aggregate.directory_count,
                )
            })
    });
    let measured = measured.or_else(|mft_error| {
        let fallback = snapshot_service
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot_or_scan_recursive(&request.path, request.context.generation.value(), || {
                folder_size_request_cancelled(pending, &request)
            })
            .and_then(|snapshot| {
                (snapshot.status == crate::folder_size_service::SnapshotStatusV1::Complete)
                    .then_some((
                        snapshot.aggregate.recursive_bytes,
                        Some(explorer_ui::folder_size_column::DirectoryFactsV1 {
                            mft_generation: snapshot
                                .mft_generation
                                .unwrap_or(snapshot.refresh_generation),
                            file_count: snapshot.aggregate.file_count,
                            folder_count: snapshot.aggregate.directory_count.saturating_sub(1),
                        }),
                    ))
                    .ok_or_else(|| {
                        format!(
                            "shared folder snapshot ended with {:?}: {}",
                            snapshot.status,
                            snapshot.diagnostic.as_deref().unwrap_or("no diagnostic")
                        )
                    })
            });
        match fallback {
            Ok(value) => {
                backend_status.store(1, Ordering::Release);
                Ok(value)
            }
            Err(fallback_error) => Err(format!(
                "MFT query failed: {mft_error}; recursive fallback failed: {fallback_error}"
            )),
        }
    });
    let (exact_bytes, directory_facts, error) = match measured {
        Ok((exact_bytes, directory_facts)) => (Some(exact_bytes), directory_facts, None),
        Err(reason) => {
            let service_diagnostics =
                crate::mft_query::query_durability_diagnostics().map(|volumes| {
                    volumes.into_iter().find(|volume| {
                        request
                            .path
                            .to_string_lossy()
                            .as_bytes()
                            .first()
                            .is_some_and(|letter| letter.to_ascii_uppercase() == volume.volume)
                    })
                });
            let diagnostic = format!(
                "SUPEREXPLORER_FOLDER_SIZE_UNAVAILABLE path={} tab_id={:?} request_id={:?} generation={} item_id={:?} elapsed_ms={} stage=mft_service_batch_query error={reason} service_diagnostics={service_diagnostics:?}",
                request.path.display(),
                request.context.tab_id,
                request.context.request_id,
                request.context.generation.value(),
                request.item_id,
                started.elapsed().as_millis(),
            );
            eprintln!("{diagnostic}");
            explorer_common::record_process_error_message(
                ErrorSeverity::Error,
                "folder_size",
                "mft_service_batch_query",
                &diagnostic,
                Some(file!()),
            );
            backend_status.store(4, Ordering::Release);
            (None, None, Some(reason))
        }
    };
    result_tx
        .send(explorer_ui::folder_size_column::FolderSizeResultV1 {
            context: request.context,
            item_id: request.item_id,
            exact_bytes,
            directory_facts,
            partial: false,
            error,
        })
        .is_ok()
}

impl ApplicationVisualColumnRuntimeV1 {
    fn start(
        _measure: explorer_extension_host::SinglePluginVisualMeasureRuntimeV1,
        renderer: explorer_extension_host::SinglePluginVisualRenderRuntimeV1,
        snapshot_service: Arc<Mutex<crate::folder_size_service::FolderSizeServiceV1>>,
    ) -> Result<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1, Error> {
        tracing::info!(
            requirement = "folder.aggregate",
            "legacy visual-column measurement callback retained for compatibility but bypassed; host snapshot data is authoritative"
        );
        Self::start_with_renderer(Some(renderer), snapshot_service)
    }

    fn start_directory_facts(
        snapshot_service: Arc<Mutex<crate::folder_size_service::FolderSizeServiceV1>>,
    ) -> Result<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1, Error> {
        Self::start_with_renderer(None, snapshot_service)
    }

    fn start_with_renderer(
        renderer: Option<explorer_extension_host::SinglePluginVisualRenderRuntimeV1>,
        snapshot_service: Arc<Mutex<crate::folder_size_service::FolderSizeServiceV1>>,
    ) -> Result<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1, Error> {
        let pending = Arc::new((
            Mutex::new(PendingFolderSizeWorkV1::default()),
            Condvar::new(),
        ));
        let backend_status = Arc::new(AtomicU8::new(0));
        let backend_active = Arc::new(AtomicBool::new(false));
        let (result_tx, result_rx) =
            mpsc::sync_channel::<explorer_ui::folder_size_column::FolderSizeResultV1>(1_024);
        let worker_pending = Arc::clone(&pending);
        let worker_backend_status = Arc::clone(&backend_status);
        let worker_backend_active = Arc::clone(&backend_active);
        let worker_result_tx = result_tx.clone();
        let worker_snapshot_service = Arc::clone(&snapshot_service);
        std::thread::Builder::new()
            .name("p0-folder-size-batch".to_owned())
            .spawn(move || {
                loop {
                    let batch = {
                        let (lock, ready) = &*worker_pending;
                        let mut state = lock
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        while state.requests.is_none() && !state.stopped {
                            state = ready
                                .wait(state)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                        if state.stopped {
                            return;
                        }
                        take_folder_size_batch(&mut state, 256)
                    };
                    if batch.is_empty() {
                        continue;
                    }
                    let mut active = HashMap::new();
                    for (index, request) in batch.into_iter().enumerate() {
                        if folder_size_request_cancelled(&worker_pending, &request) {
                            finish_folder_size_request(&worker_pending, &request);
                        } else if !fs::metadata(&request.path).is_ok_and(folder_size_candidate) {
                            finish_folder_size_request(&worker_pending, &request);
                            if worker_result_tx
                                .send(explorer_ui::folder_size_column::FolderSizeResultV1 {
                                    context: request.context,
                                    item_id: request.item_id,
                                    exact_bytes: None,
                                    directory_facts: None,
                                    partial: false,
                                    error: None,
                                })
                                .is_err()
                            {
                                return;
                            }
                        } else {
                            active.insert(index as u64 + 1, (request, Instant::now()));
                        }
                    }
                    if active.is_empty() {
                        continue;
                    }
                    if active
                        .values()
                        .any(|(request, _)| request.require_directory_facts)
                        && let Some(delay_ms) =
                            std::env::var_os("SUPEREXPLORER_DIRECTORY_FACTS_VALIDATION_DELAY_MS")
                                .and_then(|value| value.to_string_lossy().parse::<u64>().ok())
                                .map(|value| value.min(5_000))
                                .filter(|value| *value > 0)
                    {
                        std::thread::sleep(Duration::from_millis(delay_ms));
                    }
                    let cache_memory_mb = active
                        .values()
                        .next()
                        .map(|(request, _)| request.mft_cache_memory_mb)
                        .unwrap_or_default();
                    let mut protocol_requests = active
                        .iter()
                        .map(
                            |(request_id, (request, _))| crate::mft_query::FolderBatchRequestV1 {
                                request_id: *request_id,
                                path: request.path.clone(),
                            },
                        )
                        .collect::<Vec<_>>();
                    protocol_requests.sort_unstable_by_key(|request| request.request_id);
                    worker_backend_active.store(true, Ordering::Release);
                    worker_backend_status.store(2, Ordering::Release);
                    let batch_context = active
                        .values()
                        .next()
                        .map(|(request, _)| request.context.clone());
                    let batch_result = crate::mft_query::query_folders_batch(
                        &protocol_requests,
                        cache_memory_mb,
                        || {
                            let state = worker_pending
                                .0
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            state.stopped
                                || batch_context.as_ref().is_some_and(|context| {
                                    state.cancelled.contains(&context.request_id)
                                })
                        },
                        |completion| {
                            let Some((request, started)) = active.remove(&completion.request_id)
                            else {
                                return Err(format!(
                                    "MFT folder batch completed unknown request {}",
                                    completion.request_id
                                ));
                            };
                            publish_mft_folder_result_v1(
                                &worker_pending,
                                &worker_backend_status,
                                &worker_snapshot_service,
                                &worker_result_tx,
                                request,
                                started,
                                completion.result,
                            )
                            .then_some(())
                            .ok_or_else(|| "folder-size result receiver disconnected".to_owned())
                        },
                    );
                    worker_backend_active.store(false, Ordering::Release);
                    if let Err(error) = batch_result {
                        for (_, (request, started)) in active.drain() {
                            if !publish_mft_folder_result_v1(
                                &worker_pending,
                                &worker_backend_status,
                                &worker_snapshot_service,
                                &worker_result_tx,
                                request,
                                started,
                                Err(error.clone()),
                            ) {
                                return;
                            }
                        }
                    } else {
                        for (_, (request, _)) in active.drain() {
                            finish_folder_size_request(&worker_pending, &request);
                        }
                    }
                }
            })
            .context("failed to start P0 folder-size batch worker")?;
        drop(result_tx);
        Ok(Arc::new(Self {
            pending,
            results: Mutex::new(result_rx),
            backend_status,
            backend_active,
            directory_facts_active: AtomicBool::new(false),
            directory_facts_focus: crate::mft_focus::FocusWindowReporterV1::new(),
            renderer: renderer
                .map(|renderer| {
                    AsyncCellRendererV1::start(renderer, FOLDER_SIZE_RENDERER_CONTRIBUTION_ID_V1)
                })
                .transpose()?,
        }))
    }
}

#[cfg(windows)]
fn folder_size_candidate(metadata: fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    metadata.is_dir()
        && metadata.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) == 0
}

#[cfg(not(windows))]
fn folder_size_candidate(metadata: fs::Metadata) -> bool {
    metadata.is_dir()
}

impl explorer_ui::folder_size_column::VisualColumnRuntimePortV1
    for ApplicationVisualColumnRuntimeV1
{
    fn config(&self) -> explorer_ui::folder_size_column::VisualColumnConfigV1 {
        explorer_ui::folder_size_column::VisualColumnConfigV1::default()
    }

    fn configure_cache_budgets(&self, budgets: explorer_model::CacheBudgetSettingsV1) {
        configure_host_extension_cache_budgets(
            budgets.extension_memory_mb,
            budgets.extension_disk_mb,
        );
        explorer_shell_win::set_shell_disk_cache_limits(
            u64::from(budgets.icon_disk_mb) * 1024 * 1024,
            u64::from(budgets.thumbnail_disk_mb) * 1024 * 1024,
        );
        explorer_shell_win::set_shell_bc7_runtime_gates(
            budgets.icon_bc7_enabled,
            budgets.thumbnail_bc7_enabled,
        );
        let to_u16 = |value: u32| value.min(u32::from(u16::MAX)) as u16;
        configure_mft_budget_snapshot(crate::mft_query::MftCacheBudgetLimitsV1 {
            persisted_index_mb: to_u16(budgets.mft_persisted_index_mb),
            volume_index_mb: to_u16(budgets.mft_volume_index_mb),
            file_data_mb: to_u16(budgets.mft_file_data_mb),
            aggregate_mb: to_u16(budgets.mft_aggregates_mb),
            lru_mb: to_u16(budgets.mft_lru_mb),
        });
    }

    fn set_directory_facts_active(&self, active: bool) {
        if self.directory_facts_active.swap(active, Ordering::AcqRel) != active {
            self.directory_facts_focus.set_focused(active);
        }
    }

    fn backend_status(
        &self,
    ) -> (
        explorer_ui::folder_size_column::FolderSizeBackendStatusV1,
        bool,
    ) {
        let status = match self.backend_status.load(Ordering::Acquire) {
            1 => explorer_ui::folder_size_column::FolderSizeBackendStatusV1::HostCache,
            2 => explorer_ui::folder_size_column::FolderSizeBackendStatusV1::MftService,
            4 => explorer_ui::folder_size_column::FolderSizeBackendStatusV1::MftUnavailable,
            _ => explorer_ui::folder_size_column::FolderSizeBackendStatusV1::Idle,
        };
        (status, self.backend_active.load(Ordering::Acquire))
    }

    fn submit_folder_size_requests(
        &self,
        requests: Vec<explorer_ui::folder_size_column::FolderSizeRequestV1>,
    ) {
        if requests.is_empty() {
            return;
        }
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        enqueue_folder_size_requests(&mut state, requests);
        ready.notify_all();
    }

    fn cancel_folder_size_context(&self, context: &explorer_model::RequestContext) {
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cancel_folder_size_context(&mut state, context);
        ready.notify_one();
    }

    fn invalidate_directory_cache(&self, _directory: &Path) {}

    fn drain_folder_size_results(
        &self,
    ) -> Vec<explorer_ui::folder_size_column::FolderSizeResultV1> {
        let Ok(results) = self.results.lock() else {
            return Vec::new();
        };
        results.try_iter().collect()
    }

    fn drain_render_results(&self) -> bool {
        self.renderer
            .as_ref()
            .is_some_and(AsyncCellRendererV1::drain_ready)
    }

    fn render_cell(
        &self,
        context: explorer_extension_ui_api::CellRenderContextV1,
    ) -> explorer_extension_ui_api::CellRenderPlanV1 {
        match self.renderer.as_ref() {
            Some(renderer) => renderer.render_or_enqueue(context, "Loading folder size"),
            None => explorer_extension_ui_api::CellRenderPlanV1::text_only(
                "Folder size unavailable",
                context.theme.muted_foreground,
            ),
        }
    }
}

impl Drop for ApplicationVisualColumnRuntimeV1 {
    fn drop(&mut self) {
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.stopped = true;
        state.requests = None;
        ready.notify_one();
    }
}

struct ApplicationCodeLinesRuntimeV1 {
    pending: Arc<(Mutex<PendingCodeLinesWorkV1>, Condvar)>,
    request_epoch: Arc<AtomicU64>,
    results: Mutex<mpsc::Receiver<explorer_ui::code_lines_column::CodeLinesResultV1>>,
    cached_results: Mutex<Vec<explorer_ui::code_lines_column::CodeLinesResultV1>>,
    cache: Arc<Mutex<HostExtensionColumnCacheV1<CodeLinesCachedValueV1>>>,
    renderer: AsyncCellRendererV1,
    mode: BatchDetailsColumnModeV1,
    option_package_id: String,
    folder_admission: explorer_ui::code_lines_column::FolderAdmissionPolicyV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CodeLinesCachedValueV1 {
    value: Option<explorer_ui::code_lines_column::CodeLinesValueV1>,
    error: Option<String>,
}

impl HostExtensionColumnCacheValueV1 for CodeLinesCachedValueV1 {
    fn encode_cache_value(&self) -> serde_json::Value {
        serde_json::json!({
            "value": self.value.as_ref().map(|value| serde_json::json!({
                "language": value.language,
                "code": value.code,
                "comments": value.comments,
                "blanks": value.blanks,
                "total": value.total,
            })),
            "error": self.error,
        })
    }

    fn decode_cache_value(value: &serde_json::Value) -> Option<Self> {
        let code = value.get("value")?;
        let decoded = if code.is_null() {
            None
        } else {
            Some(explorer_ui::code_lines_column::CodeLinesValueV1 {
                language: code.get("language")?.as_str()?.to_owned(),
                code: code.get("code")?.as_u64()?,
                comments: code.get("comments")?.as_u64()?,
                blanks: code.get("blanks")?.as_u64()?,
                total: code.get("total")?.as_u64()?,
            })
        };
        Some(Self {
            value: decoded,
            error: value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        })
    }
}

fn partition_code_lines_cache_hits(
    cache: &Mutex<HostExtensionColumnCacheV1<CodeLinesCachedValueV1>>,
    requests: Vec<explorer_ui::code_lines_column::CodeLinesRequestV1>,
) -> (
    Vec<explorer_ui::code_lines_column::CodeLinesResultV1>,
    Vec<explorer_ui::code_lines_column::CodeLinesRequestV1>,
) {
    if let Some(root) = request_cache_root(requests.iter().map(|request| request.path.as_path()))
        && let Ok(mut cache) = cache.lock()
    {
        cache.retain_window(&root, 3);
    }
    let mut hits = Vec::new();
    let mut misses = Vec::new();
    for request in requests {
        let cached = cache.lock().ok().and_then(|cache| {
            let admission = cache.admission(&request.path)?;
            cache.get(&admission)
        });
        if let Some(cached) = cached {
            hits.push(explorer_ui::code_lines_column::CodeLinesResultV1 {
                context: request.context,
                item_id: request.item_id,
                value: cached.value,
                error: cached.error,
            });
        } else {
            misses.push(request);
        }
    }
    (hits, misses)
}

fn partition_batch_details_cache_hits(
    cache: &Mutex<HostExtensionColumnCacheV1<CodeLinesCachedValueV1>>,
    mode: BatchDetailsColumnModeV1,
    requests: Vec<explorer_ui::code_lines_column::CodeLinesRequestV1>,
) -> (
    Vec<explorer_ui::code_lines_column::CodeLinesResultV1>,
    Vec<explorer_ui::code_lines_column::CodeLinesRequestV1>,
) {
    if mode == BatchDetailsColumnModeV1::LockOwner {
        return (Vec::new(), requests);
    }
    partition_code_lines_cache_hits(cache, requests)
}

fn batch_details_cache_admission(
    cache: &Mutex<HostExtensionColumnCacheV1<CodeLinesCachedValueV1>>,
    mode: BatchDetailsColumnModeV1,
    path: &Path,
) -> Option<HostExtensionColumnCacheAdmissionV1> {
    if mode == BatchDetailsColumnModeV1::LockOwner {
        return None;
    }
    cache.lock().ok().and_then(|cache| cache.admission(path))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BatchDetailsColumnModeV1 {
    CodeLines,
    LuaCodeLines,
    LockOwner,
}

#[derive(Default)]
struct PendingCodeLinesWorkV1 {
    requests: Option<Vec<explorer_ui::code_lines_column::CodeLinesRequestV1>>,
    /// The generation currently being processed or queued. Incremental
    /// requests for this same visible folder append instead of cancelling
    /// earlier files that have not reached the provider yet.
    active: Option<(explorer_model::TabId, explorer_model::Generation)>,
    stopped: bool,
}

impl ApplicationCodeLinesRuntimeV1 {
    fn start(
        provider: explorer_extension_host::SinglePluginBatchColumnRuntimeV1,
        renderer: explorer_extension_host::SinglePluginVisualRenderRuntimeV1,
        mode: BatchDetailsColumnModeV1,
        option_package_id: String,
    ) -> Result<explorer_ui::code_lines_column::CodeLinesRuntimeHandleV1, Error> {
        let contribution_id = match mode {
            BatchDetailsColumnModeV1::CodeLines => CODE_LINES_CONTRIBUTION_ID_V1,
            BatchDetailsColumnModeV1::LuaCodeLines => LUA_CODE_LINES_CONTRIBUTION_ID_V1,
            BatchDetailsColumnModeV1::LockOwner => LOCK_OWNER_CONTRIBUTION_ID_V1,
        };
        let folder_admission = provider
            .folder_admission_policy(contribution_id)
            .map_or_else(
                explorer_ui::code_lines_column::FolderAdmissionPolicyV1::default,
                |policy| explorer_ui::code_lines_column::FolderAdmissionPolicyV1 {
                    max_file_count: policy.max_file_count.into_option(),
                    max_folder_count: policy.max_folder_count.into_option(),
                },
            );
        let pending = Arc::new((
            Mutex::new(PendingCodeLinesWorkV1::default()),
            Condvar::new(),
        ));
        let worker_pending = pending.clone();
        let cache_namespace = match mode {
            BatchDetailsColumnModeV1::CodeLines => "rust-code-lines",
            BatchDetailsColumnModeV1::LuaCodeLines => "lua-code-lines",
            BatchDetailsColumnModeV1::LockOwner => "lock-owner",
        };
        let cache = Arc::new(Mutex::new(HostExtensionColumnCacheV1::<
            CodeLinesCachedValueV1,
        >::persistent(cache_namespace)));
        let worker_cache = Arc::clone(&cache);
        let request_epoch = Arc::new(AtomicU64::new(0));
        let worker_epoch = request_epoch.clone();
        let (result_tx, result_rx) =
            mpsc::sync_channel::<explorer_ui::code_lines_column::CodeLinesResultV1>(1_024);
        std::thread::Builder::new()
            .name("rust-tokei-code-lines".to_owned())
            .spawn(move || {
                let Ok(config) = explorer_extension_host::ExtensionResultBufferConfigV1::try_new(
                    8,
                    8,
                    64,
                    64,
                    64,
                    1_024,
                    1_024,
                    1_024,
                    64 * 1024 * 1024,
                    64 * 1024 * 1024,
                    64 * 1024 * 1024,
                ) else {
                    return;
                };
                let runtime = explorer_extension_host::ExtensionJobRuntimeV1::new(config);
                loop {
                    let requests = {
                        let (lock, ready) = &*worker_pending;
                        let mut state = lock
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        while state.requests.is_none() && !state.stopped {
                            state = ready
                                .wait(state)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                        if state.stopped {
                            return;
                        }
                        state.requests.take().unwrap_or_default()
                    };
                    let epoch = worker_epoch.load(Ordering::Acquire);
                    let mut prepared = Vec::new();
                    let mut prepared_bytes = 0_usize;
                    for request in requests {
                        if worker_epoch.load(Ordering::Acquire) != epoch {
                            break;
                        }
                        let cache_admission =
                            batch_details_cache_admission(&worker_cache, mode, &request.path);
                        let cached = cache_admission.as_ref().and_then(|admission| {
                            worker_cache
                                .lock()
                                .ok()
                                .and_then(|cache| cache.get(admission))
                        });
                        if let Some(cached) = cached {
                            if current_code_lines_epoch(&worker_epoch, epoch)
                                && !publish_code_lines_result(
                                    &result_tx,
                                    explorer_ui::code_lines_column::CodeLinesResultV1 {
                                        context: request.context,
                                        item_id: request.item_id,
                                        value: cached.value,
                                        error: cached.error,
                                    },
                                )
                            {
                                return;
                            }
                            continue;
                        }
                        let bytes = match mode {
                            BatchDetailsColumnModeV1::CodeLines
                            | BatchDetailsColumnModeV1::LuaCodeLines => {
                                read_code_lines_path_bounded(&request.path)
                            }
                            BatchDetailsColumnModeV1::LockOwner => Ok(Some(Vec::new())),
                        };
                        let bytes = match bytes {
                            Ok(Some(bytes)) => bytes,
                            Ok(None) => {
                                if current_code_lines_epoch(&worker_epoch, epoch)
                                    && !publish_code_lines_result(
                                        &result_tx,
                                        explorer_ui::code_lines_column::CodeLinesResultV1 {
                                            context: request.context,
                                            item_id: request.item_id,
                                            value: None,
                                            error: Some("Unsupported source".to_owned()),
                                        },
                                    )
                                {
                                    return;
                                }
                                continue;
                            }
                            Err(error) => {
                                if current_code_lines_epoch(&worker_epoch, epoch)
                                    && !publish_code_lines_result(
                                        &result_tx,
                                        explorer_ui::code_lines_column::CodeLinesResultV1 {
                                            context: request.context,
                                            item_id: request.item_id,
                                            value: None,
                                            error: Some(error),
                                        },
                                    )
                                {
                                    return;
                                }
                                continue;
                            }
                        };
                        if worker_epoch.load(Ordering::Acquire) != epoch {
                            continue;
                        }
                        if !prepared.is_empty()
                            && (prepared.len() == CODE_LINES_BATCH_ITEMS_V1
                                || prepared_bytes.saturating_add(bytes.len())
                                    > explorer_extension_host::MAX_BATCH_COLUMN_INPUT_BYTES_V1)
                        {
                            process_code_lines_batch(
                                &provider,
                                &runtime,
                                std::mem::take(&mut prepared),
                                epoch,
                                &worker_epoch,
                                &result_tx,
                                mode,
                                &worker_cache,
                            );
                            prepared_bytes = 0;
                        }
                        prepared_bytes = prepared_bytes.saturating_add(bytes.len());
                        prepared.push((request, bytes, cache_admission));
                    }
                    if !prepared.is_empty() && worker_epoch.load(Ordering::Acquire) == epoch {
                        process_code_lines_batch(
                            &provider,
                            &runtime,
                            prepared,
                            epoch,
                            &worker_epoch,
                            &result_tx,
                            mode,
                            &worker_cache,
                        );
                    }
                }
            })
            .context("failed to start Rust tokei Code lines worker")?;
        Ok(Arc::new(Self {
            pending,
            request_epoch,
            results: Mutex::new(result_rx),
            cached_results: Mutex::new(Vec::new()),
            cache,
            renderer: AsyncCellRendererV1::start(
                renderer,
                match mode {
                    BatchDetailsColumnModeV1::CodeLines => CODE_LINES_RENDERER_CONTRIBUTION_ID_V1,
                    BatchDetailsColumnModeV1::LuaCodeLines => {
                        LUA_CODE_LINES_RENDERER_CONTRIBUTION_ID_V1
                    }
                    BatchDetailsColumnModeV1::LockOwner => LOCK_OWNER_RENDERER_CONTRIBUTION_ID_V1,
                },
            )?,
            mode,
            option_package_id,
            folder_admission,
        }))
    }
}

type PendingCodeLinesInputV1 = (
    explorer_ui::code_lines_column::CodeLinesRequestV1,
    Vec<u8>,
    Option<HostExtensionColumnCacheAdmissionV1>,
);

fn prepare_code_lines_batch_inputs(
    requests: Vec<PendingCodeLinesInputV1>,
    generation: u64,
    mode: BatchDetailsColumnModeV1,
) -> (
    Vec<PendingCodeLinesInputV1>,
    Vec<explorer_extension_host::HostBatchColumnItemV1>,
    Vec<PendingCodeLinesInputV1>,
) {
    let mut dispatchable = Vec::with_capacity(requests.len());
    let mut inputs = Vec::with_capacity(requests.len());
    let mut rejected = Vec::new();
    for (request, bytes, cache_admission) in requests {
        let input = (|| {
            let metadata = fs::metadata(&request.path).ok();
            let modified = metadata
                .as_ref()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok());
            let canonical = fs::canonicalize(&request.path).ok()?;
            let mut identity = 0xcbf2_9ce4_8422_2325_u64;
            for byte in canonical.to_string_lossy().as_bytes() {
                identity ^= u64::from(*byte);
                identity = identity.wrapping_mul(0x0000_0100_0000_01b3);
            }
            Some(explorer_extension_host::HostBatchColumnItemV1 {
                file_name: request
                    .path
                    .file_name()?
                    .to_string_lossy()
                    .into_owned()
                    .into(),
                source: explorer_extension_host::HostInputStreamSourceV1::from_host_snapshot(
                    bytes.clone(),
                    generation,
                    true,
                )?,
                cache_identity: format!("fs-v1-{identity:016x}").into(),
                modified_unix_seconds: modified.map(|duration| duration.as_secs()).into(),
                modified_subsec_nanos: modified.map_or(0, |duration| duration.subsec_nanos()),
                source_size: metadata.map(|metadata| metadata.len()).into(),
                lock_owner_resource: (mode == BatchDetailsColumnModeV1::LockOwner)
                    .then(|| request.path.clone()),
            })
        })();
        let pending = (request, bytes, cache_admission);
        if let Some(input) = input {
            dispatchable.push(pending);
            inputs.push(input);
        } else {
            rejected.push(pending);
        }
    }
    (dispatchable, inputs, rejected)
}

fn process_code_lines_batch(
    provider: &explorer_extension_host::SinglePluginBatchColumnRuntimeV1,
    runtime: &explorer_extension_host::ExtensionJobRuntimeV1,
    requests: Vec<(
        explorer_ui::code_lines_column::CodeLinesRequestV1,
        Vec<u8>,
        Option<HostExtensionColumnCacheAdmissionV1>,
    )>,
    epoch: u64,
    current_epoch: &AtomicU64,
    results: &mpsc::SyncSender<explorer_ui::code_lines_column::CodeLinesResultV1>,
    mode: BatchDetailsColumnModeV1,
    cache: &Mutex<HostExtensionColumnCacheV1<CodeLinesCachedValueV1>>,
) {
    let Some(first) = requests.first() else {
        return;
    };
    let generation = first.0.context.generation.value().max(1);
    let (requests, inputs, rejected) = prepare_code_lines_batch_inputs(requests, generation, mode);
    if current_epoch.load(Ordering::Acquire) != epoch {
        return;
    }
    emit_code_lines_batch_error(rejected, "Code lines input could not be prepared", results);
    if requests.is_empty() {
        return;
    }
    let contribution_id = match mode {
        BatchDetailsColumnModeV1::CodeLines => CODE_LINES_CONTRIBUTION_ID_V1,
        BatchDetailsColumnModeV1::LuaCodeLines => LUA_CODE_LINES_CONTRIBUTION_ID_V1,
        BatchDetailsColumnModeV1::LockOwner => LOCK_OWNER_CONTRIBUTION_ID_V1,
    };
    let lock_owner_query =
        (mode == BatchDetailsColumnModeV1::LockOwner).then(|| lock_owner_query_service(generation));
    let Ok(mut ticket) = provider.prepare_dispatch_with_lock_owner_query(
        runtime,
        contribution_id,
        generation,
        generation,
        generation,
        generation,
        inputs,
        lock_owner_query,
    ) else {
        emit_code_lines_batch_error(requests, "Code lines provider is unavailable", results);
        return;
    };
    let terminal = match provider.invoke_prepared(contribution_id, &mut ticket) {
        Ok(terminal) => terminal,
        Err(_) => {
            ticket.fail_marker_clear();
            emit_code_lines_batch_error(requests, "Code lines provider failed", results);
            return;
        }
    };
    let accepted = runtime.drain(ticket.job(), generation, generation, generation, 64);
    let mut emitted = 0_usize;
    for batch in accepted {
        let Some(rows) = runtime.apply_accepted_batch(&batch, |index| {
            let display = requests
                .get(index)
                .and_then(|(request, _, _)| request.path.file_name())
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            (display, index as u128 + 1)
        }) else {
            continue;
        };
        for row in rows {
            let Some((request, _, cache_admission)) = requests.get(emitted) else {
                break;
            };
            let value = match row.value() {
                Some(explorer_extension_host::ExtensionValueViewV1::StructuredCanonicalJson(
                    bytes,
                )) => parse_batch_details_value(bytes, mode),
                _ => None,
            };
            let error = (value.is_none()).then(|| match row.outcome().into_raw() {
                2 => "Unsupported source".to_owned(),
                3 => "Source unavailable".to_owned(),
                _ => "Code lines provider returned no value".to_owned(),
            });
            if let Some(admission) = cache_admission.as_ref()
                && host_extension_column_cache_key(&request.path).as_ref() == Some(&admission.key)
                && let Ok(mut cache) = cache.lock()
            {
                cache.insert(
                    admission.clone(),
                    CodeLinesCachedValueV1 {
                        value: value.clone(),
                        error: error.clone(),
                    },
                );
            }
            if current_epoch.load(Ordering::Acquire) == epoch {
                if !publish_code_lines_result(
                    results,
                    explorer_ui::code_lines_column::CodeLinesResultV1 {
                        context: request.context.clone(),
                        item_id: request.item_id.clone(),
                        value,
                        error,
                    },
                ) {
                    return;
                }
            }
            emitted += 1;
        }
    }
    let _ = ticket.publish_terminal_after_marker_clear(terminal);
    if emitted < requests.len() {
        emit_code_lines_batch_error(
            requests.into_iter().skip(emitted).collect(),
            "Code lines provider returned an incomplete batch",
            results,
        );
    }
}

fn emit_code_lines_batch_error(
    requests: Vec<(
        explorer_ui::code_lines_column::CodeLinesRequestV1,
        Vec<u8>,
        Option<HostExtensionColumnCacheAdmissionV1>,
    )>,
    message: &str,
    results: &mpsc::SyncSender<explorer_ui::code_lines_column::CodeLinesResultV1>,
) {
    for (request, _, _) in requests {
        if !publish_code_lines_result(
            results,
            explorer_ui::code_lines_column::CodeLinesResultV1 {
                context: request.context,
                item_id: request.item_id,
                value: None,
                error: Some(message.to_owned()),
            },
        ) {
            return;
        }
    }
}

fn current_code_lines_epoch(current_epoch: &AtomicU64, epoch: u64) -> bool {
    current_epoch.load(Ordering::Acquire) == epoch
}

#[cfg(test)]
fn is_code_lines_directory_row(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_dir())
}

fn publish_code_lines_result(
    results: &mpsc::SyncSender<explorer_ui::code_lines_column::CodeLinesResultV1>,
    result: explorer_ui::code_lines_column::CodeLinesResultV1,
) -> bool {
    // A result is terminal for one visible file. Blocking until the GPUI side
    // drains preserves it; `try_send` used to discard it silently when a
    // directory arrived faster than the UI pump.
    results.send(result).is_ok()
}

/// Returns `Ok(None)` for a source exceeding the provider's supported input
/// limit. It is an Unsupported value, not a provider failure or a valid zero.
fn read_code_lines_file_bounded(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let file = fs::File::open(path).map_err(|_| "Source unavailable".to_owned())?;
    let maximum = explorer_extension_host::MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1;
    let mut bytes = Vec::new();
    file.take(u64::try_from(maximum).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| "Source unavailable".to_owned())?;
    if bytes.len() > maximum {
        return Ok(None);
    }
    Ok(Some(bytes))
}

const CODE_LINES_DIRECTORY_MAGIC_V1: &[u8; 8] = b"SECLDIR1";

fn read_code_lines_path_bounded(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "Source unavailable".to_owned())?;
    if metadata.file_type().is_symlink() {
        return Ok(None);
    }
    if metadata.is_file() {
        return read_code_lines_file_bounded(path);
    }
    if !metadata.is_dir() {
        return Ok(None);
    }
    let maximum = explorer_extension_host::MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1;
    let tokei_config = tokei::Config::default();
    let mut packed = Vec::with_capacity(64 * 1024);
    packed.extend_from_slice(CODE_LINES_DIRECTORY_MAGIC_V1);
    let mut stack = vec![path.to_path_buf()];
    let mut visited_files = 0_usize;
    let mut packed_files = 0_usize;
    while let Some(directory) = stack.pop() {
        let entries = fs::read_dir(directory).map_err(|_| "Source unavailable".to_owned())?;
        for entry in entries.flatten() {
            let child = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&child) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                stack.push(child);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            visited_files = visited_files.saturating_add(1);
            if visited_files > 100_000 {
                return Ok(None);
            }
            let relative = child.strip_prefix(path).unwrap_or(&child);
            if tokei::LanguageType::from_path(relative, &tokei_config).is_none() {
                continue;
            }
            let Some(bytes) = read_code_lines_file_bounded(&child)? else {
                continue;
            };
            let name = relative.to_string_lossy();
            let name_bytes = name.as_bytes();
            let record_size = 4_usize
                .saturating_add(8)
                .saturating_add(name_bytes.len())
                .saturating_add(bytes.len());
            if packed.len().saturating_add(record_size) > maximum {
                return Ok(None);
            }
            let Ok(name_len) = u32::try_from(name_bytes.len()) else {
                return Ok(None);
            };
            let Ok(data_len) = u64::try_from(bytes.len()) else {
                return Ok(None);
            };
            packed.extend_from_slice(&name_len.to_le_bytes());
            packed.extend_from_slice(&data_len.to_le_bytes());
            packed.extend_from_slice(name_bytes);
            packed.extend_from_slice(&bytes);
            packed_files = packed_files.saturating_add(1);
        }
    }
    Ok((packed_files != 0).then_some(packed))
}

const LOCK_OWNER_CACHE_TTL_V1: Duration = Duration::from_secs(2);
const LOCK_OWNER_CACHE_CAP_V1: usize = 1_024;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LockOwnerCacheKeyV1 {
    canonical_path: PathBuf,
    source_size: u64,
    modified_seconds: u64,
    modified_nanos: u32,
}

#[derive(Clone)]
struct LockOwnerCacheEntryV1 {
    generation: u64,
    stored_at: Instant,
    status: explorer_extension_api::LockOwnerQueryStatusV1,
    owners: Vec<explorer_extension_api::LockOwnerRecordV1>,
}

static LOCK_OWNER_CACHE_V1: OnceLock<Mutex<HashMap<LockOwnerCacheKeyV1, LockOwnerCacheEntryV1>>> =
    OnceLock::new();

fn lock_owner_cache_key(path: &Path) -> Option<LockOwnerCacheKeyV1> {
    let canonical_path = fs::canonicalize(path).ok()?;
    let metadata = fs::metadata(&canonical_path).ok()?;
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(LockOwnerCacheKeyV1 {
        canonical_path,
        source_size: metadata.len(),
        modified_seconds: modified.as_secs(),
        modified_nanos: modified.subsec_nanos(),
    })
}

fn lock_owner_cache_lookup(
    key: &LockOwnerCacheKeyV1,
    generation: u64,
    now: Instant,
) -> Option<(
    explorer_extension_api::LockOwnerQueryStatusV1,
    Vec<explorer_extension_api::LockOwnerRecordV1>,
)> {
    let cache = LOCK_OWNER_CACHE_V1
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()?;
    let entry = cache.get(key)?;
    (entry.generation == generation
        && now.saturating_duration_since(entry.stored_at) <= LOCK_OWNER_CACHE_TTL_V1)
        .then(|| (entry.status, entry.owners.clone()))
}

fn lock_owner_cache_store(
    key: LockOwnerCacheKeyV1,
    generation: u64,
    status: explorer_extension_api::LockOwnerQueryStatusV1,
    owners: Vec<explorer_extension_api::LockOwnerRecordV1>,
    now: Instant,
) {
    if status != explorer_extension_api::LockOwnerQueryStatusV1::READY
        && status != explorer_extension_api::LockOwnerQueryStatusV1::EMPTY
    {
        return;
    }
    let Ok(mut cache) = LOCK_OWNER_CACHE_V1
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    else {
        return;
    };
    if cache.len() >= LOCK_OWNER_CACHE_CAP_V1 && !cache.contains_key(&key) {
        cache.clear();
    }
    cache.insert(
        key,
        LockOwnerCacheEntryV1 {
            generation,
            stored_at: now,
            status,
            owners,
        },
    );
}

fn lock_owner_query_service(
    generation: u64,
) -> explorer_extension_host::HostLockOwnerQueryServiceV1 {
    explorer_extension_host::HostLockOwnerQueryServiceV1::new(move |resources, control| {
        if control.is_cancelled() {
            return (
                explorer_extension_api::LockOwnerQueryStatusV1::CANCELLED,
                Vec::new(),
            );
        }
        if control.deadline_elapsed() {
            return (
                explorer_extension_api::LockOwnerQueryStatusV1::DEADLINE_ELAPSED,
                Vec::new(),
            );
        }

        let now = Instant::now();
        let mut results = vec![None; resources.len()];
        let mut misses = Vec::new();
        let mut miss_paths = Vec::new();
        for (index, resource) in resources.iter().enumerate() {
            let cache_key = lock_owner_cache_key(&resource.path);
            if let Some((status, mut owners)) = cache_key
                .as_ref()
                .and_then(|key| lock_owner_cache_lookup(key, generation, now))
            {
                for owner in &mut owners {
                    owner.item = resource.item;
                }
                results[index] = Some((status, owners));
            } else {
                misses.push((index, cache_key));
                miss_paths.push(resource.path.clone());
            }
        }

        let current_batch = explorer_shell_win::discover_current_directory_owners_read_only(
            &miss_paths,
            &|| control.is_cancelled(),
            control.deadline(),
        );
        if matches!(
            current_batch,
            explorer_shell_win::CurrentDirectoryOwnerBatchTerminal::Cancelled
        ) || control.is_cancelled()
        {
            return (
                explorer_extension_api::LockOwnerQueryStatusV1::CANCELLED,
                Vec::new(),
            );
        }
        if matches!(
            current_batch,
            explorer_shell_win::CurrentDirectoryOwnerBatchTerminal::DeadlineElapsed
        ) || control.deadline_elapsed()
        {
            return (
                explorer_extension_api::LockOwnerQueryStatusV1::DEADLINE_ELAPSED,
                Vec::new(),
            );
        }

        for (miss_index, (resource_index, cache_key)) in misses.into_iter().enumerate() {
            if control.is_cancelled() {
                return (
                    explorer_extension_api::LockOwnerQueryStatusV1::CANCELLED,
                    Vec::new(),
                );
            }
            if control.deadline_elapsed() {
                return (
                    explorer_extension_api::LockOwnerQueryStatusV1::DEADLINE_ELAPSED,
                    Vec::new(),
                );
            }
            let current = current_directory_item_terminal(&current_batch, miss_index);
            let restart_manager = if !fs::metadata(&resources[resource_index].path)
                .is_ok_and(|metadata| metadata.is_dir())
            {
                let request = explorer_model::LockOwnerDiscoveryRequest {
                    resources: vec![explorer_model::LocationDescriptor::file_system(
                        resources[resource_index].path.clone(),
                    )],
                };
                explorer_shell_win::discover_lock_owners_read_only(
                    &request,
                    &explorer_model::CancellationToken::new(),
                )
            } else {
                explorer_model::LockOwnerDiscoveryTerminal::Empty
            };
            let combined = compose_lock_owner_terminals(restart_manager, current);
            let (status, owners) =
                project_lock_owner_terminal(combined, resources[resource_index].item);
            if let Some(cache_key) = cache_key {
                let mut cached = owners.clone();
                for owner in &mut cached {
                    owner.item = explorer_extension_api::ItemHandleV1::from_host([0; 16], 0);
                }
                lock_owner_cache_store(cache_key, generation, status, cached, now);
            }
            results[resource_index] = Some((status, owners));
        }

        aggregate_lock_owner_batch(results)
    })
}

fn current_directory_item_terminal(
    batch: &explorer_shell_win::CurrentDirectoryOwnerBatchTerminal,
    resource_index: usize,
) -> explorer_model::LockOwnerDiscoveryTerminal {
    match batch {
        explorer_shell_win::CurrentDirectoryOwnerBatchTerminal::Complete(items) => items
            .iter()
            .find(|item| item.resource_index == resource_index)
            .map_or(explorer_model::LockOwnerDiscoveryTerminal::Empty, |item| {
                if item.owners.is_empty() {
                    explorer_model::LockOwnerDiscoveryTerminal::Empty
                } else {
                    explorer_model::LockOwnerDiscoveryTerminal::Ready(item.owners.clone())
                }
            }),
        explorer_shell_win::CurrentDirectoryOwnerBatchTerminal::Cancelled => {
            explorer_model::LockOwnerDiscoveryTerminal::Cancelled
        }
        explorer_shell_win::CurrentDirectoryOwnerBatchTerminal::DeadlineElapsed => {
            explorer_model::LockOwnerDiscoveryTerminal::DeadlineElapsed
        }
        explorer_shell_win::CurrentDirectoryOwnerBatchTerminal::Unavailable(error) => {
            explorer_model::LockOwnerDiscoveryTerminal::Unavailable(error.clone())
        }
    }
}

fn compose_lock_owner_terminals(
    restart_manager: explorer_model::LockOwnerDiscoveryTerminal,
    current_directory: explorer_model::LockOwnerDiscoveryTerminal,
) -> explorer_model::LockOwnerDiscoveryTerminal {
    use explorer_model::LockOwnerDiscoveryTerminal as Terminal;

    if matches!(restart_manager, Terminal::Cancelled)
        || matches!(current_directory, Terminal::Cancelled)
    {
        return Terminal::Cancelled;
    }
    if matches!(restart_manager, Terminal::DeadlineElapsed)
        || matches!(current_directory, Terminal::DeadlineElapsed)
    {
        return Terminal::DeadlineElapsed;
    }

    let mut owners = Vec::new();
    if let Terminal::Ready(ready) = &restart_manager {
        owners.extend(ready.iter().cloned());
    }
    if let Terminal::Ready(ready) = &current_directory {
        for owner in ready {
            if !owners.iter().any(|existing| {
                existing.identity.process_id == owner.identity.process_id
                    && existing.identity.creation_time_100ns == owner.identity.creation_time_100ns
            }) {
                owners.push(owner.clone());
            }
        }
    }
    if !owners.is_empty() {
        owners.sort_by(|left, right| {
            left.identity
                .process_id
                .cmp(&right.identity.process_id)
                .then_with(|| {
                    left.identity
                        .creation_time_100ns
                        .cmp(&right.identity.creation_time_100ns)
                })
                .then_with(|| {
                    left.display_name
                        .to_lowercase()
                        .cmp(&right.display_name.to_lowercase())
                })
                .then_with(|| {
                    lock_owner_application_type_rank(left.application_type)
                        .cmp(&lock_owner_application_type_rank(right.application_type))
                })
        });
        owners.truncate(RoadmapLimits::default().lock_recovery_max_owners);
        return Terminal::Ready(owners);
    }
    if matches!(restart_manager, Terminal::Failed(_)) {
        return restart_manager;
    }
    if matches!(current_directory, Terminal::Failed(_)) {
        return current_directory;
    }
    if matches!(restart_manager, Terminal::Unavailable(_)) {
        return restart_manager;
    }
    if matches!(current_directory, Terminal::Unavailable(_)) {
        return current_directory;
    }
    Terminal::Empty
}

fn project_lock_owner_terminal(
    outcome: explorer_model::LockOwnerDiscoveryTerminal,
    item: explorer_extension_api::ItemHandleV1,
) -> (
    explorer_extension_api::LockOwnerQueryStatusV1,
    Vec<explorer_extension_api::LockOwnerRecordV1>,
) {
    use explorer_model::LockOwnerDiscoveryTerminal as Terminal;
    match outcome {
        Terminal::Ready(owners) => (
            explorer_extension_api::LockOwnerQueryStatusV1::READY,
            owners
                .into_iter()
                .map(|owner| explorer_extension_api::LockOwnerRecordV1 {
                    item,
                    process_id: owner.identity.process_id,
                    application_type: explorer_extension_api::LockOwnerApplicationTypeV1::from_raw(
                        lock_owner_application_type_rank(owner.application_type),
                    ),
                    display_name: owner.display_name.into(),
                    service_name: "".into(),
                })
                .collect(),
        ),
        Terminal::Empty => (
            explorer_extension_api::LockOwnerQueryStatusV1::EMPTY,
            Vec::new(),
        ),
        Terminal::Cancelled => (
            explorer_extension_api::LockOwnerQueryStatusV1::CANCELLED,
            Vec::new(),
        ),
        Terminal::DeadlineElapsed => (
            explorer_extension_api::LockOwnerQueryStatusV1::DEADLINE_ELAPSED,
            Vec::new(),
        ),
        Terminal::Unavailable(_) => (
            explorer_extension_api::LockOwnerQueryStatusV1::UNAVAILABLE,
            Vec::new(),
        ),
        Terminal::Failed(_) => (
            explorer_extension_api::LockOwnerQueryStatusV1::HOST_ERROR,
            Vec::new(),
        ),
    }
}

fn aggregate_lock_owner_batch(
    results: Vec<
        Option<(
            explorer_extension_api::LockOwnerQueryStatusV1,
            Vec<explorer_extension_api::LockOwnerRecordV1>,
        )>,
    >,
) -> (
    explorer_extension_api::LockOwnerQueryStatusV1,
    Vec<explorer_extension_api::LockOwnerRecordV1>,
) {
    let mut owners = Vec::new();
    let mut ownerless_status = explorer_extension_api::LockOwnerQueryStatusV1::EMPTY;
    let mut ownerless = false;
    for (status, item_owners) in results.into_iter().flatten() {
        if status == explorer_extension_api::LockOwnerQueryStatusV1::CANCELLED {
            return (status, Vec::new());
        }
        if status == explorer_extension_api::LockOwnerQueryStatusV1::DEADLINE_ELAPSED {
            return (status, Vec::new());
        }
        if item_owners.is_empty() {
            ownerless = true;
            if lock_owner_status_rank(status) > lock_owner_status_rank(ownerless_status) {
                ownerless_status = status;
            }
        } else {
            owners.extend(item_owners);
        }
    }
    let status = if ownerless {
        ownerless_status
    } else {
        explorer_extension_api::LockOwnerQueryStatusV1::READY
    };
    (status, owners)
}

fn lock_owner_status_rank(status: explorer_extension_api::LockOwnerQueryStatusV1) -> u8 {
    if status == explorer_extension_api::LockOwnerQueryStatusV1::HOST_ERROR {
        3
    } else if status == explorer_extension_api::LockOwnerQueryStatusV1::UNAVAILABLE {
        2
    } else {
        1
    }
}

const fn lock_owner_application_type_rank(
    application_type: explorer_model::LockOwnerApplicationType,
) -> u32 {
    match application_type {
        explorer_model::LockOwnerApplicationType::Unknown => 0,
        explorer_model::LockOwnerApplicationType::MainWindow => 1,
        explorer_model::LockOwnerApplicationType::OtherWindow => 2,
        explorer_model::LockOwnerApplicationType::Service => 3,
        explorer_model::LockOwnerApplicationType::Explorer => 4,
        explorer_model::LockOwnerApplicationType::Console => 5,
        explorer_model::LockOwnerApplicationType::Critical => 6,
    }
}

fn parse_batch_details_value(
    bytes: &[u8],
    mode: BatchDetailsColumnModeV1,
) -> Option<explorer_ui::code_lines_column::CodeLinesValueV1> {
    if mode != BatchDetailsColumnModeV1::LockOwner {
        return parse_code_lines_value(bytes);
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let count = value.get("count")?.as_u64()?;
    Some(explorer_ui::code_lines_column::CodeLinesValueV1 {
        language: value.get("names")?.as_str()?.to_owned(),
        code: count,
        comments: 0,
        blanks: 0,
        total: count,
    })
}

fn parse_code_lines_value(
    bytes: &[u8],
) -> Option<explorer_ui::code_lines_column::CodeLinesValueV1> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    Some(explorer_ui::code_lines_column::CodeLinesValueV1 {
        language: value.get("language")?.as_str()?.to_owned(),
        code: value.get("code")?.as_u64()?,
        comments: value.get("comments")?.as_u64()?,
        blanks: value.get("blanks")?.as_u64()?,
        total: value.get("total")?.as_u64()?,
    })
}

impl explorer_ui::code_lines_column::CodeLinesRuntimePortV1 for ApplicationCodeLinesRuntimeV1 {
    fn config(&self) -> explorer_ui::code_lines_column::CodeLinesColumnConfigV1 {
        let mut config = explorer_ui::code_lines_column::CodeLinesColumnConfigV1::default();
        config.option_package_id.clone_from(&self.option_package_id);
        if self.mode == BatchDetailsColumnModeV1::LockOwner {
            config.descriptor = explorer_ui::code_lines_column::lock_owner_column_descriptor();
        } else if self.mode == BatchDetailsColumnModeV1::LuaCodeLines {
            config.descriptor.id = explorer_model::ColumnId::Extension {
                package_id: "lua-tokei-code-lines-column".to_owned(),
                column_id: explorer_ui::code_lines_column::CODE_LINES_COLUMN_ID.to_owned(),
            };
            config.descriptor.display_name = "Code lines".to_owned();
        }
        config.folder_admission = self.folder_admission;
        config
    }

    fn submit_code_lines_requests(
        &self,
        requests: Vec<explorer_ui::code_lines_column::CodeLinesRequestV1>,
    ) {
        let (hits, misses) = partition_batch_details_cache_hits(&self.cache, self.mode, requests);
        if let Ok(mut cached_results) = self.cached_results.lock() {
            cached_results.extend(hits);
        }
        let Some(first) = misses.first() else {
            return;
        };
        let active = (first.context.tab_id, first.context.generation);
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.active != Some(active) {
            self.request_epoch.fetch_add(1, Ordering::AcqRel);
            state.active = Some(active);
            state.requests = Some(misses);
        } else {
            let queued = state.requests.get_or_insert_with(Vec::new);
            for request in misses {
                if request.context.tab_id == active.0
                    && request.context.generation == active.1
                    && !queued.iter().any(|queued_request| {
                        queued_request.item_id == request.item_id
                            && queued_request.path == request.path
                    })
                {
                    queued.push(request);
                }
            }
        }
        ready.notify_one();
    }

    fn cancel_code_lines_context(&self, context: &explorer_model::RequestContext) {
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.active == Some((context.tab_id, context.generation)) {
            self.request_epoch.fetch_add(1, Ordering::AcqRel);
            state.active = None;
            state.requests = None;
            ready.notify_one();
        }
    }

    fn invalidate_directory_cache(&self, directory: &Path) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.invalidate_directory(directory);
        }
    }

    fn drain_code_lines_results(&self) -> Vec<explorer_ui::code_lines_column::CodeLinesResultV1> {
        let mut ready = self
            .cached_results
            .lock()
            .map_or_else(|_| Vec::new(), |mut results| std::mem::take(&mut *results));
        if let Ok(results) = self.results.lock() {
            ready.extend(results.try_iter());
        }
        ready
    }

    fn drain_render_results(&self) -> bool {
        self.renderer.drain_ready()
    }

    fn render_cell(
        &self,
        context: explorer_extension_ui_api::CellRenderContextV1,
    ) -> explorer_extension_ui_api::CellRenderPlanV1 {
        self.renderer
            .render_or_enqueue(context, "Loading code lines")
    }
}

impl Drop for ApplicationCodeLinesRuntimeV1 {
    fn drop(&mut self) {
        self.request_epoch.fetch_add(1, Ordering::AcqRel);
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.stopped = true;
        state.active = None;
        state.requests = None;
        ready.notify_one();
    }
}

/// App-owned bridge for the one independent Size Map example. Requests for the
/// current request context are merged, while a changed context replaces the
/// pending batch and cancels in-flight work. Filesystem work is bounded off
/// the GPUI thread and only copied nodes reach the plugin renderer.
struct ApplicationSizeMapRuntimeV1 {
    pending: Arc<(Mutex<PendingSizeMapWorkV1>, Condvar)>,
    request_epoch: Arc<AtomicU64>,
    results: Mutex<mpsc::Receiver<explorer_ui::size_map_view::SizeMapMeasureResultV1>>,
    result_tx: mpsc::Sender<explorer_ui::size_map_view::SizeMapMeasureResultV1>,
    renderer: AsyncSizeMapRendererV1,
    package_incarnation: u64,
}

#[derive(Default)]
struct PendingSizeMapWorkV1 {
    context: Option<explorer_model::RequestContext>,
    epoch: u64,
    requests: Vec<explorer_ui::size_map_view::SizeMapMeasureRequestV1>,
    stopped: bool,
}

impl ApplicationSizeMapRuntimeV1 {
    fn start(
        renderer: explorer_extension_host::SinglePluginSizeMapViewRuntimeV1,
        snapshot_service: Arc<Mutex<crate::folder_size_service::FolderSizeServiceV1>>,
    ) -> Result<explorer_ui::size_map_view::SizeMapRuntimeHandleV1, Error> {
        let pending = Arc::new((Mutex::new(PendingSizeMapWorkV1::default()), Condvar::new()));
        let worker_pending = pending.clone();
        let request_epoch = Arc::new(AtomicU64::new(0));
        let worker_epoch = request_epoch.clone();
        let worker_snapshot_service = Arc::clone(&snapshot_service);
        // A bounded result channel used to drop the tail of a directory would
        // leave those nodes permanently loading: the UI deliberately submits
        // each item only once per generation. Keep the bounded input batch and
        // terminalize overflow instead; results themselves must not be shed.
        let (result_tx, result_rx) =
            mpsc::channel::<explorer_ui::size_map_view::SizeMapMeasureResultV1>();
        let worker_result_tx = result_tx.clone();
        std::thread::Builder::new()
            .name("p0-size-map-scan".to_owned())
            .spawn(move || {
                loop {
                    let (batch_epoch, requests) = {
                        let (lock, ready) = &*worker_pending;
                        let mut state = lock
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        while state.requests.is_empty() && !state.stopped {
                            state = ready
                                .wait(state)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                        if state.stopped {
                            return;
                        }
                        (state.epoch, std::mem::take(&mut state.requests))
                    };
                    if worker_epoch.load(Ordering::Acquire) != batch_epoch {
                        continue;
                    }
                    let cache_root =
                        request_cache_root(requests.iter().map(|request| request.path.as_path()));
                    // Publish an initial state for every direct child before
                    // walking any subtree. This keeps the map interactive and
                    // lets the renderer show all known siblings while exact
                    // recursive totals arrive one at a time.
                    for request in requests.iter().cloned() {
                        if worker_epoch.load(Ordering::Acquire) != batch_epoch {
                            break;
                        }
                        if worker_result_tx
                            .send(size_map_scanning_result(
                                request.clone(),
                                preferred_size_map_scan_method(&request.path),
                            ))
                            .is_err()
                        {
                            return;
                        }
                    }
                    let scan_method = "Shared folder snapshot";
                    for request in requests.iter().cloned() {
                        if worker_result_tx
                            .send(size_map_scanning_result(request, scan_method))
                            .is_err()
                        {
                            return;
                        }
                    }
                    for request in requests {
                        if worker_epoch.load(Ordering::Acquire) != batch_epoch {
                            break;
                        }
                        let scan = match fs::symlink_metadata(&request.path) {
                            Ok(metadata) if metadata.is_file() => SizeMapTreeScanV1 {
                                outcome: SizeMapScanOutcomeV1 {
                                    bytes: metadata.len(),
                                    terminal: SizeMapScanTerminalV1::Complete,
                                    diagnostic: None,
                                },
                                nodes: Vec::new(),
                            },
                            Ok(metadata) if metadata.is_dir() => worker_snapshot_service
                                .lock()
                                .map_err(|_| "folder snapshot service poisoned".to_owned())
                                .and_then(|mut service| {
                                    service.snapshot_or_scan(
                                        &request.path,
                                        request.context.generation.value(),
                                        || worker_epoch.load(Ordering::Acquire) != batch_epoch,
                                    )
                                })
                                .map(|snapshot| {
                                    project_shared_snapshot_to_size_map(
                                        &snapshot,
                                        &request.path,
                                        &request.item_id,
                                    )
                                })
                                .unwrap_or_else(|error| SizeMapTreeScanV1 {
                                    outcome: SizeMapScanOutcomeV1 {
                                        bytes: 0,
                                        terminal: SizeMapScanTerminalV1::Failed,
                                        diagnostic: Some(error),
                                    },
                                    nodes: Vec::new(),
                                }),
                            Ok(_) => SizeMapTreeScanV1 {
                                outcome: SizeMapScanOutcomeV1 {
                                    bytes: 0,
                                    terminal: SizeMapScanTerminalV1::Unavailable,
                                    diagnostic: Some(
                                        "Size Map item is not a regular file or directory"
                                            .to_owned(),
                                    ),
                                },
                                nodes: Vec::new(),
                            },
                            Err(error) => SizeMapTreeScanV1 {
                                outcome: SizeMapScanOutcomeV1 {
                                    bytes: 0,
                                    terminal: SizeMapScanTerminalV1::Unavailable,
                                    diagnostic: Some(error.to_string()),
                                },
                                nodes: Vec::new(),
                            },
                        };
                        if scan.outcome.terminal == SizeMapScanTerminalV1::Cancelled
                            || worker_epoch.load(Ordering::Acquire) != batch_epoch
                        {
                            break;
                        }
                        for nodes in scan.nodes.chunks(SIZE_MAP_TREE_DELTA_BATCH_CAP_V1) {
                            if worker_epoch.load(Ordering::Acquire) != batch_epoch {
                                break;
                            }
                            if worker_result_tx
                                .send(explorer_ui::size_map_view::SizeMapMeasureResultV1 {
                                    context: request.context.clone(),
                                    item_id: request.item_id.clone(),
                                    exact_bytes: None,
                                    partial: true,
                                    error: Some("Breadth-first fallback".to_owned()),
                                    tree_nodes: nodes.to_vec(),
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
                        if worker_epoch.load(Ordering::Acquire) != batch_epoch {
                            break;
                        }
                        if worker_result_tx
                            .send(explorer_ui::size_map_view::SizeMapMeasureResultV1 {
                                context: request.context,
                                item_id: request.item_id,
                                exact_bytes: (scan.outcome.terminal
                                    == SizeMapScanTerminalV1::Complete)
                                    .then_some(scan.outcome.bytes),
                                partial: scan.outcome.terminal != SizeMapScanTerminalV1::Complete,
                                error: (scan.outcome.terminal != SizeMapScanTerminalV1::Complete)
                                    .then(|| {
                                        format!(
                                            "{}: {}",
                                            scan.outcome.terminal.label(),
                                            scan.outcome
                                                .diagnostic
                                                .as_deref()
                                                .unwrap_or("Size Map scan did not complete")
                                        )
                                    }),
                                tree_nodes: Vec::new(),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    if let Some(root) = cache_root
                        && let Ok(mut service) = worker_snapshot_service.lock()
                    {
                        service.retain_cache_window(&root, 3);
                    }
                }
            })
            .context("failed to start P0 Size Map worker")?;
        Ok(Arc::new(Self {
            pending,
            request_epoch,
            results: Mutex::new(result_rx),
            result_tx,
            renderer: AsyncSizeMapRendererV1::start(renderer)?,
            package_incarnation: NEXT_SIZE_MAP_RUNTIME_INCARNATION_V1
                .fetch_add(1, Ordering::AcqRel)
                .max(1),
        }))
    }
}

impl explorer_ui::size_map_view::ExtensionSizeMapRuntimePortV1 for ApplicationSizeMapRuntimeV1 {
    fn config(&self) -> explorer_ui::size_map_view::SizeMapViewConfigV1 {
        explorer_ui::size_map_view::SizeMapViewConfigV1::default()
    }

    fn submit_measure_requests(
        &self,
        requests: Vec<explorer_ui::size_map_view::SizeMapMeasureRequestV1>,
    ) {
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let rejected = enqueue_size_map_requests(&mut state, &self.request_epoch, requests);
        let has_pending = !state.requests.is_empty();
        drop(state);
        for result in rejected {
            // A terminal queue-limit result must not be dropped because the UI
            // will not re-submit it within this generation.
            if self.result_tx.send(result).is_err() {
                return;
            }
        }
        if has_pending {
            ready.notify_one();
        }
    }

    fn cancel_measure_context(&self, context: &explorer_model::RequestContext) {
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.context.as_ref() == Some(context) {
            let epoch = self
                .request_epoch
                .fetch_add(1, Ordering::AcqRel)
                .saturating_add(1);
            state.context = None;
            state.epoch = epoch;
            state.requests.clear();
            ready.notify_one();
        }
    }

    fn drain_measure_results(&self) -> Vec<explorer_ui::size_map_view::SizeMapMeasureResultV1> {
        let Ok(results) = self.results.lock() else {
            return Vec::new();
        };
        // Avoid merging and laying out the full 10,000-node projection in one
        // GPUI frame. The periodic pump drains the remaining batches later.
        results.try_iter().take(4).collect()
    }

    fn drain_render_results(&self) -> bool {
        self.renderer.drain_ready()
    }

    fn render_size_map(
        &self,
        context: explorer_ui::size_map_view::SizeMapRenderContextV1,
    ) -> explorer_ui::size_map_view::SizeMapRenderPlanV1 {
        use explorer_extension_ui_api::{
            CellColorV1, CellThemeV1, SizeMapNodeKindV1, SizeMapNodeStatusV1,
        };
        let generation = context.request_context.generation.value();
        let item_ids = context
            .nodes
            .iter()
            .map(|node| node.item_id.clone())
            .collect::<Vec<_>>();
        let node_ids = item_ids.iter().map(size_map_node_id).collect::<Vec<_>>();
        // The public ABI represents an item with one u64-backed StableId. A
        // collision must fail closed instead of projecting a returned
        // rectangle onto a different Shell item. Ordinary row position is
        // deliberately not used as an identity source.
        if node_ids.iter().collect::<HashSet<_>>().len() != node_ids.len() {
            return size_map_render_fallback("Size Map item identity collision");
        }
        let (projected_indexes, omitted_indexes) = partition_size_map_projection(&context.nodes);
        let mut mappings = projected_indexes
            .iter()
            .map(|index| {
                let node = &context.nodes[*index];
                let node_id = node_ids[*index];
                let public_status = size_map_node_status_v1(node);
                let status = match public_status.into_raw() {
                    1 => "Complete",
                    2 => "Partial",
                    3 => "Unavailable",
                    4 => "Failed",
                    5 => "Cancelled",
                    6 => "Resource limited",
                    _ => "Unavailable",
                };
                (
                    node_id,
                    SizeMapProjectionV1::Item(
                        explorer_ui::size_map_view::SizeMapInteractionTargetV1 {
                            item_id: node.item_id.clone(),
                            selection_item_id: node.selection_item_id.clone(),
                            location: node.location.clone(),
                            is_container: node.is_container,
                        },
                        status.to_owned(),
                    ),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut public_nodes = projected_indexes
            .iter()
            .map(|index| {
                let node = &context.nodes[*index];
                explorer_extension_ui_api::SizeMapNodeV1 {
                    node_id: node_ids[*index],
                    parent_id: node
                        .parent_item_id
                        .as_ref()
                        .and_then(|parent| {
                            item_ids
                                .iter()
                                .position(|item_id| item_id == parent)
                                .map(|parent_index| node_ids[parent_index])
                        })
                        .into(),
                    name: node.display_name.clone().into(),
                    kind: if node.is_container {
                        SizeMapNodeKindV1::DIRECTORY
                    } else {
                        SizeMapNodeKindV1::FILE
                    },
                    exact_bytes: node.exact_bytes.into(),
                    status: size_map_node_status_v1(node),
                }
            })
            .collect::<Vec<_>>();
        if !omitted_indexes.is_empty() {
            let mut identity = Vec::with_capacity(omitted_indexes.len() * 16);
            identity.extend_from_slice(b"superexplorer:size-map:other:v1");
            identity.extend_from_slice(&generation.to_le_bytes());
            for index in &omitted_indexes {
                identity.extend_from_slice(context.nodes[*index].item_id.provider_bytes());
            }
            let mut other_id = explorer_extension_ui_api::StableIdV1::new(
                explorer_extension_ui_api::EXTENSION_ID_NAMESPACE_V1,
                revision_for(&identity),
            );
            if node_ids.contains(&other_id) {
                other_id = explorer_extension_ui_api::StableIdV1::new(
                    explorer_extension_ui_api::EXTENSION_ID_NAMESPACE_V1,
                    other_id.value ^ 0xa5a5_a5a5_a5a5_a5a5,
                );
            }
            let complete = omitted_indexes.iter().all(|index| {
                let node = &context.nodes[*index];
                !node.partial && node.error.is_none() && node.exact_bytes.is_some()
            });
            let omitted_ids = omitted_indexes
                .iter()
                .map(|index| context.nodes[*index].item_id.clone())
                .collect::<HashSet<_>>();
            let bytes = complete.then(|| {
                omitted_indexes.iter().fold(0_u64, |total, index| {
                    let node = &context.nodes[*index];
                    if node
                        .parent_item_id
                        .as_ref()
                        .is_some_and(|parent| omitted_ids.contains(parent))
                    {
                        total
                    } else {
                        total.saturating_add(node.exact_bytes.unwrap_or_default())
                    }
                })
            });
            mappings.insert(
                other_id,
                SizeMapProjectionV1::Aggregate(aggregate_size_map_items(
                    &context.nodes,
                    &omitted_indexes,
                )),
            );
            public_nodes.push(explorer_extension_ui_api::SizeMapNodeV1 {
                node_id: other_id,
                parent_id: None.into(),
                name: format!("Other ({} items)", omitted_indexes.len()).into(),
                kind: SizeMapNodeKindV1::OTHER,
                exact_bytes: bytes.into(),
                status: if complete {
                    SizeMapNodeStatusV1::COMPLETE
                } else {
                    SizeMapNodeStatusV1::PARTIAL
                },
            });
        }
        let foreground = if context.dark_theme {
            CellColorV1::rgba(245, 245, 245, 255)
        } else {
            CellColorV1::rgba(32, 32, 32, 255)
        };
        let background = if context.dark_theme {
            CellColorV1::rgba(32, 32, 32, 255)
        } else {
            CellColorV1::rgba(250, 250, 250, 255)
        };
        let selected_node_ids = context
            .selected
            .iter()
            .filter_map(|selected| {
                projected_indexes
                    .iter()
                    .find(|index| context.nodes[**index].item_id == *selected)
                    .map(|index| node_ids[*index])
            })
            .collect();
        let scan_method = context
            .nodes
            .iter()
            .filter_map(|node| node.error.as_deref())
            .find(|error| {
                error.contains("NTFS MFT")
                    || error.contains("Breadth-first fallback")
                    || error.contains("Detecting scan method")
            })
            .unwrap_or("Detecting scan method");
        let mut public_context = explorer_extension_ui_api::SizeMapRenderContextV1 {
            snapshot: explorer_extension_ui_api::ViewSnapshotIdentityV1 {
                location_generation: generation,
                refresh_generation: generation,
                render_revision: 1,
            },
            nodes: public_nodes.into(),
            viewport: explorer_extension_ui_api::SizeMapViewportV1 {
                width_milli: context.viewport_width_milli,
                height_milli: context.viewport_height_milli,
                dpi_milli: 1_000,
            },
            theme: CellThemeV1 {
                foreground,
                muted_foreground: CellColorV1::rgba(112, 112, 112, 255),
                background,
                selection_background: CellColorV1::rgba(0, 95, 184, 255),
                accent: CellColorV1::rgba(0, 120, 212, 255),
            },
            selected_node_ids,
            settings: scan_method.into(),
        };
        let key = size_map_render_key(
            &mut public_context,
            &context.request_context,
            &item_ids,
            self.package_incarnation,
        );
        let width = context.viewport_width_milli as f32 / 1_000.0;
        let height = context.viewport_height_milli as f32 / 1_000.0;
        self.renderer.render_or_enqueue(SizeMapRenderRequestV1 {
            key,
            context: public_context,
            mappings,
            width,
            height,
        })
    }
}

impl Drop for ApplicationSizeMapRuntimeV1 {
    fn drop(&mut self) {
        self.request_epoch.fetch_add(1, Ordering::AcqRel);
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.stopped = true;
        state.requests.clear();
        ready.notify_one();
    }
}

#[cfg(feature = "uitest-support")]
const UITEST_EXTENSION_STATE_ROOT_ENV_V1: &str = "EXPLORER_UITEST_EXTENSION_STATE_ROOT";

#[cfg(feature = "uitest-support")]
const UITEST_SAFE_MODE_PROBE_FILE_V1: &str = "safe-mode-probe-v1.json";

/// Returns the test-owned extension state root only in a binary compiled with
/// the non-default UITEST feature. Production binaries never inspect this
/// environment variable and always use the host's Windows Known Folder root.
#[cfg(feature = "uitest-support")]
fn uitest_extension_state_root_v1() -> Result<Option<PathBuf>, Error> {
    let Some(root) = std::env::var_os(UITEST_EXTENSION_STATE_ROOT_ENV_V1) else {
        return Ok(None);
    };
    let root = PathBuf::from(root);
    if !root.is_dir() {
        anyhow::bail!("UITEST extension state root must be an existing directory");
    }
    root.canonicalize()
        .map(Some)
        .context("failed to canonicalize UITEST extension state root")
}

#[cfg(not(feature = "uitest-support"))]
fn uitest_extension_state_root_v1() -> Result<Option<PathBuf>, Error> {
    Ok(None)
}

#[cfg(feature = "uitest-support")]
fn write_uitest_safe_mode_probe_v1(
    state_root: &std::path::Path,
    recovered_callback_denied: bool,
) -> Result<(), Error> {
    let bytes = if recovered_callback_denied {
        b"{\"schema_version\":1,\"recovered_callback_denied\":true}".as_slice()
    } else {
        b"{\"schema_version\":1,\"recovered_callback_denied\":false}".as_slice()
    };
    let temporary = state_root.join("safe-mode-probe-v1.tmp");
    let destination = state_root.join(UITEST_SAFE_MODE_PROBE_FILE_V1);
    std::fs::write(&temporary, bytes).context("failed to write UITEST Safe Mode probe")?;
    std::fs::rename(&temporary, &destination)
        .context("failed to publish UITEST Safe Mode probe")?;
    Ok(())
}

/// Path-free suspect identity presented by the application Safe Mode offer.
///
/// Every string originates from the host's recovered marker validation, which
/// permits only bounded package/entrypoint/root identity components and a
/// lowercase manifest digest. Filesystem locations and marker paths never
/// reach this application-facing value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeModeSuspectV1 {
    package_id: String,
    sealed_manifest_digest: String,
    entrypoint_id: String,
    root_module_id: String,
    primary_interface_namespace: u32,
    primary_interface_value: u64,
}

impl SafeModeSuspectV1 {
    #[must_use]
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    #[must_use]
    pub fn sealed_manifest_digest(&self) -> &str {
        &self.sealed_manifest_digest
    }

    #[must_use]
    pub fn entrypoint_id(&self) -> &str {
        &self.entrypoint_id
    }

    #[must_use]
    pub fn root_module_id(&self) -> &str {
        &self.root_module_id
    }

    #[must_use]
    pub const fn primary_interface_namespace(&self) -> u32 {
        self.primary_interface_namespace
    }

    #[must_use]
    pub const fn primary_interface_value(&self) -> u64 {
        self.primary_interface_value
    }
}

/// An explicit, path-free Safe Mode confirmation offer owned by application
/// startup. Its opaque ID can only be sent back to the resident extension host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeModeIncidentOfferV1<IncidentId> {
    incident_id: IncidentId,
    presentation_token: u64,
    kind: explorer_extension_host::NativeSafeModeIncidentKindV1,
    suspect: Option<SafeModeSuspectV1>,
}

impl<IncidentId: Copy> SafeModeIncidentOfferV1<IncidentId> {
    #[must_use]
    pub const fn incident_id(&self) -> IncidentId {
        self.incident_id
    }

    /// Returns the lifecycle-local opaque token used by a UI presenter.
    #[must_use]
    pub const fn presentation_token(&self) -> u64 {
        self.presentation_token
    }

    #[must_use]
    pub const fn kind(&self) -> explorer_extension_host::NativeSafeModeIncidentKindV1 {
        self.kind
    }

    #[must_use]
    pub const fn suspect(&self) -> Option<&SafeModeSuspectV1> {
        self.suspect.as_ref()
    }
}

/// Application's concrete Safe Mode offer, keyed by the host-owned opaque ID.
pub type SafeModeIncidentOffer =
    SafeModeIncidentOfferV1<explorer_extension_host::NativeSafeModeIncidentIdV1>;

trait SafeModeIncidentPortV1 {
    type IncidentId: Copy + Eq;
    type Error;

    fn offers(&self) -> Vec<SafeModeIncidentOfferV1<Self::IncidentId>>;
    fn denies_native_callbacks(&self) -> bool;
    fn confirm(&self, incident_id: Self::IncidentId) -> Result<(), Self::Error>;
}

impl SafeModeIncidentPortV1 for explorer_extension_host::ExtensionHost {
    type IncidentId = explorer_extension_host::NativeSafeModeIncidentIdV1;
    type Error = explorer_extension_host::NativeLifecycleErrorV1;

    fn offers(&self) -> Vec<SafeModeIncidentOffer> {
        self.safe_mode_incidents()
            .into_iter()
            .enumerate()
            .map(|(index, incident)| match incident {
                explorer_extension_host::NativeSafeModeIncidentV1::RegistrarInProgress {
                    incident_id,
                    package_id,
                    sealed_manifest_digest,
                    entrypoint_id,
                    root_module_id,
                    primary_interface_namespace,
                    primary_interface_value,
                    ..
                } => SafeModeIncidentOfferV1 {
                    incident_id,
                    presentation_token: index as u64 + 1,
                    kind:
                        explorer_extension_host::NativeSafeModeIncidentKindV1::RegistrarInProgress,
                    suspect: Some(SafeModeSuspectV1 {
                        package_id,
                        sealed_manifest_digest,
                        entrypoint_id,
                        root_module_id,
                        primary_interface_namespace,
                        primary_interface_value,
                    }),
                },
                explorer_extension_host::NativeSafeModeIncidentV1::UnsafeMarkerState {
                    incident_id,
                } => SafeModeIncidentOfferV1 {
                    incident_id,
                    presentation_token: index as u64 + 1,
                    kind: explorer_extension_host::NativeSafeModeIncidentKindV1::UnsafeMarkerState,
                    suspect: None,
                },
            })
            .collect()
    }

    fn denies_native_callbacks(&self) -> bool {
        self.safe_mode_denies_all()
    }

    fn confirm(
        &self,
        incident_id: explorer_extension_host::NativeSafeModeIncidentIdV1,
    ) -> Result<(), explorer_extension_host::NativeLifecycleErrorV1> {
        self.confirm_safe_mode_incident(incident_id)
    }
}

fn confirm_offered_safe_mode_incident_v1<P: SafeModeIncidentPortV1>(
    port: &P,
    offers: &mut Vec<SafeModeIncidentOfferV1<P::IncidentId>>,
    incident_id: P::IncidentId,
) -> Result<bool, P::Error> {
    if !offers.iter().any(|offer| offer.incident_id == incident_id) {
        return Ok(false);
    }
    port.confirm(incident_id)?;
    offers.retain(|offer| offer.incident_id != incident_id);
    Ok(true)
}

fn confirm_presented_safe_mode_incident_v1<P: SafeModeIncidentPortV1>(
    port: &P,
    offers: &mut Vec<SafeModeIncidentOfferV1<P::IncidentId>>,
    presentation_token: u64,
) -> Result<bool, P::Error> {
    let Some(incident_id) = offers
        .iter()
        .find(|offer| offer.presentation_token() == presentation_token)
        .map(SafeModeIncidentOfferV1::incident_id)
    else {
        return Ok(false);
    };
    confirm_offered_safe_mode_incident_v1(port, offers, incident_id)
}

fn emit_post_commit_safe_mode_telemetry_v1<E>(emit: impl FnOnce() -> Result<(), E>) {
    let _ = emit();
}

fn schedule_visual_diagnostics(
    window: &mut gpui::Window,
    fixture: VisualFixtureConfig,
    tokens: UiTokens,
    diagnostics: DiagnosticsSession,
    remaining_frames: u8,
) {
    window.on_next_frame(move |window, cx| {
        if remaining_frames > 1 {
            schedule_visual_diagnostics(window, fixture, tokens, diagnostics, remaining_frames - 1);
            return;
        }
        let actual_scale = window.scale_factor().to_string();
        let regions = cx
            .global::<explorer_ui::diagnostics::RegionDiagnosticsRecorder>()
            .snapshot(window.scale_factor());
        match fixture.write_diagnostics(window, tokens, &regions) {
            Ok(()) => {
                let _ = diagnostics.record_event(
                    "visual_fixture_ready",
                    &[
                        ("theme", fixture.theme.name()),
                        (
                            "expected_dpi_percent",
                            &fixture.expected_dpi_percent.to_string(),
                        ),
                        ("actual_scale_factor", &actual_scale),
                        ("font", &fixture.font),
                        ("state", &fixture.placeholder_state),
                    ],
                );
            }
            Err(error) => {
                tracing::error!(%error, "visual fixture diagnostics failed");
                diagnostics.record_error(
                    ErrorSeverity::Error,
                    "application",
                    "write_visual_fixture_diagnostics",
                    error.as_ref(),
                    Some(file!()),
                );
                let _ = diagnostics
                    .record_event("visual_fixture_failed", &[("error", &error.to_string())]);
            }
        }
    });
}

/// Owns all process-wide resources around the blocking GPUI event loop.
pub struct ApplicationLifecycle {
    resources: Arc<Mutex<ShutdownResources>>,
}

#[derive(Default)]
struct FolderOptionsWindowControllerV1 {
    window: Option<gpui::WindowHandle<explorer_ui::folder_options_window::FolderOptionsWindow>>,
    owner: Option<gpui::WindowHandle<ExplorerRoot>>,
    lifecycle: FolderOptionsControllerLifecycleV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FolderOptionsOpenIntentV1 {
    Activate { generation: u64 },
    Create { generation: u64 },
}

#[derive(Default)]
struct FolderOptionsControllerLifecycleV1 {
    generation: u64,
    live: bool,
    creating: bool,
}

impl FolderOptionsControllerLifecycleV1 {
    fn begin_open(&mut self) -> FolderOptionsOpenIntentV1 {
        if self.live || self.creating {
            return FolderOptionsOpenIntentV1::Activate {
                generation: self.generation,
            };
        }
        self.generation = self.generation.saturating_add(1);
        self.creating = true;
        FolderOptionsOpenIntentV1::Create {
            generation: self.generation,
        }
    }

    fn creation_succeeded(&mut self, generation: u64) -> bool {
        if !self.creating || generation != self.generation {
            return false;
        }
        self.creating = false;
        self.live = true;
        true
    }

    fn creation_failed(&mut self, generation: u64) {
        if self.creating && generation == self.generation {
            self.creating = false;
        }
    }

    fn close(&mut self) -> bool {
        let changed = self.live || self.creating;
        self.live = false;
        self.creating = false;
        changed
    }
}

impl FolderOptionsWindowControllerV1 {
    fn install(
        &mut self,
        generation: u64,
        window: gpui::WindowHandle<explorer_ui::folder_options_window::FolderOptionsWindow>,
        owner: gpui::WindowHandle<ExplorerRoot>,
    ) -> bool {
        if !self.lifecycle.creation_succeeded(generation) {
            return false;
        }
        self.window = Some(window);
        self.owner = Some(owner);
        true
    }

    fn clear(
        &mut self,
    ) -> Option<(
        gpui::WindowHandle<explorer_ui::folder_options_window::FolderOptionsWindow>,
        gpui::WindowHandle<ExplorerRoot>,
    )> {
        self.lifecycle.close();
        self.window.take().zip(self.owner.take())
    }
}

struct ShutdownResources {
    diagnostics: DiagnosticsSession,
    extension_host: Option<explorer_extension_host::ExtensionHost>,
    loaded_extension_summary: Option<String>,
    visual_column_runtime: Option<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1>,
    visual_column_extension_loaded: bool,
    code_lines_runtimes: Vec<explorer_ui::code_lines_column::CodeLinesRuntimeHandleV1>,
    size_map_runtime: Option<explorer_ui::size_map_view::SizeMapRuntimeHandleV1>,
    virtual_folder_runtime: Option<explorer_extension_host::SinglePluginVirtualFolderRuntimeV1>,
    extension_job_ui_inbox: Option<explorer_extension_host::ExtensionJobUiInboxV1>,
    extension_job_ui_ingress: Option<explorer_extension_host::ExtensionJobUiIngressV1>,
    safe_mode_incident_offers: Vec<SafeModeIncidentOffer>,
    broker_warmup: Option<std::thread::JoinHandle<()>>,
    broker: Option<explorer_extension_broker::BrokerClient>,
    shell_sta: Option<Arc<ShellStaHandle>>,
    shutdown: bool,
}

impl ApplicationLifecycle {
    /// Applies Windows prerequisites and starts the sole Shell STA.
    ///
    /// # Errors
    ///
    /// Returns DPI, Shell initialization, or diagnostic write failures without starting GPUI.
    pub fn start(diagnostics: DiagnosticsSession) -> Result<Self, Error> {
        Self::start_with_plugins(diagnostics, &[])
    }

    /// Starts the application and, when supplied, loads one development plugin DLL.
    ///
    /// # Errors
    ///
    /// Returns prerequisite, host startup, DLL loading, or diagnostic failures.
    pub fn start_with_plugin(
        diagnostics: DiagnosticsSession,
        plugin_dll: Option<&Path>,
    ) -> Result<Self, Error> {
        Self::start_with_plugins(
            diagnostics,
            &plugin_dll
                .into_iter()
                .map(Path::to_path_buf)
                .collect::<Vec<_>>(),
        )
    }

    /// Starts the application with every explicitly supplied official or development plugin DLL.
    pub fn start_with_plugins(
        diagnostics: DiagnosticsSession,
        plugin_dlls: &[PathBuf],
    ) -> Result<Self, Error> {
        let dpi_outcome = initialize_dpi_awareness()?;
        let dpi_outcome_text = format!("{dpi_outcome:?}");
        diagnostics.record_event("windows_prerequisites_ready", &[("dpi", &dpi_outcome_text)])?;
        let shell_sta = Arc::new(ShellStaHandle::start()?);
        diagnostics.record_event("shell_sta_ready", &[])?;
        let _uitest_state_root = uitest_extension_state_root_v1()?;
        let installed_sepacks = discover_installed_sepacks()?;
        #[cfg(feature = "uitest-support")]
        let extension_config = _uitest_state_root.as_ref().map_or_else(
            explorer_extension_host::ExtensionHostConfigV1::default,
            |state_root| {
                explorer_extension_host::ExtensionHostConfigV1::default()
                    .with_integration_test_state_root(state_root.clone())
            },
        );
        #[cfg(not(feature = "uitest-support"))]
        let extension_config = explorer_extension_host::ExtensionHostConfigV1::default();
        let extension_config = if installed_sepacks.is_empty() {
            extension_config
        } else {
            extension_config
                .with_local_developer_mode(explorer_extension_host::LocalDeveloperModeV1::Enabled)
                .with_local_developer_archives(installed_sepacks)
        };
        let mut extension_host =
            explorer_extension_host::ExtensionHost::with_config(extension_config);
        extension_host.start()?;
        let mut startup_plugin_dlls = plugin_dlls.to_vec();
        startup_plugin_dlls.extend(extension_host.startup_plugin_dlls().iter().cloned());
        startup_plugin_dlls.sort();
        startup_plugin_dlls.dedup();
        let mut direct_loaded = Vec::new();
        for path in &startup_plugin_dlls {
            match extension_host.load_single_plugin_visual_column_runtime(path) {
                Ok(loaded) => Some((path, loaded)),
                Err(explorer_extension_host::SinglePluginLoadErrorV1::BlockedBySafeMode) => {
                    diagnostics.record_event("development_plugin_blocked_by_safe_mode", &[])?;
                    None
                }
                Err(error) => {
                    // A plugin load/registration failure is treated as a global
                    // extension fault. Persist the fail-closed choice before
                    // returning so the next launch runs core-only until the
                    // user explicitly re-enables extensions in Folder Options.
                    let _ = extension_host.set_global_feature_desired(
                        explorer_extension_host::DesiredStateV1::Disabled,
                    );
                    return Err(error.into());
                }
            }
            .map(|loaded| direct_loaded.push(loaded));
        }
        let mut summaries = Vec::new();
        let mut visual_column_runtime = None;
        let mut code_lines_runtimes = Vec::new();
        let mut size_map_runtime = None;
        let mut virtual_folder_runtime = None;
        crate::folder_size_service::retire_obsolete_details_snapshots_v1();
        let folder_size_service = Arc::new(Mutex::new(
            crate::folder_size_service::FolderSizeServiceV1::with_capacity(256),
        ));
        for (path, loaded) in direct_loaded {
            let (summary, measure, renderer, size_map_renderer, batch_columns, virtual_folders) =
                loaded.into_parts_with_virtual_folders();
            let supports_folder_size = summary.contributions().iter().any(|contribution| {
                contribution.contribution_id() == FOLDER_SIZE_CONTRIBUTION_ID_V1
            }) && summary.contributions().iter().any(|contribution| {
                contribution.contribution_id() == FOLDER_SIZE_RENDERER_CONTRIBUTION_ID_V1
            });
            let supports_code_lines = batch_columns.contains(CODE_LINES_CONTRIBUTION_ID_V1);
            let supports_lua_code_lines = batch_columns.contains(LUA_CODE_LINES_CONTRIBUTION_ID_V1);
            let supports_lock_owner = batch_columns.contains(LOCK_OWNER_CONTRIBUTION_ID_V1);
            let (visual_runtime, code_runtime) =
                if supports_code_lines || supports_lua_code_lines || supports_lock_owner {
                    (
                        None,
                        Some(ApplicationCodeLinesRuntimeV1::start(
                            batch_columns,
                            renderer,
                            if supports_lock_owner {
                                BatchDetailsColumnModeV1::LockOwner
                            } else if supports_lua_code_lines {
                                BatchDetailsColumnModeV1::LuaCodeLines
                            } else {
                                BatchDetailsColumnModeV1::CodeLines
                            },
                            if supports_lock_owner {
                                "rust-lock-owner-column".to_owned()
                            } else if path
                                .to_string_lossy()
                                .to_ascii_lowercase()
                                .contains("lua-tokei-code-lines-column")
                            {
                                "lua-tokei-code-lines-column".to_owned()
                            } else {
                                "rust-tokei-code-lines-column".to_owned()
                            },
                        )?),
                    )
                } else if supports_folder_size {
                    (
                        Some(ApplicationVisualColumnRuntimeV1::start(
                            measure,
                            renderer,
                            Arc::clone(&folder_size_service),
                        )?),
                        None,
                    )
                } else {
                    (None, None)
                };
            // The host retains a Size Map renderer only after validating
            // that its descriptor is a VIEW_MODE contribution, so this
            // single check rejects descriptor-only and wrong-kind entries.
            let supports_size_map =
                size_map_renderer.has_view_contribution(SIZE_MAP_VIEW_CONTRIBUTION_ID_V1);
            let map_runtime = if supports_size_map {
                Some(ApplicationSizeMapRuntimeV1::start(
                    size_map_renderer,
                    Arc::clone(&folder_size_service),
                )?)
            } else {
                None
            };
            summaries.push(format_single_plugin_summary(path, &summary));
            if visual_column_runtime.is_none() {
                visual_column_runtime = visual_runtime;
            }
            if let Some(code_runtime) = code_runtime {
                code_lines_runtimes.push(code_runtime);
            }
            if size_map_runtime.is_none() {
                size_map_runtime = map_runtime;
            }
            if virtual_folder_runtime.is_none()
                && virtual_folders.contains(SEVEN_Z_RESOURCE_CONTRIBUTION_ID_V1)
            {
                virtual_folder_runtime = Some(virtual_folders);
            }
        }
        let visual_column_extension_loaded = visual_column_runtime.is_some();
        if visual_column_runtime.is_none() {
            visual_column_runtime = Some(ApplicationVisualColumnRuntimeV1::start_directory_facts(
                Arc::clone(&folder_size_service),
            )?);
        }
        let loaded_extension_summary = (!summaries.is_empty()).then(|| summaries.join(" | "));
        if let Some(summary) = loaded_extension_summary.as_deref() {
            diagnostics.record_event("development_plugin_loaded", &[("summary", summary)])?;
        }
        let extension_job_ui_ingress = extension_host.extension_job_ui_ingress();
        let extension_job_ui_inbox = extension_host.take_extension_job_ui_inbox();
        let safe_mode_incident_offers = extension_host.offers();
        let safe_mode_denies_native_callbacks = extension_host.denies_native_callbacks();
        #[cfg(feature = "uitest-support")]
        if let Some(state_root) = _uitest_state_root.as_deref() {
            write_uitest_safe_mode_probe_v1(
                state_root,
                extension_host.integration_test_recovered_callback_is_denied(),
            )?;
        }
        if !safe_mode_incident_offers.is_empty() || safe_mode_denies_native_callbacks {
            diagnostics.record_event(
                "extension_safe_mode_offer_ready",
                &[
                    ("incidents", &safe_mode_incident_offers.len().to_string()),
                    (
                        "native_callbacks_denied",
                        &safe_mode_denies_native_callbacks.to_string(),
                    ),
                ],
            )?;
        }
        diagnostics.record_event("extension_host_ready", &[])?;
        let broker = std::env::current_exe().ok().map(|application| {
            explorer_extension_broker::BrokerClient::adjacent_to(
                &application,
                explorer_extension_broker::BrokerPolicy::default(),
            )
        });
        match broker.as_ref() {
            Some(client) if client.is_available() => {
                diagnostics.record_event("extension_broker_configured", &[])?;
            }
            Some(_) => diagnostics.record_event(
                "extension_broker_unavailable",
                &[("reason", "adjacent broker executable is unavailable")],
            )?,
            None => diagnostics.record_event(
                "extension_broker_unavailable",
                &[("reason", "application executable location unavailable")],
            )?,
        }
        let broker_warmup = broker
            .as_ref()
            .filter(|client| client.is_available())
            .and_then(|client| {
                let client = client.clone();
                let diagnostics = diagnostics.clone();
                std::thread::Builder::new()
                    .name("extension-broker-warmup".to_owned())
                    .spawn(move || {
                        let verified_health = broker_ui_health(&client);
                        let health = format!("{verified_health:?}");
                        let snapshot = client.lifecycle_snapshot();
                        let generation = snapshot.generation.to_string();
                        let broker_pid = snapshot.broker_pid.unwrap_or_default().to_string();
                        let _ = diagnostics.record_event(
                            "extension_broker_warmup_finished",
                            &[
                                ("health", &health),
                                ("generation", &generation),
                                ("broker_pid", &broker_pid),
                            ],
                        );
                        if verified_health == explorer_ui::state::BrokerUiHealth::Healthy {
                            let _ = diagnostics.record_event(
                                "extension_broker_ready",
                                &[("generation", &generation), ("broker_pid", &broker_pid)],
                            );
                        }
                    })
                    .map_err(|error| {
                        tracing::warn!(%error, "failed to start extension broker warmup");
                        error
                    })
                    .ok()
            });
        Ok(Self {
            resources: Arc::new(Mutex::new(ShutdownResources {
                diagnostics,
                extension_host: Some(extension_host),
                loaded_extension_summary,
                visual_column_runtime,
                visual_column_extension_loaded,
                code_lines_runtimes,
                size_map_runtime,
                virtual_folder_runtime,
                extension_job_ui_inbox,
                extension_job_ui_ingress,
                safe_mode_incident_offers,
                broker_warmup,
                broker,
                shell_sta: Some(shell_sta),
                shutdown: false,
            })),
        })
    }

    fn take_extension_job_ui_bridge(
        &self,
    ) -> Result<
        Option<(
            explorer_extension_host::ExtensionJobUiInboxV1,
            explorer_extension_host::ExtensionJobUiIngressV1,
        )>,
        Error,
    > {
        self.resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
            .map(|mut resources| {
                resources
                    .extension_job_ui_inbox
                    .take()
                    .zip(resources.extension_job_ui_ingress.take())
            })
    }

    /// Runs GPUI until the final window closes or a test harness requests quit.
    ///
    /// # Errors
    ///
    /// Returns a synchronized launch error if GPUI cannot create the initial window.
    #[allow(
        clippy::too_many_lines,
        reason = "application startup keeps platform, lifecycle, window, fixture, and auto-close ownership visible in one audited path"
    )]
    pub fn run_gpui(&self) -> Result<(), Error> {
        let launch_error = Arc::new(Mutex::new(None::<String>));
        let mut extension_job_ui_bridge = self.take_extension_job_ui_bridge()?;
        let closure_error = Arc::clone(&launch_error);
        let diagnostics = self.diagnostics()?;
        let diagnostics_after_run = diagnostics.clone();
        let shell_sta = self.shell_service()?;
        let broker_client = self.broker_client()?;
        let broker_health = configured_broker_ui_health(&broker_client);
        let retry_client = broker_client.clone();
        let broker_retry: explorer_ui::BrokerRetryObserver =
            Arc::new(move || broker_ui_health(&retry_client));
        explorer_ui::navigation_pane::configure_adb_navigation_devices(
            crate::remote_service::discover_adb_navigation_devices(),
        );
        explorer_ui::navigation_pane::configure_sftp_navigation_profiles(
            crate::remote_service::configured_sftp_navigation_profiles(),
        );
        crate::remote_service::start_adb_navigation_refresh();
        let shell_service: Arc<dyn explorer_model::ExplorerService> =
            Arc::new(crate::brokered_service::BrokeredExplorerService::new(
                Arc::clone(&shell_sta),
                broker_client,
                self.take_virtual_folder_runtime()?,
            ));
        let remote_runtime = crate::remote_service::configured_remote_runtime();
        let sftp_login_runtime = Arc::clone(&remote_runtime);
        let shell_service: Arc<dyn explorer_model::ExplorerService> =
            Arc::new(crate::remote_service::RemoteExplorerService::new(
                shell_service,
                Arc::clone(&remote_runtime.providers),
            ));
        let shutdown_resources = Arc::clone(&self.resources);
        let mut installed_package_ids = {
            let resources = self
                .resources
                .lock()
                .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?;
            resources
                .extension_host
                .as_ref()
                .map(|host| {
                    host.discovered_package_ids()
                        .iter()
                        .cloned()
                        .collect::<std::collections::BTreeSet<_>>()
                })
                .unwrap_or_default()
        };
        installed_package_ids.extend(
            OFFICIAL_PLUGIN_PACKAGE_IDS
                .iter()
                .map(|package_id| (*package_id).to_owned()),
        );
        let extension_desired_states = {
            let resources = self
                .resources
                .lock()
                .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?;
            resources
                .extension_host
                .as_ref()
                .and_then(explorer_extension_host::ExtensionHost::feature_state)
                .map(|state| {
                    installed_package_ids
                        .iter()
                        .map(|package_id| {
                            (
                                package_id.clone(),
                                state.package_desired(package_id)
                                    == explorer_extension_host::DesiredStateV1::Enabled,
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        let safe_mode_offers = self.safe_mode_ui_offers()?;
        let loaded_extension_summary = self.loaded_extension_summary()?;
        let visual_column_runtime = self.visual_column_runtime()?;
        let visual_column_extension_loaded = self.visual_column_extension_loaded()?;
        let code_lines_runtimes = self.code_lines_runtimes()?;
        let size_map_runtime = self.size_map_runtime()?;
        let safe_mode_resources = Arc::clone(&self.resources);
        let safe_mode_confirm: explorer_ui::SafeModeConfirmObserverV1 = Arc::new(move |token| {
            ApplicationLifecycle::confirm_safe_mode_incident_for_presentation_token(
                &safe_mode_resources,
                token,
            )
            .map_err(|error| error.to_string())
            .and_then(|confirmed| {
                confirmed
                    .then_some(())
                    .ok_or_else(|| "Safe Mode offer is no longer active".to_owned())
            })
        });
        let auto_close = std::env::var("EXPLORER_AUTO_CLOSE_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis);
        let visual_fixture = VisualFixtureConfig::from_environment()?;
        let show_splash =
            crate::branding::should_show_splash(visual_fixture.is_some(), auto_close.is_some());
        let initial_location = configured_initial_location()?;
        let (restored_tabs, restored_placement) = if visual_fixture.is_none() {
            load_session_restore(&diagnostics, initial_location.clone())
        } else {
            (None, None)
        };
        let (
            mut persistence,
            durable_observer,
            reset_observer,
            restore_preference,
            quick_access,
            bookmarks,
        ) = if visual_fixture.is_none() {
            create_session_persistence(restored_placement)
        } else {
            (
                None,
                None,
                None,
                true,
                Vec::new(),
                explorer_model::Bookmarks::default(),
            )
        };
        let extension_settings_resources = Arc::clone(&self.resources);
        let extension_settings_observer: explorer_ui::ExtensionSettingsObserver =
            Arc::new(move |updates| {
                let mut resources = extension_settings_resources
                    .lock()
                    .map_err(|_| "application lifecycle mutex was poisoned".to_owned())?;
                let host = resources
                    .extension_host
                    .as_mut()
                    .ok_or_else(|| "extension host is not available".to_owned())?;
                for incident in host.safe_mode_incidents() {
                    host.confirm_safe_mode_incident(incident.incident_id())
                        .map_err(|error| error.to_string())?;
                }
                host.set_package_feature_desired_batch(updates.into_iter().map(
                    |(package_id, enabled)| {
                        (
                            package_id,
                            if enabled {
                                explorer_extension_host::DesiredStateV1::Enabled
                            } else {
                                explorer_extension_host::DesiredStateV1::Disabled
                            },
                        )
                    },
                ))
                .map_err(|error| error.to_string())
            });

        let platform = gpui_windows::WindowsPlatform::new(false)
            .context("failed to initialize GPUI-CE Windows platform")?;
        gpui::Application::with_platform(Rc::new(platform))
            .with_assets(crate::branding::AppAssets)
            .run(move |cx| {
                if visual_fixture.is_some() {
                    cx.set_global(explorer_ui::diagnostics::RegionDiagnosticsRecorder::default());
                }
                cx.bind_keys(explorer_ui::actions::gpui_text_input_bindings());
                cx.bind_keys(explorer_ui::actions::gpui_key_bindings());

                let folder_options_controller =
                    Rc::new(RefCell::new(FolderOptionsWindowControllerV1::default()));

                cx.on_app_quit(move |_| {
                    if let Err(error) = shutdown_shared(&shutdown_resources) {
                        tracing::error!(%error, "application quit cleanup failed");
                        if let Ok(resources) = shutdown_resources.lock() {
                            resources.diagnostics.record_error(
                                ErrorSeverity::Error,
                                "application",
                                "quit_cleanup",
                                error.as_ref(),
                                Some(file!()),
                            );
                        }
                    }
                    std::future::ready(())
                })
                .detach();

                let folder_options_controller_for_close = Rc::clone(&folder_options_controller);
                cx.on_window_closed(move |cx, closed_id| {
                    let closing = {
                        let mut controller = folder_options_controller_for_close.borrow_mut();
                        let options_closed = controller
                            .window
                            .is_some_and(|handle| handle.window_id() == closed_id);
                        let owner_closed = controller
                            .owner
                            .is_some_and(|handle| handle.window_id() == closed_id);
                        (options_closed || owner_closed)
                            .then(|| (options_closed, controller.clear()))
                    };
                    if let Some((options_closed, Some((options, owner)))) = closing {
                        if options_closed {
                            let _ = owner.update(cx, |root, owner_window, cx| {
                                root.dispatch_folder_options_action(
                                    explorer_ui::actions::ExplorerAction::CloseFolderOptions,
                                    explorer_ui::actions::ActionSource::Programmatic,
                                    owner_window,
                                    cx,
                                );
                            });
                        } else {
                            let _ = options.update(cx, |_, window, _| window.remove_window());
                        }
                    }
                    if cx.windows().is_empty() {
                        cx.quit();
                    }
                })
                .detach();

                let window_options = visual_fixture.as_ref().map_or_else(
                    || {
                        restored_placement.map_or_else(
                            || initial_window_options(cx),
                            window_options_with_placement,
                        )
                    },
                    |fixture| window_options_with_size(cx, fixture.width, fixture.height),
                );
                let fixture_for_window = visual_fixture.clone();
                let initial_location_for_window = initial_location.clone();
                let restored_tabs_for_window = restored_tabs.clone();
                let durable_observer_for_window = durable_observer.clone();
                let extension_settings_observer_for_window =
                    extension_settings_observer.clone();
                let reset_observer_for_window = reset_observer.clone();
                let restore_preference_for_window = restore_preference;
                let quick_access_for_window = quick_access.clone();
                let bookmarks_for_window = bookmarks.clone();
                let extension_desired_states_for_window = extension_desired_states.clone();
                let loaded_extension_summary_for_window = loaded_extension_summary.clone();
                let visual_column_runtime_for_window = visual_column_runtime.clone();
                let visual_column_extension_loaded_for_window = visual_column_extension_loaded;
                let code_lines_runtimes_for_window = code_lines_runtimes.clone();
                let size_map_runtime_for_window = size_map_runtime.clone();
                let folder_options_controller_for_window = Rc::clone(&folder_options_controller);
                let folder_options_diagnostics = diagnostics.clone();
                let fixture_diagnostics = diagnostics.clone();
                let tokens = fixture_tokens(fixture_for_window.as_ref());
                let main_window = match cx.open_window(window_options, move |window, cx| {
                    let drag_threshold = system_drag_threshold(window);
                    let visual_state = fixture_for_window
                        .as_ref()
                        .filter(|fixture| !fixture.real_shell)
                        .map(VisualFixtureConfig::state);
                    if let Some(fixture) = fixture_for_window {
                        let diagnostics = fixture_diagnostics;
                        let frames = if fixture.real_shell { 30 } else { 1 };
                        schedule_visual_diagnostics(window, fixture, tokens, diagnostics, frames);
                    }
                    let owner_window =
                        gpui::WindowHandle::<ExplorerRoot>::new(window.window_handle().window_id());
                    let root = cx.new(move |cx| {
                        let extension_ui_pump =
                            extension_job_ui_bridge.take().and_then(|(inbox, ingress)| {
                                ApplicationExtensionUiPumpV1::new(inbox, ingress)
                            });
                        create_focused_explorer_root(
                            tokens,
                            shell_service,
                            drag_threshold,
                            visual_state,
                            initial_location_for_window,
                            restored_tabs_for_window,
                            durable_observer_for_window,
                            Some(extension_settings_observer_for_window),
                            reset_observer_for_window,
                            restore_preference_for_window,
                            quick_access_for_window,
                            bookmarks_for_window,
                            extension_desired_states_for_window,
                            broker_health,
                            broker_retry,
                            safe_mode_offers,
                            safe_mode_confirm,
                            loaded_extension_summary_for_window,
                            visual_column_runtime_for_window,
                            visual_column_extension_loaded_for_window,
                            code_lines_runtimes_for_window,
                            size_map_runtime_for_window,
                            extension_ui_pump.map(|pump| {
                                Box::new(pump) as Box<dyn explorer_ui::ExtensionUiPumpPortV1>
                            }),
                            window,
                            cx,
                        )
                    });
                    let controller = Rc::clone(&folder_options_controller_for_window);
                    let observer_diagnostics = folder_options_diagnostics.clone();
                    let bookmark_manager_handle = Rc::new(RefCell::new(None::<
                        gpui::WindowHandle<
                            explorer_ui::bookmark_manager_window::BookmarkManagerWindow,
                        >,
                    >));
                    let bookmark_action_handle = Rc::new(RefCell::new(None::<
                        gpui::WindowHandle<
                            explorer_ui::bookmark_action_window::BookmarkActionWindow,
                        >,
                    >));
                    let bookmark_folder_editor_handle = Rc::new(RefCell::new(None::<
                        gpui::WindowHandle<
                            explorer_ui::bookmark_folder_editor_window::BookmarkFolderEditorWindow,
                        >,
                    >));
                    root.update(cx, |root, _| {
                        let runtime = Arc::clone(&sftp_login_runtime);
                        root.attach_sftp_address_login_observer(Arc::new(move |input| {
                            runtime.login_address(input)
                        }));
                        root.attach_folder_options_window_observer(Rc::new(move |create, snapshot, cx| {
                            let existing = controller.borrow().window;
                            if let Some(existing) = existing {
                                if existing
                                    .update(cx, |_, window, _| window.activate_window())
                                    .is_ok()
                                {
                                    let _ = controller.borrow_mut().lifecycle.begin_open();
                                    return true;
                                }
                                let _ = controller.borrow_mut().clear();
                            }
                            if !create {
                                return false;
                            }
                            let Some(snapshot) = snapshot else {
                                tracing::warn!("Folder Options window creation lacked its draft snapshot");
                                return false;
                            };
                            let generation = match controller.borrow_mut().lifecycle.begin_open() {
                                FolderOptionsOpenIntentV1::Create { generation } => generation,
                                FolderOptionsOpenIntentV1::Activate { .. } => return true,
                            };
                            let options =
                                explorer_ui::folder_options_window::folder_options_window_options(
                                    cx,
                                );
                            let opened = cx.open_window(options, move |window, cx| {
                                cx.new(|cx| {
                                    explorer_ui::folder_options_window::FolderOptionsWindow::new(
                                        tokens,
                                        owner_window,
                                        snapshot,
                                        window,
                                        cx,
                                    )
                                })
                            });
                            match opened {
                                Ok(handle) => {
                                    if !controller
                                        .borrow_mut()
                                        .install(generation, handle, owner_window)
                                    {
                                        tracing::warn!(generation, "Folder Options ignored a stale creation result");
                                        return false;
                                    }
                                    let _ = observer_diagnostics
                                        .record_event("folder_options_window_ready", &[]);
                                    true
                                }
                                Err(error) => {
                                    controller
                                        .borrow_mut()
                                        .lifecycle
                                        .creation_failed(generation);
                                    tracing::warn!(%error, "Folder Options window creation failed");
                                    let message = error.to_string();
                                    let _ = observer_diagnostics.record_event(
                                        "folder_options_window_create_failed",
                                        &[("error", &message)],
                                    );
                                    false
                                }
                            }
                        }));
                        root.attach_bookmark_editor_window_observer(Rc::new(move |snapshot, cx| {
                            let options = explorer_ui::bookmark_editor_window::bookmark_editor_window_options(cx);
                            let opened = cx.open_window(options, move |window, cx| {
                                cx.new(|cx| {
                                    explorer_ui::bookmark_editor_window::BookmarkEditorWindow::new(
                                        tokens,
                                        owner_window,
                                        snapshot,
                                        window,
                                        cx,
                                    )
                                })
                            });
                            match opened {
                                Ok(_) => true,
                                Err(error) => {
                                    tracing::warn!(%error, "Bookmark editor window creation failed");
                                    false
                                }
                            }
                        }));
                        root.attach_bookmark_folder_editor_window_observer(Rc::new(
                            move |snapshot, cx| {
                                if let Some(existing) = *bookmark_folder_editor_handle.borrow() {
                                    if existing
                                        .update(cx, |editor, window, cx| {
                                            editor.replace_snapshot(snapshot.clone(), window, cx);
                                            window.activate_window();
                                        })
                                        .is_ok()
                                    {
                                        return true;
                                    }
                                    *bookmark_folder_editor_handle.borrow_mut() = None;
                                }
                                let options = explorer_ui::bookmark_folder_editor_window::bookmark_folder_editor_window_options(cx);
                                let opened = cx.open_window(options, move |window, cx| {
                                    cx.new(|cx| {
                                        explorer_ui::bookmark_folder_editor_window::BookmarkFolderEditorWindow::new(
                                            tokens,
                                            owner_window,
                                            snapshot,
                                            window,
                                            cx,
                                        )
                                    })
                                });
                                match opened {
                                    Ok(handle) => {
                                        *bookmark_folder_editor_handle.borrow_mut() = Some(handle);
                                        true
                                    }
                                    Err(error) => {
                                        tracing::warn!(%error, "Bookmark folder editor window creation failed");
                                        false
                                    }
                                }
                            },
                        ));
                        root.attach_bookmark_manager_window_observer(Rc::new(move |snapshot, cx| {
                            if let Some(existing) = *bookmark_manager_handle.borrow() {
                                if existing
                                    .update(cx, |manager, window, cx| {
                                        manager.replace_snapshot(snapshot.clone(), window, cx);
                                        window.activate_window();
                                    })
                                    .is_ok()
                                {
                                    return true;
                                }
                                *bookmark_manager_handle.borrow_mut() = None;
                            }
                            let options = explorer_ui::bookmark_manager_window::bookmark_manager_window_options(cx);
                            let opened = cx.open_window(options, move |window, cx| {
                                cx.new(|cx| {
                                    explorer_ui::bookmark_manager_window::BookmarkManagerWindow::new(
                                        tokens,
                                        owner_window,
                                        snapshot,
                                        window,
                                        cx,
                                    )
                                })
                            });
                            match opened {
                                Ok(handle) => {
                                    *bookmark_manager_handle.borrow_mut() = Some(handle);
                                    true
                                }
                                Err(error) => {
                                    tracing::warn!(%error, "Bookmark manager window creation failed");
                                    false
                                }
                            }
                        }));
                        root.attach_bookmark_action_window_observer(Rc::new(move |snapshot, cx| {
                            if let Some(existing) = *bookmark_action_handle.borrow() {
                                if existing
                                    .update(cx, |action_window, window, cx| {
                                        action_window.replace_snapshot(snapshot.clone(), window, cx);
                                        window.activate_window();
                                    })
                                    .is_ok()
                                {
                                    return true;
                                }
                                *bookmark_action_handle.borrow_mut() = None;
                            }
                            let options = explorer_ui::bookmark_action_window::bookmark_action_window_options(cx);
                            let opened = cx.open_window(options, move |window, cx| {
                                cx.new(|cx| {
                                    explorer_ui::bookmark_action_window::BookmarkActionWindow::new(
                                        tokens,
                                        owner_window,
                                        snapshot,
                                        window,
                                        cx,
                                    )
                                })
                            });
                            match opened {
                                Ok(handle) => {
                                    *bookmark_action_handle.borrow_mut() = Some(handle);
                                    true
                                }
                                Err(error) => {
                                    tracing::warn!(%error, "Bookmark action window creation failed");
                                    false
                                }
                            }
                        }));
                    });
                    root
                }) {
                    Ok(handle) => {
                        let _ = diagnostics.record_event("window_ready", &[]);
                        handle
                    }
                    Err(error) => {
                        let mut launch_error = closure_error
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        *launch_error = Some(error.to_string());
                        cx.quit();
                        return;
                    }
                };

                if std::env::var_os("SUPEREXPLORER_UITEST_OPEN_FOLDER_OPTIONS").is_some() {
                    let _ = main_window.update(cx, |root, window, cx| {
                        root.dispatch_action_for_test(
                            explorer_ui::actions::ExplorerAction::OpenFolderOptions,
                            explorer_ui::actions::ActionSource::Programmatic,
                            window,
                            cx,
                        );
                    });
                }

                if show_splash && let Err(error) = crate::branding::open_splash(cx, main_window) {
                    tracing::warn!(%error, "startup splash could not be created");
                    diagnostics.record_error(
                        ErrorSeverity::Warning,
                        "application",
                        "open_startup_splash",
                        error.as_ref(),
                        Some(file!()),
                    );
                }

                if let Some(delay) = auto_close {
                    cx.spawn(async move |cx| {
                        cx.background_executor().timer(delay).await;
                        cx.update(|cx| cx.quit());
                    })
                    .detach();
                }
            });

        let error = launch_error
            .lock()
            .map_err(|_| anyhow::anyhow!("GPUI launch error mutex was poisoned"))?
            .take();
        if let Some(coordinator) = &mut persistence {
            let flushed = coordinator.shutdown(Duration::from_secs(5));
            let health = coordinator.health();
            let _ = diagnostics_after_run.record_event(
                "session_persistence_stopped",
                &[
                    ("flushed", &flushed.to_string()),
                    ("writes", &health.successful_writes.to_string()),
                    ("failures", &health.failed_writes.to_string()),
                ],
            );
        }
        if let Some(error) = error {
            Err(anyhow::anyhow!(error)).context("failed to open initial GPUI window")
        } else {
            Ok(())
        }
    }

    /// Performs idempotent reverse-order process shutdown.
    ///
    /// # Errors
    ///
    /// Returns a bounded Shell join or diagnostics flush failure.
    pub fn shutdown(&mut self) -> Result<(), Error> {
        shutdown_shared(&self.resources)
    }

    /// Returns the startup-recovered, path-free incidents that require explicit
    /// user confirmation. Inspecting this list never clears native denial.
    ///
    /// # Errors
    ///
    /// Returns an error if application lifecycle state is unavailable.
    pub fn safe_mode_incident_offers(&self) -> Result<Vec<SafeModeIncidentOffer>, Error> {
        self.resources
            .lock()
            .map(|resources| resources.safe_mode_incident_offers.clone())
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
    }

    /// Explicitly confirms one startup Safe Mode offer through the resident
    /// extension host. No offer is cleared merely by being displayed.
    ///
    /// # Errors
    ///
    /// Returns a host confirmation failure or lifecycle-state error. Unknown or
    /// already-confirmed offers return `Ok(false)` without calling the host.
    pub fn confirm_safe_mode_incident(
        &self,
        incident_id: explorer_extension_host::NativeSafeModeIncidentIdV1,
    ) -> Result<bool, Error> {
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?;
        let mut offers = std::mem::take(&mut resources.safe_mode_incident_offers);
        let result = resources
            .extension_host
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("extension host is not available"))
            .and_then(|host| {
                confirm_offered_safe_mode_incident_v1(host, &mut offers, incident_id)
                    .map_err(Error::from)
            });
        resources.safe_mode_incident_offers = offers;
        let confirmed = result?;
        if confirmed {
            let remaining = resources.safe_mode_incident_offers.len().to_string();
            emit_post_commit_safe_mode_telemetry_v1(|| {
                resources.diagnostics.record_event(
                    "extension_safe_mode_incident_confirmed",
                    &[("remaining_incidents", &remaining)],
                )
            });
        }
        Ok(confirmed)
    }

    fn confirm_safe_mode_incident_for_presentation_token(
        shared: &Arc<Mutex<ShutdownResources>>,
        token: u64,
    ) -> Result<bool, Error> {
        let mut resources = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?;
        let mut offers = std::mem::take(&mut resources.safe_mode_incident_offers);
        let result = resources
            .extension_host
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("extension host is not available"))
            .and_then(|host| {
                confirm_presented_safe_mode_incident_v1(host, &mut offers, token)
                    .map_err(Error::from)
            });
        resources.safe_mode_incident_offers = offers;
        let confirmed = result?;
        if confirmed {
            let remaining = resources.safe_mode_incident_offers.len().to_string();
            emit_post_commit_safe_mode_telemetry_v1(|| {
                resources.diagnostics.record_event(
                    "extension_safe_mode_incident_confirmed",
                    &[("remaining_incidents", &remaining)],
                )
            });
        }
        Ok(confirmed)
    }

    fn safe_mode_ui_offers(&self) -> Result<Vec<explorer_ui::SafeModeOfferV1>, Error> {
        self.safe_mode_incident_offers().map(|offers| {
            offers
                .into_iter()
                .map(|offer| {
                    let suspect = offer.suspect();
                    explorer_ui::SafeModeOfferV1 {
                        presentation_token: offer.presentation_token(),
                        package_id: suspect.map(|value| value.package_id().to_owned()),
                        primary_interface_namespace: suspect
                            .map(SafeModeSuspectV1::primary_interface_namespace),
                        primary_interface_value: suspect
                            .map(SafeModeSuspectV1::primary_interface_value),
                        operation: format!("{:?}", offer.kind()),
                    }
                })
                .collect()
        })
    }

    fn diagnostics(&self) -> Result<DiagnosticsSession, Error> {
        self.resources
            .lock()
            .map(|resources| resources.diagnostics.clone())
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
    }

    fn loaded_extension_summary(&self) -> Result<Option<String>, Error> {
        self.resources
            .lock()
            .map(|resources| resources.loaded_extension_summary.clone())
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
    }

    fn visual_column_runtime(
        &self,
    ) -> Result<Option<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1>, Error> {
        self.resources
            .lock()
            .map(|resources| resources.visual_column_runtime.clone())
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
    }

    fn visual_column_extension_loaded(&self) -> Result<bool, Error> {
        self.resources
            .lock()
            .map(|resources| resources.visual_column_extension_loaded)
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
    }

    fn code_lines_runtimes(
        &self,
    ) -> Result<Vec<explorer_ui::code_lines_column::CodeLinesRuntimeHandleV1>, Error> {
        self.resources
            .lock()
            .map(|resources| resources.code_lines_runtimes.clone())
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
    }

    fn size_map_runtime(
        &self,
    ) -> Result<Option<explorer_ui::size_map_view::SizeMapRuntimeHandleV1>, Error> {
        self.resources
            .lock()
            .map(|resources| resources.size_map_runtime.clone())
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
    }

    fn take_virtual_folder_runtime(
        &self,
    ) -> Result<Option<explorer_extension_host::SinglePluginVirtualFolderRuntimeV1>, Error> {
        self.resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))
            .map(|mut resources| resources.virtual_folder_runtime.take())
    }

    fn shell_service(&self) -> Result<Arc<ShellStaHandle>, Error> {
        self.resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?
            .shell_sta
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Shell STA is not available"))
    }

    fn broker_client(&self) -> Result<explorer_extension_broker::BrokerClient, Error> {
        self.resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?
            .broker
            .clone()
            .ok_or_else(|| anyhow::anyhow!("extension broker client is not available"))
    }
}

fn discover_installed_sepacks() -> Result<Vec<PathBuf>, Error> {
    let executable =
        std::env::current_exe().context("could not resolve SuperExplorer executable")?;
    let Some(parent) = executable.parent() else {
        return Ok(Vec::new());
    };
    let root = parent.join("plugins");
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut archives = Vec::new();
    for entry in entries.take(1_024) {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        if is_sepack_path(&path) {
            archives.push(path);
        }
    }
    archives.sort();
    Ok(archives)
}

fn is_sepack_path(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sepack"))
}

const OFFICIAL_PLUGIN_PACKAGE_IDS: &[&str] = &[
    "rust-folder-size-visual-column",
    "rust-folder-size-map-view",
    "rust-tokei-code-lines-column",
    "lua-tokei-code-lines-column",
    "rust-lock-owner-column",
    "rust-exif-rename-command",
    "rust-7z-virtual-folder",
    "lua-bulk-folder-generator",
];

#[allow(
    clippy::too_many_arguments,
    reason = "the GPUI composition root wires explicit lifecycle-owned services and restore inputs"
)]
fn create_explorer_root(
    tokens: UiTokens,
    shell_service: Arc<dyn explorer_model::ExplorerService>,
    drag_threshold: (f32, f32),
    visual_state: Option<explorer_ui::VisualFixtureState>,
    initial_location: Option<explorer_model::HistoryEntry>,
    restored_tabs: Option<explorer_model::ExplorerWindowState>,
    durable_observer: Option<explorer_ui::DurableStateObserver>,
    extension_settings_observer: Option<explorer_ui::ExtensionSettingsObserver>,
    reset_observer: Option<explorer_ui::SessionResetObserver>,
    restore_preference: bool,
    quick_access: Vec<explorer_model::PersistedQuickAccessPin>,
    bookmarks: explorer_model::Bookmarks,
    extension_desired_states: Vec<(String, bool)>,
    broker_health: explorer_ui::state::BrokerUiHealth,
    broker_retry: explorer_ui::BrokerRetryObserver,
    visual_column_runtime: Option<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1>,
    visual_column_extension_loaded: bool,
    code_lines_runtimes: Vec<explorer_ui::code_lines_column::CodeLinesRuntimeHandleV1>,
    size_map_runtime: Option<explorer_ui::size_map_view::SizeMapRuntimeHandleV1>,
    extension_ui_pump: Option<Box<dyn explorer_ui::ExtensionUiPumpPortV1>>,
    window: &gpui::Window,
    cx: &mut gpui::Context<ExplorerRoot>,
) -> ExplorerRoot {
    let mut root = match visual_state {
        Some(state) => {
            let mut root = ExplorerRoot::for_visual_fixture(tokens, state);
            root.attach_service_for_shell_assets(shell_service);
            root
        }
        None => explorer_root(
            tokens,
            shell_service,
            drag_threshold,
            initial_location,
            restored_tabs,
        ),
    };
    root.configure_restore_previous_session(restore_preference);
    root.configure_quick_access(quick_access);
    root.configure_bookmarks(bookmarks);
    root.configure_extension_desired_states(&extension_desired_states);
    root.configure_broker_health(broker_health, broker_retry);
    root.attach_command_prompt_launcher(Arc::new(|working_directory| {
        explorer_shell_win::launch_command_prompt(working_directory.as_deref())
            .map_err(|error| format!("Unable to open Command Prompt: {error}"))
    }));
    root.attach_bookmark_file_launcher(Arc::new(|location| {
        explorer_shell_win::open_default(&location).map_err(|error| error.to_string())
    }));
    if let Some(runtime) = visual_column_runtime {
        if visual_column_extension_loaded {
            root.attach_visual_column_runtime(runtime);
        } else {
            root.attach_directory_facts_runtime(runtime);
        }
    }
    for runtime in code_lines_runtimes {
        root.attach_code_lines_runtime(runtime);
    }
    if let Some(runtime) = size_map_runtime {
        root.attach_size_map_runtime(runtime);
    }
    if let Some(observer) = durable_observer {
        root.attach_durable_state_observer(observer, window, cx);
    }
    if let Some(observer) = extension_settings_observer {
        root.attach_extension_settings_observer(observer);
    }
    if let Some(observer) = reset_observer {
        root.attach_session_reset_observer(observer);
    }
    if let Some(pump) = extension_ui_pump {
        root.attach_extension_ui_pump(pump);
    }
    root.start_service_pump(window.window_handle(), cx);
    root
}

#[allow(
    clippy::too_many_arguments,
    reason = "the composition root passes independent platform, restore, persistence, and focus adapters"
)]
fn create_focused_explorer_root(
    tokens: UiTokens,
    shell_service: Arc<dyn explorer_model::ExplorerService>,
    drag_threshold: (f32, f32),
    visual_state: Option<explorer_ui::VisualFixtureState>,
    initial_location: Option<explorer_model::HistoryEntry>,
    restored_tabs: Option<explorer_model::ExplorerWindowState>,
    durable_observer: Option<explorer_ui::DurableStateObserver>,
    extension_settings_observer: Option<explorer_ui::ExtensionSettingsObserver>,
    reset_observer: Option<explorer_ui::SessionResetObserver>,
    restore_preference: bool,
    quick_access: Vec<explorer_model::PersistedQuickAccessPin>,
    bookmarks: explorer_model::Bookmarks,
    extension_desired_states: Vec<(String, bool)>,
    broker_health: explorer_ui::state::BrokerUiHealth,
    broker_retry: explorer_ui::BrokerRetryObserver,
    safe_mode_offers: Vec<explorer_ui::SafeModeOfferV1>,
    safe_mode_confirm: explorer_ui::SafeModeConfirmObserverV1,
    loaded_extension_summary: Option<String>,
    visual_column_runtime: Option<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1>,
    visual_column_extension_loaded: bool,
    code_lines_runtimes: Vec<explorer_ui::code_lines_column::CodeLinesRuntimeHandleV1>,
    size_map_runtime: Option<explorer_ui::size_map_view::SizeMapRuntimeHandleV1>,
    extension_ui_pump: Option<Box<dyn explorer_ui::ExtensionUiPumpPortV1>>,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<ExplorerRoot>,
) -> ExplorerRoot {
    let focus_handle = cx.focus_handle();
    focus_handle.focus(window, cx);
    let mut root = create_explorer_root(
        tokens,
        shell_service,
        drag_threshold,
        visual_state,
        initial_location,
        restored_tabs,
        durable_observer,
        extension_settings_observer,
        reset_observer,
        restore_preference,
        quick_access,
        bookmarks,
        extension_desired_states,
        broker_health,
        broker_retry,
        visual_column_runtime,
        visual_column_extension_loaded,
        code_lines_runtimes,
        size_map_runtime,
        extension_ui_pump,
        window,
        cx,
    );
    root.configure_shell_icon_scale(window.scale_factor());
    root.attach_pointer_capture_factory(Arc::new(|hwnd| {
        crate::pointer_capture::NativePointerCapture::acquire(hwnd)
    }));
    root.attach_text_inputs(cx);
    root.attach_focus_handle(focus_handle);
    let focus_reporter = crate::mft_focus::FocusWindowReporterV1::new();
    focus_reporter.set_focused(window.is_window_active());
    cx.observe_window_activation(window, move |_, window, _| {
        focus_reporter.set_focused(window.is_window_active());
    })
    .detach();
    if !safe_mode_offers.is_empty() {
        root.configure_safe_mode_offers(safe_mode_offers, safe_mode_confirm);
    }
    root.configure_loaded_extension_summary(loaded_extension_summary);
    let build = explorer_common::AppBuildInfo::current();
    root.configure_about_info(explorer_ui::state::AboutInfoV1 {
        version: build.package_version.to_owned(),
        build_date: build.build_date.to_owned(),
        git_hash: build.git_revision.to_owned(),
        author: build.author.to_owned(),
    });
    root
}

fn format_single_plugin_summary(
    path: &Path,
    summary: &explorer_extension_host::SinglePluginLoadSummaryV1,
) -> String {
    let plugin_id = summary.plugin_id();
    let plugin_name = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("development-plugin")
        .replace('_', "-");
    let contributions = summary
        .contributions()
        .iter()
        .map(|contribution| {
            let kind = match contribution.kind().into_raw() {
                1 => "Column",
                2 => "GPUI Renderer",
                3 => "Command",
                4 => "Form",
                5 => "Operation Plan",
                6 => "View Mode",
                7 => "Resource",
                _ => "Unknown",
            };
            format!("{} ({})", contribution.contribution_id(), kind)
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{} — Plugin {}:{}:{} — {}",
        plugin_name,
        plugin_id.namespace.authority(),
        plugin_id.namespace.revision(),
        plugin_id.value,
        contributions
    )
}

fn fixture_tokens(fixture: Option<&VisualFixtureConfig>) -> UiTokens {
    match high_contrast_tokens() {
        Ok(Some(tokens)) => tokens,
        Ok(None) => fixture.map_or_else(UiTokens::default, VisualFixtureConfig::tokens),
        Err(error) => {
            tracing::warn!(%error, "Windows high-contrast query failed; using configured theme");
            fixture.map_or_else(UiTokens::default, VisualFixtureConfig::tokens)
        }
    }
}

fn broker_ui_health(
    client: &explorer_extension_broker::BrokerClient,
) -> explorer_ui::state::BrokerUiHealth {
    match client.verify() {
        Ok(()) => explorer_ui::state::BrokerUiHealth::Healthy,
        Err(explorer_extension_broker::BrokerClientError::Unavailable) => {
            explorer_ui::state::BrokerUiHealth::Unavailable
        }
        Err(explorer_extension_broker::BrokerClientError::VersionMismatch) => {
            explorer_ui::state::BrokerUiHealth::VersionMismatch
        }
        Err(explorer_extension_broker::BrokerClientError::Timeout) => {
            explorer_ui::state::BrokerUiHealth::Timeout
        }
        Err(
            explorer_extension_broker::BrokerClientError::Start
            | explorer_extension_broker::BrokerClientError::Disconnected
            | explorer_extension_broker::BrokerClientError::Protocol,
        ) => explorer_ui::state::BrokerUiHealth::Crash,
    }
}

fn configured_broker_ui_health(
    client: &explorer_extension_broker::BrokerClient,
) -> explorer_ui::state::BrokerUiHealth {
    if client.is_available() {
        explorer_ui::state::BrokerUiHealth::Healthy
    } else {
        explorer_ui::state::BrokerUiHealth::Unavailable
    }
}

fn system_drag_threshold(window: &gpui::Window) -> (f32, f32) {
    explorer_shell_win::SystemDragThreshold::current().logical_at_scale(window.scale_factor())
}

fn configured_initial_location() -> Result<Option<explorer_model::HistoryEntry>, Error> {
    let Some(value) = std::env::var_os("EXPLORER_INITIAL_PATH") else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() || !path.is_dir() {
        anyhow::bail!("EXPLORER_INITIAL_PATH must be an existing absolute directory");
    }
    let title = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map_or_else(|| path.display().to_string(), str::to_owned);
    Ok(Some(explorer_model::HistoryEntry::new(
        explorer_model::LocationDescriptor::file_system(path.display().to_string()),
        title,
    )))
}

fn explorer_root(
    tokens: UiTokens,
    shell_service: Arc<dyn explorer_model::ExplorerService>,
    drag_threshold: (f32, f32),
    initial_location: Option<explorer_model::HistoryEntry>,
    restored_tabs: Option<explorer_model::ExplorerWindowState>,
) -> ExplorerRoot {
    let mut root = if let Some(restored) = restored_tabs {
        ExplorerRoot::with_service_drag_threshold_and_restored_window(
            tokens,
            shell_service,
            drag_threshold,
            restored,
        )
    } else {
        match initial_location {
            Some(initial) => ExplorerRoot::with_service_drag_threshold_and_initial_location(
                tokens,
                Arc::clone(&shell_service),
                drag_threshold,
                initial,
            ),
            None => {
                ExplorerRoot::with_service_and_drag_threshold(tokens, shell_service, drag_threshold)
            }
        }
    };
    root.configure_tortoise_git_available(explorer_shell_win::tortoise_git_is_installed());
    root.configure_new_items(explorer_shell_win::registered_shell_new_items_in_worker());
    root
}

fn load_session_restore(
    diagnostics: &DiagnosticsSession,
    configured: Option<explorer_model::HistoryEntry>,
) -> (
    Option<explorer_model::ExplorerWindowState>,
    Option<explorer_model::PersistedWindowPlacement>,
) {
    let limits = RoadmapLimits::default();
    let Ok(store) = crate::session_store::WindowsSessionStore::from_environment(limits) else {
        let _ = diagnostics.record_event("session_restore_unavailable", &[]);
        return (None, None);
    };
    let Ok(outcome) = store.load() else {
        let _ = diagnostics.record_event("session_restore_failed", &[]);
        return (None, None);
    };
    let Some(envelope) = outcome
        .envelope
        .filter(|value| value.payload.restore_enabled)
    else {
        let _ = diagnostics.record_event(
            "session_restore_defaults",
            &[(
                "rejected_artifacts",
                &outcome.rejected_artifacts.to_string(),
            )],
        );
        return (None, None);
    };
    let Ok(plan) = envelope.restore_plan(limits) else {
        let _ = diagnostics.record_event("session_restore_plan_rejected", &[]);
        return (None, None);
    };
    let placement = crate::session_lifecycle::primary_monitor_work_area().map(|monitor| {
        crate::session_lifecycle::fit_window_placement(plan.window, &[monitor], 640, 480)
    });
    if !should_restore_saved_tabs(configured.as_ref()) {
        let _ = diagnostics.record_event(
            "session_restore_location_overridden",
            &[("tabs", &plan.tabs.len().to_string())],
        );
        return (None, placement);
    }
    let fallback = configured.unwrap_or_else(|| {
        explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\"),
            "This PC",
        )
    });
    let restored = plan.resolve_window(fallback, resolve_saved_location).ok();
    let source = format!("{:?}", outcome.source);
    let _ = diagnostics.record_event(
        "session_restore_ready",
        &[
            ("source", &source),
            ("tabs", &plan.tabs.len().to_string()),
            ("migration", &outcome.migration_performed.to_string()),
        ],
    );
    (restored, placement)
}

const fn should_restore_saved_tabs(configured: Option<&explorer_model::HistoryEntry>) -> bool {
    configured.is_none()
}

fn resolve_saved_location(
    descriptor: &explorer_model::LocationDescriptor,
) -> Option<explorer_model::HistoryEntry> {
    if let Some(path) = descriptor.path() {
        if !path.is_dir() {
            return None;
        }
        let title = path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .map_or_else(|| path.display().to_string(), str::to_owned);
        return Some(explorer_model::HistoryEntry::new(descriptor.clone(), title));
    }
    Some(explorer_model::HistoryEntry::new(
        descriptor.clone(),
        "Shell location",
    ))
}

fn create_session_persistence(
    _restored_placement: Option<explorer_model::PersistedWindowPlacement>,
) -> (
    Option<crate::session_lifecycle::PersistenceCoordinator>,
    Option<explorer_ui::DurableStateObserver>,
    Option<explorer_ui::SessionResetObserver>,
    bool,
    Vec<explorer_model::PersistedQuickAccessPin>,
    explorer_model::Bookmarks,
) {
    let limits = RoadmapLimits::default();
    let Ok(store) = crate::session_store::WindowsSessionStore::from_environment(limits) else {
        return (
            None,
            None,
            None,
            true,
            Vec::new(),
            explorer_model::Bookmarks::default(),
        );
    };
    let loaded = store.load().ok().and_then(|outcome| outcome.envelope);
    let generation = loaded
        .as_ref()
        .map_or(1, |envelope| envelope.write_generation.saturating_add(1));
    let quick_access = loaded
        .as_ref()
        .map_or_else(Vec::new, |envelope| envelope.payload.quick_access.clone());
    let legacy_bookmarks = loaded
        .as_ref()
        .map_or_else(explorer_model::Bookmarks::default, |envelope| {
            envelope.payload.bookmarks.clone()
        });
    let bookmark_store = crate::bookmark_store::WindowsBookmarkStore::from_environment(limits).ok();
    let bookmarks = bookmark_store.as_ref().map_or_else(
        || legacy_bookmarks.clone(),
        |store| {
            let resolution = store.load_or_migrate(&legacy_bookmarks);
            if let Some(warning) = &resolution.warning {
                tracing::warn!(operation = "bookmark_load_or_migrate", %warning, "Independent bookmark persistence is temporarily unavailable");
            }
            resolution.bookmarks
        },
    );
    let restore_enabled = loaded
        .as_ref()
        .is_none_or(|envelope| envelope.payload.restore_enabled);
    let store: Arc<dyn explorer_model::SessionStore> = Arc::new(store);
    let bookmark_store = bookmark_store
        .map(|store| Arc::new(store) as Arc<dyn crate::bookmark_store::BookmarkStore>);
    let coordinator = crate::session_lifecycle::PersistenceCoordinator::start_with_bookmarks(
        store,
        bookmark_store,
        Duration::from_millis(limits.preview_debounce_ms.max(250)),
        Duration::from_secs(2),
    );
    let handle = coordinator.handle();
    let generation = Arc::new(AtomicU64::new(generation));
    let reset_handle = handle.clone();
    let reset_observer: explorer_ui::SessionResetObserver =
        Arc::new(move |scope| reset_handle.request_reset(scope));
    let observer: explorer_ui::DurableStateObserver = Arc::new(
        move |window, restore_enabled, quick_access, bookmarks, placement| {
            let write_generation = generation.fetch_add(1, Ordering::AcqRel);
            handle.accepted_runtime(
                crate::session_lifecycle::DurableTransition::ViewSettingsChanged,
                crate::session_lifecycle::RuntimeSessionSnapshot {
                    window,
                    placement,
                    quick_access,
                    bookmarks,
                    restore_enabled,
                    write_generation,
                    provenance: explorer_model::SessionProvenance {
                        app_version: env!("CARGO_PKG_VERSION").to_owned(),
                        app_revision: option_env!("GIT_REVISION").unwrap_or("unknown").to_owned(),
                        windows_build: std::env::var("OS").unwrap_or_else(|_| "Windows".to_owned()),
                    },
                    limits,
                },
            )
        },
    );
    (
        Some(coordinator),
        Some(observer),
        Some(reset_observer),
        restore_enabled,
        quick_access,
        bookmarks,
    )
}

fn shutdown_shared(resources: &Arc<Mutex<ShutdownResources>>) -> Result<(), Error> {
    let mut resources = resources
        .lock()
        .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?;
    resources.shutdown()
}

impl ShutdownResources {
    fn shutdown(&mut self) -> Result<(), Error> {
        if self.shutdown {
            return Ok(());
        }
        self.shutdown = true;

        let mut failures = Vec::new();
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_started", &[("stage", "extension_host")]);
        if let Some(mut extension_host) = self.extension_host.take() {
            extension_host.shutdown();
        }
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_finished", &[("stage", "extension_host")]);
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_started", &[("stage", "broker_warmup")]);
        if let Some(warmup) = self.broker_warmup.take()
            && warmup.join().is_err()
        {
            failures.push("extension broker warmup thread panicked".to_owned());
        }
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_finished", &[("stage", "broker_warmup")]);
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_started", &[("stage", "broker")]);
        if let Some(broker) = self.broker.take() {
            broker.shutdown();
        }
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_finished", &[("stage", "broker")]);
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_started", &[("stage", "shell_sta")]);
        if let Some(shell_sta) = self.shell_sta.take()
            && let Err(error) = shell_sta.shutdown_and_join(SHELL_JOIN_TIMEOUT)
        {
            failures.push(format!("Shell STA: {error}"));
        }
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_finished", &[("stage", "shell_sta")]);
        if let Err(error) = self.diagnostics.record_event("application_stopped", &[]) {
            failures.push(format!("application_stopped event: {error}"));
        }
        if let Err(error) = self.diagnostics.record_event("clean_shutdown", &[]) {
            failures.push(format!("clean_shutdown event: {error}"));
        }
        if let Err(error) = self.diagnostics.shutdown() {
            failures.push(format!("diagnostics shutdown: {error}"));
        }

        if failures.is_empty() {
            Ok(())
        } else {
            let error = anyhow::anyhow!("application cleanup failed: {}", failures.join("; "));
            self.diagnostics.record_error(
                ErrorSeverity::Error,
                "application",
                "shutdown",
                error.as_ref(),
                Some(file!()),
            );
            Err(error)
        }
    }
}

impl Drop for ApplicationLifecycle {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::error!(%error, "application lifecycle cleanup failed");
            if let Ok(resources) = self.resources.lock() {
                resources.diagnostics.record_error(
                    ErrorSeverity::Error,
                    "application",
                    "drop_cleanup",
                    error.as_ref(),
                    Some(file!()),
                );
            }
        }
    }
}

fn size_map_scanning_result(
    request: explorer_ui::size_map_view::SizeMapMeasureRequestV1,
    method: &str,
) -> explorer_ui::size_map_view::SizeMapMeasureResultV1 {
    explorer_ui::size_map_view::SizeMapMeasureResultV1 {
        context: request.context,
        item_id: request.item_id,
        exact_bytes: None,
        partial: true,
        error: Some(method.to_owned()),
        tree_nodes: Vec::new(),
    }
}

fn preferred_size_map_scan_method(path: &Path) -> &'static str {
    #[cfg(windows)]
    {
        let text = path.as_os_str().to_string_lossy();
        let bytes = text.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/')
        {
            return "NTFS MFT";
        }
    }
    let _ = path;
    "Breadth-first fallback"
}

fn size_map_terminal_result(
    request: explorer_ui::size_map_view::SizeMapMeasureRequestV1,
    message: impl Into<String>,
) -> explorer_ui::size_map_view::SizeMapMeasureResultV1 {
    explorer_ui::size_map_view::SizeMapMeasureResultV1 {
        context: request.context,
        item_id: request.item_id,
        exact_bytes: None,
        partial: true,
        error: Some(message.into()),
        tree_nodes: Vec::new(),
    }
}

fn enqueue_size_map_requests(
    state: &mut PendingSizeMapWorkV1,
    request_epoch: &AtomicU64,
    requests: Vec<explorer_ui::size_map_view::SizeMapMeasureRequestV1>,
) -> Vec<explorer_ui::size_map_view::SizeMapMeasureResultV1> {
    let Some(context) = requests.first().map(|request| request.context.clone()) else {
        return Vec::new();
    };
    let mut rejected = Vec::new();
    let mut accepted = Vec::new();
    for request in requests {
        if request.context != context {
            rejected.push(size_map_terminal_result(
                request,
                "Size Map request batch mixed contexts; refresh to retry",
            ));
        } else {
            accepted.push(request);
        }
    }
    if state.context.as_ref() != Some(&context) {
        state.context = Some(context);
        state.epoch = request_epoch
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        state.requests.clear();
    }
    for request in accepted {
        if state
            .requests
            .iter()
            .any(|queued| queued.item_id == request.item_id)
        {
            continue;
        }
        if state.requests.len() >= SIZE_MAP_REQUEST_QUEUE_CAP_V1 {
            rejected.push(size_map_terminal_result(
                request,
                "Size Map request queue limit reached; refresh to retry",
            ));
        } else {
            state.requests.push(request);
        }
    }
    rejected
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SizeMapScanTerminalV1 {
    Complete,
    Partial,
    Cancelled,
    Unavailable,
    ResourceLimited,
    Failed,
}

impl SizeMapScanTerminalV1 {
    const fn label(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Cancelled => "cancelled",
            Self::Unavailable => "unavailable",
            Self::ResourceLimited => "resource-limited",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SizeMapScanOutcomeV1 {
    bytes: u64,
    terminal: SizeMapScanTerminalV1,
    diagnostic: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SizeMapTreeScanV1 {
    outcome: SizeMapScanOutcomeV1,
    nodes: Vec<explorer_ui::size_map_view::SizeMapTreeNodeV1>,
}

fn project_shared_snapshot_to_size_map(
    snapshot: &crate::folder_size_service::FolderSnapshotV1,
    root: &Path,
    root_item_id: &explorer_model::ShellItemId,
) -> SizeMapTreeScanV1 {
    use crate::folder_size_service::{SnapshotNodeKindV1, SnapshotStatusV1};

    let mut identities =
        HashMap::from([(snapshot.root_id, (root_item_id.clone(), root.to_path_buf()))]);
    let mut nodes = Vec::new();
    let visible_limit = usize::try_from(SIZE_MAP_VISIBLE_NODE_LIMIT_V1).unwrap_or(usize::MAX);
    for node in snapshot
        .nodes
        .iter()
        .filter(|node| node.id != snapshot.root_id)
    {
        let Some(parent) = node
            .parent
            .and_then(|parent| identities.get(&parent).cloned())
        else {
            continue;
        };
        let path = parent.1.join(&node.name);
        let item_id = size_map_tree_item_id(
            root_item_id,
            path.strip_prefix(root).unwrap_or(&path),
            node.id.0,
        );
        identities.insert(node.id, (item_id.clone(), path.clone()));
        if nodes.len() >= visible_limit {
            continue;
        }
        let is_container = node.kind == SnapshotNodeKindV1::Directory;
        nodes.push(explorer_ui::size_map_view::SizeMapTreeNodeV1 {
            item_id,
            root_item_id: root_item_id.clone(),
            parent_item_id: parent.0,
            location: explorer_model::LocationDescriptor::file_system(&path),
            display_name: node.name.clone(),
            type_name: if is_container { "Folder" } else { "File" }.to_owned(),
            is_container,
            exact_bytes: Some(node.recursive_bytes),
            partial: node.status != SnapshotStatusV1::Complete,
            error: (node.status != SnapshotStatusV1::Complete)
                .then(|| "Folder snapshot is partial".to_owned()),
        });
    }
    let terminal = match snapshot.status {
        SnapshotStatusV1::Complete => SizeMapScanTerminalV1::Complete,
        SnapshotStatusV1::Partial => SizeMapScanTerminalV1::Partial,
        SnapshotStatusV1::Cancelled => SizeMapScanTerminalV1::Cancelled,
        SnapshotStatusV1::Unavailable => SizeMapScanTerminalV1::Unavailable,
        SnapshotStatusV1::ResourceLimited => SizeMapScanTerminalV1::ResourceLimited,
        SnapshotStatusV1::Failed => SizeMapScanTerminalV1::Failed,
    };
    SizeMapTreeScanV1 {
        outcome: SizeMapScanOutcomeV1 {
            bytes: snapshot.aggregate.recursive_bytes,
            terminal,
            diagnostic: snapshot.diagnostic.clone(),
        },
        nodes,
    }
}

fn size_map_tree_item_id(
    root_item_id: &explorer_model::ShellItemId,
    relative: &Path,
    salt: u64,
) -> explorer_model::ShellItemId {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    b"superexplorer:size-map-tree-node:v1".hash(&mut hasher);
    root_item_id.provider_bytes().hash(&mut hasher);
    relative.to_string_lossy().to_lowercase().hash(&mut hasher);
    salt.hash(&mut hasher);
    explorer_model::ShellItemId::from_provider_bytes(hasher.finish().to_le_bytes())
        .unwrap_or_else(|| root_item_id.clone())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, AtomicUsize, Ordering},
        sync::{Arc, Mutex},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use abi_stable::std_types::{ROption, RVec};
    use explorer_common::{ExplorerError, ExplorerErrorKind};
    use explorer_extension_api::{
        IncrementalResultBatchV1, IncrementalResultEntryV1, ItemHandleV1, JobContextV1,
        JobTerminalV1, LockOwnerApplicationTypeV1, LockOwnerQueryStatusV1, LockOwnerRecordV1,
        PluginItemResultV1, PluginValueV1, SinkSubmitStatusV1,
    };

    #[test]
    fn folder_options_controller_is_single_instance_retryable_and_idempotently_closed() {
        let mut lifecycle = super::FolderOptionsControllerLifecycleV1::default();
        assert_eq!(
            lifecycle.begin_open(),
            super::FolderOptionsOpenIntentV1::Create { generation: 1 }
        );
        assert_eq!(
            lifecycle.begin_open(),
            super::FolderOptionsOpenIntentV1::Activate { generation: 1 }
        );

        lifecycle.creation_failed(1);
        assert_eq!(
            lifecycle.begin_open(),
            super::FolderOptionsOpenIntentV1::Create { generation: 2 }
        );
        assert!(lifecycle.creation_succeeded(2));
        assert!(!lifecycle.creation_succeeded(1));
        assert_eq!(
            lifecycle.begin_open(),
            super::FolderOptionsOpenIntentV1::Activate { generation: 2 }
        );

        assert!(lifecycle.close());
        assert!(!lifecycle.close());
        assert_eq!(
            lifecycle.begin_open(),
            super::FolderOptionsOpenIntentV1::Create { generation: 3 }
        );
    }

    #[test]
    fn production_plugin_discovery_accepts_only_sepack_archives() {
        assert!(super::is_sepack_path(Path::new("plugin.sepack")));
        assert!(super::is_sepack_path(Path::new("plugin.SePack")));
        assert!(!super::is_sepack_path(Path::new("plugin.dll")));
    }
    use explorer_extension_host::{
        ExtensionJobAuthorityV1, ExtensionJobRuntimeRequestV1, ExtensionJobRuntimeV1,
        ExtensionJobUiIngressV1, ExtensionResultBufferConfigV1,
    };
    use explorer_model::{
        FileEntry, FileEntryMetadata, Generation, LocationDescriptor, LockOwner,
        LockOwnerApplicationType, LockOwnerDiscoveryTerminal, LockOwnerEligibility,
        LockOwnerIdentity, RequestContext, ShellItemId, TabId, ViewMode,
    };

    #[test]
    fn mft_diagnostics_match_only_the_complete_acknowledged_snapshot() {
        let limits = crate::mft_query::MftCacheBudgetLimitsV1 {
            persisted_index_mb: 1_024,
            volume_index_mb: 512,
            file_data_mb: 256,
            aggregate_mb: 512,
            lru_mb: 2_048,
        };
        let mib = 1024 * 1024;
        let diagnostics = crate::mft_query::MftCacheDiagnosticsV1 {
            limit_bytes: 2_048 * mib,
            persisted_index_limit_bytes: Some(1_024 * mib),
            volume_index_limit_bytes: Some(512 * mib),
            file_data_limit_bytes: Some(256 * mib),
            aggregate_limit_bytes: Some(512 * mib),
            ..Default::default()
        };
        assert!(super::mft_diagnostics_match_limits(&diagnostics, limits));
        let stale = crate::mft_query::MftCacheDiagnosticsV1 {
            limit_bytes: 512 * mib,
            ..diagnostics
        };
        assert!(!super::mft_diagnostics_match_limits(&stale, limits));
    }
    use explorer_ui::ExtensionUiPumpPortV1 as _;

    use super::preferred_size_map_scan_method;

    use super::{
        ApplicationExtensionReadyProjectorV1, ApplicationExtensionUiPumpV1,
        BatchDetailsColumnModeV1, CodeLinesCachedValueV1, HostExtensionColumnCacheV1,
        LockOwnerCacheKeyV1, PendingFolderSizeWorkV1, PendingSizeMapWorkV1,
        SIZE_MAP_REQUEST_QUEUE_CAP_V1, SafeModeIncidentOfferV1, SafeModeIncidentPortV1,
        SizeMapProjectionV1, aggregate_lock_owner_batch, batch_details_cache_admission,
        cancel_folder_size_context, cell_render_key, compose_lock_owner_terminals,
        confirm_offered_safe_mode_incident_v1, confirm_presented_safe_mode_incident_v1,
        emit_post_commit_safe_mode_telemetry_v1, enqueue_folder_size_requests,
        enqueue_size_map_requests, is_code_lines_directory_row, lock_owner_cache_lookup,
        lock_owner_cache_store, partition_batch_details_cache_hits,
        partition_code_lines_cache_hits, prepare_code_lines_batch_inputs, project_size_map_plan,
        read_code_lines_file_bounded, read_code_lines_path_bounded, should_restore_saved_tabs,
        size_map_node_id, size_map_render_key, take_folder_size_batch,
    };

    struct FakeSafeModePortV1 {
        denied: bool,
        confirmed: Mutex<Vec<u8>>,
    }

    fn test_lock_owner(process_id: u32, creation_time_100ns: u64, name: &str) -> LockOwner {
        LockOwner {
            identity: LockOwnerIdentity {
                process_id,
                creation_time_100ns,
            },
            display_name: name.to_owned(),
            application_type: LockOwnerApplicationType::Console,
            restartable: false,
            eligibility: LockOwnerEligibility::Protected,
        }
    }

    fn test_lock_owner_error(operation: &str) -> ExplorerError {
        ExplorerError::new(
            ExplorerErrorKind::Availability,
            operation,
            true,
            "lock owner unavailable",
            "test terminal",
        )
    }

    #[test]
    fn lock_owner_composition_merges_deduplicates_and_orders_sources() {
        let result = compose_lock_owner_terminals(
            LockOwnerDiscoveryTerminal::Ready(vec![test_lock_owner(
                42,
                100,
                "restart-manager.exe",
            )]),
            LockOwnerDiscoveryTerminal::Ready(vec![
                test_lock_owner(42, 100, "cmd.exe"),
                test_lock_owner(7, 200, "cmd.exe"),
            ]),
        );

        let LockOwnerDiscoveryTerminal::Ready(owners) = result else {
            panic!("owners from either discovery source must produce READY");
        };
        assert_eq!(
            owners
                .iter()
                .map(|owner| owner.identity.process_id)
                .collect::<Vec<_>>(),
            vec![7, 42]
        );
        assert_eq!(owners[1].display_name, "restart-manager.exe");
    }

    #[test]
    fn lock_owner_composition_applies_global_and_ownerless_terminal_precedence() {
        assert!(matches!(
            compose_lock_owner_terminals(
                LockOwnerDiscoveryTerminal::Ready(vec![test_lock_owner(1, 1, "cmd.exe")]),
                LockOwnerDiscoveryTerminal::Cancelled,
            ),
            LockOwnerDiscoveryTerminal::Cancelled
        ));
        assert!(matches!(
            compose_lock_owner_terminals(
                LockOwnerDiscoveryTerminal::DeadlineElapsed,
                LockOwnerDiscoveryTerminal::Failed(test_lock_owner_error("failed")),
            ),
            LockOwnerDiscoveryTerminal::DeadlineElapsed
        ));
        assert!(matches!(
            compose_lock_owner_terminals(
                LockOwnerDiscoveryTerminal::Unavailable(test_lock_owner_error("unavailable")),
                LockOwnerDiscoveryTerminal::Failed(test_lock_owner_error("failed")),
            ),
            LockOwnerDiscoveryTerminal::Failed(_)
        ));
    }

    #[test]
    fn lock_owner_composition_covers_the_frozen_two_source_truth_table() {
        use LockOwnerDiscoveryTerminal as Terminal;

        fn terminal_kind(value: Terminal) -> &'static str {
            match value {
                Terminal::Ready(_) => "ready",
                Terminal::Empty => "empty",
                Terminal::Cancelled => "cancelled",
                Terminal::DeadlineElapsed => "deadline",
                Terminal::Unavailable(_) => "unavailable",
                Terminal::Failed(_) => "host-error",
            }
        }

        let owner = || Terminal::Ready(vec![test_lock_owner(9, 1, "cmd.exe")]);
        let unavailable = || Terminal::Unavailable(test_lock_owner_error("unavailable"));
        let failed = || Terminal::Failed(test_lock_owner_error("failed"));
        let cases = [
            (owner(), Terminal::Cancelled, "cancelled"),
            (Terminal::Cancelled, owner(), "cancelled"),
            (owner(), Terminal::DeadlineElapsed, "deadline"),
            (Terminal::DeadlineElapsed, owner(), "deadline"),
            (owner(), owner(), "ready"),
            (owner(), Terminal::Empty, "ready"),
            (Terminal::Empty, owner(), "ready"),
            (owner(), unavailable(), "ready"),
            (unavailable(), owner(), "ready"),
            (owner(), failed(), "ready"),
            (failed(), owner(), "ready"),
            (failed(), Terminal::Empty, "host-error"),
            (Terminal::Empty, failed(), "host-error"),
            (unavailable(), Terminal::Empty, "unavailable"),
            (Terminal::Empty, unavailable(), "unavailable"),
            (Terminal::Empty, Terminal::Empty, "empty"),
        ];
        for (restart_manager, current_directory, expected) in cases {
            assert_eq!(
                terminal_kind(compose_lock_owner_terminals(
                    restart_manager,
                    current_directory
                )),
                expected
            );
        }
    }

    #[test]
    fn lock_owner_composition_is_input_order_independent_and_truncates_after_sorting() {
        let maximum = explorer_common::RoadmapLimits::default().lock_recovery_max_owners;
        let mut ascending = (0..(maximum + 17))
            .map(|index| test_lock_owner(u32::try_from(index + 1).unwrap(), 1, "cmd.exe"))
            .collect::<Vec<_>>();
        let mut descending = ascending.clone();
        descending.reverse();

        let LockOwnerDiscoveryTerminal::Ready(first) = compose_lock_owner_terminals(
            LockOwnerDiscoveryTerminal::Ready(ascending.clone()),
            LockOwnerDiscoveryTerminal::Empty,
        ) else {
            panic!("ordered source must produce owners");
        };
        let LockOwnerDiscoveryTerminal::Ready(second) = compose_lock_owner_terminals(
            LockOwnerDiscoveryTerminal::Ready(descending),
            LockOwnerDiscoveryTerminal::Empty,
        ) else {
            panic!("reversed source must produce owners");
        };
        ascending.truncate(maximum);
        assert_eq!(first, ascending);
        assert_eq!(second, ascending);
        assert_eq!(first.len(), maximum);
    }

    #[test]
    fn lock_owner_batch_preserves_owners_while_reporting_worst_ownerless_status() {
        let item = ItemHandleV1::from_host([9; 16], 1);
        let owner = LockOwnerRecordV1 {
            item,
            process_id: 91,
            application_type: LockOwnerApplicationTypeV1::MAIN_WINDOW,
            display_name: "cmd.exe".into(),
            service_name: "".into(),
        };

        let (status, owners) = aggregate_lock_owner_batch(vec![
            Some((LockOwnerQueryStatusV1::READY, vec![owner.clone()])),
            Some((LockOwnerQueryStatusV1::UNAVAILABLE, Vec::new())),
            Some((LockOwnerQueryStatusV1::HOST_ERROR, Vec::new())),
        ]);
        assert_eq!(status, LockOwnerQueryStatusV1::HOST_ERROR);
        assert_eq!(owners.len(), 1);
        assert_eq!(owners[0].process_id, owner.process_id);

        let (status, owners) = aggregate_lock_owner_batch(vec![
            Some((LockOwnerQueryStatusV1::READY, vec![owner])),
            Some((LockOwnerQueryStatusV1::CANCELLED, Vec::new())),
        ]);
        assert_eq!(status, LockOwnerQueryStatusV1::CANCELLED);
        assert!(owners.is_empty());
    }

    #[test]
    fn lock_owner_bypasses_the_durable_code_lines_value_cache() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("superexplorer-lock-cache-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("occupied");
        fs::create_dir(&path).unwrap();
        let cache = Mutex::new(HostExtensionColumnCacheV1::default());

        let admission = cache.lock().unwrap().admission(&path).unwrap();
        cache.lock().unwrap().insert(
            admission,
            CodeLinesCachedValueV1 {
                value: Some(explorer_ui::code_lines_column::CodeLinesValueV1 {
                    language: String::new(),
                    code: 0,
                    comments: 0,
                    blanks: 0,
                    total: 0,
                }),
                error: None,
            },
        );
        let request = explorer_ui::code_lines_column::CodeLinesRequestV1 {
            context: RequestContext::new(TabId::new(), Generation::new(1)),
            item_id: ShellItemId::from_provider_bytes(b"occupied-folder".to_vec()).unwrap(),
            path: path.clone(),
        };

        assert!(
            batch_details_cache_admission(&cache, BatchDetailsColumnModeV1::CodeLines, &path)
                .is_some()
        );
        assert!(
            batch_details_cache_admission(&cache, BatchDetailsColumnModeV1::LockOwner, &path)
                .is_none(),
            "dynamic owner state must never be intercepted by the durable code-lines cache"
        );
        let (hits, misses) = partition_batch_details_cache_hits(
            &cache,
            BatchDetailsColumnModeV1::LockOwner,
            vec![request],
        );
        assert!(hits.is_empty());
        assert_eq!(
            misses.len(),
            1,
            "the provider must run even after a durable blank was stored"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn host_data_column_cache_retains_only_three_levels_below_active_folder() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("superexplorer-three-level-cache-{nonce}"));
        let a = root.join("a");
        let b1 = a.join("b1");
        let b2 = a.join("b2");
        let c1 = b1.join("c1");
        let c2 = b2.join("c2");
        let d1 = c1.join("d1");
        let outside = root.with_extension("outside");
        for directory in [&c1, &c2, &d1, &outside] {
            fs::create_dir_all(directory).unwrap();
        }

        let mut cache = HostExtensionColumnCacheV1::<u64>::default();
        for (index, directory) in [&a, &b1, &b2, &c1, &c2, &d1, &outside]
            .into_iter()
            .enumerate()
        {
            let admission = cache.admission(directory).unwrap();
            assert!(cache.insert(admission, index as u64));
        }

        cache.retain_window(&root, 3);
        let retained = cache
            .values
            .keys()
            .map(|key| key.canonical_path.clone())
            .collect::<HashSet<_>>();
        for expected in [&a, &b1, &b2, &c1, &c2] {
            assert!(retained.contains(&expected.canonicalize().unwrap()));
        }
        assert!(!retained.contains(&d1.canonicalize().unwrap()));
        assert!(!retained.contains(&outside.canonicalize().unwrap()));
        assert_eq!(retained.len(), 5);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn host_extension_column_cache_reuses_same_mtime_and_rejects_changed_mtime() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("superexplorer-column-cache-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("value.rs");
        fs::write(&source, "fn main() {}\n").unwrap();
        let file = fs::OpenOptions::new().write(true).open(&source).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(10)))
            .unwrap();

        let mut cache = HostExtensionColumnCacheV1::<u64>::default();
        let original = cache.admission(&source).expect("host metadata admission");
        assert!(cache.insert(original.clone(), 41));
        assert_eq!(cache.get(&original), Some(41));
        assert_eq!(cache.telemetry.entries.load(Ordering::Acquire), 1);
        assert!(cache.telemetry.bytes.load(Ordering::Acquire) > 0);

        let code_cache = Mutex::new(HostExtensionColumnCacheV1::default());
        let code_value = explorer_ui::code_lines_column::CodeLinesValueV1 {
            language: "Rust".to_owned(),
            code: 1,
            comments: 0,
            blanks: 0,
            total: 1,
        };
        let code_admission = code_cache.lock().unwrap().admission(&source).unwrap();
        code_cache.lock().unwrap().insert(
            code_admission,
            CodeLinesCachedValueV1 {
                value: Some(code_value.clone()),
                error: None,
            },
        );
        let code_request = explorer_ui::code_lines_column::CodeLinesRequestV1 {
            context: RequestContext::new(TabId::new(), Generation::new(1)),
            item_id: ShellItemId::from_provider_bytes(b"cached-code".to_vec()).unwrap(),
            path: source.clone(),
        };
        let (hits, misses) =
            partition_code_lines_cache_hits(&code_cache, vec![code_request.clone()]);
        assert_eq!(
            hits.first().and_then(|hit| hit.value.as_ref()),
            Some(&code_value)
        );
        assert!(
            misses.is_empty(),
            "Rust/Lua cache hits must bypass provider dispatch"
        );

        file.set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(20)))
            .unwrap();
        let changed = cache
            .admission(&source)
            .expect("changed metadata admission");
        assert_ne!(changed.key, original.key);
        assert_eq!(cache.get(&changed), None);
        let (hits, misses) = partition_code_lines_cache_hits(&code_cache, vec![code_request]);
        assert!(hits.is_empty());
        assert_eq!(
            misses.len(),
            1,
            "changed mtime must invoke Rust/Lua providers"
        );

        let other_root = root.join("other");
        fs::create_dir(&other_root).unwrap();
        let other = other_root.join("other.rs");
        fs::write(&other, "fn other() {}\n").unwrap();
        let current = cache.admission(&source).unwrap();
        let other_admission = cache.admission(&other).unwrap();
        assert!(cache.insert(current.clone(), 7));
        assert!(cache.insert(other_admission.clone(), 9));
        cache.invalidate_directory(&root);
        assert_eq!(
            cache.get(&current),
            None,
            "F5 invalidates current directory"
        );
        assert_eq!(
            cache.get(&other_admission),
            Some(9),
            "other directories survive F5"
        );
        assert!(
            !cache.insert(current, 11),
            "pre-F5 work cannot repopulate the scope"
        );
        let refreshed = cache.admission(&source).unwrap();
        assert!(cache.insert(refreshed.clone(), 12));
        assert_eq!(cache.get(&refreshed), Some(12));

        drop(file);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn lock_owner_cache_is_generation_metadata_and_ttl_scoped() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let key = LockOwnerCacheKeyV1 {
            canonical_path: PathBuf::from(format!(r"C:\lock-owner-cache-{nonce}")),
            source_size: 7,
            modified_seconds: 11,
            modified_nanos: 13,
        };
        let now = Instant::now();
        lock_owner_cache_store(
            key.clone(),
            17,
            LockOwnerQueryStatusV1::EMPTY,
            Vec::new(),
            now,
        );
        assert_eq!(
            lock_owner_cache_lookup(&key, 17, now).map(|value| value.0),
            Some(LockOwnerQueryStatusV1::EMPTY)
        );
        assert!(lock_owner_cache_lookup(&key, 18, now).is_none());
        assert!(lock_owner_cache_lookup(&key, 17, now + Duration::from_secs(3)).is_none());

        let unavailable = LockOwnerCacheKeyV1 {
            canonical_path: PathBuf::from(format!(r"C:\lock-owner-unavailable-{nonce}")),
            ..key
        };
        lock_owner_cache_store(
            unavailable.clone(),
            17,
            LockOwnerQueryStatusV1::UNAVAILABLE,
            Vec::new(),
            now,
        );
        assert!(lock_owner_cache_lookup(&unavailable, 17, now).is_none());
    }

    #[test]
    fn lock_owner_f5_clears_occupied_state_and_rejects_delayed_pre_refresh_result() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let key = LockOwnerCacheKeyV1 {
            canonical_path: PathBuf::from(format!(r"C:\lock-owner-f5-{nonce}")),
            source_size: 0,
            modified_seconds: 0,
            modified_nanos: 0,
        };
        let item = ItemHandleV1::from_host([7; 16], 1);
        let owner = LockOwnerRecordV1 {
            item,
            process_id: 77,
            application_type: LockOwnerApplicationTypeV1::MAIN_WINDOW,
            display_name: "cmd.exe".into(),
            service_name: "".into(),
        };
        let now = Instant::now();
        lock_owner_cache_store(
            key.clone(),
            40,
            LockOwnerQueryStatusV1::READY,
            vec![owner.clone()],
            now,
        );
        assert!(lock_owner_cache_lookup(&key, 40, now).is_some());

        let refreshed_generation = 41;
        assert!(
            lock_owner_cache_lookup(&key, refreshed_generation, now).is_none(),
            "F5 generation must immediately hide the occupied pre-refresh value"
        );
        lock_owner_cache_store(
            key.clone(),
            40,
            LockOwnerQueryStatusV1::READY,
            vec![owner],
            now,
        );
        assert!(
            lock_owner_cache_lookup(&key, refreshed_generation, now).is_none(),
            "a delayed pre-F5 source result cannot repopulate the current generation"
        );
        lock_owner_cache_store(
            key.clone(),
            refreshed_generation,
            LockOwnerQueryStatusV1::EMPTY,
            Vec::new(),
            now,
        );
        assert_eq!(
            lock_owner_cache_lookup(&key, refreshed_generation, now).map(|value| value.0),
            Some(LockOwnerQueryStatusV1::EMPTY),
            "fresh discovery after exit or subtree departure clears the cell"
        );
    }

    #[test]
    fn lock_owner_scope_generations_reject_delayed_refresh_folder_and_tab_results() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let key = LockOwnerCacheKeyV1 {
            canonical_path: PathBuf::from(format!(r"C:\lock-owner-scopes-{nonce}")),
            source_size: 0,
            modified_seconds: 0,
            modified_nanos: 0,
        };
        let now = Instant::now();
        for (transition, stale, current) in [
            ("rapid F5", 50, 51),
            ("folder change", 51, 52),
            ("tab change", 52, 53),
        ] {
            lock_owner_cache_store(
                key.clone(),
                stale,
                LockOwnerQueryStatusV1::EMPTY,
                Vec::new(),
                now,
            );
            assert!(
                lock_owner_cache_lookup(&key, current, now).is_none(),
                "{transition} must reject its older source result"
            );
        }
    }

    #[test]
    fn cell_render_key_covers_every_public_immutable_render_input() {
        let color = explorer_extension_ui_api::CellColorV1::rgba(1, 2, 3, 255);
        let context = explorer_extension_ui_api::CellRenderContextV1 {
            value: ROption::RSome(PluginValueV1::integer(7)),
            exact_bytes: ROption::RSome(7),
            aggregate: ROption::RNone,
            loading: false,
            error: ROption::RNone,
            selected: false,
            hovered: false,
            dpi_milli: 1_000,
            theme: explorer_extension_ui_api::CellThemeV1 {
                foreground: color,
                muted_foreground: color,
                background: color,
                selection_background: color,
                accent: color,
            },
            settings: "default".into(),
            item_id: explorer_extension_ui_api::StableIdV1::new(
                explorer_extension_ui_api::EXTENSION_ID_NAMESPACE_V1,
                1,
            ),
            render_generation: 1,
            request_generation: 1,
        };
        let baseline = cell_render_key(&context);
        let changed_request = explorer_extension_ui_api::CellRenderContextV1 {
            request_generation: 2,
            ..context.clone()
        };
        let changed_theme = explorer_extension_ui_api::CellRenderContextV1 {
            theme: explorer_extension_ui_api::CellThemeV1 {
                accent: explorer_extension_ui_api::CellColorV1::rgba(9, 2, 3, 255),
                ..context.theme
            },
            ..context.clone()
        };
        let changed_value = explorer_extension_ui_api::CellRenderContextV1 {
            value: ROption::RSome(PluginValueV1::integer(8)),
            ..context.clone()
        };
        let variants = [
            changed_request,
            changed_theme,
            changed_value,
            explorer_extension_ui_api::CellRenderContextV1 {
                exact_bytes: ROption::RSome(8),
                ..context.clone()
            },
            explorer_extension_ui_api::CellRenderContextV1 {
                aggregate: ROption::RSome(explorer_extension_ui_api::CellAggregateV1 {
                    largest_sibling_value: ROption::RSome(PluginValueV1::integer(8)),
                    largest_sibling_bytes: ROption::RSome(8),
                }),
                ..context.clone()
            },
            explorer_extension_ui_api::CellRenderContextV1 {
                loading: true,
                ..context.clone()
            },
            explorer_extension_ui_api::CellRenderContextV1 {
                error: ROption::RSome("failed".into()),
                ..context.clone()
            },
            explorer_extension_ui_api::CellRenderContextV1 {
                selected: true,
                ..context.clone()
            },
            explorer_extension_ui_api::CellRenderContextV1 {
                hovered: true,
                ..context.clone()
            },
            explorer_extension_ui_api::CellRenderContextV1 {
                dpi_milli: 1_250,
                ..context.clone()
            },
            explorer_extension_ui_api::CellRenderContextV1 {
                settings: "text-only".into(),
                ..context.clone()
            },
            explorer_extension_ui_api::CellRenderContextV1 {
                item_id: explorer_extension_ui_api::StableIdV1::new(
                    explorer_extension_ui_api::EXTENSION_ID_NAMESPACE_V1,
                    2,
                ),
                ..context.clone()
            },
            explorer_extension_ui_api::CellRenderContextV1 {
                render_generation: 2,
                ..context
            },
        ];
        for variant in variants {
            assert_ne!(baseline, cell_render_key(&variant));
        }
    }

    #[test]
    fn oversized_code_lines_source_is_unsupported_not_an_error_or_zero() {
        let path = std::env::temp_dir().join(format!(
            "superexplorer-code-lines-oversized-{}-{}.txt",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &path,
            vec![b'x'; explorer_extension_host::MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1 + 1],
        )
        .unwrap();
        assert!(matches!(read_code_lines_file_bounded(&path), Ok(None)));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn directory_rows_are_recognized_before_admitted_recursive_measurement() {
        let root = std::env::temp_dir().join(format!(
            "superexplorer-code-lines-visible-directory-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("large-project/src")).unwrap();
        fs::write(root.join("large-project/src/main.rs"), b"fn main() {}\n").unwrap();

        assert!(is_code_lines_directory_row(&root.join("large-project")));
        assert!(!is_code_lines_directory_row(
            &root.join("large-project/src/main.rs")
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn code_lines_directory_source_is_bounded_and_contains_recursive_file_names() {
        let root = std::env::temp_dir().join(format!(
            "superexplorer-code-lines-directory-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), b"fn main() {}\n").unwrap();
        fs::write(root.join("script.py"), b"print('ok')\n").unwrap();
        let packed = read_code_lines_path_bounded(&root).unwrap().unwrap();
        assert!(packed.starts_with(b"SECLDIR1"));
        assert!(
            packed
                .windows("main.rs".len())
                .any(|bytes| bytes == b"main.rs")
        );
        assert!(
            packed
                .windows("script.py".len())
                .any(|bytes| bytes == b"script.py")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn code_lines_directory_pack_ignores_large_binary_payloads_before_snapshotting() {
        let root = std::env::temp_dir().join(format!(
            "superexplorer-code-lines-source-filter-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".git/objects")).unwrap();
        fs::write(root.join("main.rs"), b"fn main() {}\n").unwrap();
        fs::write(
            root.join(".git/objects/one.bin"),
            vec![0_u8; 7 * 1024 * 1024],
        )
        .unwrap();
        fs::write(
            root.join(".git/objects/two.bin"),
            vec![0_u8; 7 * 1024 * 1024],
        )
        .unwrap();

        let packed = read_code_lines_path_bounded(&root).unwrap().unwrap();
        assert!(packed.starts_with(b"SECLDIR1"));
        assert!(packed.len() < 1024);
        assert!(packed.windows(7).any(|bytes| bytes == b"main.rs"));
        assert!(!packed.windows(7).any(|bytes| bytes == b"one.bin"));
        assert!(
            explorer_extension_host::HostInputStreamSourceV1::from_host_snapshot(packed, 1, true)
                .is_some()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn code_lines_directory_pack_rejects_empty_and_single_stream_overflow() {
        let root = std::env::temp_dir().join(format!(
            "superexplorer-code-lines-source-bound-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("binary.dat"), b"not source").unwrap();
        assert_eq!(read_code_lines_path_bounded(&root).unwrap(), None);

        fs::write(
            root.join("maximum.rs"),
            vec![b'x'; explorer_extension_host::MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1],
        )
        .unwrap();
        assert_eq!(
            read_code_lines_path_bounded(&root).unwrap(),
            None,
            "record framing must not create a source larger than the stream contract"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn code_lines_batch_preparation_isolates_an_invalid_row() {
        let root = std::env::temp_dir().join(format!(
            "superexplorer-code-lines-batch-isolation-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let valid_path = root.join("valid.rs");
        fs::write(&valid_path, b"fn valid() {}\n").unwrap();
        let context = RequestContext::new(TabId::new(), Generation::new(7));
        let make_request =
            |id: &[u8], path: PathBuf| explorer_ui::code_lines_column::CodeLinesRequestV1 {
                context: context.clone(),
                item_id: ShellItemId::from_provider_bytes(id.to_vec()).unwrap(),
                path,
            };
        let requests = vec![
            (
                make_request(b"valid", valid_path),
                b"fn valid() {}\n".to_vec(),
                None,
            ),
            (
                make_request(b"missing", root.join("missing.rs")),
                b"fn missing() {}\n".to_vec(),
                None,
            ),
        ];

        let (dispatchable, inputs, rejected) =
            prepare_code_lines_batch_inputs(requests, 7, BatchDetailsColumnModeV1::CodeLines);
        assert_eq!(dispatchable.len(), 1);
        assert_eq!(inputs.len(), 1);
        assert_eq!(rejected.len(), 1);
        assert_eq!(dispatchable[0].0.item_id.provider_bytes(), b"valid");
        assert_eq!(rejected[0].0.item_id.provider_bytes(), b"missing");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn code_lines_real_folder_snapshots_respect_stream_contract() {
        let Some(root) =
            std::env::var_os("SUPEREXPLORER_CODE_LINES_REAL_FOLDER").map(PathBuf::from)
        else {
            return;
        };
        let mut directories = fs::read_dir(&root)
            .expect("real-folder diagnostic root")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir()))
            .collect::<Vec<_>>();
        directories.sort();
        assert!(
            !directories.is_empty(),
            "diagnostic root has no child directories"
        );

        for directory in directories {
            match read_code_lines_path_bounded(&directory) {
                Ok(Some(snapshot)) => {
                    assert!(
                        snapshot.len()
                            <= explorer_extension_host::MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1,
                        "accepted snapshot exceeded stream contract: {}",
                        directory.display()
                    );
                    assert!(
                        explorer_extension_host::HostInputStreamSourceV1::from_host_snapshot(
                            snapshot, 1, true
                        )
                        .is_some(),
                        "accepted snapshot could not prepare: {}",
                        directory.display()
                    );
                    eprintln!("dispatchable: {}", directory.display());
                }
                Ok(None) => eprintln!("unsupported: {}", directory.display()),
                Err(error) => eprintln!("unavailable: {} ({error})", directory.display()),
            }
        }
    }

    #[test]
    fn size_map_render_key_covers_measurements_viewport_and_selection() {
        let color = explorer_extension_ui_api::CellColorV1::rgba(1, 2, 3, 255);
        let mut context = explorer_extension_ui_api::SizeMapRenderContextV1 {
            snapshot: explorer_extension_ui_api::ViewSnapshotIdentityV1 {
                location_generation: 1,
                refresh_generation: 1,
                render_revision: 1,
            },
            nodes: RVec::from(vec![explorer_extension_ui_api::SizeMapNodeV1 {
                node_id: explorer_extension_ui_api::StableIdV1::new(
                    explorer_extension_ui_api::EXTENSION_ID_NAMESPACE_V1,
                    1,
                ),
                parent_id: ROption::RNone,
                name: "one".into(),
                kind: explorer_extension_ui_api::SizeMapNodeKindV1::FILE,
                exact_bytes: ROption::RSome(10),
                status: explorer_extension_ui_api::SizeMapNodeStatusV1::COMPLETE,
            }]),
            viewport: explorer_extension_ui_api::SizeMapViewportV1 {
                width_milli: 1_000,
                height_milli: 1_000,
                dpi_milli: 1_000,
            },
            theme: explorer_extension_ui_api::CellThemeV1 {
                foreground: color,
                muted_foreground: color,
                background: color,
                selection_background: color,
                accent: color,
            },
            selected_node_ids: RVec::new(),
            settings: "default".into(),
        };
        let request_context = RequestContext::new(TabId::new(), Generation::new(1));
        let item_ids = vec![ShellItemId::from_provider_bytes([1_u8]).unwrap()];
        let baseline = size_map_render_key(&mut context, &request_context, &item_ids, 1);
        let mut changed_viewport = context.clone();
        changed_viewport.viewport.width_milli = 1_001;
        let mut changed_selection = context.clone();
        changed_selection.selected_node_ids =
            RVec::from(vec![explorer_extension_ui_api::StableIdV1::new(
                explorer_extension_ui_api::EXTENSION_ID_NAMESPACE_V1,
                1,
            )]);
        let mut changed_measurement = context;
        changed_measurement.nodes[0].exact_bytes = ROption::RSome(11);
        assert_ne!(
            baseline,
            size_map_render_key(&mut changed_viewport, &request_context, &item_ids, 1)
        );
        assert_ne!(
            baseline,
            size_map_render_key(&mut changed_selection, &request_context, &item_ids, 1)
        );
        assert_ne!(
            baseline,
            size_map_render_key(&mut changed_measurement, &request_context, &item_ids, 1)
        );
    }

    #[test]
    fn size_map_projection_retains_aggregated_items_for_search_and_uia() {
        let node_id = explorer_extension_ui_api::StableIdV1::new(
            explorer_extension_ui_api::EXTENSION_ID_NAMESPACE_V1,
            900,
        );
        let item_id = ShellItemId::from_provider_bytes(b"tiny-item".to_vec()).unwrap();
        let plan = explorer_extension_ui_api::SizeMapRenderPlanV1 {
            snapshot: explorer_extension_ui_api::ViewSnapshotIdentityV1 {
                location_generation: 1,
                refresh_generation: 1,
                render_revision: 2,
            },
            rectangles: RVec::from(vec![explorer_extension_ui_api::SizeMapRectangleV1 {
                node_id,
                x_millionths: 0,
                y_millionths: 0,
                width_millionths: 1_000_000,
                height_millionths: 1_000_000,
                color: explorer_extension_ui_api::CellColorV1::rgba(1, 2, 3, 255),
                label: "Other (1 item)".into(),
                detail: "10 bytes".into(),
            }]),
            status: "Exact sizes".into(),
        };
        let projected = project_size_map_plan(
            plan,
            HashMap::from([(
                node_id,
                SizeMapProjectionV1::Aggregate(vec![
                    explorer_ui::size_map_view::SizeMapAggregateItemV1 {
                        item_id: item_id.clone(),
                        label: "tiny.rs".to_owned(),
                        detail: "10 bytes".to_owned(),
                    },
                ]),
            )]),
            800.0,
            600.0,
        );
        assert_eq!(projected.rectangles.len(), 1);
        assert_eq!(projected.rectangles[0].item_id, None);
        assert_eq!(projected.rectangles[0].aggregate_items.len(), 1);
        assert_eq!(projected.rectangles[0].aggregate_items[0].item_id, item_id);
        assert_eq!(projected.rectangles[0].aggregate_items[0].label, "tiny.rs");
    }

    #[test]
    fn size_map_render_key_rejects_cross_tab_cache_reuse_and_row_identity_reminting() {
        let color = explorer_extension_ui_api::CellColorV1::rgba(1, 2, 3, 255);
        let item_ids = vec![ShellItemId::from_provider_bytes([7_u8]).unwrap()];
        let mut first = explorer_extension_ui_api::SizeMapRenderContextV1 {
            snapshot: explorer_extension_ui_api::ViewSnapshotIdentityV1 {
                location_generation: 1,
                refresh_generation: 1,
                render_revision: 1,
            },
            nodes: RVec::from(vec![explorer_extension_ui_api::SizeMapNodeV1 {
                node_id: size_map_node_id(&item_ids[0]),
                parent_id: ROption::RNone,
                name: "same-row".into(),
                kind: explorer_extension_ui_api::SizeMapNodeKindV1::FILE,
                exact_bytes: ROption::RSome(10),
                status: explorer_extension_ui_api::SizeMapNodeStatusV1::COMPLETE,
            }]),
            viewport: explorer_extension_ui_api::SizeMapViewportV1 {
                width_milli: 1_000,
                height_milli: 1_000,
                dpi_milli: 1_000,
            },
            theme: explorer_extension_ui_api::CellThemeV1 {
                foreground: color,
                muted_foreground: color,
                background: color,
                selection_background: color,
                accent: color,
            },
            selected_node_ids: RVec::new(),
            settings: "default".into(),
        };
        let second_tab = TabId::new();
        let first_context = RequestContext::new(TabId::new(), Generation::new(1));
        let second_context = RequestContext::new(second_tab, Generation::new(1));
        let first_key = size_map_render_key(&mut first, &first_context, &item_ids, 1);
        let first_revision = first.snapshot.render_revision;
        let mut second = first.clone();
        let second_key = size_map_render_key(&mut second, &second_context, &item_ids, 1);

        let mut updated_package = first.clone();
        let updated_package_key =
            size_map_render_key(&mut updated_package, &first_context, &item_ids, 2);

        assert_ne!(first_key, second_key);
        assert_ne!(first_revision, second.snapshot.render_revision);
        assert_ne!(
            first_key, updated_package_key,
            "a new package incarnation must not reuse the old renderer cache"
        );
        assert_ne!(
            first_revision, updated_package.snapshot.render_revision,
            "a package update must mint a new host render revision"
        );
        assert_ne!(
            size_map_node_id(&item_ids[0]),
            size_map_node_id(&ShellItemId::from_provider_bytes([8_u8]).unwrap()),
            "public node identities must derive from the actual Shell item, not row 0"
        );
    }

    impl SafeModeIncidentPortV1 for FakeSafeModePortV1 {
        type IncidentId = u8;
        type Error = ();

        fn offers(&self) -> Vec<SafeModeIncidentOfferV1<Self::IncidentId>> {
            Vec::new()
        }

        fn denies_native_callbacks(&self) -> bool {
            self.denied
        }

        fn confirm(&self, incident_id: Self::IncidentId) -> Result<(), Self::Error> {
            self.confirmed.lock().unwrap().push(incident_id);
            Ok(())
        }
    }

    struct CountingProjectorV1 {
        calls: Arc<AtomicUsize>,
        fail: bool,
    }

    impl ApplicationExtensionReadyProjectorV1 for CountingProjectorV1 {
        fn project_ready(
            &mut self,
            _pump: &mut explorer_extension_host::ExtensionJobUiPumpV1,
            _runtime: &Arc<ExtensionJobRuntimeV1>,
            _ingress: &ExtensionJobUiIngressV1,
        ) -> Result<usize, explorer_extension_host::ExtensionJobUiPumpErrorV1> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(explorer_extension_host::ExtensionJobUiPumpErrorV1::WrongUiThread);
            }
            Ok(0)
        }
    }

    struct ApplyingProjectorV1 {
        calls: Arc<AtomicUsize>,
    }

    impl ApplicationExtensionReadyProjectorV1 for ApplyingProjectorV1 {
        fn project_ready(
            &mut self,
            pump: &mut explorer_extension_host::ExtensionJobUiPumpV1,
            runtime: &Arc<ExtensionJobRuntimeV1>,
            ingress: &ExtensionJobUiIngressV1,
        ) -> Result<usize, explorer_extension_host::ExtensionJobUiPumpErrorV1> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let ready = pump.take_ready(16)?;
            let mut applied = 0;
            for signal in ready.signals {
                let (item, location, source) = signal.generations();
                for batch in runtime.drain(signal.job(), item, location, source, 16) {
                    if runtime
                        .apply_accepted_batch(&batch, |_| ("fixture-item".to_owned(), 1))
                        .is_some()
                    {
                        ingress.notify_applied(&batch);
                        applied += 1;
                    }
                }
            }
            Ok(applied)
        }
    }

    fn runtime() -> Arc<ExtensionJobRuntimeV1> {
        Arc::new(ExtensionJobRuntimeV1::new(
            ExtensionResultBufferConfigV1::try_new(4, 4, 16, 16, 16, 16, 16, 16, 4096, 4096, 4096)
                .unwrap(),
        ))
    }

    fn request() -> ExtensionJobRuntimeRequestV1 {
        ExtensionJobRuntimeRequestV1 {
            authority: ExtensionJobAuthorityV1::for_integration_test("app-fixture"),
            job_generation: 1,
            item_generation: 1,
            location_generation: 1,
            source_generation: 1,
            has_item: true,
            input_stream: None,
        }
    }

    fn batch(context: &JobContextV1) -> IncrementalResultBatchV1 {
        IncrementalResultBatchV1 {
            job: context.job,
            sink_capability: context.sink.capability,
            job_generation: context.job_generation,
            location: context.location,
            location_generation: context.location_generation,
            source_generation: context.source_generation,
            sequence: 0,
            entries: RVec::from(vec![IncrementalResultEntryV1 {
                item: context.item.into_option().unwrap(),
                item_generation: context.item_generation,
                source_generation: context.source_generation,
                result: PluginItemResultV1::value(
                    PluginValueV1::text("fixture").unwrap(),
                    ROption::RNone,
                ),
            }]),
        }
    }

    fn queued_fixture() -> (
        Arc<ExtensionJobRuntimeV1>,
        ExtensionJobUiIngressV1,
        explorer_extension_host::ExtensionJobUiInboxV1,
        JobContextV1,
    ) {
        let runtime = runtime();
        let (ingress, inbox) = ExtensionJobUiIngressV1::new_integration_pair(Arc::clone(&runtime));
        let context = runtime.open_job_for_integration_test(request()).unwrap();
        assert_eq!(
            runtime
                .submit_for_integration_test(&context, batch(&context))
                .status,
            SinkSubmitStatusV1::ACCEPTED
        );
        (runtime, ingress, inbox, context)
    }

    fn directory_entries(count: u64) -> Vec<FileEntry> {
        (0..count)
            .map(|index| FileEntry {
                id: ShellItemId::from_provider_bytes(index.to_le_bytes()).unwrap(),
                location: LocationDescriptor::file_system(format!(r"C:\fixture\{index}.txt")),
                display_name: format!("{index}.txt"),
                is_container: false,
                metadata: FileEntryMetadata::default(),
            })
            .collect()
    }

    #[test]
    fn directory_fixture_is_visible_before_extension_projection_runs() {
        let root = explorer_ui::ExplorerRoot::for_directory_fixture(
            explorer_ui::UiTokens::default(),
            directory_entries(1_000),
            ViewMode::Details,
        );
        let calls = Arc::new(AtomicUsize::new(0));
        assert_eq!(root.fixture_visible_entry_count(), Some(1_000));
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let (runtime, ingress, inbox, context) = queued_fixture();
        let mut app_pump = ApplicationExtensionUiPumpV1::with_ready_projector(
            inbox,
            ingress,
            Box::new(ApplyingProjectorV1 {
                calls: Arc::clone(&calls),
            }),
        )
        .unwrap();
        let now = Instant::now();
        assert!(!app_pump.poll_due(now));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(root.fixture_visible_entry_count(), Some(1_000));
        let deadline = app_pump.pump.next_deadline().unwrap().unwrap();
        assert!(app_pump.poll_due(deadline));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(matches!(
            runtime.finish_for_integration_test(context.job, JobTerminalV1::COMPLETED),
            explorer_extension_host::ExtensionJobFinishOutcomeV1::Published(
                JobTerminalV1::COMPLETED
            )
        ));
        runtime.retire(context.job).unwrap();
    }

    #[test]
    fn projector_injection_runs_before_poll_and_neither_deferred_nor_error_consumes_ready_work() {
        let (runtime, ingress, inbox, context) = queued_fixture();
        let mut deferred = ApplicationExtensionUiPumpV1::new(inbox, ingress).unwrap();
        assert!(!deferred.poll_due(Instant::now()));
        assert_eq!(deferred.pump.take_ready(1).unwrap().signals.len(), 1);
        assert!(matches!(
            runtime.finish_for_integration_test(context.job, JobTerminalV1::COMPLETED),
            explorer_extension_host::ExtensionJobFinishOutcomeV1::Published(
                JobTerminalV1::COMPLETED
            )
        ));
        runtime.retire(context.job).unwrap();

        let (runtime, ingress, inbox, context) = queued_fixture();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut app_pump = ApplicationExtensionUiPumpV1::with_ready_projector(
            inbox,
            ingress,
            Box::new(CountingProjectorV1 {
                calls: Arc::clone(&calls),
                fail: true,
            }),
        )
        .unwrap();
        app_pump.set_ready_projector(Box::new(CountingProjectorV1 {
            calls: Arc::clone(&calls),
            fail: true,
        }));
        assert!(!app_pump.poll_due(Instant::now()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(app_pump.pump.take_ready(1).unwrap().signals.len(), 1);
        assert!(matches!(
            runtime.finish_for_integration_test(context.job, JobTerminalV1::COMPLETED),
            explorer_extension_host::ExtensionJobFinishOutcomeV1::Published(
                JobTerminalV1::COMPLETED
            )
        ));
        runtime.retire(context.job).unwrap();
    }

    #[test]
    fn explicit_start_location_overrides_saved_tabs() {
        let configured = explorer_model::HistoryEntry::new(
            LocationDescriptor::file_system(r"D:\requested"),
            "requested",
        );

        assert!(!should_restore_saved_tabs(Some(&configured)));
        assert!(should_restore_saved_tabs(None));
    }

    #[test]
    fn safe_mode_offer_remains_denied_until_explicit_confirmation() {
        let port = FakeSafeModePortV1 {
            denied: true,
            confirmed: Mutex::new(Vec::new()),
        };
        let mut offers = vec![SafeModeIncidentOfferV1 {
            incident_id: 7,
            presentation_token: 1,
            kind: explorer_extension_host::NativeSafeModeIncidentKindV1::UnsafeMarkerState,
            suspect: None,
        }];

        assert!(port.denies_native_callbacks());
        assert_eq!(offers.len(), 1);
        assert!(port.confirmed.lock().unwrap().is_empty());

        assert_eq!(
            confirm_offered_safe_mode_incident_v1(&port, &mut offers, 7),
            Ok(true)
        );
        assert!(offers.is_empty());
        assert_eq!(port.confirmed.lock().unwrap().as_slice(), &[7]);
        assert_eq!(
            confirm_offered_safe_mode_incident_v1(&port, &mut offers, 7),
            Ok(false)
        );
        assert_eq!(port.confirmed.lock().unwrap().as_slice(), &[7]);
    }

    #[test]
    fn stale_safe_mode_presenter_token_does_not_confirm_a_shifted_offer() {
        let port = FakeSafeModePortV1 {
            denied: true,
            confirmed: Mutex::new(Vec::new()),
        };
        let mut offers = vec![
            SafeModeIncidentOfferV1 {
                incident_id: 7,
                presentation_token: 101,
                kind: explorer_extension_host::NativeSafeModeIncidentKindV1::UnsafeMarkerState,
                suspect: None,
            },
            SafeModeIncidentOfferV1 {
                incident_id: 9,
                presentation_token: 202,
                kind: explorer_extension_host::NativeSafeModeIncidentKindV1::UnsafeMarkerState,
                suspect: None,
            },
        ];

        assert_eq!(
            confirm_presented_safe_mode_incident_v1(&port, &mut offers, 101),
            Ok(true)
        );
        assert_eq!(port.confirmed.lock().unwrap().as_slice(), &[7]);

        assert_eq!(
            confirm_presented_safe_mode_incident_v1(&port, &mut offers, 101),
            Ok(false)
        );
        assert_eq!(port.confirmed.lock().unwrap().as_slice(), &[7]);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].incident_id(), 9);
    }

    #[test]
    fn post_commit_safe_mode_telemetry_failure_does_not_mask_confirmation() {
        let attempts = AtomicUsize::new(0);
        emit_post_commit_safe_mode_telemetry_v1(|| {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(())
        });
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn folder_size_requests_deduplicate_but_retain_distinct_tab_generations() {
        let tab = TabId::new();
        let first_context = RequestContext::new(tab, Generation::new(1));
        let second_context = RequestContext::new(tab, Generation::new(2));
        let request = |context: RequestContext, id: u64| {
            explorer_ui::folder_size_column::FolderSizeRequestV1 {
                context,
                item_id: ShellItemId::from_provider_bytes(id.to_le_bytes()).unwrap(),
                path: format!(r"C:\fixture\{id}").into(),
                mft_cache_memory_mb: 512,
                require_directory_facts: false,
            }
        };
        let mut pending = PendingFolderSizeWorkV1::default();

        enqueue_folder_size_requests(&mut pending, vec![request(first_context.clone(), 1)]);
        enqueue_folder_size_requests(
            &mut pending,
            vec![request(first_context.clone(), 1), request(first_context, 2)],
        );
        assert_eq!(pending.requests.as_ref().unwrap().len(), 2);

        enqueue_folder_size_requests(&mut pending, vec![request(second_context, 3)]);
        assert_eq!(pending.requests.as_ref().unwrap().len(), 3);
        assert_eq!(
            pending.requests.as_ref().unwrap()[2].item_id,
            ShellItemId::from_provider_bytes(3_u64.to_le_bytes()).unwrap()
        );
    }

    #[test]
    fn mft_failure_falls_back_to_exact_recursive_directory_facts() {
        let fixture = tempfile::tempdir().unwrap();
        let child = fixture.path().join("child");
        fs::create_dir(&child).unwrap();
        fs::write(fixture.path().join("root.txt"), b"a").unwrap();
        fs::write(child.join("nested.txt"), b"bc").unwrap();

        let context = RequestContext::new(TabId::new(), Generation::new(7));
        let item_id = ShellItemId::from_provider_bytes([1]).unwrap();
        let request = explorer_ui::folder_size_column::FolderSizeRequestV1 {
            context: context.clone(),
            item_id: item_id.clone(),
            path: fixture.path().to_path_buf(),
            mft_cache_memory_mb: 512,
            require_directory_facts: true,
        };
        let pending = (
            Mutex::new(PendingFolderSizeWorkV1::default()),
            std::sync::Condvar::new(),
        );
        let backend_status = std::sync::atomic::AtomicU8::new(2);
        let snapshots = Arc::new(Mutex::new(
            crate::folder_size_service::FolderSizeServiceV1::with_capacity(8),
        ));
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);

        assert!(super::publish_mft_folder_result_v1(
            &pending,
            &backend_status,
            &snapshots,
            &sender,
            request,
            Instant::now(),
            Err("forced MFT failure".to_owned()),
        ));

        let result = receiver.recv().unwrap();
        assert_eq!(result.context, context);
        assert_eq!(result.item_id, item_id);
        assert_eq!(result.exact_bytes, Some(3));
        assert_eq!(
            result.directory_facts,
            Some(explorer_ui::folder_size_column::DirectoryFactsV1 {
                mft_generation: 7,
                file_count: 2,
                folder_count: 1,
            })
        );
        assert_eq!(result.error, None);
        assert_eq!(backend_status.load(Ordering::Acquire), 1);
    }

    #[test]
    fn folder_size_workers_claim_independent_visible_requests() {
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let request = |id: u64| explorer_ui::folder_size_column::FolderSizeRequestV1 {
            context: context.clone(),
            item_id: ShellItemId::from_provider_bytes(id.to_le_bytes()).unwrap(),
            path: format!(r"C:\fixture\{id}").into(),
            mft_cache_memory_mb: 512,
            require_directory_facts: false,
        };
        let mut pending = PendingFolderSizeWorkV1::default();
        enqueue_folder_size_requests(&mut pending, (1..=4).map(request).collect());

        let claimed = (0..4)
            .map(|_| take_folder_size_batch(&mut pending, 1).remove(0).item_id)
            .collect::<HashSet<_>>();

        assert_eq!(claimed.len(), 4);
        assert!(pending.requests.is_none());
        assert_eq!(pending.in_flight.len(), 4);
    }

    #[test]
    fn folder_size_batch_preserves_visible_order_and_one_view_generation() {
        let first = RequestContext::new(TabId::new(), Generation::new(1));
        let second = RequestContext::new(TabId::new(), Generation::new(2));
        let request = |context: RequestContext, id: u64| {
            explorer_ui::folder_size_column::FolderSizeRequestV1 {
                context,
                item_id: ShellItemId::from_provider_bytes(id.to_le_bytes()).unwrap(),
                path: format!(r"C:\fixture\{id}").into(),
                mft_cache_memory_mb: 512,
                require_directory_facts: false,
            }
        };
        let mut pending = PendingFolderSizeWorkV1::default();
        enqueue_folder_size_requests(
            &mut pending,
            vec![
                request(first.clone(), 1),
                request(first.clone(), 2),
                request(second, 9),
                request(first, 3),
            ],
        );

        let batch = take_folder_size_batch(&mut pending, 2);
        let ids = batch
            .iter()
            .map(|request| {
                request
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, ["1", "2"]);
        assert_eq!(pending.in_flight.len(), 2);
        assert_eq!(pending.requests.as_ref().unwrap().len(), 2);
        assert_eq!(
            pending.requests.as_ref().unwrap()[0].path,
            PathBuf::from(r"C:\fixture\9")
        );
    }

    #[test]
    fn active_folder_size_request_is_not_queued_again_by_repeated_ui_submissions() {
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let request = explorer_ui::folder_size_column::FolderSizeRequestV1 {
            context,
            item_id: ShellItemId::from_provider_bytes(1_u64.to_le_bytes()).unwrap(),
            path: PathBuf::from(r"C:\fixture\slow"),
            mft_cache_memory_mb: 512,
            require_directory_facts: false,
        };
        let mut pending = PendingFolderSizeWorkV1::default();

        enqueue_folder_size_requests(&mut pending, vec![request.clone()]);
        assert_eq!(
            take_folder_size_batch(&mut pending, usize::MAX),
            vec![request.clone()]
        );
        assert!(pending.requests.is_none());
        assert_eq!(pending.in_flight.len(), 1);

        for _ in 0..10 {
            enqueue_folder_size_requests(&mut pending, vec![request.clone()]);
        }

        assert!(
            pending.requests.as_ref().is_none_or(Vec::is_empty),
            "an in-flight recursive scan must not be queued from its root again"
        );
        assert_eq!(pending.in_flight.len(), 1);
    }

    #[test]
    fn new_request_id_in_same_generation_does_not_restart_folder_size_work() {
        let tab = TabId::new();
        let first_context = RequestContext::new(tab, Generation::new(1));
        let next_frame_context = RequestContext::new(tab, Generation::new(1));
        assert_ne!(first_context.request_id, next_frame_context.request_id);
        let request =
            |context: RequestContext| explorer_ui::folder_size_column::FolderSizeRequestV1 {
                context,
                item_id: ShellItemId::from_provider_bytes(1_u64.to_le_bytes()).unwrap(),
                path: PathBuf::from(r"C:\fixture\slow"),
                mft_cache_memory_mb: 512,
                require_directory_facts: false,
            };
        let mut pending = PendingFolderSizeWorkV1::default();

        enqueue_folder_size_requests(&mut pending, vec![request(first_context)]);
        let active = take_folder_size_batch(&mut pending, usize::MAX);
        enqueue_folder_size_requests(&mut pending, vec![request(next_frame_context)]);

        assert_eq!(active.len(), 1);
        assert!(pending.requests.as_ref().is_none_or(Vec::is_empty));
        assert_eq!(pending.in_flight.len(), 1);
    }

    #[test]
    fn cancelled_folder_size_context_releases_old_work_and_accepts_new_generation() {
        let tab = TabId::new();
        let old_request = explorer_ui::folder_size_column::FolderSizeRequestV1 {
            context: RequestContext::new(tab, Generation::new(1)),
            item_id: ShellItemId::from_provider_bytes(1_u64.to_le_bytes()).unwrap(),
            path: PathBuf::from(r"C:\fixture\old-slow"),
            mft_cache_memory_mb: 512,
            require_directory_facts: false,
        };
        let new_request = explorer_ui::folder_size_column::FolderSizeRequestV1 {
            context: RequestContext::new(tab, Generation::new(2)),
            item_id: ShellItemId::from_provider_bytes(2_u64.to_le_bytes()).unwrap(),
            path: PathBuf::from(r"C:\fixture\new"),
            mft_cache_memory_mb: 512,
            require_directory_facts: false,
        };
        let mut pending = PendingFolderSizeWorkV1::default();

        enqueue_folder_size_requests(&mut pending, vec![old_request.clone()]);
        assert_eq!(
            take_folder_size_batch(&mut pending, usize::MAX),
            vec![old_request.clone()]
        );
        cancel_folder_size_context(&mut pending, &old_request.context);
        enqueue_folder_size_requests(&mut pending, vec![new_request.clone()]);

        assert!(
            !pending
                .in_flight
                .contains_key(&super::FolderSizeWorkIdentityV1::from(&old_request)),
            "the inactive context must release its logical in-flight identity"
        );
        assert!(pending.cancelled.contains(&old_request.context.request_id));
        assert_eq!(pending.requests.as_deref(), Some([new_request].as_slice()));
    }

    #[test]
    fn size_map_requests_merge_per_context_and_terminalize_queue_overflow() {
        let tab = TabId::new();
        let first_context = RequestContext::new(tab, Generation::new(1));
        let second_context = RequestContext::new(tab, Generation::new(2));
        let request = |context: RequestContext, id: u64| {
            explorer_ui::size_map_view::SizeMapMeasureRequestV1 {
                context,
                item_id: ShellItemId::from_provider_bytes(id.to_le_bytes()).unwrap(),
                path: format!(r"C:\\fixture\\{id}").into(),
            }
        };
        let epoch = AtomicU64::new(0);
        let mut pending = PendingSizeMapWorkV1::default();

        assert!(
            enqueue_size_map_requests(
                &mut pending,
                &epoch,
                vec![request(first_context.clone(), 1)],
            )
            .is_empty()
        );
        assert_eq!(pending.epoch, 1);
        assert_eq!(pending.requests.len(), 1);

        assert!(
            enqueue_size_map_requests(&mut pending, &epoch, vec![request(first_context, 2)],)
                .is_empty()
        );
        assert_eq!(pending.epoch, 1, "same context must not cancel work");
        assert_eq!(pending.requests.len(), 2);

        assert!(
            enqueue_size_map_requests(
                &mut pending,
                &epoch,
                vec![request(second_context.clone(), 3)],
            )
            .is_empty()
        );
        assert_eq!(pending.epoch, 2);
        assert_eq!(pending.requests.len(), 1, "new context replaces old batch");

        let overflow = enqueue_size_map_requests(
            &mut pending,
            &epoch,
            (4..u64::try_from(SIZE_MAP_REQUEST_QUEUE_CAP_V1).unwrap() + 4)
                .map(|id| request(second_context.clone(), id))
                .collect(),
        );
        assert_eq!(pending.requests.len(), SIZE_MAP_REQUEST_QUEUE_CAP_V1);
        assert_eq!(overflow.len(), 1);
        assert!(overflow[0].partial);
        assert!(
            overflow[0]
                .error
                .as_deref()
                .is_some_and(|message| message.contains("queue limit"))
        );
    }

    #[test]
    fn local_drive_size_map_announces_mft_before_index_construction() {
        #[cfg(windows)]
        assert_eq!(
            preferred_size_map_scan_method(Path::new(r"D:\folder")),
            "NTFS MFT"
        );
        assert_eq!(
            preferred_size_map_scan_method(Path::new(r"\\server\share")),
            "Breadth-first fallback"
        );
    }
}

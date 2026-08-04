//! Production process composition root.

use std::{
    collections::{HashMap, HashSet},
    fs,
    hash::{Hash, Hasher},
    io::Read as _,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
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
use crate::{
    automation_service::AutomationComposition, system_theme::high_contrast_tokens,
    visual_fixture::VisualFixtureConfig,
};

const SHELL_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

const FOLDER_SIZE_CONTRIBUTION_ID_V1: &str = "folder-size";
const FOLDER_SIZE_RENDERER_CONTRIBUTION_ID_V1: &str = "folder-size-renderer";
const SIZE_MAP_VIEW_CONTRIBUTION_ID_V1: &str = "size-map";
const SIZE_MAP_REQUEST_QUEUE_CAP_V1: usize = 1_024;
const CODE_LINES_CONTRIBUTION_ID_V1: &str = "rust-tokei:code-lines";
const CODE_LINES_RENDERER_CONTRIBUTION_ID_V1: &str = "rust-tokei:code-lines-renderer";
const LOCK_OWNER_CONTRIBUTION_ID_V1: &str = "rust-lock-owner:owners";
const LOCK_OWNER_RENDERER_CONTRIBUTION_ID_V1: &str = "rust-lock-owner:owners-renderer";
const CODE_LINES_BATCH_ITEMS_V1: usize = 128;
const DIRECT_RENDER_QUEUE_CAP_V1: usize = 256;
const DIRECT_RENDER_CACHE_CAP_V1: usize = 512;
const SIZE_MAP_RENDER_QUEUE_CAP_V1: usize = 8;
const SIZE_MAP_RENDER_CACHE_CAP_V1: usize = 4;

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
    bytes.extend_from_slice(&context.generation.to_le_bytes());
    bytes.extend_from_slice(&context.render_revision.to_le_bytes());
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
) -> SizeMapRenderKeyV1 {
    context.render_revision = 0;
    let mut revision_input = size_map_snapshot_bytes(context);
    append_size_map_host_scope(&mut revision_input, request_context, item_ids);
    context.render_revision = revision_for(&revision_input);

    let mut key = size_map_snapshot_bytes(context);
    append_size_map_host_scope(&mut key, request_context, item_ids);
    SizeMapRenderKeyV1(key)
}

/// Bounded bridge between GPUI and one retained direct-DLL renderer. The GPUI
/// caller only polls a cache and uses `try_send`; the plugin callback (and its
/// durable call marker) always run on this worker thread.
struct AsyncCellRendererV1 {
    requests: mpsc::SyncSender<explorer_extension_ui_api::CellRenderContextV1>,
    results: Mutex<
        mpsc::Receiver<(
            CellRenderKeyV1,
            Option<explorer_extension_ui_api::CellRenderPlanV1>,
        )>,
    >,
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
                    let plan = renderer.render(contribution_id, context).ok();
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
                if let Some(plan) = plan
                    && let Ok(mut cache) = self.cache.lock()
                {
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
    mappings: HashMap<explorer_extension_ui_api::StableIdV1, (explorer_model::ShellItemId, String)>,
    width: f32,
    height: f32,
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
                    let generation = context.generation;
                    let render_revision = context.render_revision;
                    let plan = match renderer.render(SIZE_MAP_VIEW_CONTRIBUTION_ID_V1, context) {
                        Ok(plan)
                            if plan.generation == generation
                                && plan.render_revision == render_revision =>
                        {
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
        rectangles: Vec::new(),
        status: Some(status.to_owned()),
        available: false,
    }
}

fn project_size_map_plan(
    plan: explorer_extension_ui_api::SizeMapRenderPlanV1,
    mappings: HashMap<explorer_extension_ui_api::StableIdV1, (explorer_model::ShellItemId, String)>,
    width: f32,
    height: f32,
) -> explorer_ui::size_map_view::SizeMapRenderPlanV1 {
    explorer_ui::size_map_view::SizeMapRenderPlanV1 {
        rectangles: plan
            .rectangles
            .into_iter()
            .filter_map(|rectangle| {
                let (item_id, status) = mappings.get(&rectangle.node_id)?.clone();
                Some(explorer_ui::size_map_view::SizeMapRectangleV1 {
                    item_id,
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
    renderer: AsyncCellRendererV1,
}

#[derive(Default)]
struct PendingFolderSizeWorkV1 {
    requests: Option<Vec<explorer_ui::folder_size_column::FolderSizeRequestV1>>,
    in_flight: HashSet<FolderSizeWorkIdentityV1>,
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
        if !state.in_flight.contains(&identity)
            && !pending
                .iter()
                .any(|queued| FolderSizeWorkIdentityV1::from(queued) == identity)
        {
            pending.push(request);
        }
    }
}

fn take_folder_size_requests(
    state: &mut PendingFolderSizeWorkV1,
) -> Vec<explorer_ui::folder_size_column::FolderSizeRequestV1> {
    let requests = state.requests.take().unwrap_or_default();
    state
        .in_flight
        .extend(requests.iter().map(FolderSizeWorkIdentityV1::from));
    requests
}

fn finish_folder_size_request(
    pending: &(Mutex<PendingFolderSizeWorkV1>, Condvar),
    request: &explorer_ui::folder_size_column::FolderSizeRequestV1,
) {
    let (lock, _) = pending;
    lock.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .in_flight
        .remove(&FolderSizeWorkIdentityV1::from(request));
}

impl ApplicationVisualColumnRuntimeV1 {
    fn start(
        mut measure: explorer_extension_host::SinglePluginVisualMeasureRuntimeV1,
        renderer: explorer_extension_host::SinglePluginVisualRenderRuntimeV1,
    ) -> Result<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1, Error> {
        let pending = Arc::new((
            Mutex::new(PendingFolderSizeWorkV1::default()),
            Condvar::new(),
        ));
        let worker_pending = pending.clone();
        let (result_tx, result_rx) =
            mpsc::sync_channel::<explorer_ui::folder_size_column::FolderSizeResultV1>(1_024);
        std::thread::Builder::new()
            .name("p0-folder-size".to_owned())
            .spawn(move || {
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
                        take_folder_size_requests(&mut state)
                    };
                    for request in requests {
                        let measured = measure.measure_folder_size(
                            FOLDER_SIZE_CONTRIBUTION_ID_V1,
                            explorer_extension_ui_api::FolderSizeMeasureRequestV1 {
                                filesystem_path: request.path.to_string_lossy().into_owned().into(),
                                max_entries: 100_000,
                                max_depth: 128,
                                // This callback already runs off the GPUI thread. A
                                // foreground budget must never terminate the scan:
                                // let it finish and populate the plugin cache even
                                // when navigation makes its UI result stale.
                                deadline_millis: 0,
                            },
                        );
                        finish_folder_size_request(&worker_pending, &request);
                        let (exact_bytes, partial, error) = match measured {
                            Ok(result) => (
                                (!result.partial).then_some(result.exact_bytes),
                                result.partial,
                                result.error.into_option().map(String::from),
                            ),
                            Err(error) => (None, true, Some(error.to_string())),
                        };
                        if result_tx
                            .send(explorer_ui::folder_size_column::FolderSizeResultV1 {
                                context: request.context,
                                item_id: request.item_id,
                                exact_bytes,
                                partial,
                                error,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            })
            .context("failed to start P0 folder-size worker")?;
        Ok(Arc::new(Self {
            pending,
            results: Mutex::new(result_rx),
            renderer: AsyncCellRendererV1::start(
                renderer,
                FOLDER_SIZE_RENDERER_CONTRIBUTION_ID_V1,
            )?,
        }))
    }
}

impl explorer_ui::folder_size_column::VisualColumnRuntimePortV1
    for ApplicationVisualColumnRuntimeV1
{
    fn config(&self) -> explorer_ui::folder_size_column::VisualColumnConfigV1 {
        explorer_ui::folder_size_column::VisualColumnConfigV1::default()
    }

    fn submit_folder_size_requests(
        &self,
        requests: Vec<explorer_ui::folder_size_column::FolderSizeRequestV1>,
    ) {
        let (lock, ready) = &*self.pending;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        enqueue_folder_size_requests(&mut state, requests);
        ready.notify_one();
    }

    fn drain_folder_size_results(
        &self,
    ) -> Vec<explorer_ui::folder_size_column::FolderSizeResultV1> {
        let Ok(results) = self.results.lock() else {
            return Vec::new();
        };
        results.try_iter().collect()
    }

    fn drain_render_results(&self) -> bool {
        self.renderer.drain_ready()
    }

    fn render_cell(
        &self,
        context: explorer_extension_ui_api::CellRenderContextV1,
    ) -> explorer_extension_ui_api::CellRenderPlanV1 {
        self.renderer
            .render_or_enqueue(context, "Loading folder size")
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
    renderer: AsyncCellRendererV1,
    mode: BatchDetailsColumnModeV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BatchDetailsColumnModeV1 {
    CodeLines,
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
    ) -> Result<explorer_ui::code_lines_column::CodeLinesRuntimeHandleV1, Error> {
        let pending = Arc::new((
            Mutex::new(PendingCodeLinesWorkV1::default()),
            Condvar::new(),
        ));
        let worker_pending = pending.clone();
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
                        let bytes = match mode {
                            BatchDetailsColumnModeV1::CodeLines => {
                                read_code_lines_file_bounded(&request.path)
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
                            );
                            prepared_bytes = 0;
                        }
                        prepared_bytes = prepared_bytes.saturating_add(bytes.len());
                        prepared.push((request, bytes));
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
                        );
                    }
                }
            })
            .context("failed to start Rust tokei Code lines worker")?;
        Ok(Arc::new(Self {
            pending,
            request_epoch,
            results: Mutex::new(result_rx),
            renderer: AsyncCellRendererV1::start(
                renderer,
                match mode {
                    BatchDetailsColumnModeV1::CodeLines => CODE_LINES_RENDERER_CONTRIBUTION_ID_V1,
                    BatchDetailsColumnModeV1::LockOwner => LOCK_OWNER_RENDERER_CONTRIBUTION_ID_V1,
                },
            )?,
            mode,
        }))
    }
}

fn process_code_lines_batch(
    provider: &explorer_extension_host::SinglePluginBatchColumnRuntimeV1,
    runtime: &explorer_extension_host::ExtensionJobRuntimeV1,
    requests: Vec<(explorer_ui::code_lines_column::CodeLinesRequestV1, Vec<u8>)>,
    epoch: u64,
    current_epoch: &AtomicU64,
    results: &mpsc::SyncSender<explorer_ui::code_lines_column::CodeLinesResultV1>,
    mode: BatchDetailsColumnModeV1,
) {
    let Some(first) = requests.first() else {
        return;
    };
    let generation = first.0.context.generation.value().max(1);
    let inputs = requests
        .iter()
        .filter_map(|(request, bytes)| {
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
                lock_owner_resource: (mode == BatchDetailsColumnModeV1::LockOwner)
                    .then(|| request.path.clone()),
            })
        })
        .collect::<Vec<_>>();
    if current_epoch.load(Ordering::Acquire) != epoch {
        return;
    }
    if inputs.len() != requests.len() {
        emit_code_lines_batch_error(requests, "Code lines input could not be prepared", results);
        return;
    }
    let contribution_id = match mode {
        BatchDetailsColumnModeV1::CodeLines => CODE_LINES_CONTRIBUTION_ID_V1,
        BatchDetailsColumnModeV1::LockOwner => LOCK_OWNER_CONTRIBUTION_ID_V1,
    };
    let lock_owner_query =
        (mode == BatchDetailsColumnModeV1::LockOwner).then(lock_owner_query_service);
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
                .and_then(|(request, _)| request.path.file_name())
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            (display, index as u128 + 1)
        }) else {
            continue;
        };
        for row in rows {
            let Some((request, _)) = requests.get(emitted) else {
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
    requests: Vec<(explorer_ui::code_lines_column::CodeLinesRequestV1, Vec<u8>)>,
    message: &str,
    results: &mpsc::SyncSender<explorer_ui::code_lines_column::CodeLinesResultV1>,
) {
    for (request, _) in requests {
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

fn lock_owner_query_service() -> explorer_extension_host::HostLockOwnerQueryServiceV1 {
    explorer_extension_host::HostLockOwnerQueryServiceV1::new(|path, _deadline_millis| {
        let request = explorer_model::LockOwnerDiscoveryRequest {
            resources: vec![explorer_model::LocationDescriptor::file_system(
                path.clone(),
            )],
        };
        let outcome = explorer_shell_win::discover_lock_owners_read_only(
            &request,
            &explorer_model::CancellationToken::new(),
        );
        match outcome {
            explorer_model::LockOwnerDiscoveryTerminal::Ready(owners) => (
                explorer_extension_api::LockOwnerQueryStatusV1::READY,
                owners
                    .into_iter()
                    .map(|owner| explorer_extension_api::LockOwnerRecordV1 {
                        item: explorer_extension_api::ItemHandleV1::from_host([0; 16], 0),
                        process_id: owner.identity.process_id,
                        application_type:
                            explorer_extension_api::LockOwnerApplicationTypeV1::from_raw(
                                match owner.application_type {
                                    explorer_model::LockOwnerApplicationType::Unknown => 0,
                                    explorer_model::LockOwnerApplicationType::MainWindow => 1,
                                    explorer_model::LockOwnerApplicationType::OtherWindow => 2,
                                    explorer_model::LockOwnerApplicationType::Service => 3,
                                    explorer_model::LockOwnerApplicationType::Explorer => 4,
                                    explorer_model::LockOwnerApplicationType::Console => 5,
                                    explorer_model::LockOwnerApplicationType::Critical => 6,
                                },
                            ),
                        display_name: owner.display_name.into(),
                        service_name: "".into(),
                    })
                    .collect(),
            ),
            explorer_model::LockOwnerDiscoveryTerminal::Empty => (
                explorer_extension_api::LockOwnerQueryStatusV1::EMPTY,
                Vec::new(),
            ),
            explorer_model::LockOwnerDiscoveryTerminal::Cancelled => (
                explorer_extension_api::LockOwnerQueryStatusV1::CANCELLED,
                Vec::new(),
            ),
            explorer_model::LockOwnerDiscoveryTerminal::Unavailable(_) => (
                explorer_extension_api::LockOwnerQueryStatusV1::UNAVAILABLE,
                Vec::new(),
            ),
            explorer_model::LockOwnerDiscoveryTerminal::Failed(_) => (
                explorer_extension_api::LockOwnerQueryStatusV1::HOST_ERROR,
                Vec::new(),
            ),
        }
    })
}

fn parse_batch_details_value(
    bytes: &[u8],
    mode: BatchDetailsColumnModeV1,
) -> Option<explorer_ui::code_lines_column::CodeLinesValueV1> {
    if mode == BatchDetailsColumnModeV1::CodeLines {
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
        if self.mode == BatchDetailsColumnModeV1::LockOwner {
            config.descriptor = explorer_ui::code_lines_column::lock_owner_column_descriptor();
        }
        config
    }

    fn submit_code_lines_requests(
        &self,
        requests: Vec<explorer_ui::code_lines_column::CodeLinesRequestV1>,
    ) {
        let Some(first) = requests.first() else {
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
            state.requests = Some(requests);
        } else {
            let queued = state.requests.get_or_insert_with(Vec::new);
            for request in requests {
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

    fn drain_code_lines_results(&self) -> Vec<explorer_ui::code_lines_column::CodeLinesResultV1> {
        self.results
            .lock()
            .map_or_else(|_| Vec::new(), |results| results.try_iter().collect())
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
    ) -> Result<explorer_ui::size_map_view::SizeMapRuntimeHandleV1, Error> {
        let pending = Arc::new((Mutex::new(PendingSizeMapWorkV1::default()), Condvar::new()));
        let worker_pending = pending.clone();
        let request_epoch = Arc::new(AtomicU64::new(0));
        let worker_epoch = request_epoch.clone();
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
                    // Publish an initial state for every direct child before
                    // walking any subtree. This keeps the map interactive and
                    // lets the renderer show all known siblings while exact
                    // recursive totals arrive one at a time.
                    for request in requests.iter().cloned() {
                        if worker_epoch.load(Ordering::Acquire) != batch_epoch {
                            break;
                        }
                        if worker_result_tx
                            .send(size_map_scanning_result(request))
                            .is_err()
                        {
                            return;
                        }
                    }
                    for request in requests {
                        if worker_epoch.load(Ordering::Acquire) != batch_epoch {
                            break;
                        }
                        let (bytes, partial, error, cancelled) =
                            measure_size_map_path(&request.path, 100_000, 128, || {
                                worker_epoch.load(Ordering::Acquire) != batch_epoch
                            });
                        if cancelled || worker_epoch.load(Ordering::Acquire) != batch_epoch {
                            break;
                        }
                        if worker_result_tx
                            .send(explorer_ui::size_map_view::SizeMapMeasureResultV1 {
                                context: request.context,
                                item_id: request.item_id,
                                exact_bytes: (!partial).then_some(bytes),
                                partial,
                                error,
                            })
                            .is_err()
                        {
                            break;
                        }
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
        }))
    }
}

fn size_map_scanning_result(
    request: explorer_ui::size_map_view::SizeMapMeasureRequestV1,
) -> explorer_ui::size_map_view::SizeMapMeasureResultV1 {
    explorer_ui::size_map_view::SizeMapMeasureResultV1 {
        context: request.context,
        item_id: request.item_id,
        exact_bytes: None,
        partial: true,
        error: Some("Scanning recursively".to_owned()),
    }
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

fn measure_size_map_path(
    root: &Path,
    max_entries: u32,
    max_depth: u16,
    mut cancelled: impl FnMut() -> bool,
) -> (u64, bool, Option<String>, bool) {
    let mut pending = vec![(root.to_path_buf(), 0_u16)];
    let mut visited = 0_u32;
    let mut bytes = 0_u64;
    let mut error = None;
    while let Some((path, depth)) = pending.pop() {
        if cancelled() {
            return (bytes, true, None, true);
        }
        if visited >= max_entries {
            error.get_or_insert_with(|| "Size Map scan resource limit reached".to_owned());
            break;
        }
        visited = visited.saturating_add(1);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(cause) => {
                error.get_or_insert_with(|| cause.to_string());
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            bytes = bytes.saturating_add(metadata.len());
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        if depth >= max_depth {
            error.get_or_insert_with(|| "Size Map scan depth limit reached".to_owned());
            continue;
        }
        match fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries {
                    if cancelled() {
                        return (bytes, true, None, true);
                    }
                    let queued = u32::try_from(pending.len()).unwrap_or(u32::MAX);
                    if visited.saturating_add(queued) >= max_entries {
                        error.get_or_insert_with(|| {
                            "Size Map scan resource limit reached".to_owned()
                        });
                        break;
                    }
                    match entry {
                        Ok(entry) => pending.push((entry.path(), depth.saturating_add(1))),
                        Err(cause) => {
                            error.get_or_insert_with(|| cause.to_string());
                        }
                    }
                }
            }
            Err(cause) => {
                error.get_or_insert_with(|| cause.to_string());
            }
        }
    }
    (bytes, error.is_some(), error, false)
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

    fn drain_measure_results(&self) -> Vec<explorer_ui::size_map_view::SizeMapMeasureResultV1> {
        let Ok(results) = self.results.lock() else {
            return Vec::new();
        };
        results.try_iter().collect()
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
        let mappings = context
            .nodes
            .iter()
            .zip(&node_ids)
            .map(|(node, node_id)| {
                let status = if node.partial {
                    "Partial"
                } else if node.error.is_some() {
                    "Failed"
                } else if node.exact_bytes.is_some() {
                    "Complete"
                } else {
                    "Unavailable"
                };
                (*node_id, (node.item_id.clone(), status.to_owned()))
            })
            .collect::<HashMap<_, _>>();
        let public_nodes = context
            .nodes
            .iter()
            .zip(&node_ids)
            .map(|(node, node_id)| explorer_extension_ui_api::SizeMapNodeV1 {
                node_id: *node_id,
                parent_id: None.into(),
                name: node.display_name.clone().into(),
                kind: if node.is_container {
                    SizeMapNodeKindV1::DIRECTORY
                } else {
                    SizeMapNodeKindV1::FILE
                },
                exact_bytes: node.exact_bytes.into(),
                status: if node.partial {
                    SizeMapNodeStatusV1::PARTIAL
                } else if node.error.is_some() {
                    SizeMapNodeStatusV1::FAILED
                } else if node.exact_bytes.is_some() {
                    SizeMapNodeStatusV1::COMPLETE
                } else {
                    SizeMapNodeStatusV1::UNAVAILABLE
                },
            })
            .collect::<Vec<_>>();
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
                item_ids
                    .iter()
                    .position(|item_id| item_id == selected)
                    .map(|index| node_ids[index])
            })
            .collect();
        let mut public_context = explorer_extension_ui_api::SizeMapRenderContextV1 {
            generation,
            render_revision: 0,
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
            settings: "default".into(),
        };
        let key = size_map_render_key(&mut public_context, &context.request_context, &item_ids);
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

struct ShutdownResources {
    diagnostics: DiagnosticsSession,
    automation: Option<AutomationComposition>,
    extension_host: Option<explorer_extension_host::ExtensionHost>,
    loaded_extension_summary: Option<String>,
    visual_column_runtime: Option<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1>,
    code_lines_runtime: Option<explorer_ui::code_lines_column::CodeLinesRuntimeHandleV1>,
    size_map_runtime: Option<explorer_ui::size_map_view::SizeMapRuntimeHandleV1>,
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
        Self::start_with_plugin(diagnostics, None)
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
        let dpi_outcome = initialize_dpi_awareness()?;
        let dpi_outcome_text = format!("{dpi_outcome:?}");
        diagnostics.record_event("windows_prerequisites_ready", &[("dpi", &dpi_outcome_text)])?;
        let shell_sta = Arc::new(ShellStaHandle::start()?);
        diagnostics.record_event("shell_sta_ready", &[])?;
        let automation = AutomationComposition::start()?;
        let script_count = automation.snapshots()?.len().to_string();
        diagnostics.record_event("automation_ready", &[("scripts", &script_count)])?;
        let _uitest_state_root = uitest_extension_state_root_v1()?;
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
        let mut extension_host =
            explorer_extension_host::ExtensionHost::with_config(extension_config);
        extension_host.start()?;
        let direct_loaded = match plugin_dll {
            Some(path) => match extension_host.load_single_plugin_visual_column_runtime(path) {
                Ok(loaded) => Some((path, loaded)),
                Err(explorer_extension_host::SinglePluginLoadErrorV1::BlockedBySafeMode) => {
                    diagnostics.record_event("development_plugin_blocked_by_safe_mode", &[])?;
                    None
                }
                Err(error) => return Err(error.into()),
            },
            None => None,
        };
        let (loaded_extension_summary, visual_column_runtime, code_lines_runtime, size_map_runtime) =
            if let Some((path, loaded)) = direct_loaded {
                let (summary, measure, renderer, size_map_renderer, batch_columns) =
                    loaded.into_parts_with_batch_columns();
                let supports_folder_size =
                    summary.contributions().iter().any(|contribution| {
                        contribution.contribution_id() == FOLDER_SIZE_CONTRIBUTION_ID_V1
                    }) && summary.contributions().iter().any(|contribution| {
                        contribution.contribution_id() == FOLDER_SIZE_RENDERER_CONTRIBUTION_ID_V1
                    });
                let supports_code_lines = batch_columns.contains(CODE_LINES_CONTRIBUTION_ID_V1);
                let supports_lock_owner = batch_columns.contains(LOCK_OWNER_CONTRIBUTION_ID_V1);
                let (visual_runtime, code_lines_runtime) =
                    if supports_code_lines || supports_lock_owner {
                        (
                            None,
                            Some(ApplicationCodeLinesRuntimeV1::start(
                                batch_columns,
                                renderer,
                                if supports_lock_owner {
                                    BatchDetailsColumnModeV1::LockOwner
                                } else {
                                    BatchDetailsColumnModeV1::CodeLines
                                },
                            )?),
                        )
                    } else if supports_folder_size {
                        (
                            Some(ApplicationVisualColumnRuntimeV1::start(measure, renderer)?),
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
                let size_map_runtime = if supports_size_map {
                    Some(ApplicationSizeMapRuntimeV1::start(size_map_renderer)?)
                } else {
                    None
                };
                (
                    Some(format_single_plugin_summary(path, &summary)),
                    visual_runtime,
                    code_lines_runtime,
                    size_map_runtime,
                )
            } else {
                (None, None, None, None)
            };
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
                automation: Some(automation),
                extension_host: Some(extension_host),
                loaded_extension_summary,
                visual_column_runtime,
                code_lines_runtime,
                size_map_runtime,
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
        let shell_service: Arc<dyn explorer_model::ExplorerService> =
            Arc::new(crate::brokered_service::BrokeredExplorerService::new(
                Arc::clone(&shell_sta),
                broker_client,
            ));
        let shutdown_resources = Arc::clone(&self.resources);
        let folder_scripts = self.automation_handle()?;
        let safe_mode_offers = self.safe_mode_ui_offers()?;
        let loaded_extension_summary = self.loaded_extension_summary()?;
        let visual_column_runtime = self.visual_column_runtime()?;
        let code_lines_runtime = self.code_lines_runtime()?;
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
        let (mut persistence, durable_observer, reset_observer, restore_preference, quick_access) =
            if visual_fixture.is_none() {
                create_session_persistence(restored_placement)
            } else {
                (None, None, None, true, Vec::new())
            };

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

                cx.on_window_closed(|cx, _| {
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
                let reset_observer_for_window = reset_observer.clone();
                let restore_preference_for_window = restore_preference;
                let quick_access_for_window = quick_access.clone();
                let loaded_extension_summary_for_window = loaded_extension_summary.clone();
                let visual_column_runtime_for_window = visual_column_runtime.clone();
                let code_lines_runtime_for_window = code_lines_runtime.clone();
                let size_map_runtime_for_window = size_map_runtime.clone();
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
                    cx.new(move |cx| {
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
                            reset_observer_for_window,
                            restore_preference_for_window,
                            quick_access_for_window,
                            broker_health,
                            broker_retry,
                            folder_scripts,
                            safe_mode_offers,
                            safe_mode_confirm,
                            loaded_extension_summary_for_window,
                            visual_column_runtime_for_window,
                            code_lines_runtime_for_window,
                            size_map_runtime_for_window,
                            extension_ui_pump.map(|pump| {
                                Box::new(pump) as Box<dyn explorer_ui::ExtensionUiPumpPortV1>
                            }),
                            window,
                            cx,
                        )
                    })
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

    fn code_lines_runtime(
        &self,
    ) -> Result<Option<explorer_ui::code_lines_column::CodeLinesRuntimeHandleV1>, Error> {
        self.resources
            .lock()
            .map(|resources| resources.code_lines_runtime.clone())
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

    fn shell_service(&self) -> Result<Arc<ShellStaHandle>, Error> {
        self.resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?
            .shell_sta
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Shell STA is not available"))
    }

    fn automation_handle(&self) -> Result<explorer_automation::FolderScriptHandle, Error> {
        self.resources
            .lock()
            .map_err(|_| anyhow::anyhow!("application lifecycle mutex was poisoned"))?
            .automation
            .as_ref()
            .map(AutomationComposition::handle)
            .ok_or_else(|| anyhow::anyhow!("automation service is not available"))
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
    reset_observer: Option<explorer_ui::SessionResetObserver>,
    restore_preference: bool,
    quick_access: Vec<explorer_model::PersistedQuickAccessPin>,
    broker_health: explorer_ui::state::BrokerUiHealth,
    broker_retry: explorer_ui::BrokerRetryObserver,
    folder_scripts: explorer_automation::FolderScriptHandle,
    visual_column_runtime: Option<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1>,
    code_lines_runtime: Option<explorer_ui::code_lines_column::CodeLinesRuntimeHandleV1>,
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
    root.attach_folder_scripts(folder_scripts);
    root.configure_restore_previous_session(restore_preference);
    root.configure_quick_access(quick_access);
    root.configure_broker_health(broker_health, broker_retry);
    if let Some(runtime) = visual_column_runtime {
        root.attach_visual_column_runtime(runtime);
    }
    if let Some(runtime) = code_lines_runtime {
        root.attach_code_lines_runtime(runtime);
    }
    if let Some(runtime) = size_map_runtime {
        root.attach_size_map_runtime(runtime);
    }
    if let Some(observer) = durable_observer {
        root.attach_durable_state_observer(observer, window, cx);
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
    reset_observer: Option<explorer_ui::SessionResetObserver>,
    restore_preference: bool,
    quick_access: Vec<explorer_model::PersistedQuickAccessPin>,
    broker_health: explorer_ui::state::BrokerUiHealth,
    broker_retry: explorer_ui::BrokerRetryObserver,
    folder_scripts: explorer_automation::FolderScriptHandle,
    safe_mode_offers: Vec<explorer_ui::SafeModeOfferV1>,
    safe_mode_confirm: explorer_ui::SafeModeConfirmObserverV1,
    loaded_extension_summary: Option<String>,
    visual_column_runtime: Option<explorer_ui::folder_size_column::VisualColumnRuntimeHandleV1>,
    code_lines_runtime: Option<explorer_ui::code_lines_column::CodeLinesRuntimeHandleV1>,
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
        reset_observer,
        restore_preference,
        quick_access,
        broker_health,
        broker_retry,
        folder_scripts,
        visual_column_runtime,
        code_lines_runtime,
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
    if !safe_mode_offers.is_empty() {
        root.configure_safe_mode_offers(safe_mode_offers, safe_mode_confirm);
    }
    root.configure_loaded_extension_summary(loaded_extension_summary);
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
) {
    let limits = RoadmapLimits::default();
    let Ok(store) = crate::session_store::WindowsSessionStore::from_environment(limits) else {
        return (None, None, None, true, Vec::new());
    };
    let loaded = store.load().ok().and_then(|outcome| outcome.envelope);
    let generation = loaded
        .as_ref()
        .map_or(1, |envelope| envelope.write_generation.saturating_add(1));
    let quick_access = loaded
        .as_ref()
        .map_or_else(Vec::new, |envelope| envelope.payload.quick_access.clone());
    let restore_enabled = loaded
        .as_ref()
        .is_none_or(|envelope| envelope.payload.restore_enabled);
    let store: Arc<dyn explorer_model::SessionStore> = Arc::new(store);
    let coordinator = crate::session_lifecycle::PersistenceCoordinator::start(
        store,
        Duration::from_millis(limits.preview_debounce_ms.max(250)),
        Duration::from_secs(2),
    );
    let handle = coordinator.handle();
    let generation = Arc::new(AtomicU64::new(generation));
    let reset_handle = handle.clone();
    let reset_observer: explorer_ui::SessionResetObserver =
        Arc::new(move |scope| reset_handle.request_reset(scope));
    let observer: explorer_ui::DurableStateObserver =
        Arc::new(move |window, restore_enabled, quick_access, placement| {
            let write_generation = generation.fetch_add(1, Ordering::AcqRel);
            handle.accepted_runtime(
                crate::session_lifecycle::DurableTransition::ViewSettingsChanged,
                crate::session_lifecycle::RuntimeSessionSnapshot {
                    window,
                    placement,
                    quick_access,
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
        });
    (
        Some(coordinator),
        Some(observer),
        Some(reset_observer),
        restore_enabled,
        quick_access,
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
            .record_event("shutdown_stage_started", &[("stage", "automation")]);
        if let Some(mut automation) = self.automation.take() {
            automation.shutdown();
        }
        let _ = self
            .diagnostics
            .record_event("shutdown_stage_finished", &[("stage", "automation")]);
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, AtomicUsize, Ordering},
        sync::{Arc, Mutex},
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    use abi_stable::std_types::{ROption, RVec};
    use explorer_extension_api::{
        IncrementalResultBatchV1, IncrementalResultEntryV1, JobContextV1, JobTerminalV1,
        PluginItemResultV1, PluginValueV1, SinkSubmitStatusV1,
    };
    use explorer_extension_host::{
        ExtensionJobAuthorityV1, ExtensionJobRuntimeRequestV1, ExtensionJobRuntimeV1,
        ExtensionJobUiIngressV1, ExtensionResultBufferConfigV1,
    };
    use explorer_model::{
        FileEntry, FileEntryMetadata, Generation, LocationDescriptor, RequestContext, ShellItemId,
        TabId, ViewMode,
    };
    use explorer_ui::ExtensionUiPumpPortV1 as _;

    use super::{
        ApplicationExtensionReadyProjectorV1, ApplicationExtensionUiPumpV1,
        PendingFolderSizeWorkV1, PendingSizeMapWorkV1, SIZE_MAP_REQUEST_QUEUE_CAP_V1,
        SafeModeIncidentOfferV1, SafeModeIncidentPortV1, cell_render_key,
        confirm_offered_safe_mode_incident_v1, confirm_presented_safe_mode_incident_v1,
        emit_post_commit_safe_mode_telemetry_v1, enqueue_folder_size_requests,
        enqueue_size_map_requests, measure_size_map_path, read_code_lines_file_bounded,
        should_restore_saved_tabs, size_map_node_id, size_map_render_key,
        take_folder_size_requests,
    };

    struct FakeSafeModePortV1 {
        denied: bool,
        confirmed: Mutex<Vec<u8>>,
    }

    #[test]
    fn cell_render_key_covers_request_theme_and_value_snapshot() {
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
            ..context
        };
        assert_ne!(baseline, cell_render_key(&changed_request));
        assert_ne!(baseline, cell_render_key(&changed_theme));
        assert_ne!(baseline, cell_render_key(&changed_value));
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
    fn size_map_render_key_covers_measurements_viewport_and_selection() {
        let color = explorer_extension_ui_api::CellColorV1::rgba(1, 2, 3, 255);
        let mut context = explorer_extension_ui_api::SizeMapRenderContextV1 {
            generation: 1,
            render_revision: 0,
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
        let baseline = size_map_render_key(&mut context, &request_context, &item_ids);
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
            size_map_render_key(&mut changed_viewport, &request_context, &item_ids)
        );
        assert_ne!(
            baseline,
            size_map_render_key(&mut changed_selection, &request_context, &item_ids)
        );
        assert_ne!(
            baseline,
            size_map_render_key(&mut changed_measurement, &request_context, &item_ids)
        );
    }

    #[test]
    fn size_map_render_key_rejects_cross_tab_cache_reuse_and_row_identity_reminting() {
        let color = explorer_extension_ui_api::CellColorV1::rgba(1, 2, 3, 255);
        let item_ids = vec![ShellItemId::from_provider_bytes([7_u8]).unwrap()];
        let mut first = explorer_extension_ui_api::SizeMapRenderContextV1 {
            generation: 1,
            render_revision: 0,
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
        let first_key = size_map_render_key(&mut first, &first_context, &item_ids);
        let first_revision = first.render_revision;
        let mut second = first.clone();
        let second_key = size_map_render_key(&mut second, &second_context, &item_ids);

        assert_ne!(first_key, second_key);
        assert_ne!(first_revision, second.render_revision);
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
    fn active_folder_size_request_is_not_queued_again_by_repeated_ui_submissions() {
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let request = explorer_ui::folder_size_column::FolderSizeRequestV1 {
            context,
            item_id: ShellItemId::from_provider_bytes(1_u64.to_le_bytes()).unwrap(),
            path: PathBuf::from(r"C:\fixture\slow"),
        };
        let mut pending = PendingFolderSizeWorkV1::default();

        enqueue_folder_size_requests(&mut pending, vec![request.clone()]);
        assert_eq!(
            take_folder_size_requests(&mut pending),
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
            };
        let mut pending = PendingFolderSizeWorkV1::default();

        enqueue_folder_size_requests(&mut pending, vec![request(first_context)]);
        let active = take_folder_size_requests(&mut pending);
        enqueue_folder_size_requests(&mut pending, vec![request(next_frame_context)]);

        assert_eq!(active.len(), 1);
        assert!(pending.requests.as_ref().is_none_or(Vec::is_empty));
        assert_eq!(pending.in_flight.len(), 1);
    }

    #[test]
    fn generation_change_keeps_active_scan_and_queues_new_generation_independently() {
        let tab = TabId::new();
        let old_request = explorer_ui::folder_size_column::FolderSizeRequestV1 {
            context: RequestContext::new(tab, Generation::new(1)),
            item_id: ShellItemId::from_provider_bytes(1_u64.to_le_bytes()).unwrap(),
            path: PathBuf::from(r"C:\fixture\old-slow"),
        };
        let new_request = explorer_ui::folder_size_column::FolderSizeRequestV1 {
            context: RequestContext::new(tab, Generation::new(2)),
            item_id: ShellItemId::from_provider_bytes(2_u64.to_le_bytes()).unwrap(),
            path: PathBuf::from(r"C:\fixture\new"),
        };
        let mut pending = PendingFolderSizeWorkV1::default();

        enqueue_folder_size_requests(&mut pending, vec![old_request.clone()]);
        assert_eq!(
            take_folder_size_requests(&mut pending),
            vec![old_request.clone()]
        );
        enqueue_folder_size_requests(&mut pending, vec![new_request.clone()]);

        assert!(
            pending
                .in_flight
                .contains(&super::FolderSizeWorkIdentityV1::from(&old_request)),
            "navigation must not cancel the scan that is already populating the exact cache"
        );
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
    fn size_map_scan_publishes_complete_recursive_total_after_initial_progress() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root: PathBuf = std::env::temp_dir().join(format!(
            "superexplorer-size-map-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("root.bin"), [0_u8; 7]).unwrap();
        fs::write(root.join("nested").join("child.bin"), [0_u8; 11]).unwrap();

        let result = measure_size_map_path(&root, 100, 16, || false);
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(result, (18, false, None, false));
    }
}

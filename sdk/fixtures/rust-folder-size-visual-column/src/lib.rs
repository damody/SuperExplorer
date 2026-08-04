//! Standalone Rust folder-size visual-column example using the public
//! Rust-first extension author API.
//!
//! The fixture is a minimal folder-size visual-column example. It proves that a
//! clean consumer can export the SDK root and implement ordinary Rust measure
//! and renderer traits without declaring FFI callbacks, GPUI types, or root
//! layout.

use std::{
    env, fs,
    io::Read as _,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult, RString, RVec},
};
use explorer_extension_api::{
    AbiErrorCodeV1, AbiErrorV1, ExtensionRegistrarImplementationV1, ExtensionRootModuleV1,
    ExtensionRootModuleV1_Ref, PluginMetadataV1, RegisteredContributionKindV1,
    RegisteredContributionV1, RegistrarOutputResultV1, RegistrarOutputV1, RegistrarRequestV1,
    RegistrationOutcomeV1, StableIdV1, StableSortValueKindV1, ABI_SCHEMA_V1,
    EXTENSION_ID_NAMESPACE_V1, ROOT_MODULE_CONTRACT_ID_V1, SDK_MAJOR_VERSION_V1,
};
use explorer_extension_ui_api::{
    CellColorV1, CellRenderContextV1, CellRenderPlanV1, FolderSizeMeasureRequestV1,
    FolderSizeMeasureResultV1, VisualColumnImplementationV1, VisualColumnObjectV1,
};

const MARKER_ENVIRONMENT_VARIABLE: &str = "RUST_FOLDER_SIZE_REGISTRAR_MARKER";
const CACHE_SCHEMA_VERSION: u32 = 1;
const CACHE_MAX_FILES: usize = 256;
const CACHE_MAX_RECORD_BYTES: u64 = 4 * 1024;
const CACHE_STABILITY_ATTEMPTS: usize = 3;
const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 1_001);
const PRIMARY_INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 1_002);

struct P0ConsumerRegistrar;

struct FolderSizeMeasureColumn;

impl VisualColumnImplementationV1 for FolderSizeMeasureColumn {
    fn measure_folder_size(
        &self,
        request: FolderSizeMeasureRequestV1,
    ) -> FolderSizeMeasureResultV1 {
        measure_folder_size_with_cache(&request, plugin_cache_directory().as_deref())
    }

    fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1 {
        CellRenderPlanV1::text_only("Folder size is measuring", context.theme.muted_foreground)
    }
}

struct FolderSizeRenderer;

impl VisualColumnImplementationV1 for FolderSizeRenderer {
    fn measure_folder_size(&self, _: FolderSizeMeasureRequestV1) -> FolderSizeMeasureResultV1 {
        FolderSizeMeasureResultV1::partial(0, "renderer does not measure folders")
    }

    fn render(&self, context: CellRenderContextV1) -> CellRenderPlanV1 {
        let CellRenderContextV1 {
            exact_bytes,
            aggregate,
            loading,
            error,
            selected,
            hovered,
            theme,
            settings,
            ..
        } = context;
        if loading {
            return CellRenderPlanV1::text_only("Calculating...", theme.muted_foreground);
        }
        if let ROption::RSome(error) = error {
            return CellRenderPlanV1::text_only(error, theme.muted_foreground);
        }
        let Some(exact_bytes) = exact_bytes.into_option() else {
            return CellRenderPlanV1::text_only("", theme.muted_foreground);
        };
        let mut plan =
            CellRenderPlanV1::text_only(compact_byte_label(exact_bytes), theme.foreground);
        plan.detail = RString::from("folder total");
        plan.bar_color = if selected {
            theme.selection_background
        } else if hovered {
            theme.accent
        } else {
            CellColorV1::rgba(theme.accent.red, theme.accent.green, theme.accent.blue, 128)
        };
        if settings.as_str() != "text-only" {
            if let ROption::RSome(aggregate) = aggregate {
                if let ROption::RSome(largest_bytes) = aggregate.largest_sibling_bytes {
                    if largest_bytes != 0 {
                        let fraction = (u128::from(exact_bytes.min(largest_bytes)) * 1_000_000)
                            / u128::from(largest_bytes);
                        plan.set_proportional_bar_millionths(
                            u32::try_from(fraction).unwrap_or(u32::MAX),
                        );
                    }
                }
            }
        }
        plan
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FolderSizeCacheKey {
    path_key: String,
    directory_modified_nanos: u128,
    max_entries: u32,
    max_depth: u16,
}

impl FolderSizeCacheKey {
    fn from_request(request: &FolderSizeMeasureRequestV1) -> Option<Self> {
        let path = Path::new(request.filesystem_path.as_str());
        if root_is_reparse_point(path) {
            return None;
        }
        let canonical = fs::canonicalize(path).ok()?;
        let metadata = fs::metadata(&canonical).ok()?;
        if !metadata.is_dir() {
            return None;
        }
        let directory_modified_nanos = metadata
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos();
        Some(Self {
            path_key: directory_identity(&canonical, &metadata),
            directory_modified_nanos,
            max_entries: request.max_entries.max(1),
            max_depth: request.max_depth,
        })
    }

    fn file_name(&self) -> String {
        // A deterministic private cache filename avoids exposing a user path
        // in the cache directory and is stable across process restarts.
        format!(
            "{:016x}.folder-size-cache",
            stable_path_hash(&self.path_key)
        )
    }

    fn identity_fingerprint(&self) -> String {
        // The cache record also carries the entire canonical path identity in
        // a lossless byte encoding. The filename hash is only an index; a
        // hash collision cannot return another directory's exact result.
        hex_encode(self.path_key.as_bytes())
    }
}

#[cfg(windows)]
fn root_is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt as _;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    })
}

#[cfg(not(windows))]
fn root_is_reparse_point(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

#[cfg(windows)]
fn directory_identity(canonical: &Path, _: &fs::Metadata) -> String {
    use std::{iter, os::windows::ffi::OsStrExt as _};

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    struct ByHandleFileInformation {
        file_attributes: u32,
        creation_time: FileTime,
        last_access_time: FileTime,
        last_write_time: FileTime,
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut std::ffi::c_void,
            creation: u32,
            flags: u32,
            template: isize,
        ) -> isize;
        fn GetFileInformationByHandle(
            handle: isize,
            information: *mut ByHandleFileInformation,
        ) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }
    const FILE_SHARE_READ: u32 = 0x1;
    const FILE_SHARE_WRITE: u32 = 0x2;
    const FILE_SHARE_DELETE: u32 = 0x4;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const INVALID_HANDLE_VALUE: isize = -1;

    let path = canonical
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            0,
        )
    };
    if handle != INVALID_HANDLE_VALUE {
        let mut information = std::mem::MaybeUninit::<ByHandleFileInformation>::uninit();
        let succeeded = unsafe { GetFileInformationByHandle(handle, information.as_mut_ptr()) };
        let _ = unsafe { CloseHandle(handle) };
        if succeeded != 0 {
            let information = unsafe { information.assume_init() };
            let file_index = (u64::from(information.file_index_high) << 32)
                | u64::from(information.file_index_low);
            return format!(
                "win-file:{:08x}:{file_index:016x}",
                information.volume_serial_number
            );
        }
    }
    let bytes = canonical
        .as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    format!("win-path:{}", hex_encode(&bytes))
}

#[cfg(not(windows))]
fn directory_identity(canonical: &Path, _: &fs::Metadata) -> String {
    canonical.to_string_lossy().into_owned()
}

fn stable_path_hash(path_key: &str) -> u64 {
    // FNV-1a is intentionally implemented locally rather than using
    // `DefaultHasher`, whose implementation is not a persistent format.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in path_key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FolderSizeCacheEntry {
    key: FolderSizeCacheKey,
    exact_bytes: u64,
}

impl FolderSizeCacheEntry {
    fn encode(&self) -> String {
        format!(
            "schema={CACHE_SCHEMA_VERSION}\npath_identity={}\nmodified={}\nmax_entries={}\nmax_depth={}\nexact_bytes={}\n",
            self.key.identity_fingerprint(),
            self.key.directory_modified_nanos,
            self.key.max_entries,
            self.key.max_depth,
            self.exact_bytes,
        )
    }

    fn decode(key: FolderSizeCacheKey, input: &str) -> Option<Self> {
        let mut schema = None;
        let mut path_identity = None;
        let mut modified = None;
        let mut max_entries = None;
        let mut max_depth = None;
        let mut exact_bytes = None;
        for line in input.lines() {
            let (name, value) = line.split_once('=')?;
            match name {
                "schema" => schema = value.parse::<u32>().ok(),
                "path_identity" => path_identity = Some(value),
                "modified" => modified = value.parse::<u128>().ok(),
                "max_entries" => max_entries = value.parse::<u32>().ok(),
                "max_depth" => max_depth = value.parse::<u16>().ok(),
                "exact_bytes" => exact_bytes = value.parse::<u64>().ok(),
                _ => return None,
            }
        }
        let expected_identity = key.identity_fingerprint();
        (schema == Some(CACHE_SCHEMA_VERSION)
            && path_identity == Some(expected_identity.as_str())
            && modified == Some(key.directory_modified_nanos)
            && max_entries == Some(key.max_entries)
            && max_depth == Some(key.max_depth))
        .then_some(Self {
            key,
            exact_bytes: exact_bytes?,
        })
    }
}

fn plugin_cache_directory() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .or_else(|| env::var_os("APPDATA"))
        .map(|root| {
            PathBuf::from(root)
                .join("RustGpuiExplorer")
                .join("plugins")
                .join("rust-folder-size-visual-column")
                .join("folder-size")
                .join("v1")
        })
}

fn read_cached_exact(cache_directory: Option<&Path>, key: &FolderSizeCacheKey) -> Option<u64> {
    let path = cache_directory?.join(key.file_name());
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > CACHE_MAX_RECORD_BYTES
    {
        return None;
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).ok()?);
    fs::File::open(path)
        .ok()?
        .take(CACHE_MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if u64::try_from(bytes.len()).ok()? > CACHE_MAX_RECORD_BYTES {
        return None;
    }
    let contents = std::str::from_utf8(&bytes).ok()?;
    FolderSizeCacheEntry::decode(key.clone(), contents).map(|entry| entry.exact_bytes)
}

fn prune_cache(cache_directory: &Path) {
    let Ok(entries) = fs::read_dir(cache_directory) else {
        return;
    };
    let mut entries = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with(".folder-size-cache")
        })
        .map(|entry| {
            let modified = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (modified, entry.path())
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(modified, _)| *modified);
    let excess = entries
        .len()
        .saturating_sub(CACHE_MAX_FILES.saturating_sub(1));
    for (_, path) in entries.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}

fn store_cached_exact(cache_directory: Option<&Path>, entry: &FolderSizeCacheEntry) {
    let Some(cache_directory) = cache_directory else {
        return;
    };
    if fs::create_dir_all(cache_directory).is_err() {
        return;
    }
    prune_cache(cache_directory);
    let destination = cache_directory.join(entry.key.file_name());
    let temporary = cache_directory.join(format!(
        ".{}.{}-{}.tmp",
        entry.key.file_name(),
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));
    if fs::write(&temporary, entry.encode()).is_ok() {
        // A corrupt or interrupted cache write is a cache miss on the next
        // call; completed records replace the old record atomically.
        if atomic_replace_cache_file(&temporary, &destination).is_err() {
            let _ = fs::remove_file(temporary);
        }
    }
}

#[cfg(windows)]
fn atomic_replace_cache_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    use std::{iter, os::windows::ffi::OsStrExt};

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }
    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(iter::once(0))
        .collect::<Vec<_>>();
    // Both paths are in the same plugin-owned cache directory, so this is an
    // atomic replacement on the local volume rather than a cross-volume move.
    if unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING,
        )
    } != 0
    {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(windows))]
fn atomic_replace_cache_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(temporary, destination)
}

fn measure_folder_size_with_cache(
    request: &FolderSizeMeasureRequestV1,
    cache_directory: Option<&Path>,
) -> FolderSizeMeasureResultV1 {
    if root_is_reparse_point(Path::new(request.filesystem_path.as_str())) {
        return FolderSizeMeasureResultV1::partial(
            0,
            "folder root symlinks and reparse points are not followed",
        );
    }
    for _ in 0..CACHE_STABILITY_ATTEMPTS {
        let Some(key) = FolderSizeCacheKey::from_request(request) else {
            let (exact_bytes, partial_error) = measure_path_bytes(request);
            return FolderSizeMeasureResultV1::partial(
                exact_bytes,
                partial_error.unwrap_or_else(|| RString::from("folder metadata is unavailable")),
            );
        };
        if let Some(exact_bytes) = read_cached_exact(cache_directory, &key) {
            return FolderSizeMeasureResultV1::complete(exact_bytes);
        }
        let (exact_bytes, partial_error) = measure_path_bytes(request);
        if let Some(error) = partial_error {
            // Partial, failed, and capped measurements are diagnostics only;
            // none may poison an exact persistent cache entry.
            return FolderSizeMeasureResultV1::partial(exact_bytes, error);
        }
        if FolderSizeCacheKey::from_request(request) == Some(key.clone()) {
            store_cached_exact(cache_directory, &FolderSizeCacheEntry { key, exact_bytes });
            return FolderSizeMeasureResultV1::complete(exact_bytes);
        }
        // The container changed during this background pass. Recompute rather
        // than publishing stale bytes as an exact value.
    }
    FolderSizeMeasureResultV1::partial(
        0,
        "folder changed repeatedly while calculating; retry is required",
    )
}

fn measure_path_bytes(request: &FolderSizeMeasureRequestV1) -> (u64, Option<RString>) {
    #[cfg(test)]
    if let Some(marker) = env::var_os("RUST_FOLDER_SIZE_SCAN_MARKER") {
        let _ = fs::write(marker, b"recursive scan entered");
    }
    let max_entries = request.max_entries.max(1);
    let mut visited = 0_u32;
    let mut total = 0_u64;
    let mut partial_error = None;
    let mut pending = vec![(
        Path::new(request.filesystem_path.as_str()).to_path_buf(),
        0_u16,
    )];

    while let Some((path, depth)) = pending.pop() {
        if visited >= max_entries {
            partial_error = Some(RString::from("folder measurement entry limit reached"));
            break;
        }
        visited = visited.saturating_add(1);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                partial_error.get_or_insert_with(|| RString::from(error.to_string()));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            let Some(next_total) = total.checked_add(metadata.len()) else {
                partial_error = Some(RString::from("folder measurement size overflow"));
                break;
            };
            total = next_total;
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        if depth >= request.max_depth {
            partial_error
                .get_or_insert_with(|| RString::from("folder measurement depth limit reached"));
            continue;
        }
        match fs::read_dir(&path) {
            Ok(entries) => {
                for entry in entries {
                    let queued = u32::try_from(pending.len()).unwrap_or(u32::MAX);
                    if visited.saturating_add(queued) >= max_entries {
                        partial_error =
                            Some(RString::from("folder measurement entry limit reached"));
                        break;
                    }
                    match entry {
                        Ok(entry) => pending.push((entry.path(), depth.saturating_add(1))),
                        Err(error) => {
                            partial_error.get_or_insert_with(|| RString::from(error.to_string()));
                        }
                    }
                }
            }
            Err(error) => {
                partial_error.get_or_insert_with(|| RString::from(error.to_string()));
            }
        }
    }
    (total, partial_error)
}

fn compact_byte_label(bytes: u64) -> RString {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        RString::from(format!("{bytes} B"))
    } else {
        RString::from(format!("{value:.1} {}", UNITS[unit]))
    }
}

impl ExtensionRegistrarImplementationV1 for P0ConsumerRegistrar {
    fn create() -> Self {
        Self
    }

    fn register(&self, request: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        if request.abi_schema != ABI_SCHEMA_V1 {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::SCHEMA_MISMATCH,
                ROOT_MODULE_CONTRACT_ID_V1,
                request.abi_schema.into_raw(),
            ));
        }
        if request.root_contract_id != ROOT_MODULE_CONTRACT_ID_V1 {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::UNSUPPORTED_ID,
                request.root_contract_id,
                0,
            ));
        }
        if request.sdk_major != SDK_MAJOR_VERSION_V1 {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::SDK_MAJOR_MISMATCH,
                ROOT_MODULE_CONTRACT_ID_V1,
                u32::from(request.sdk_major),
            ));
        }

        if let Err(error) = mark_callback_invocation() {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::REGISTRATION_REJECTED,
                ROOT_MODULE_CONTRACT_ID_V1,
                error.len() as u32,
            ));
        }

        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(2),
            // NativeExtensionLifecycleV1 admits only non-empty output whose
            // accepted count matches the contribution batch exactly. This is
            // deliberately bound to the fixture manifest's `column` feature
            // and its declared `abi` capability.
            contributions: RVec::from(vec![
                RegisteredContributionV1 {
                    feature_id: RString::from("column"),
                    contribution_id: RString::from("folder-size"),
                    kind: RegisteredContributionKindV1::COLUMN,
                    required_capabilities: RVec::from(vec![
                        RString::from("abi"),
                        RString::from("filesystem.read"),
                    ]),
                    interface_id: PRIMARY_INTERFACE_ID,
                    expected_sort: ROption::RSome(StableSortValueKindV1::U64),
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RSome(RString::from("folder-size-renderer")),
                    provider: ROption::RNone,
                    visual_column: ROption::RSome(VisualColumnObjectV1::new(
                        FolderSizeMeasureColumn,
                    )),
                    size_map_view: ROption::RNone,
                    batch_column_provider: ROption::RNone,
                },
                RegisteredContributionV1 {
                    feature_id: RString::from("column"),
                    contribution_id: RString::from("folder-size-renderer"),
                    kind: RegisteredContributionKindV1::GPUI_RENDERER,
                    required_capabilities: RVec::from(vec![RString::from("abi")]),
                    interface_id: PRIMARY_INTERFACE_ID,
                    expected_sort: ROption::RNone,
                    opaque_contract: ROption::RNone,
                    renderer_contribution_id: ROption::RNone,
                    provider: ROption::RNone,
                    visual_column: ROption::RSome(VisualColumnObjectV1::new(FolderSizeRenderer)),
                    size_map_view: ROption::RNone,
                    batch_column_provider: ROption::RNone,
                },
            ]),
        })
    }
}

fn marker_path() -> Option<PathBuf> {
    env::var_os(MARKER_ENVIRONMENT_VARIABLE).map(PathBuf::from)
}

fn mark_callback_invocation() -> Result<(), RString> {
    let Some(path) = marker_path() else {
        return Ok(());
    };
    fs::write(&path, b"rust folder-size visual column registrar invoked").map_err(|error| {
        RString::from(format!(
            "could not write Rust folder-size registrar marker {}: {error}",
            path.display()
        ))
    })
}

/// The sole ABI root module. `abi_stable` exports its fixed loader symbol;
/// semantic identity is data in [`ExtensionRootModuleV1`], never an
/// author-configurable manifest string.
#[export_root_module]
pub fn plugin_root() -> ExtensionRootModuleV1_Ref {
    ExtensionRootModuleV1::new::<P0ConsumerRegistrar>(
        PluginMetadataV1 {
            plugin_id: PLUGIN_ID,
            primary_interface_id: PRIMARY_INTERFACE_ID,
        },
        ROption::RNone,
    )
    .leak_into_prefix()
}

#[cfg(test)]
mod tests {
    use std::{
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    use explorer_extension_api::{registrar_request_v1, AbiSchemaIdV1, IdNamespaceV1};

    use super::*;

    #[test]
    fn root_is_the_fixed_public_v1_contract() {
        let root = plugin_root();

        assert_eq!(root.abi_schema(), ABI_SCHEMA_V1);
        assert_eq!(root.root_contract_id(), ROOT_MODULE_CONTRACT_ID_V1);
        assert_eq!(root.sdk_major(), SDK_MAJOR_VERSION_V1);
        assert_eq!(root.metadata().plugin_id, PLUGIN_ID);
        assert_eq!(root.ui_abi_fingerprint_sha256(), ROption::RNone);
    }

    #[test]
    fn mismatched_root_contract_is_rejected_before_marker_write() {
        let root = plugin_root();
        let registrar = root.create_registrar().create().into_result().unwrap();
        let request = RegistrarRequestV1 {
            root_contract_id: StableIdV1::new(IdNamespaceV1::new(0x1234, 1), 1),
            ..registrar_request_v1()
        };

        assert!(matches!(
            registrar.register(request).into_result(),
            Err(AbiErrorV1 {
                code: AbiErrorCodeV1::UNSUPPORTED_ID,
                ..
            })
        ));
    }

    #[test]
    fn matching_public_contract_calls_the_registrar() {
        let root = plugin_root();
        let registrar = root.create_registrar().create().into_result().unwrap();
        let result = registrar
            .register(registrar_request_v1())
            .into_result()
            .unwrap();

        assert_eq!(result.outcome, RegistrationOutcomeV1::accepted(2));
        assert_eq!(result.contributions.len(), 2);
        let contribution = &result.contributions[0];
        assert_eq!(contribution.feature_id, "column");
        assert_eq!(contribution.contribution_id, "folder-size");
        assert_eq!(contribution.kind, RegisteredContributionKindV1::COLUMN);
        assert_eq!(
            contribution.required_capabilities.as_slice(),
            ["abi", "filesystem.read"]
        );
        assert!(matches!(contribution.visual_column, ROption::RSome(_)));
        assert_eq!(
            result.contributions[1].contribution_id,
            "folder-size-renderer"
        );
    }

    #[test]
    fn folder_measurement_is_typed_and_renderer_plans_a_proportional_bar() {
        let measure = FolderSizeMeasureColumn.measure_folder_size(FolderSizeMeasureRequestV1 {
            filesystem_path: RString::from("Z:\\superexplorer-p0-missing-folder"),
            max_entries: 10_000,
            max_depth: 64,
            deadline_millis: 1_000,
        });
        assert!(measure.partial);

        let color = CellColorV1::rgba(1, 2, 3, 255);
        let context = CellRenderContextV1 {
            value: ROption::RSome(explorer_extension_api::PluginValueV1::integer(10)),
            exact_bytes: ROption::RSome(10),
            aggregate: ROption::RSome(explorer_extension_ui_api::CellAggregateV1 {
                largest_sibling_value: ROption::RSome(
                    explorer_extension_api::PluginValueV1::integer(20),
                ),
                largest_sibling_bytes: ROption::RSome(20),
            }),
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
            settings: RString::from("bar-and-text"),
            item_id: explorer_extension_api::StableIdV1::new(
                explorer_extension_api::EXTENSION_ID_NAMESPACE_V1,
                1,
            ),
            render_generation: 1,
            request_generation: 1,
        };
        let plan = FolderSizeRenderer.render(context.clone());
        assert_eq!(plan.proportional_bar_millionths, 500_000);
        let text_only = FolderSizeRenderer.render(CellRenderContextV1 {
            settings: RString::from("text-only"),
            ..context
        });
        assert_eq!(text_only.proportional_bar_millionths, 0);
    }

    #[test]
    fn schema_mismatch_is_typed() {
        let root = plugin_root();
        let registrar = root.create_registrar().create().into_result().unwrap();
        let request = RegistrarRequestV1 {
            abi_schema: AbiSchemaIdV1::new(0x5345, 2),
            ..registrar_request_v1()
        };

        assert!(matches!(
            registrar.register(request).into_result(),
            Err(AbiErrorV1 {
                code: AbiErrorCodeV1::SCHEMA_MISMATCH,
                ..
            })
        ));
    }

    fn temporary_directory(label: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let suffix = NEXT.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "rust-folder-size-visual-column-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn measurement_request(path: &Path, max_entries: u32) -> FolderSizeMeasureRequestV1 {
        FolderSizeMeasureRequestV1 {
            filesystem_path: path.to_string_lossy().into_owned().into(),
            max_entries,
            max_depth: 32,
            // A one-millisecond foreground hint must not terminate the
            // background calculation or prevent a complete cache write.
            deadline_millis: 1,
        }
    }

    #[test]
    fn completed_background_measurement_ignores_foreground_hint_and_reuses_cache() {
        let root = temporary_directory("measurement");
        let cache = temporary_directory("cache");
        let nested = root.join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(root.join("first.bin"), [1_u8; 7]).unwrap();
        fs::write(nested.join("second.bin"), [2_u8; 11]).unwrap();
        let request = measurement_request(&root, 100);

        let first = measure_folder_size_with_cache(&request, Some(&cache));
        assert_eq!(first, FolderSizeMeasureResultV1::complete(18));
        let key = FolderSizeCacheKey::from_request(&request).expect("directory identity");
        assert_eq!(read_cached_exact(Some(&cache), &key), Some(18));

        // The same metadata/settings key resolves before a second recursive
        // walk. An exact cache hit has no partial/error disguise.
        let second = measure_folder_size_with_cache(&request, Some(&cache));
        assert_eq!(second, FolderSizeMeasureResultV1::complete(18));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    #[ignore = "spawned only by the process-restart cache test"]
    fn cache_process_probe() {
        let root = PathBuf::from(std::env::var_os("RUST_FOLDER_SIZE_TEST_ROOT").unwrap());
        let cache = PathBuf::from(std::env::var_os("RUST_FOLDER_SIZE_TEST_CACHE").unwrap());
        let result = measure_folder_size_with_cache(&measurement_request(&root, 100), Some(&cache));
        assert_eq!(result, FolderSizeMeasureResultV1::complete(18));
    }

    #[test]
    fn exact_cache_is_reused_by_a_fresh_process_when_directory_mtime_is_unchanged() {
        let root = temporary_directory("restart-measurement");
        let cache = temporary_directory("restart-cache");
        let marker = cache.join("scan.marker");
        let nested = root.join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(root.join("first.bin"), [1_u8; 7]).unwrap();
        fs::write(nested.join("second.bin"), [2_u8; 11]).unwrap();

        let run_probe = || {
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "tests::cache_process_probe",
                    "--nocapture",
                ])
                .env("RUST_FOLDER_SIZE_TEST_ROOT", &root)
                .env("RUST_FOLDER_SIZE_TEST_CACHE", &cache)
                .env("RUST_FOLDER_SIZE_SCAN_MARKER", &marker)
                .status()
                .unwrap()
        };
        assert!(run_probe().success());
        assert!(
            marker.is_file(),
            "cold process must enter the recursive scan"
        );
        fs::remove_file(&marker).unwrap();
        assert!(run_probe().success());
        assert!(
            !marker.exists(),
            "fresh process must return the same-mtime exact cache before scanning"
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn cache_key_rejects_different_measurement_settings() {
        let root = temporary_directory("settings");
        let cache = temporary_directory("settings-cache");
        fs::write(root.join("entry.bin"), [7_u8; 3]).unwrap();
        let request = measurement_request(&root, 100);
        assert!(!measure_folder_size_with_cache(&request, Some(&cache)).partial);

        let changed_settings = measurement_request(&root, 101);
        let changed_key = FolderSizeCacheKey::from_request(&changed_settings).unwrap();
        assert_eq!(read_cached_exact(Some(&cache), &changed_key), None);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn cache_miss_follows_directory_modified_time_change() {
        let root = temporary_directory("mtime");
        let cache = temporary_directory("mtime-cache");
        fs::write(root.join("first.bin"), [1_u8; 3]).unwrap();
        let request = measurement_request(&root, 100);
        assert!(!measure_folder_size_with_cache(&request, Some(&cache)).partial);
        let original_key = FolderSizeCacheKey::from_request(&request).unwrap();

        // Creating a direct child advances the directory's modification
        // identity. A record for the prior identity cannot be reused.
        fs::write(root.join("second.bin"), [2_u8; 5]).unwrap();
        let changed_key = FolderSizeCacheKey::from_request(&request).unwrap();
        assert_ne!(
            changed_key.directory_modified_nanos,
            original_key.directory_modified_nanos
        );
        assert_eq!(read_cached_exact(Some(&cache), &changed_key), None);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(cache).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn cache_identity_rejects_a_directory_recreated_at_the_same_path() {
        let root = temporary_directory("recreated");
        let request = measurement_request(&root, 100);
        let original = FolderSizeCacheKey::from_request(&request).unwrap();

        fs::remove_dir(&root).unwrap();
        fs::create_dir(&root).unwrap();
        let recreated = FolderSizeCacheKey::from_request(&request).unwrap();
        assert_ne!(recreated.path_key, original.path_key);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_or_oversized_cache_is_a_miss_not_an_exact_value() {
        let root = temporary_directory("corrupt");
        let cache = temporary_directory("corrupt-cache");
        fs::write(root.join("entry.bin"), [1_u8; 3]).unwrap();
        let request = measurement_request(&root, 100);
        assert!(!measure_folder_size_with_cache(&request, Some(&cache)).partial);
        let key = FolderSizeCacheKey::from_request(&request).unwrap();
        fs::write(cache.join(key.file_name()), "not a cache record").unwrap();
        assert_eq!(read_cached_exact(Some(&cache), &key), None);
        fs::write(
            cache.join(key.file_name()),
            vec![b'x'; usize::try_from(CACHE_MAX_RECORD_BYTES + 1).unwrap()],
        )
        .unwrap();
        assert_eq!(read_cached_exact(Some(&cache), &key), None);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn partial_measurements_never_enter_the_exact_cache() {
        let cache = temporary_directory("partial-cache");
        let missing = cache.join("missing-folder");
        let result =
            measure_folder_size_with_cache(&measurement_request(&missing, 100), Some(&cache));
        assert!(result.partial);
        assert!(fs::read_dir(&cache).unwrap().flatten().all(|entry| !entry
            .file_name()
            .to_string_lossy()
            .ends_with(".folder-size-cache")));
        fs::remove_dir_all(cache).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn directory_symlink_root_is_partial_and_never_cached() {
        use std::os::windows::fs::symlink_dir;

        let target = temporary_directory("symlink-target");
        let parent = temporary_directory("symlink-parent");
        let cache = temporary_directory("symlink-cache");
        fs::write(target.join("payload.bin"), [3_u8; 9]).unwrap();
        let link = parent.join("linked-root");
        if symlink_dir(&target, &link).is_err() {
            fs::remove_dir_all(target).unwrap();
            fs::remove_dir_all(parent).unwrap();
            fs::remove_dir_all(cache).unwrap();
            return;
        }

        let result = measure_folder_size_with_cache(&measurement_request(&link, 100), Some(&cache));
        assert!(result.partial);
        assert!(fs::read_dir(&cache).unwrap().flatten().all(|entry| !entry
            .file_name()
            .to_string_lossy()
            .ends_with(".folder-size-cache")));

        fs::remove_dir_all(parent).unwrap();
        fs::remove_dir_all(target).unwrap();
        fs::remove_dir_all(cache).unwrap();
    }
}

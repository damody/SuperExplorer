#![allow(
    unsafe_code,
    reason = "the optional official Everything SDK exposes a C ABI"
)]

use std::path::{Path, PathBuf};

use explorer_model::{
    CancellationToken, FileEntry, FileEntryMetadata, LocationDescriptor, ShellItemId,
};
use explorer_search::{Comparison, Expr, PropertyKey, Value};
use libloading::Library;

#[cfg(test)]
static FORCE_UNAVAILABLE_FOR_TEST: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

type SetSearch = unsafe extern "system" fn(*const u16);
type SetDword = unsafe extern "system" fn(u32);
type Query = unsafe extern "system" fn(i32) -> i32;
type GetDword = unsafe extern "system" fn() -> u32;
type GetPath = unsafe extern "system" fn(u32, *mut u16, u32) -> u32;
type IsResult = unsafe extern "system" fn(u32) -> i32;
type Reset = unsafe extern "system" fn();
type GetResultSize = unsafe extern "system" fn(u32, *mut i64) -> i32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedFolderEntryV1 {
    pub path: PathBuf,
    pub bytes: u64,
    pub is_directory: bool,
}

pub(crate) struct EverythingProvider {
    _library: Library,
    set_search: SetSearch,
    set_max: SetDword,
    set_offset: SetDword,
    set_request_flags: SetDword,
    query: Query,
    get_num_results: GetDword,
    get_result_path: GetPath,
    is_folder_result: IsResult,
    get_result_size: GetResultSize,
    is_db_loaded: unsafe extern "system" fn() -> i32,
    get_target_machine: GetDword,
    get_last_error: GetDword,
    reset: Reset,
}

impl EverythingProvider {
    pub(crate) fn open_adjacent() -> Result<Self, String> {
        #[cfg(test)]
        if FORCE_UNAVAILABLE_FOR_TEST.load(std::sync::atomic::Ordering::Acquire) {
            return Err("Everything disabled by deterministic fallback test".to_owned());
        }
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let parent = executable
            .parent()
            .ok_or_else(|| "executable has no parent".to_owned())?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = parent.join("Everything64.dll");
        let canonical = path
            .canonicalize()
            .map_err(|_| "Everything SDK is not packaged beside the executable".to_owned())?;
        if canonical.parent() != Some(parent.as_path()) {
            return Err("Everything SDK path escaped the application directory".to_owned());
        }
        // SAFETY: the canonical path is application-owned; every copied symbol is validated before
        // the Library is stored for at least as long as those pointers.
        unsafe {
            let library = Library::new(&canonical).map_err(|error| error.to_string())?;
            macro_rules! symbol {
                ($name:literal, $ty:ty) => {
                    *library
                        .get::<$ty>(concat!($name, "\0").as_bytes())
                        .map_err(|error| error.to_string())?
                };
            }
            let provider = Self {
                set_search: symbol!("Everything_SetSearchW", SetSearch),
                set_max: symbol!("Everything_SetMax", SetDword),
                set_offset: symbol!("Everything_SetOffset", SetDword),
                set_request_flags: symbol!("Everything_SetRequestFlags", SetDword),
                query: symbol!("Everything_QueryW", Query),
                get_num_results: symbol!("Everything_GetNumResults", GetDword),
                get_result_path: symbol!("Everything_GetResultFullPathNameW", GetPath),
                is_folder_result: symbol!("Everything_IsFolderResult", IsResult),
                get_result_size: symbol!("Everything_GetResultSize", GetResultSize),
                is_db_loaded: symbol!("Everything_IsDBLoaded", unsafe extern "system" fn() -> i32),
                get_target_machine: symbol!("Everything_GetTargetMachine", GetDword),
                get_last_error: symbol!("Everything_GetLastError", GetDword),
                reset: symbol!("Everything_Reset", Reset),
                _library: library,
            };
            if (provider.is_db_loaded)() == 0 || (provider.get_target_machine)() != 2 {
                return Err(format!(
                    "Everything IPC unavailable ({})",
                    (provider.get_last_error)()
                ));
            }
            Ok(provider)
        }
    }

    pub(crate) fn query(
        &mut self,
        root: &Path,
        expression: &Expr,
        cancellation: &CancellationToken,
        deliver: impl FnMut(Vec<FileEntry>) -> Result<(), ()>,
    ) -> Result<(), String> {
        query_provider(self, root, expression, cancellation, deliver)
    }
}

/// Reads the same bounded path/size/kind record shape consumed by the shared
/// folder snapshot service. Results are validated against the live filesystem;
/// any stale, escaped, or reparse entry rejects the entire accelerated result.
pub fn query_folder_index(
    root: &Path,
    max_entries: usize,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<IndexedFolderEntryV1>, String> {
    let mut provider = EverythingProvider::open_adjacent()?;
    query_folder_index_provider(&mut provider, root, max_entries, cancelled)
}

fn query_folder_index_provider(
    provider: &mut impl EverythingApi,
    root: &Path,
    max_entries: usize,
    cancelled: impl Fn() -> bool,
) -> Result<Vec<IndexedFolderEntryV1>, String> {
    const PAGE: u32 = 4_096;
    const REQUEST_FULL_PATH_AND_SIZE: u32 = 0x0000_0004 | 0x0000_0010;
    let canonical_root = root
        .canonicalize()
        .map_err(|_| "Everything root is unavailable".to_owned())?;
    let search = format!("path:\"{}\"", escape(&canonical_root.to_string_lossy()));
    let wide = search.encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut offset = 0_u32;
    let mut output = Vec::new();
    loop {
        if cancelled() {
            return Err("cancelled".to_owned());
        }
        provider.reset();
        provider.set_search(&wide);
        provider.set_offset(offset);
        provider.set_max(PAGE);
        provider.set_request_flags(REQUEST_FULL_PATH_AND_SIZE);
        if !EverythingApi::query(provider) {
            return Err(format!(
                "Everything IPC query failed ({})",
                provider.last_error()
            ));
        }
        let count = provider.result_count();
        for index in 0..count {
            if output.len() >= max_entries {
                return Err("Everything result exceeds folder snapshot node limit".to_owned());
            }
            let path = provider
                .result_path(index)
                .ok_or_else(|| "Everything returned an invalid path".to_owned())?;
            if !path_within_scope(&path, &canonical_root) || path == canonical_root {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|_| "Everything result is stale".to_owned())?;
            if metadata.file_type().is_symlink() || metadata_is_reparse(&metadata) {
                return Err("Everything subtree contains a reparse point".to_owned());
            }
            let is_directory = provider.result_is_folder(index);
            if is_directory != metadata.is_dir() {
                return Err("Everything result kind is stale".to_owned());
            }
            let indexed_size = provider.result_size(index);
            let bytes = if is_directory { 0 } else { metadata.len() };
            if !is_directory && indexed_size != Some(bytes) {
                return Err("Everything result size is stale".to_owned());
            }
            output.push(IndexedFolderEntryV1 {
                path,
                bytes,
                is_directory,
            });
        }
        if count < PAGE {
            return Ok(output);
        }
        offset = offset.saturating_add(count);
    }
}

#[cfg(windows)]
fn metadata_is_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse(_: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
pub(crate) struct ForcedUnavailableGuard;

#[cfg(test)]
impl Drop for ForcedUnavailableGuard {
    fn drop(&mut self) {
        FORCE_UNAVAILABLE_FOR_TEST.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[cfg(test)]
pub(crate) fn force_unavailable_for_test() -> ForcedUnavailableGuard {
    assert!(
        !FORCE_UNAVAILABLE_FOR_TEST.swap(true, std::sync::atomic::Ordering::AcqRel),
        "Everything fallback test override must be serialized"
    );
    ForcedUnavailableGuard
}

trait EverythingApi {
    fn reset(&mut self);
    fn set_search(&mut self, search: &[u16]);
    fn set_offset(&mut self, offset: u32);
    fn set_max(&mut self, maximum: u32);
    fn set_request_flags(&mut self, flags: u32);
    fn query(&mut self) -> bool;
    fn result_count(&self) -> u32;
    fn result_path(&self, index: u32) -> Option<PathBuf>;
    fn result_is_folder(&self, index: u32) -> bool;
    fn result_size(&self, index: u32) -> Option<u64>;
    fn last_error(&self) -> u32;
}

impl EverythingApi for EverythingProvider {
    fn reset(&mut self) {
        // SAFETY: all function pointers were resolved from the live owned SDK library.
        unsafe { (self.reset)() }
    }
    fn set_search(&mut self, search: &[u16]) {
        // SAFETY: caller supplies a live NUL-terminated UTF-16 buffer.
        unsafe { (self.set_search)(search.as_ptr()) }
    }
    fn set_offset(&mut self, offset: u32) {
        unsafe { (self.set_offset)(offset) }
    }
    fn set_max(&mut self, maximum: u32) {
        unsafe { (self.set_max)(maximum) }
    }
    fn set_request_flags(&mut self, flags: u32) {
        unsafe { (self.set_request_flags)(flags) }
    }
    fn query(&mut self) -> bool {
        unsafe { (self.query)(1) != 0 }
    }
    fn result_count(&self) -> u32 {
        unsafe { (self.get_num_results)() }
    }
    fn result_path(&self, index: u32) -> Option<PathBuf> {
        let mut buffer = vec![0_u16; 32_768];
        let capacity = u32::try_from(buffer.len()).ok()?;
        let length =
            unsafe { (self.get_result_path)(index, buffer.as_mut_ptr(), capacity) as usize };
        (length > 0 && length < buffer.len())
            .then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..length])))
    }
    fn result_is_folder(&self, index: u32) -> bool {
        unsafe { (self.is_folder_result)(index) != 0 }
    }
    fn result_size(&self, index: u32) -> Option<u64> {
        let mut size = 0_i64;
        (unsafe { (self.get_result_size)(index, &raw mut size) != 0 } && size >= 0)
            .then_some(size as u64)
    }
    fn last_error(&self) -> u32 {
        unsafe { (self.get_last_error)() }
    }
}

fn query_provider(
    provider: &mut impl EverythingApi,
    root: &Path,
    expression: &Expr,
    cancellation: &CancellationToken,
    mut deliver: impl FnMut(Vec<FileEntry>) -> Result<(), ()>,
) -> Result<(), String> {
    const PAGE: u32 = 256;
    let search = format!(
        "path:\"{}\" <{}>",
        escape(&root.to_string_lossy()),
        render_expression(expression)
    );
    let wide: Vec<u16> = search.encode_utf16().chain([0]).collect();
    let mut offset = 0_u32;
    loop {
        if cancellation.is_cancelled() {
            return Err("cancelled".to_owned());
        }
        provider.reset();
        provider.set_search(&wide);
        provider.set_offset(offset);
        provider.set_max(PAGE);
        provider.set_request_flags(0x0000_0004);
        if !provider.query() {
            return Err(format!(
                "Everything IPC query failed ({})",
                provider.last_error()
            ));
        }
        let count = provider.result_count();
        if count == 0 {
            return Ok(());
        }
        let mut entries = Vec::with_capacity(count as usize);
        for index in 0..count {
            if cancellation.is_cancelled() {
                return Err("cancelled".to_owned());
            }
            let Some(path) = provider.result_path(index) else {
                continue;
            };
            if !path_within_scope(&path, root) {
                continue;
            }
            let Some(id) = ShellItemId::from_provider_bytes(
                path.to_string_lossy().to_lowercase().into_bytes(),
            ) else {
                continue;
            };
            let is_container = provider.result_is_folder(index);
            let filesystem_metadata = std::fs::symlink_metadata(&path).ok();
            let entry = FileEntry {
                id,
                display_name: path
                    .file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                location: LocationDescriptor::FileSystem(path),
                is_container,
                metadata: FileEntryMetadata {
                    size_bytes: filesystem_metadata
                        .as_ref()
                        .filter(|_| !is_container)
                        .map(std::fs::Metadata::len),
                    ..FileEntryMetadata::default()
                },
            };
            if explorer_search::matches_entry(expression, &entry) {
                entries.push(entry);
            }
        }
        if !entries.is_empty() {
            deliver(entries).map_err(|()| "result channel closed".to_owned())?;
        }
        if count < PAGE {
            return Ok(());
        }
        offset = offset.saturating_add(count);
    }
}

fn escape(value: &str) -> String {
    value.replace('"', "\\\"")
}

fn path_within_scope(path: &Path, root: &Path) -> bool {
    let normalize = |value: &Path| {
        value
            .to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    let path = normalize(path);
    let root = normalize(root);
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|suffix| suffix.starts_with('\\'))
}

fn render_expression(expression: &Expr) -> String {
    match expression {
        Expr::Text {
            value, glob: true, ..
        } => render_filename_glob(value),
        Expr::Text { value, .. } => format!("\"{}\"", escape(value)),
        Expr::Filter {
            key,
            comparison,
            value,
        } => {
            let property = match key {
                PropertyKey::Name => "name",
                PropertyKey::Type => "ext",
                PropertyKey::Size => "size",
                PropertyKey::DateModified => "dm",
            };
            let operator = match comparison {
                Comparison::Equal => "",
                Comparison::Greater => ">",
                Comparison::GreaterOrEqual => ">=",
                Comparison::Less => "<",
                Comparison::LessOrEqual => "<=",
            };
            let value = match value {
                Value::Text(value) => format!("\"{}\"", escape(value.trim_start_matches('.'))),
                Value::Size(value) => value.0.to_string(),
                Value::Date(value) => {
                    format!("{:04}-{:02}-{:02}", value.year, value.month, value.day)
                }
            };
            format!("{property}:{operator}{value}")
        }
        Expr::Not(inner) => format!("!<{}>", render_expression(inner)),
        Expr::And(left, right) => format!(
            "<{}> <{}>",
            render_expression(left),
            render_expression(right)
        ),
        Expr::Or(left, right) => format!(
            "<{}> | <{}>",
            render_expression(left),
            render_expression(right)
        ),
    }
}

fn render_filename_glob(pattern: &str) -> String {
    let mut output = String::with_capacity(pattern.len());
    let mut characters = pattern.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            match characters.next() {
                Some(escaped @ ('*' | '?' | '\\')) => push_everything_literal(&mut output, escaped),
                Some(other) => {
                    push_everything_literal(&mut output, '\\');
                    push_everything_literal(&mut output, other);
                }
                None => push_everything_literal(&mut output, '\\'),
            }
        } else if matches!(character, '*' | '?') {
            output.push(character);
        } else {
            push_everything_literal(&mut output, character);
        }
    }
    output
}

fn push_everything_literal(output: &mut String, character: char) {
    if character.is_alphanumeric() || !character.is_ascii() || matches!(character, '.' | '_' | '-')
    {
        output.push(character);
    } else {
        use std::fmt::Write as _;
        let _ = write!(output, "#x{:x}:", u32::from(character));
    }
}

#[cfg(test)]
mod tests {
    use super::{EverythingApi, escape, path_within_scope, query_provider, render_expression};

    #[derive(Default)]
    struct FakeApi {
        results: Vec<std::path::PathBuf>,
        offset: u32,
        maximum: u32,
        search: String,
        fail: bool,
        queries: usize,
    }

    impl EverythingApi for FakeApi {
        fn reset(&mut self) {}
        fn set_search(&mut self, search: &[u16]) {
            let end = search
                .iter()
                .position(|value| *value == 0)
                .unwrap_or(search.len());
            self.search = String::from_utf16_lossy(&search[..end]);
        }
        fn set_offset(&mut self, offset: u32) {
            self.offset = offset;
        }
        fn set_max(&mut self, maximum: u32) {
            self.maximum = maximum;
        }
        fn set_request_flags(&mut self, _flags: u32) {}
        fn query(&mut self) -> bool {
            self.queries += 1;
            !self.fail
        }
        fn result_count(&self) -> u32 {
            u32::try_from(self.results.len().saturating_sub(self.offset as usize))
                .unwrap_or(u32::MAX)
                .min(self.maximum)
        }
        fn result_path(&self, index: u32) -> Option<std::path::PathBuf> {
            self.results
                .get(self.offset.saturating_add(index) as usize)
                .cloned()
        }
        fn result_is_folder(&self, _index: u32) -> bool {
            false
        }
        fn result_size(&self, _index: u32) -> Option<u64> {
            Some(0)
        }
        fn last_error(&self) -> u32 {
            55
        }
    }
    #[test]
    fn scope_literals_escape_quotes() {
        assert_eq!(escape(r#"C:\a"b"#), r#"C:\a\"b"#);
    }
    #[test]
    fn parsed_expression_is_rendered_without_raw_structure_injection() {
        let expression = explorer_search::parse(r#"name:"a|b" type:txt size:>1KB"#).unwrap();
        let rendered = render_expression(&expression);
        assert!(rendered.contains("name:\"a|b\""));
        assert!(rendered.contains("ext:\"txt\""));
        assert!(rendered.contains("size:>1024"));
        assert!(!rendered.contains(['(', ')']));
        assert!(rendered.contains('<'));
    }

    #[test]
    fn filename_globs_preserve_wildcards_inside_a_name_candidate() {
        let expression = explorer_search::parse(r"foo*.rs").unwrap();
        assert_eq!(render_expression(&expression), "foo*.rs");

        let expression = explorer_search::parse(r"*a|b?.rs").unwrap();
        let rendered = render_expression(&expression);
        assert_eq!(rendered, "*a#x7c:b?.rs");

        let expression = explorer_search::parse(r"literal\*star?.rs").unwrap();
        assert_eq!(render_expression(&expression), "literal#x2a:star?.rs");
    }
    #[test]
    fn provider_results_are_rechecked_against_exact_scope_boundaries() {
        assert!(path_within_scope(
            std::path::Path::new(r"C:\foo\child.txt"),
            std::path::Path::new(r"c:\FOO")
        ));
        assert!(!path_within_scope(
            std::path::Path::new(r"C:\foobar\child.txt"),
            std::path::Path::new(r"C:\foo")
        ));
    }

    #[test]
    fn fake_provider_pages_cancels_escapes_and_keeps_failures_private() {
        let root = std::path::Path::new(r"C:\private-root");
        let mut provider = FakeApi {
            results: (0..300)
                .map(|index| {
                    std::path::PathBuf::from(format!("C:\\private-root\\a|\"b-{index}.txt"))
                })
                .chain([std::path::PathBuf::from(
                    r"C:\private-root\provider-false-positive.txt",
                )])
                .collect(),
            ..FakeApi::default()
        };
        let expression = explorer_search::parse(r#"name:"a|\"b""#).unwrap();
        let cancellation = explorer_model::CancellationToken::new();
        let mut batches = Vec::new();
        query_provider(&mut provider, root, &expression, &cancellation, |entries| {
            batches.push(entries.len());
            Ok(())
        })
        .unwrap();
        assert_eq!(batches, vec![256, 44]);
        assert_eq!(provider.queries, 2);
        assert!(provider.search.starts_with(r#"path:"C:\private-root""#));
        assert!(provider.search.contains(r#"name:"a|\"b""#));

        let mut provider = FakeApi {
            results: (0..300)
                .map(|index| {
                    std::path::PathBuf::from(format!("C:\\private-root\\a|\"b-cancel-{index}.txt"))
                })
                .collect(),
            ..FakeApi::default()
        };
        let cancellation = explorer_model::CancellationToken::new();
        let cancel_from_callback = cancellation.clone();
        let result = query_provider(&mut provider, root, &expression, &cancellation, |_| {
            cancel_from_callback.cancel();
            Ok(())
        });
        assert_eq!(result.unwrap_err(), "cancelled");
        assert_eq!(provider.queries, 1);

        let mut provider = FakeApi {
            fail: true,
            ..FakeApi::default()
        };
        let error = query_provider(
            &mut provider,
            root,
            &expression,
            &explorer_model::CancellationToken::new(),
            |_| Ok(()),
        )
        .unwrap_err();
        assert!(error.contains("55"));
        assert!(!error.contains("private-root"));
        assert!(!error.contains("a|"));
    }

    #[test]
    fn successful_zero_result_provider_is_not_an_error() {
        let root = std::path::Path::new(r"C:\private-root");
        let mut provider = FakeApi::default();
        let expression = explorer_search::parse("*.rs").unwrap();
        let mut deliveries = 0;
        let result = query_provider(
            &mut provider,
            root,
            &expression,
            &explorer_model::CancellationToken::new(),
            |_| {
                deliveries += 1;
                Ok(())
            },
        );
        assert_eq!(result, Ok(()));
        assert_eq!(provider.queries, 1);
        assert_eq!(deliveries, 0);
    }

    #[test]
    fn everything_candidates_use_the_shared_glob_post_filter() {
        let root = std::path::Path::new(r"C:\fixture");
        let mut provider = FakeApi {
            results: ["lib.rs", "MAIN.RS", "notes.txt", "nested.rs.bak"]
                .map(|name| root.join(name))
                .into(),
            ..FakeApi::default()
        };
        let expression = explorer_search::parse("*.rs").unwrap();
        let mut names = Vec::new();
        query_provider(
            &mut provider,
            root,
            &expression,
            &explorer_model::CancellationToken::new(),
            |entries| {
                names.extend(entries.into_iter().map(|entry| entry.display_name));
                Ok(())
            },
        )
        .unwrap();
        names.sort();
        assert_eq!(names, ["MAIN.RS", "lib.rs"]);
    }
}

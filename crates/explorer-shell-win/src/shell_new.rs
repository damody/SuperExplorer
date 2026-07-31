//! Bounded, data-only discovery of Explorer `ShellNew` registrations.
#![allow(
    unsafe_code,
    reason = "reading public per-user and merged class registrations requires Win32 registry calls"
)]

use std::{ffi::c_void, path::PathBuf};

use explorer_model::{ShellNewItemDescriptor, ShellNewItemRecipe};
use windows::{
    Win32::System::Registry::{
        HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, REG_VALUE_TYPE, RRF_RT_REG_BINARY,
        RRF_RT_REG_MULTI_SZ, RRF_RT_REG_SZ, RegGetValueW,
    },
    core::{HSTRING, PCWSTR},
};

const SHELL_NEW_LIST: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\Discardable\PostSetup\ShellNew";
const MAX_CLASSES: usize = 128;
const MAX_VALUE_BYTES: u32 = 64 * 1024;

/// Returns only recipes that can be executed without invoking arbitrary registry handlers.
pub fn registered_shell_new_items() -> Vec<ShellNewItemDescriptor> {
    catalog_from_source(&RegistryCatalogSource)
}

trait CatalogSource {
    fn classes(&self) -> Option<Vec<String>>;
    fn string(&self, key: &str, name: &str) -> Option<String>;
    fn binary(&self, key: &str, name: &str) -> Option<Vec<u8>>;
    fn value_exists(&self, key: &str, name: &str) -> bool;
}

struct RegistryCatalogSource;

impl CatalogSource for RegistryCatalogSource {
    fn classes(&self) -> Option<Vec<String>> {
        read_multi_string(HKEY_CURRENT_USER, SHELL_NEW_LIST, "Classes")
    }

    fn string(&self, key: &str, name: &str) -> Option<String> {
        read_string(HKEY_CLASSES_ROOT, key, name)
    }

    fn binary(&self, key: &str, name: &str) -> Option<Vec<u8>> {
        read_binary(HKEY_CLASSES_ROOT, key, name)
    }

    fn value_exists(&self, key: &str, name: &str) -> bool {
        registry_value_exists(HKEY_CLASSES_ROOT, key, name)
    }
}

fn catalog_from_source(source: &impl CatalogSource) -> Vec<ShellNewItemDescriptor> {
    let mut result = Vec::new();
    let Some(classes) = source.classes() else {
        return result;
    };
    for extension in classes
        .into_iter()
        .filter(|value| value.starts_with('.') && value.len() <= 32)
        .take(MAX_CLASSES)
    {
        let prog_id = source.string(&extension, "").unwrap_or_default();
        let mut candidates = vec![format!(r"{extension}\ShellNew")];
        if !prog_id.is_empty() {
            candidates.push(format!(r"{extension}\{prog_id}\ShellNew"));
            candidates.push(format!(r"{prog_id}\ShellNew"));
        }
        let descriptor = candidates.into_iter().find_map(|key| {
            let descriptor = safe_recipe(source, &key).map(|recipe| ShellNewItemDescriptor {
                stable_id: extension.to_ascii_lowercase(),
                display_name: display_name(source, &extension, &prog_id),
                extension: Some(extension.clone()),
                default_stem: format!("New {}", display_name(source, &extension, &prog_id)),
                recipe,
            })?;
            descriptor.validate().is_ok().then_some(descriptor)
        });
        if let Some(descriptor) = descriptor
            && !result.iter().any(|existing: &ShellNewItemDescriptor| {
                existing
                    .stable_id
                    .eq_ignore_ascii_case(&descriptor.stable_id)
            })
        {
            result.push(descriptor);
        }
    }
    result
}

/// Discovers the data-only `ShellNew` catalog on a short-lived Shell STA and returns only owned
/// descriptors. The UI thread never reads merged class registration or activates a handler.
pub fn registered_shell_new_items_in_worker() -> Vec<ShellNewItemDescriptor> {
    std::thread::Builder::new()
        .name("explorer-shell-new-catalog".to_owned())
        .spawn(|| {
            let Ok(_apartment) = crate::sta::ApartmentGuard::initialize() else {
                return Vec::new();
            };
            registered_shell_new_items()
        })
        .ok()
        .and_then(|worker| worker.join().ok())
        .unwrap_or_default()
}

fn safe_recipe(source: &impl CatalogSource, key: &str) -> Option<ShellNewItemRecipe> {
    if let Some(file_name) = source.string(key, "FileName") {
        let path = PathBuf::from(file_name);
        if path.is_absolute() && path.is_file() {
            return Some(ShellNewItemRecipe::TemplateFile(path));
        }
    }
    if let Some(data) = source.binary(key, "Data") {
        return Some(ShellNewItemRecipe::Data(data));
    }
    source
        .value_exists(key, "NullFile")
        .then_some(ShellNewItemRecipe::EmptyFile)
}

fn display_name(source: &impl CatalogSource, extension: &str, prog_id: &str) -> String {
    source
        .string(prog_id, "")
        .filter(|value| !value.starts_with('@') && !value.trim().is_empty())
        .unwrap_or_else(|| format!("{} File", extension.trim_start_matches('.').to_uppercase()))
}

fn read_string(
    root: windows::Win32::System::Registry::HKEY,
    key: &str,
    name: &str,
) -> Option<String> {
    let bytes = read_value(root, key, name, RRF_RT_REG_SZ)?;
    let words = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|word| *word != 0)
        .collect::<Vec<_>>();
    String::from_utf16(&words).ok()
}

fn read_multi_string(
    root: windows::Win32::System::Registry::HKEY,
    key: &str,
    name: &str,
) -> Option<Vec<String>> {
    let bytes = read_value(root, key, name, RRF_RT_REG_MULTI_SZ)?;
    let words = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    Some(
        words
            .split(|word| *word == 0)
            .filter(|part| !part.is_empty())
            .filter_map(|part| String::from_utf16(part).ok())
            .collect(),
    )
}

fn read_binary(
    root: windows::Win32::System::Registry::HKEY,
    key: &str,
    name: &str,
) -> Option<Vec<u8>> {
    read_value(root, key, name, RRF_RT_REG_BINARY)
}

fn registry_value_exists(
    root: windows::Win32::System::Registry::HKEY,
    key: &str,
    name: &str,
) -> bool {
    let key = HSTRING::from(key);
    let name = HSTRING::from(name);
    let mut size = 0_u32;
    unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            windows::Win32::System::Registry::RRF_RT_ANY,
            None,
            None,
            Some(&raw mut size),
        )
        .is_ok()
    }
}

fn read_value(
    root: windows::Win32::System::Registry::HKEY,
    key: &str,
    name: &str,
    flags: windows::Win32::System::Registry::REG_ROUTINE_FLAGS,
) -> Option<Vec<u8>> {
    let key = HSTRING::from(key);
    let name = HSTRING::from(name);
    let mut size = 0_u32;
    let mut kind = REG_VALUE_TYPE::default();
    let status = unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            flags,
            Some(&raw mut kind),
            None,
            Some(&raw mut size),
        )
    };
    if status.is_err() || size == 0 || size > MAX_VALUE_BYTES {
        return None;
    }
    let mut bytes = vec![0_u8; usize::try_from(size).ok()?];
    let status = unsafe {
        RegGetValueW(
            root,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            flags,
            Some(&raw mut kind),
            Some(bytes.as_mut_ptr().cast::<c_void>()),
            Some(&raw mut size),
        )
    };
    status.is_ok().then(|| {
        bytes.truncate(usize::try_from(size).unwrap_or(0));
        bytes
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    #[derive(Default)]
    struct FixtureCatalogSource {
        classes: Vec<String>,
        strings: HashMap<(String, String), String>,
        binaries: HashMap<(String, String), Vec<u8>>,
        values: HashSet<(String, String)>,
    }

    impl FixtureCatalogSource {
        fn string(mut self, key: &str, name: &str, value: &str) -> Self {
            self.strings
                .insert((key.to_owned(), name.to_owned()), value.to_owned());
            self
        }

        fn binary(mut self, key: &str, name: &str, value: Vec<u8>) -> Self {
            self.binaries
                .insert((key.to_owned(), name.to_owned()), value);
            self
        }

        fn value(mut self, key: &str, name: &str) -> Self {
            self.values.insert((key.to_owned(), name.to_owned()));
            self
        }
    }

    impl CatalogSource for FixtureCatalogSource {
        fn classes(&self) -> Option<Vec<String>> {
            Some(self.classes.clone())
        }

        fn string(&self, key: &str, name: &str) -> Option<String> {
            self.strings
                .get(&(key.to_owned(), name.to_owned()))
                .cloned()
        }

        fn binary(&self, key: &str, name: &str) -> Option<Vec<u8>> {
            self.binaries
                .get(&(key.to_owned(), name.to_owned()))
                .cloned()
        }

        fn value_exists(&self, key: &str, name: &str) -> bool {
            self.values.contains(&(key.to_owned(), name.to_owned()))
        }
    }

    #[test]
    fn live_catalog_is_bounded_owned_and_excludes_handler_only_entries() {
        let entries = registered_shell_new_items();
        assert!(entries.len() <= MAX_CLASSES);
        assert!(entries.iter().all(|entry| {
            entry
                .extension
                .as_deref()
                .is_some_and(|ext| ext.starts_with('.'))
                && matches!(
                    entry.recipe,
                    ShellNewItemRecipe::EmptyFile
                        | ShellNewItemRecipe::Data(_)
                        | ShellNewItemRecipe::TemplateFile(_)
                )
        }));
    }

    #[test]
    fn catalog_discovery_worker_returns_only_valid_owned_descriptors() {
        let entries = registered_shell_new_items_in_worker();
        assert!(entries.len() <= MAX_CLASSES);
        assert!(entries.iter().all(|entry| entry.validate().is_ok()));
    }

    #[test]
    fn registry_fixture_filters_handler_only_malformed_duplicate_and_unsafe_recipes() {
        let source = FixtureCatalogSource {
            classes: vec![
                ".txt".to_owned(),
                ".TXT".to_owned(),
                ".zip".to_owned(),
                ".handler".to_owned(),
                "missing-dot".to_owned(),
                format!(".{}", "x".repeat(40)),
                ".unsafe".to_owned(),
            ],
            ..FixtureCatalogSource::default()
        }
        .string(".txt", "", "txtfile")
        .string("txtfile", "", "Text Document")
        .value(r".txt\ShellNew", "NullFile")
        .binary(r".zip\ShellNew", "Data", vec![0x50, 0x4b, 0x05, 0x06])
        // A handler-only registration has no safe data-only recipe.
        .value(r".handler\ShellNew", "Handler")
        // Relative templates are rejected before they reach file operations.
        .string(r".unsafe\ShellNew", "FileName", r"relative\template.bin");

        let entries = catalog_from_source(&source);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].stable_id, ".txt");
        assert_eq!(entries[0].display_name, "Text Document");
        assert_eq!(entries[0].recipe, ShellNewItemRecipe::EmptyFile);
        assert_eq!(entries[1].stable_id, ".zip");
        assert_eq!(
            entries[1].recipe,
            ShellNewItemRecipe::Data(vec![0x50, 0x4b, 0x05, 0x06])
        );
    }
}

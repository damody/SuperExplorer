//! Completeness and plural spot checks for every `AppLocale` catalog.

use std::{collections::BTreeSet, fs, path::Path};

use explorer_i18n::{AppLocale, Catalog, FluentArgs};

const FILES: [&str; 8] = [
    "chrome.ftl",
    "menus.ftl",
    "dialogs.ftl",
    "status.ftl",
    "settings.ftl",
    "a11y.ftl",
    "formatting.ftl",
    "desktop.ftl",
];

fn locales_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("locales")
}

fn message_ids(ftl: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for line in ftl.lines() {
        if line.starts_with(' ') || line.starts_with('\t') || line.starts_with('#') {
            continue;
        }
        let Some((id, _)) = line.split_once('=') else {
            continue;
        };
        let id = id.trim();
        if id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
            && id.starts_with(|ch: char| ch.is_ascii_lowercase())
        {
            ids.insert(id.to_owned());
        }
    }
    ids
}

fn english_ids() -> BTreeSet<String> {
    let root = locales_root().join("en");
    let mut ids = BTreeSet::new();
    for name in FILES {
        let text = fs::read_to_string(root.join(name)).unwrap_or_else(|err| {
            panic!("read en/{name}: {err}");
        });
        ids.extend(message_ids(&text));
    }
    ids
}

#[test]
fn every_locale_matches_english_message_id_set() {
    let expected = english_ids();
    assert!(!expected.is_empty(), "english catalog must not be empty");
    assert!(
        expected.contains("search-in"),
        "english catalog must keep Task 1 search-in"
    );
    assert!(expected.contains("copy-items"));
    assert!(expected.contains("menu-copy"));
    assert!(expected.contains("file-size-unit-kb"));
    assert!(expected.contains("language-follow-windows"));
    assert!(expected.contains("language-auto"));

    for locale in AppLocale::ALL {
        let dir = locales_root().join(locale.bcp47());
        assert!(dir.is_dir(), "missing locale directory {}", locale.bcp47());
        let mut actual = BTreeSet::new();
        for name in FILES {
            let path = dir.join(name);
            assert!(path.is_file(), "missing {}", path.display());
            let text = fs::read_to_string(&path).unwrap_or_else(|err| {
                panic!("read {}: {err}", path.display());
            });
            actual.extend(message_ids(&text));
        }
        let missing: Vec<_> = expected.difference(&actual).cloned().collect();
        let extra: Vec<_> = actual.difference(&expected).cloned().collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "{} missing {:?} extra {:?}",
            locale.bcp47(),
            missing,
            extra
        );
    }
}

fn strip_isolates(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(*ch, '\u{2066}' | '\u{2067}' | '\u{2068}' | '\u{2069}'))
        .collect()
}

fn sample_args() -> FluentArgs<'static> {
    let mut args = FluentArgs::new();
    args.set("count", 2);
    args.set("selected", 1);
    args.set("action", "Copy");
    args.set("volume", 50);
    args.set("reason", "reason");
    args.set("pid", 1);
    args.set("folder", "Docs");
    args.set("hint", "hint");
    args.set("letter", "C");
    args.set("label", "Label");
    args.set("path", "C:\\tmp");
    args.set("name", "Name");
    args.set("column", "Name");
    args.set("icon", "*");
    args.set("kind", "File");
    args.set("size", "1 KB");
    args.set("columns", "Name");
    args.set("value", "1.0");
    args.set("unit", "KB");
    args.set("package", "pkg");
    args.set("bio", "bio");
    args.set("date", "2026-01-01");
    args.set("url", "https://example.invalid");
    args.set("mode", "0755");
    args.set("base", "photos");
    args.set("ordinal", 2);
    args.set("tool", "tool");
    args.set("from", "a");
    args.set("to", "b");
    args.set("source", "src");
    args.set("destination", "dst");
    args.set("summary", "summary");
    args.set("phase", "phase");
    args.set("completed", 1);
    args.set("total", 2);
    args.set("percent", 50);
    args.set("bytes", "1 KB");
    args.set("total-bytes", "2 KB");
    args.set("speed", "");
    args.set("error", "err");
    args.set("succeeded", 1);
    args.set("route", "route");
    args.set("code", 1);
    args.set("target", "device");
    args.set("free", "1 GB");
    args.set("limit", "1 GB");
    args.set("first", "a.txt");
    args.set("month", "January");
    args.set("year", 2026);
    args.set("day", 1);
    args.set("hour-12", "03");
    args.set("hour-24", "15");
    args.set("minute", "30");
    args.set("second", "23");
    args.set("period", "PM");
    args.set("host", "ftp.example");
    args.set("detail", "detail");
    args.set("operation", "copy");
    args.set("status", "failed");
    args.set("native", "");
    args
}

#[test]
fn catalog_lookup_is_non_empty_and_not_the_key() {
    let ids = english_ids();
    let args = sample_args();
    for locale in AppLocale::ALL {
        let catalog = Catalog::new(locale);
        for id in &ids {
            let value = strip_isolates(&catalog.t_args(id, &args));
            assert!(!value.is_empty(), "{} {id} resolved empty", locale.bcp47());
            assert_ne!(
                value,
                *id,
                "{} {id} fell back to the message id",
                locale.bcp47()
            );
        }
    }
}

fn copy_items(locale: AppLocale, count: i64) -> String {
    let catalog = Catalog::new(locale);
    let mut args = FluentArgs::new();
    args.set("count", count);
    strip_isolates(&catalog.t_args("copy-items", &args))
}

#[test]
fn copy_items_plural_spot_checks() {
    assert_eq!(copy_items(AppLocale::En, 1), "Copy 1 item");
    assert_eq!(copy_items(AppLocale::En, 2), "Copy 2 items");
    assert_eq!(copy_items(AppLocale::En, 5), "Copy 5 items");

    let ru1 = copy_items(AppLocale::Ru, 1);
    let ru2 = copy_items(AppLocale::Ru, 2);
    let ru5 = copy_items(AppLocale::Ru, 5);
    assert_eq!(ru1, "Копировать 1 элемент");
    assert_eq!(ru2, "Копировать 2 элемента");
    assert_eq!(ru5, "Копировать 5 элементов");
    assert_ne!(ru1, ru2);
    assert_ne!(ru2, ru5);

    let pl1 = copy_items(AppLocale::Pl, 1);
    let pl2 = copy_items(AppLocale::Pl, 2);
    let pl5 = copy_items(AppLocale::Pl, 5);
    assert_eq!(pl1, "Kopiuj 1 element");
    assert_eq!(pl2, "Kopiuj 2 elementy");
    assert_eq!(pl5, "Kopiuj 5 elementów");
    assert_ne!(pl1, pl2);
    assert_ne!(pl2, pl5);
}

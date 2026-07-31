use std::path::Path;

const SPLASH_PNG: &[u8] = include_bytes!("../assets/super-explorer-splash.png");
const APPLICATION_ICO: &[u8] = include_bytes!("../assets/super-explorer.ico");

#[test]
fn cargo_package_and_windows_binary_names_are_intentionally_distinct() {
    let manifest = include_str!("../Cargo.toml");
    assert!(manifest.contains("name = \"explorer-app\""));
    assert!(manifest.contains("[[bin]]"));
    assert!(manifest.contains("name = \"SuperExplorer\""));
    assert!(manifest.contains("path = \"src/main.rs\""));
}

#[test]
fn version_resource_exposes_superexplorer_identity() {
    let resource = include_str!("../app.rc");
    for expected in [
        "VALUE \"FileDescription\", \"SuperExplorer\\0\"",
        "VALUE \"InternalName\", \"SuperExplorer\\0\"",
        "VALUE \"OriginalFilename\", \"SuperExplorer.exe\\0\"",
        "VALUE \"ProductName\", \"SuperExplorer\\0\"",
    ] {
        assert!(
            resource.contains(expected),
            "missing VERSIONINFO: {expected}"
        );
    }
}

#[test]
fn windows_resource_embeds_the_superexplorer_icon() {
    let resource = include_str!("../app.rc");
    let build_script = include_str!("../build.rs");
    assert!(resource.contains("1 ICON \"assets/super-explorer.ico\""));
    assert!(build_script.contains("cargo:rerun-if-changed=assets/super-explorer.ico"));
}

#[test]
fn cargo_exposes_the_renamed_binary_to_integration_tests() {
    let binary = Path::new(env!("CARGO_BIN_EXE_SuperExplorer"));
    assert_eq!(
        binary.file_name().and_then(|name| name.to_str()),
        Some("SuperExplorer.exe")
    );
}

#[test]
fn production_consumers_do_not_require_the_legacy_binary_name() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let files = [
        "build/build_install.lua",
        "installer/SuperExplorer.nsi",
        "scripts/finalize_windows_artifact.ps1",
        "uitest/manifest.json",
    ];
    for relative in files {
        let path = workspace.join(relative);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(
            !text.contains("explorer-app.exe"),
            "legacy production binary reference remains in {}",
            path.display()
        );
    }
}

#[test]
fn product_rename_preserves_all_persisted_data_roots() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    for relative in [
        "crates/explorer-app/src/session_store.rs",
        "crates/explorer-common/src/diagnostics.rs",
        "crates/explorer-search/src/local_index.rs",
        "crates/explorer-shell-win/src/icon_disk_cache.rs",
        "crates/explorer-shell-win/src/thumbnail.rs",
    ] {
        let path = workspace.join(relative);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(
            text.contains("RustGpuiExplorer"),
            "persisted compatibility root changed in {}",
            path.display()
        );
    }
}

#[test]
fn splash_asset_contains_transparent_background_and_opaque_logo_pixels() {
    let image = image::load_from_memory_with_format(SPLASH_PNG, image::ImageFormat::Png)
        .expect("decode splash PNG")
        .into_rgba8();
    assert_eq!(image.dimensions(), (1_175, 296));

    let mut transparent = 0_usize;
    let mut opaque = 0_usize;
    let mut yellow = 0_usize;
    let mut dark = 0_usize;
    for pixel in image.pixels() {
        let [red, green, blue, alpha] = pixel.0;
        transparent += usize::from(alpha == 0);
        opaque += usize::from(alpha == u8::MAX);
        yellow += usize::from(alpha > 240 && red > 210 && green > 130 && blue < 80);
        dark += usize::from(alpha > 240 && red < 80 && green < 80 && blue < 100);
    }
    assert!(
        transparent > 100_000,
        "splash background is not transparent"
    );
    assert!(opaque > 50_000, "splash logo is not opaque");
    assert!(yellow > 10_000, "splash lost the yellow logo palette");
    assert!(dark > 10_000, "splash lost the dark logo palette");
}

#[test]
fn application_icon_packages_all_required_windows_sizes() {
    assert_eq!(&APPLICATION_ICO[..4], &[0, 0, 1, 0]);
    let count = usize::from(u16::from_le_bytes([APPLICATION_ICO[4], APPLICATION_ICO[5]]));
    assert_eq!(count, 7);

    let mut sizes = Vec::with_capacity(count);
    for index in 0..count {
        let entry = 6 + index * 16;
        let width = match APPLICATION_ICO[entry] {
            0 => 256,
            value => u16::from(value),
        };
        let height = match APPLICATION_ICO[entry + 1] {
            0 => 256,
            value => u16::from(value),
        };
        assert_eq!(width, height, "ICO frame must be square");
        sizes.push(width);
    }
    sizes.sort_unstable();
    assert_eq!(sizes, [16, 24, 32, 48, 64, 128, 256]);
}

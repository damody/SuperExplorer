# Fluent asset inventory and provenance

- Package: `@fluentui/svg-icons@1.1.339`
- npm license metadata: MIT
- Upstream repository: `https://github.com/microsoft/fluentui-system-icons`
- Package archive: `fluentui-svg-icons-1.1.339.tgz`
- npm shasum: `21f32209487eb4507ff7af574bea0941b7ca2229`
- npm integrity: SHA-512 value emitted by `npm pack` and retained in the command transcript.
- Selection: 24 official 20px SVGs; 17 Color variants and 7 exact filled fallbacks.
- Runtime network/dependency: none. The selected SVGs are compiled into `ExplorerAssets` through `include_bytes!`.
- Complete upstream-path and SHA-256 mapping: `crates/explorer-ui/assets/remote-file/fluent-color/NOTICE.md`.
- License copy: `crates/explorer-ui/assets/remote-file/fluent-color/LICENSE`.

The seven official monochrome fallbacks are PDF, archive, script, Word, presentation, font, and disk image. Their default upstream paint is converted to `currentColor` at asset load; all Color variants retain original fills and gradients.


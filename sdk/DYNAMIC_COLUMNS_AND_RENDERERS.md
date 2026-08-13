# Dynamic columns and renderers

SuperExplorer's V1 Rust SDK keeps the author-facing API in ordinary Rust and
uses SDK-owned `abi_stable` adapters at the DLL boundary. Start with the clean
consumer in `sdk/fixtures/rust-folder-size-visual-column`.

## Column contract

Register a `ColumnDescriptorV1` with a package-local ID, typed value kind,
width bounds, alignment, applicability, cost, stable sort kind and provider
contribution. The host supplies the package namespace, so persisted IDs are
`extension:<package-id>:<local-id>` and cannot collide across packages.

An optional aggregate descriptor declares its dependency and maximum result
count. Aggregate calls are bounded and generation-scoped; partial, stale or
over-limit results are rejected. An optional renderer descriptor must accept
the column's value kind.

## Folder admission limits

A batch data-column contribution may set `folder_admission` on its
`RegisteredContributionV1`. `max_file_count` and `max_folder_count` are
optional inclusive `u64` limits; omitted values are unlimited, zero is valid,
and declaring both requires both limits to pass. These limits apply only to
folder items. Ordinary file items keep the existing dispatch path.

The host obtains exact File Count and Folder Count values from MFT Service and
evaluates the policy before creating or dispatching folder work. Pending,
partial, unavailable, stale, or over-limit facts never enter the extension
callback. Extensions receive no MFT handle and cannot override the admission
decision. The Rust and Lua Code Lines examples demonstrate
`max_file_count = 999`.

## Renderer contract

Implement `VisualColumnImplementationV1`. The render method receives only an
immutable `CellRenderContextV1`: typed value, exact bytes, aggregate,
loading/error state, selection/hover state, DPI, theme facade, settings,
host-attested item ID, render revision and request generation. It returns a
data-only `CellRenderPlanV1`; host-owned GPUI code paints the label and
proportional bar.

Render callbacks run on a bounded host worker. They must be pure and fast: do
not enumerate files, parse content, access the network, block, or retain host
state. Put I/O in the provider/job callback and return owned values. A panic is
caught at the SDK adapter and faults that renderer instead of unwinding into
the host. The host ignores plans whose complete snapshot revision is stale.

## Build and inspect the sample

From the repository root:

```powershell
cargo test --manifest-path sdk/fixtures/rust-folder-size-visual-column/Cargo.toml --locked --offline
powershell -NoProfile -File scripts/build-plugin.ps1 -PluginRoot sdk/fixtures/rust-folder-size-visual-column
```

The sample's `Cargo.toml` uses exact SDK versions with first-party relative
paths and registry versions for third-party crates. It is intentionally not a
root workspace member.

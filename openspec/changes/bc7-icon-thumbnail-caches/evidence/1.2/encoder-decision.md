# G-ENCODER decision

The selected adapter is `intel_tex_2` 0.5.0, locked in `Cargo.lock`. Its packaged manifest identifies the upstream repository as `Traverse-Research/intel-tex-rs-2` and the redistribution license as `MIT/Apache-2.0`; both license files are present in the local registry package. The adapter accepts bounded RGBA slices plus explicit dimensions/stride and returns complete BC7 block rows.

Representative locked test output:

`fixtures=5 logical_rgba_bytes=42044 padded_rgba_bytes=43840 bc7_bytes=10960 logical_ratio=0.2607 padded_ratio=0.2500 elapsed_us=4628 peak_staging_bytes=43520`

The fixtures cover 16, 20, 24, and 32 pixel icons, an odd 127x65 thumbnail, transparent alpha, opaque data, and high-contrast patterns. Repeated encoding is byte deterministic. Odd logical dimensions explain why the ratio against unpadded pixels exceeds 25%; the GPU/container ratio against the required padded RGBA allocation is exactly 25%.

Malformed stride/length and excessive dimensions fail before encoder invocation. RAII accounting returns active encoder and staging counters when encoding exits. A deterministic four-worker barrier test observes four simultaneous encoder guards and 4,096 aggregate staging bytes, verifies peak counters, then proves active encoder and staging counters return to their exact starting values. The six-test focused codec suite passes locked/offline. Source hash: `5AB8302169DAF6EA8100356F9E58297DCC312BD16999A26EF9D824316F6B3388`.

With Rust 1.97.1 on `x86_64-pc-windows-msvc`, both Debug and Release `explorer-shell-win` builds pass locked/offline with `RUSTFLAGS=-C target-cpu=x86-64`. This constrains code generation to the minimum x86-64 CPU baseline instead of using host-specific RTX workstation CPU features. The dependency and bounded adapter are accepted for continued implementation, but production writers remain deny-by-default until the independent visual/performance release gates pass.

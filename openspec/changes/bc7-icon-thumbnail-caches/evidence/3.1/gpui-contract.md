# G-GPUI-CONTRACT evidence

GPUI owns an immutable renderer-neutral `CompressedRaster` descriptor and `RenderImage::new_bc7_srgb` constructor. It carries logical dimensions, padded block geometry, row pitch, owner kind, and immutable block bytes without Shell paths or provider identities. Existing RGBA `RenderImage` admission remains unchanged.

`cargo test --manifest-path vendor/gpui-ce/crates/gpui/Cargo.toml bc7 --locked --offline` passed `bc7_render_image_preserves_logical_size_and_complete_block_rows`. The Windows crate also compiled its locked offline BC7 test target.

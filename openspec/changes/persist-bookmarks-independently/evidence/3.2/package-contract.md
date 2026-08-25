# Package preservation evidence

Command: `cargo test -p explorer-app --test product_identity`

Result: exit 0; 10 passed, 0 failed. `installer_preserves_independent_bookmark_store` binds the stable Rust compatibility constant to the NSIS source, requires an explicit uninstall/reinstall preservation declaration, and rejects recursive LocalAppData or bookmark delete targets. The NSIS source now reports bookmark preservation during uninstall.

This focused source/compile test is the repository package contract selected by task 3.2.3; it compiles all tested application targets and verifies installer text without performing an externally visible machine installation.

## 1. Canonical Shell Resolution

- [x] 1.1 Derive a canonical `FileSystem` descriptor from non-empty `SIGDN_FILESYSPATH` after Shell resolution.
- [x] 1.2 Preserve input filesystem descriptors and non-filesystem namespace descriptors when no path is available.
- [x] 1.3 Keep the original bound PIDL/folder for enumeration while publishing only the canonical metadata descriptor.

## 2. Address and Navigation Contracts

- [x] 2.1 Add model coverage proving canonical filesystem history produces complete editable address text and resubmits correctly.
- [x] 2.2 Add real Shell coverage for Documents, Downloads, Desktop, Pictures, Music, and Videos path canonicalization.
- [x] 2.3 Add fallback coverage for Home, This PC, Recycle Bin, Network, and Libraries without fabricated paths.

## 3. Headful and Verification

- [x] 3.1 Add or extend a headful UIA case that opens a filesystem-backed known folder and validates selected address text and resubmission.
- [x] 3.2 Map the new requirements in the UITEST manifest and run its coverage validation.
- [x] 3.3 Run format, clippy, focused model/Shell tests, the headful address test, and strict OpenSpec validation.

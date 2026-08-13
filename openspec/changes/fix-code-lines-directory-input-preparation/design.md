## Context

The application serializes directory contents into the existing `SECLDIR1` stream consumed by both official Code Lines providers. Collection currently uses the 64 MiB batch ceiling, while `HostInputStreamSourceV1::from_host_snapshot` rejects any single source above 8 MiB. Unsupported binary files are packed before the provider filters them, so binary-heavy repositories can hit this mismatch despite containing only a few MiB of source code.

The extension security boundary requires providers to consume Host-attested streams rather than open arbitrary paths. The fix must preserve that boundary and the current wire format.

## Goals / Non-Goals

**Goals:**

- Make every successfully collected directory snapshot valid for a single Host input stream.
- Exclude files that official tokei providers cannot classify before reading and packing them.
- Preserve truthful unsupported/unavailable outcomes and isolate preparation failures per row.
- Prove the behavior against binary-heavy fixtures and `D:\code\file_explorer`.

**Non-Goals:**

- Raising ABI or memory limits.
- Giving extensions direct filesystem access.
- Changing File Count admission, MFT queries, provider output semantics, or localization.

## Decisions

### Filter by the official tokei path classifier in the Host

The Host will call `tokei::LanguageType::from_path(relative_path, default_config)` before reading a directory child. This matches the Rust provider's classification boundary and avoids copying known unsupported payloads. Relative paths are used so extension-based and filename-based recognition remains intact.

Alternatives rejected:

- Packing all files and raising the limit increases memory exposure and still wastes I/O.
- Binary sniffing alone cannot reliably identify unsupported text formats.
- Direct provider path access violates the sealed stream authority model.

### Use the single-source limit for directory packs

The pack builder will use `MAX_HOST_INPUT_STREAM_SOURCE_BYTES_V1`, including magic and record framing. `Ok(Some(pack))` therefore means `from_host_snapshot` can accept the byte length. Empty supported-source sets and oversized packs return `Ok(None)` and are presented as unsupported.

### Prepare rows independently

Batch preparation will retain each successfully prepared `(request, input, cache admission)` row and emit a terminal error only for the specific request that cannot be canonicalized, named, or converted into a Host stream. A bad row will no longer cause `inputs.len() != requests.len()` to fail the entire batch.

### Use the locked tokei parser in the all-language provider

The Lua Code Lines provider will replace its small hand-written extension table and line classifier with `tokei::LanguageType::from_path` plus `parse_from_slice`. This keeps its accepted-language set and code/comment/blank semantics identical to the Host snapshot filter and ensures the all-language total cannot omit a language that the Host deliberately packed.

## Risks / Trade-offs

- [Host and provider tokei versions could diverge] → Both use the workspace-locked dependency; tests assert representative recognition and the existing wire format remains provider-validated.
- [A source-only repository can still exceed 8 MiB] → Return truthful `Unsupported source`; do not raise the security/memory bound silently.
- [Files can change between classification and reading] → Keep current bounded snapshot behavior and skip disappearing child entries; generation/cache identity continues to prevent stale publication.
- [Lua provider may recognize a file differently] → The Host uses the shared official tokei classifier and tests both official providers' directory format behavior.

## Migration Plan

No persisted schema or ABI migration is required. Deploy the Host change with the existing providers. Rollback is a normal code rollback because the `SECLDIR1` representation is unchanged.

## Open Questions

None. The user authorized the bounded Host-side repair and requested no further confirmation.

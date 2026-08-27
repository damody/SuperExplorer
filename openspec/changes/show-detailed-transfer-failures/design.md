## Context

`explorer-remote::TransferEngine` already returns one `TransferItemOutcome` per item, including logical source and destination, but catches `anyhow::Error` and replaces it with `transfer failed`. `explorer-app::remote_service` then replaces that diagnostic again with `A file could not be transferred.`. OperationCenter can render per-item outcomes, yet currently receives neither the actual cause nor a formatted route/stage.

The change crosses the remote engine, application service and UI. Diagnostics may originate from local I/O, ADB stderr or SFTP libraries and therefore must be useful without exposing credentials.

## Goals / Non-Goals

**Goals:**

- Preserve the actual error chain and identify the failed transfer stage.
- Associate every failed result with its logical source and computed target.
- Produce safe user-facing diagnostics for Local, ADB and SFTP routes.
- Render distinct, actionable partial-outcome rows.
- Verify the reported screenshot case with focused tests.

**Non-Goals:**

- Changing copy/move execution, conflict decisions, staging quotas or retry policy.
- Displaying internal credential objects, passwords, tokens or raw temporary paths.
- Removing the existing five-row partial-outcome limit or eight-second lifecycle.
- Running the full regression suite.

## Decisions

### Structured stage plus safe diagnostic in the transfer layer

`TransferResult::Failed` and `Partial` will carry a stage and diagnostic rather than a context-free string. The transfer engine owns the stage because it knows whether failure occurred during conflict inspection, local copy, source download, destination upload or move-source deletion. The returned `TransferItemOutcome` remains the source of logical source and destination.

Alternative: infer stages in `remote_service`. Rejected because the service only observes the final result and cannot reliably identify where `copy()` failed.

### Preserve error chains at the point of failure

Every caught `anyhow::Error` is formatted with its context chain and stored after sanitization. Existing `with_context` calls provide file-operation detail; new contexts will be added around provider download/upload and conflict inspection where necessary. Cancellation remains `Cancelled`, not a failure diagnostic.

Alternative: rely on logs. Rejected because the user needs the reason in the operation UI and may not have console access.

### Sanitize before crossing into UI state

A small pure sanitizer will remove URI userinfo and common credential assignments (`password`, `token`, `secret`) from provider diagnostic strings. Logical locations use their existing canonical editable representation, which contains provider authority/path rather than stored credentials. The sanitizer will fall back to `未提供底層錯誤` for empty diagnostics.

Sanitization occurs before constructing `ExplorerError`, so unsafe text is not retained in OperationCenter state. Internal logs are outside this UI contract and are not expanded by this change.

### UI formats each outcome using its descriptors

OperationCenter will format each partial outcome as `失敗｜來源 → 目的｜階段｜原因`, using `OperationItemOutcome.item`, `.destination`, and the safe error message. The destination filename is appended when the outcome destination is a parent directory and an item name is available. Native codes are shown only when present. Partial move failures use the same route and identify source deletion as the stage.

## Risks / Trade-offs

- **[Provider messages vary in quality]** → Preserve the full available error chain and supply an explicit fallback instead of a misleading generic sentence.
- **[Over-redaction can hide useful text]** → Restrict sanitization to URI userinfo and well-known credential keys; cover safe ADB/SFTP messages in tests.
- **[Destination descriptor can represent a parent]** → Compute display target from destination plus item name without mutating the execution request.
- **[Long rows can increase message height]** → Retain the existing five-row cap and allow normal text layout; the eight-second lifetime remains unchanged.

## Migration Plan

No persisted-data or public API migration is required. Update the internal transfer-result fields and all constructors/tests in one change. Rollback consists of reverting these internal changes; successful transfer semantics remain compatible.

## Open Questions

None. The approved design fixes scope and allows missing provider details to use the explicit fallback.

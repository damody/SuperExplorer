## Context

The installed Windows MFT Service already returns `logical_bytes`, `file_count`, `directory_count`, generation, and partial state for a recursive folder aggregate. The application currently projects only bytes into the folder-size runtime, while built-in Details IDs stop at the existing eight columns and Code Lines dispatches folder work without first consulting aggregate size. The extension manifest and public column contract have no declarative folder-admission policy.

The approved source design is `docs/superpowers/specs/2026-08-13-mft-directory-count-columns-design.md`. Existing uncommitted MFT, cache, extension, and UI work in the repository must be preserved and integrated rather than reverted. The implementation remains Windows/NTFS/MFT-only for directory facts and adds no privilege or external dependency.

## Goals / Non-Goals

**Goals:**

- Expose recursive file count and root-excluded recursive folder count as default-hidden built-in Details columns.
- Reuse one exact MFT aggregate result across the two columns and every dependent data-column contribution.
- Let any extension package's folder-applicable data-column contribution declare inclusive file/folder count limits.
- Enforce admission in the Host before extension job creation or callback dispatch.
- Keep Code Lines undispatched until exact File Count is known and admit only counts from 0 through 999.
- Preserve generation safety, cancellation, layout persistence, manifest compatibility, and exact integer sorting.

**Non-Goals:**

- User-editable limit controls or localization resource work.
- Direct-child-only counts, filesystem fallback scanning, non-NTFS support, or reparse-point traversal.
- Applying count policies to commands, views, file-only columns, or ordinary file Code Lines work.
- Changing MFT Service installation, privileges, cache budgets, or folder-size byte semantics.

## Decisions

### One Host-owned directory-facts projection

Add a narrow `DirectoryFactsV1` value containing recursive `file_count`, descendant `folder_count`, MFT generation, and exact availability. Extend the application-owned folder aggregate request/result boundary so one MFT query can publish bytes and counts without a second IPC request. A coordinator deduplicates by canonical folder identity, volume identity, and MFT generation, and fans the same result out to built-in columns and admission consumers.

Alternative: create a second count-only service/runtime. Rejected because it would duplicate canonicalization, cache admission, cancellation, invalidation, and IPC work already owned by folder aggregates.

### Exact MFT-only semantics

MFT `directory_count` includes the queried root, so the Host derives `folder_count` with `saturating_sub(1)`. A reparse-point directory contributes its own directory entry but the MFT parent hierarchy never descends through its target. Unsupported, virtual, unavailable, partial, cancelled, or stale results produce no exact facts. There is no recursive filesystem or Everything fallback.

Alternative: display and admit partial lower bounds. Rejected because a lower bound can incorrectly admit expensive work when the unknown true count exceeds the policy.

### First-class built-in IDs and shared value state

Add `ColumnId::FileCount` and `ColumnId::FolderCount`, stable IDs `builtin:file_count` and `builtin:folder_count`, container/integer/background-aggregate descriptors, and exhaustive rendering/sorting/layout/session support. Existing sessions restore both as hidden. Restoring an extensible layout reconciles it with the current built-in descriptor list: saved entries retain their order, width, and visibility, while each missing current built-in is appended once using its default width and hidden visibility. Files and ineligible containers render blank; unavailable folder facts render `—`. Only exact values enter numeric sorting.

Alternative: implement the counts as bundled extension columns. Rejected because they are Host/MFT facts used as an authority boundary and must remain available independently of extension installation or enablement.

### Declarative manifest admission

Extend validated data-column contribution metadata with optional `max_file_count` and `max_folder_count` unsigned integers. Missing means unlimited; both present means AND; zero is valid. Validation accepts the policy only on column contributions whose descriptor can apply to containers. Existing manifests with no fields retain current behavior.

The Host resolves exact facts before creating or dispatching a folder job. Pending/stale facts keep the cell in dependency-pending state; unavailable facts and exceeded limits create Host-owned terminal display states without invoking extension code. The extension cannot override admission.

Alternative: expose an MFT query handle to each plugin. Rejected because it expands authority, duplicates calls, makes behavior provider-dependent, and cannot guarantee pre-dispatch protection.

### Visibility owns directory-facts demand

The two built-in count columns are the only authority that enables directory-facts acquisition. Showing either column immediately submits deduplicated requests for the current filesystem-folder rows; restored-visible layouts and later navigation do the same. When both columns are hidden, count admission metadata alone creates no MFT request. Hiding the last visible count column cancels obsolete count-only presentation work and suppresses new requests.

An extension limit may consume a fact only while its corresponding built-in column is visible. Both columns must be visible for a contribution that declares both limits. A shared aggregate may contain both values, but a hidden value is not admission-enabled and cached hidden facts cannot bypass this rule.

Alternative: let enabled limited extensions acquire facts while the columns are hidden. Rejected because the user explicitly requires no count query when the count columns are not displayed.

### Code Lines uses the generic gate

Both official Code Lines package contributions declare `max_file_count = 999`. Folder cells display `等待 File Count…` while visible File Count facts are pending, `File Count 超過限制，因此未啟動` when the exact count is at least 1000, and `依賴 File Count，因此未啟動` when File Count is hidden or exact facts cannot be obtained. File rows bypass the folder gate. Hidden File Count presentation suppresses both dependency acquisition and folder dispatch.

Alternative: special-case the threshold only in `ApplicationCodeLinesRuntimeV1`. Rejected because future extensions would not receive the requested reusable protection and manifest validation could not describe the behavior.

### Invalidation and stale-result control

Requests and admission decisions carry the active tab/request generation plus the MFT generation. Navigation, refresh, watcher invalidation, contribution disable, or a newer MFT generation cancels or invalidates pending admission. Old facts and Code Lines results cannot repopulate current UI. One exact fact can satisfy multiple enabled contributions in the same current context.

## Data Flow

1. Shell enumeration publishes the visible entries and filesystem paths.
2. The UI identifies visible container rows only when File Count or Folder Count is visible; extension admission metadata alone creates no directory-facts demand.
3. The application coordinator deduplicates requests and asks the MFT-backed folder aggregate service.
4. MFT Service returns a generation-tagged complete aggregate or an unavailable/partial outcome.
5. The Host projects exact counts, updates both built-in value maps, and evaluates every dependent contribution policy.
6. Admitted jobs enter the existing bounded extension scheduler; rejected jobs receive Host-owned display state without callback dispatch.
7. Rendering and sorting consume context-scoped exact values only.

## Security, Performance, and Observability

- Raw MFT access remains in the installed service; plugins receive neither paths to the index nor MFT handles.
- Manifest values are bounded `u64` JSON integers and validated before registration.
- Deduplication prevents multiplying MFT queries by visible columns or enabled extensions.
- No count request or sort comparison performs filesystem I/O on the GPUI thread.
- Tests and tracing distinguish pending, unavailable, over-limit, admitted, stale-discarded, and callback-dispatched outcomes.

## Risks / Trade-offs

- [Relevant files already contain uncommitted changes] → Inspect each diff before editing, keep changes focused, and never reset or overwrite unrelated work.
- [Adding built-in enum variants creates many exhaustive-match edits] → Compile model/UI targets early and add stable-ID/session tests before wiring runtime behavior.
- [MFT hierarchy represents reparse entries without target traversal] → Count the entry once, retain target non-traversal, and cover this established topology behavior in focused tests.
- [Folder-size cache freshness differs from MFT generation freshness] → Key admission on the MFT generation and never admit a timestamp-only cached count without matching exact generation.
- [A hidden count column could still cause background MFT work] → Derive directory-facts demand only from count-column visibility and test zero submission with both columns hidden.
- [A cached hidden fact could accidentally admit Code Lines] → Gate admission on corresponding column visibility before consulting cached facts and assert zero folder dispatch while hidden.
- [Public fixture metadata changes can desynchronize package tooling] → Update manifests, exact tooling validators, generated bundle inventory, and isolated fixture tests together.
- [A persisted extensible layout predates new built-in IDs] → Reconcile missing built-ins during session-to-runtime conversion and test the exact eight-column persisted shape; registry-only registration is insufficient because the chooser iterates layout entries.

## Migration and Rollback

1. Add model identities, descriptors, persistence migration that reconciles missing current built-ins, and unit tests.
2. Project exact counts from the MFT aggregate through a shared coordinator and wire built-in display/sort state.
3. Add manifest/public contract policy and validation while preserving missing-field compatibility.
4. Gate extension job submission and add Code Lines metadata/status behavior.
5. Run focused crate tests, workspace checks, fixture/package validation, and headful evidence.

Rollback is a source rollback. Older binaries ignore no persisted new visible columns they cannot parse only if session decoding retains unknown stable IDs; therefore migration tests must prove safe retention/fallback. Existing extension packages remain valid because the policy fields are optional. No service data migration or uninstall action is required.

## Evidence-driven Corrections

- **A — task refinement:** task splitting, order, exact commands, or evidence paths may change without changing approved behavior, contracts, thresholds, or gates.
- **B — design/spec correction:** an implementation discovery within approved scope requires affected artifacts and tasks to be updated, strict validation rerun, and dependent evidence marked stale.
- **C — material change:** changing recursive semantics, the 1000-file boundary, MFT-only behavior, public contract, privilege boundary, or required validation needs user approval.

No blocking gate or threshold may be weakened silently.

## Open Questions

None. The approved design fixes count semantics, MFT-only sourcing, admission ownership, threshold behavior, status text, compatibility, and excluded scope.

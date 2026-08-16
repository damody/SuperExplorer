## 1. Prerequisites and Activation Gates

### 1.1 Independent cache ownership baseline

**目的：** Freeze a verified independent icon/thumbnail cache and Host telemetry baseline before changing representation.  
**輸入：** Approved independent-cache design and change; current model/UI/Shell tests.  
**產出：** Baseline contract report, build hashes, passing evidence links, compatibility decision.  
**依賴：** `independent-cache-budgets-telemetry-webp` implementation and required gates.  
**Owner／Wave：** Primary agent / Wave 1.  
**Gate／Evidence：** G-BASELINE (blocking); `openspec/changes/bc7-icon-thumbnail-caches/evidence/1.1/`.  
**完成門檻：** Independent settings, ownership, telemetry, MFT/extension reporting, lifecycle, Release build, UTIT, and screenshots pass with immutable evidence references.

- [x] 1.1.1 Verify every apply-required artifact and blocking gate of `independent-cache-budgets-telemetry-webp` is complete.
- [x] 1.1.2 Record icon and thumbnail ownership, limits, reporters, cache roots, and current public/internal contracts.
- [x] 1.1.3 Run the affected locked offline baseline suites and record executable, dependency, and source hashes.
- [ ] 1.1.4 Capture baseline CPU working set, disk usage, first-display latency, cache-hit latency, upload bytes, and frame-time metrics.
- [ ] 1.1.5 Write G-BASELINE evidence records for tasks 1.1.1 through 1.1.4.

### 1.2 BC7 encoder dependency gate

**目的：** Select and prove one bounded production BC7 encoder before any writer is activated.  
**輸入：** Locked Rust toolchain; Windows targets; alpha/sRGB fixtures; license policy.  
**產出：** Decision record, locked dependency update or in-tree adapter, raw build/quality/performance evidence.  
**依賴：** 1.1 baseline.  
**Owner／Wave：** Primary agent / Wave 1.  
**Gate／Evidence：** G-ENCODER (blocking); `openspec/changes/bc7-icon-thumbnail-caches/evidence/1.2/`.  
**完成門檻：** Encoder has approved provenance/license, locked Windows Release build, bounded API, alpha/sRGB correctness, deterministic dimensions, and acceptable fixture cost; otherwise BC7 writers remain disabled.

- [x] 1.2.1 Inventory viable BC7 encoders and document CPU features, API bounds, maintenance, license, and redistribution properties.
- [x] 1.2.2 Implement a spike adapter that encodes representative icon, thumbnail, alpha, high-contrast, and odd-dimension fixtures.
- [x] 1.2.3 Verify block dimensions, alpha, sRGB interpretation, deterministic output shape, and 25-percent RGBA storage ratio.
- [x] 1.2.4 Verify locked offline debug and Release Windows builds on the minimum supported CPU/runtime assumptions.
- [x] 1.2.5 Measure encode latency, peak staging bytes, concurrency behavior, and malformed-input rejection.
- [x] 1.2.6 Record the G-ENCODER accept/reject decision and evidence for tasks 1.2.1 through 1.2.5.

## 2. BC7 Container and Host Cache Pipeline

### 2.1 Versioned BC7 container codec

**目的：** Implement a fail-closed private container whose payload can be uploaded as complete BC7 UNORM block rows.  
**輸入：** Passing G-ENCODER; approved header contract and Host invalidation identity.  
**產出：** Focused codec/container module, fixtures, parser/serializer tests.  
**依賴：** 1.2.  
**Owner／Wave：** Primary agent / Wave 2.  
**Gate／Evidence：** G-CONTAINER; `openspec/changes/bc7-icon-thumbnail-caches/evidence/2.1/`.  
**完成門檻：** Valid icon/thumbnail entries round-trip; every required corruption/overflow class rejects before payload-sized allocation; format is deterministic and documented.

- [x] 2.1.1 Define fixed magic, schema, endianness, content kind, format, dimensions, pitch, length, invalidation identity, and checksum fields.
- [x] 2.1.2 Implement checked 4x4 block geometry, edge padding, logical bounds, pitch, payload, and metadata byte accounting.
- [x] 2.1.3 Implement bounded serialization and parsing without UI, filesystem, provider, or D3D dependencies.
- [x] 2.1.4 Add golden fixtures for icons, thumbnails, odd dimensions, alpha, and maximum accepted boundaries.
- [x] 2.1.5 Add rejection tests for magic, schema, kind, format, zero/excessive dimensions, padding, pitch, length, trailing data, checksum, and overflow.
- [x] 2.1.6 Write G-CONTAINER evidence records for tasks 2.1.1 through 2.1.5.

### 2.2 Atomic disk persistence and migration

**目的：** Publish BC7 entries atomically in independent roots and transition from WebP without bulk startup work.  
**輸入：** 2.1 codec; current icon/thumbnail disk cache and quota implementation.  
**產出：** `.bc7cache` paths, atomic writer/reader, scoped cleanup, disk stats, migration tests.  
**依賴：** 2.1.  
**Owner／Wave：** Primary agent / Wave 2.  
**Gate／Evidence：** G-DISK; `openspec/changes/bc7-icon-thumbnail-caches/evidence/2.2/`.  
**完成門檻：** Concurrent writers leave one valid entry; icon/thumbnail quotas remain isolated; WebP is a lazy miss; cleanup stays inside registered non-symlink roots.

- [x] 2.2.1 Replace production icon and thumbnail cache extensions/schema with separate `.bc7cache` namespaces.
- [x] 2.2.2 Implement same-directory temporary write, flush/close, atomic replacement, and failed-temporary cleanup.
- [x] 2.2.3 Implement validated reads and stale-source rejection before admitting entries to memory or GPU queues.
- [x] 2.2.4 Update independent disk quota, usage, access metadata, entry count, hit, miss, corruption, and cleanup accounting.
- [x] 2.2.5 Implement bounded lazy WebP miss/removal without startup migration, root escape, or symlink traversal.
- [x] 2.2.6 Add concurrent writer, interrupted write, read/write race, quota isolation, legacy miss, and scoped cleanup tests.
- [ ] 2.2.7 Remove the WebP dependency only after repository-wide production/test consumers are absent and locked builds pass.
- [x] 2.2.8 Write G-DISK evidence records for tasks 2.2.1 through 2.2.7.

### 2.3 Bounded conversion and memory caches

**目的：** Convert cold provider RGBA once and maintain independent byte-bounded BC7 memory caches.  
**輸入：** 1.2 encoder; 2.1 container; independent cache ownership baseline.  
**產出：** Job registry, limits, icon/thumbnail LRU stores, telemetry, lifecycle tests.  
**依賴：** 1.1, 1.2, 2.1.  
**Owner／Wave：** Primary agent / Wave 2.  
**Gate／Evidence：** G-HOST-PIPELINE; `openspec/changes/bc7-icon-thumbnail-caches/evidence/2.3/`.  
**完成門檻：** Duplicate jobs single-flight; all queue/staging/output bounds hold; stale/cancelled work cannot publish; defaults remain icon 32 MiB and thumbnail 128 MiB independently.

- [x] 2.3.1 Define a cache-kind-aware conversion key containing source identity, presentation size, generation, and format schema.
- [x] 2.3.2 Implement bounded background scheduling with single-flight deduplication, cancellation, and generation checks.
- [x] 2.3.3 Enforce queue, concurrency, per-entry dimensions, staging bytes, output bytes, and timeout limits.
- [x] 2.3.4 Implement separate icon and thumbnail BC7 memory LRUs with immediate same-kind eviction on limit reduction.
- [x] 2.3.5 Release provider RGBA and compression staging buffers after acknowledged persistence/upload ownership transfer.
- [x] 2.3.6 Expose bounded queue, staging, LRU bytes/limits/counts, hit/miss, encode, stale, cancel, and error telemetry.
- [x] 2.3.7 Add duplicate, overload, oversized, cancellation, stale generation, eviction, sibling-isolation, and buffer-release tests.
- [x] 2.3.8 Write G-HOST-PIPELINE evidence records for tasks 2.3.1 through 2.3.7.

## 3. GPUI D3D11 Compressed Rendering

### 3.1 Renderer-neutral compressed raster contract

**目的：** Add an internal GPUI image representation that preserves logical sizing without forcing other backends to understand D3D formats.  
**輸入：** Current GPUI image/atlas contracts; 2.1 validated block layout.  
**產出：** Compressed raster types/lifecycle, Windows integration seam, compatibility tests.  
**依賴：** 2.1.  
**Owner／Wave：** Primary agent / Wave 3.  
**Gate／Evidence：** G-GPUI-CONTRACT; `openspec/changes/bc7-icon-thumbnail-caches/evidence/3.1/`.  
**完成門檻：** Non-Windows/default paths remain source-compatible; logical dimensions and lifetime are explicit; filesystem and Shell identities do not enter GPUI.

- [x] 3.1.1 Map current GPUI image ownership, atlas admission, draw preparation, resource lifetime, and device-loss seams.
- [x] 3.1.2 Define the renderer-neutral immutable compressed-raster descriptor and opaque resource lifecycle.
- [x] 3.1.3 Route eligible SuperExplorer images to the compressed seam while retaining unchanged RGBA atlas admission.
- [x] 3.1.4 Add compile-time and contract tests for logical dimensions, lifetime, unsupported backends, and existing image callers.
- [x] 3.1.5 Write G-GPUI-CONTRACT evidence records for tasks 3.1.1 through 3.1.4.

### 3.2 D3D11 BC7 resources and direct upload

**目的：** Render validated blocks through immutable BC7 UNORM textures with exact byte accounting.  
**輸入：** 3.1 contract; D3D11 device/context; 2.1 block layout.  
**產出：** Capability query, texture/SRV creation, upload path, GPU LRU, renderer tests.  
**依賴：** 3.1.  
**Owner／Wave：** Primary agent / Wave 3.  
**Gate／Evidence：** G-D3D11; `openspec/changes/bc7-icon-thumbnail-caches/evidence/3.2/`.  
**完成門檻：** Supported cache hits create/sample BC7 UNORM resources using validated pitch, logical UVs exclude padding, actual GPU bytes obey kind-specific LRU limits, and no hit decodes/recompresses.

- [x] 3.2.1 Implement explicit adapter capability detection for BC7 2D shader sampling and record capability state.
- [x] 3.2.2 Implement validated `DXGI_FORMAT_BC7_UNORM` texture and shader-resource-view creation matching GPUI's existing polychrome sampling contract.
- [x] 3.2.3 Implement complete block-row upload with logical UV bounds and no RGBA intermediate on a warm hit.
- [ ] 3.2.4 Implement independent icon and thumbnail GPU byte LRUs, promotions, immediate limit reduction, and release acknowledgement.
- [ ] 3.2.5 Add descriptor, pitch, odd-size UV, direct-upload instrumentation, eviction, and resource-release tests.
- [ ] 3.2.6 Write G-D3D11 evidence records for tasks 3.2.1 through 3.2.5.

### 3.3 Fallback and device recovery

**目的：** Preserve correct images and navigation whenever the compressed path cannot complete.  
**輸入：** 3.1/3.2 renderer paths; Shell providers; feature gates.  
**產出：** Fallback state machine, device-loss recovery, bounded diagnostics, recovery tests.  
**依賴：** 3.1, 3.2.  
**Owner／Wave：** Primary agent / Wave 3.  
**Gate／Evidence：** G-FALLBACK; `openspec/changes/bc7-icon-thumbnail-caches/evidence/3.3/`.  
**完成門檻：** Unsupported, corrupt, stale, failed, cancelled, and device-loss cases display provider-backed RGBA; late handles never publish; diagnostics remain bounded.

- [x] 3.3.1 Implement provider-backed RGBA fallback without adding a BC7-to-RGBA decoder dependency.
- [x] 3.3.2 Implement compressed GPU handle invalidation and visible-item reconstruction after device loss.
- [x] 3.3.3 Prevent cancelled/stale uploads and recovered old-device handles from replacing current images.
- [ ] 3.3.4 Add unsupported-adapter, validation, texture, SRV, upload, cancellation, stale, and device-loss tests.
- [ ] 3.3.5 Verify fallback navigation, selection, logical sizing, alpha, and error isolation through integration tests.
- [ ] 3.3.6 Write G-FALLBACK evidence records for tasks 3.3.1 through 3.3.5.

## 4. Settings, Telemetry, and Product Integration

### 4.1 Independent controls and Host telemetry

**目的：** Apply and display independent BC7 memory/disk/GPU policies without exposing renderer or plugin-owned mutable state.  
**輸入：** Independent settings/telemetry baseline; 2.2/2.3/3.2 reporters.  
**產出：** Applied budgets, Host snapshots, Folder Options rows/state, model/UI tests.  
**依賴：** 1.1, 2.2, 2.3, 3.2.  
**Owner／Wave：** Primary agent / Wave 4.  
**Gate／Evidence：** G-TELEMETRY; `openspec/changes/bc7-icon-thumbnail-caches/evidence/4.1/`.  
**完成門檻：** Icon/thumbnail memory/disk/GPU used and limits, pipeline state, capability, and bounded failures refresh correctly; controls affect only their selected kind/category.

- [x] 4.1.1 Extend Host cache snapshots with independent BC7 memory, disk, GPU, queue, staging, capability, and fallback fields.
- [x] 4.1.2 Apply icon and thumbnail memory, disk, and GPU settings only to their corresponding caches.
- [x] 4.1.3 Render BC7 state and independent used/limit values in Folder Options with available, unavailable, disabled, and partial states.
- [x] 4.1.4 Preserve one-second window-scoped single-flight refresh, stale-sample rejection, background I/O, and close cancellation.
- [x] 4.1.5 Add snapshot bounds/redaction, independent control, formatting, lifecycle, and unavailable-source tests.
- [x] 4.1.6 Write G-TELEMETRY evidence records for tasks 4.1.1 through 4.1.5.

### 4.2 Feature gates, rollout, and rollback

**目的：** Enable icon and thumbnail BC7 independently only after their blocking evidence passes.  
**輸入：** All functional packages; runtime configuration/session contracts.  
**產出：** Independent gates, default decision record, rollback procedure and tests.  
**依賴：** 2.3, 3.3, 4.1.  
**Owner／Wave：** Primary agent / Wave 4.  
**Gate／Evidence：** G-ROLLOUT; `openspec/changes/bc7-icon-thumbnail-caches/evidence/4.2/`.  
**完成門檻：** Each content kind defaults on only with passing quality/performance evidence; disabling either immediately restores provider RGBA without altering the sibling setting or user data.

- [x] 4.2.1 Add independent deny-by-default icon and thumbnail BC7 runtime gates with session-safe defaults.
- [x] 4.2.2 Implement gate changes that stop new compressed jobs and route new requests to RGBA without blocking navigation.
- [x] 4.2.3 Document operator rollback, cache-derived-data behavior, evidence invalidation, and re-enable procedure.
- [x] 4.2.4 Add independent enable/disable, in-flight disable, restart, prior-session, and sibling-isolation tests.
- [ ] 4.2.5 Record the G-ROLLOUT default-enable decision after G-ICON-QUALITY, G-THUMB-QUALITY, and G-PERF resolve.

## 5. Verification and Release Decision

### 5.1 Automated and robustness verification

**目的：** Prove container, Host, renderer, settings, migration, and fallback behavior together under locked builds.  
**輸入：** Completed implementation packages and repository test runners.  
**產出：** Focused/full logs, robustness report, dependency/source hashes, evidence index.  
**依賴：** 2.1 through 4.2 except default enablement decision.  
**Owner／Wave：** Primary agent / Wave 5.  
**Gate／Evidence：** G-AUTOMATED (blocking); `openspec/changes/bc7-icon-thumbnail-caches/evidence/5.1/`.  
**完成門檻：** All focused and affected full tests pass locked/offline; malformed cache corpus cannot escape bounds/roots; Release binaries build reproducibly with no unexplained failure.

- [x] 5.1.1 Run codec/container and Shell cache focused unit suites with locked dependencies.
- [ ] 5.1.2 Run GPUI Windows renderer contract and device-recovery suites.
- [ ] 5.1.3 Run explorer-model, explorer-shell-win, explorer-ui, explorer-app, and affected workspace suites.
- [x] 5.1.4 Run malformed/truncated/oversized/path/symlink corpus tests and record bounded-resource observations.
- [x] 5.1.5 Build locked offline Windows Release binaries and record executable/dependency/source hashes.
- [ ] 5.1.6 Run repository formatting, lint/static checks, OpenSpec strict validation, task validator, and diff checks.
- [ ] 5.1.7 Write G-AUTOMATED evidence records for tasks 5.1.1 through 5.1.6.

### 5.2 Headful visual and UITest gates

**目的：** Verify compressed and fallback presentation for required icon/thumbnail fixtures in the real application.  
**輸入：** Release build; deterministic fixtures; force-BC7/fallback seams; screenshot tooling.  
**產出：** UTIT reports, indexed screenshots, visual review records, independent quality decisions.  
**依賴：** 5.1.  
**Owner／Wave：** Primary agent / Wave 6.  
**Gate／Evidence：** G-ICON-QUALITY and G-THUMB-QUALITY (blocking independently); `openspec/changes/bc7-icon-thumbnail-caches/evidence/5.2/`.  
**完成門檻：** Required sizes/classes render with correct logical geometry, alpha, selection, and acceptable identity in BC7/fallback; every screenshot has build/config/content hashes and reviewer disposition.

- [ ] 5.2.1 Add deterministic UTIT selectors and seams for BC7 hit, cold conversion, fallback, unsupported, and telemetry states.
- [ ] 5.2.2 Run 16x16, 20x20, 24x24, and 32x32 transparent-edge, overlay, text-like, and high-contrast icon fixtures.
- [ ] 5.2.3 Run alpha, gradient, photograph, odd-dimension, large, and cache-hit thumbnail fixtures.
- [ ] 5.2.4 Verify navigation, selection, scaling, logical bounds, column/view switching, and fallback parity in UTIT.
- [ ] 5.2.5 Capture and index Release screenshots with executable, adapter, setting, fixture, and content hashes.
- [ ] 5.2.6 Record independent G-ICON-QUALITY and G-THUMB-QUALITY reviewer decisions without averaging failures across kinds.

### 5.3 Release performance and memory gate

**目的：** Demonstrate compact storage and bounded resources without material interaction regression.  
**輸入：** Passing G-AUTOMATED; Release A/B harness; representative folders and cold/warm states.  
**產出：** Machine-readable raw metrics, analysis, environment manifest, pass/fail decision.  
**依賴：** 5.1; 5.2 fixtures available.  
**Owner／Wave：** Primary agent / Wave 6.  
**Gate／Evidence：** G-PERF (blocking); `openspec/changes/bc7-icon-thumbnail-caches/evidence/5.3/`.  
**完成門檻：** BC7 payload/GPU storage is at most 25% of RGBA, warm hits perform zero decode/recompression, limits hold without sustained growth, all required metrics exist, and frame time has no material regression.

- [ ] 5.3.1 Freeze A/B workload, representative folders, cache states, run counts, adapter/driver, build, and measurement procedures.
- [ ] 5.3.2 Measure icon and thumbnail BC7/RGBA payload, disk, memory, GPU allocation, and upload bytes independently.
- [ ] 5.3.3 Measure cold compression, first display, disk I/O, warm hit latency, provider calls, decode count, and recompression count.
- [ ] 5.3.4 Measure repeated-navigation CPU working set, GPU resources, queue/staging peaks, evictions, and sustained-growth behavior.
- [ ] 5.3.5 Measure scrolling and navigation frame-time distributions and compare against the frozen material-regression rule.
- [ ] 5.3.6 Save raw machine-readable runs and environment/build manifests before deriving summaries.
- [ ] 5.3.7 Record the G-PERF decision, preserving failed runs and refusing default enablement when evidence is missing or failing.

### 5.4 Final traceability and implementation readiness

**目的：** Reconcile every approved requirement, gate, task, artifact, and evidence record into an auditable release decision.  
**輸入：** All prior work packages and immutable evidence lineage.  
**產出：** Traceability matrix, stale-evidence audit, final status report, default/rollback decision.  
**依賴：** 5.1, 5.2, 5.3, 4.2.  
**Owner／Wave：** Primary agent / Wave 7.  
**Gate／Evidence：** G-FINAL; `openspec/changes/bc7-icon-thumbnail-caches/evidence/5.4/`.  
**完成門檻：** No requirement or leaf lacks current evidence; no failed/blocked/stale record is marked complete; each content kind's default state follows its independent gates; strict validation passes.

- [ ] 5.4.1 Build the proposal-to-design-to-requirement-scenario-to-task-to-evidence traceability matrix.
- [ ] 5.4.2 Audit A/B/C adjustments, reopened tasks, replacement links, hashes, and stale evidence lineage.
- [ ] 5.4.3 Confirm every conditional branch ends in passed or evidence-backed not-applicable status.
- [x] 5.4.4 Apply independent icon and thumbnail default-enable decisions from unmodified blocking gates.
- [ ] 5.4.5 Run final strict OpenSpec validation, detailed-task validation, artifact placeholder scan, and contradiction review.
- [x] 5.4.6 Write G-FINAL release/rollback report and update task status only for evidence-backed completions.

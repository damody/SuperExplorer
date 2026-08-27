## 1. Startup discovery and desired state

### 1.1 Executable-relative SePack discovery

**目的：** Import `.sepack` archives placed directly in the installed `plugins` directory without shortcut arguments.
**輸入：** Existing SePack importer/validator/sealed store and installed layout.
**產出：** Bounded deterministic archive discovery and validated manifest catalog.
**依賴：** None.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** DISCOVERY; targeted app test and build output.
**完成門檻：** Archives are sorted, symlinks/non-SePack entries are ignored, validated manifest IDs populate settings, and explicit DLL arguments remain development-compatible.

- [x] 1.1.1 Resolve `plugins` from the executable parent and enumerate at most 1,024 direct `.sepack` archives.
- [x] 1.1.2 Feed sorted archives through host import, validation, sealing, resolution, and native admission while keeping `--plugin-dll` separate.
- [x] 1.1.3 Expose validated manifest package IDs, including disabled packages, to the Extensions catalog.

### 1.2 Persisted desired state

**目的：** Make new plugins default enabled and Folder Options choices govern later startup.
**輸入：** Existing atomic `feature-state-v1.json` store and Extensions draft UI.
**產出：** Startup filtering, dynamic option rows, atomic Apply/OK callback.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** SETTINGS; host feature-state and UI folder-options tests.
**完成門檻：** Missing entries load, explicit disabled entries do not, Apply/OK persists atomically, and Cancel remains draft-only.

- [x] 1.2.1 Filter validated package native admission through global and package desired state while preserving enabled-by-default semantics.
- [x] 1.2.2 Add newly discovered validated manifest package IDs to the Extensions option list.
- [x] 1.2.3 Persist the complete package switch batch atomically before Apply/OK publishes UI state.
- [x] 1.2.4 Initialize option switches from persisted package choices without erasing unknown IDs.

## 2. Global Plugin Safe Mode

### 2.1 Panic and abnormal-exit latch

**目的：** Ensure a plugin panic or interrupted callback produces a plugin-free next startup.
**輸入：** Existing durable callback markers, native lifecycle, and global desired state.
**產出：** Retained panic markers, stale-marker global denial, and load-failure fallback.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** SAFE-MODE; complete host unit suite.
**完成門檻：** Panicked terminals retain markers, stale markers deny all native callbacks, and startup load failure persists global disable.

- [x] 2.1.1 Retain durable registrar/provider markers for translated plugin panic terminals.
- [x] 2.1.2 Reuse startup marker recovery to deny every native plugin callback on the following launch.
- [x] 2.1.3 Persist global disabled state before returning any installed-plugin load/registration failure.

### 2.2 Explicit recovery

**目的：** Keep Safe Mode across restart until the user applies Extensions settings.
**輸入：** Safe Mode incidents and the Extensions Apply/OK persistence observer.
**產出：** Explicit incident clearing, global re-enable, individual choice preservation, and restart guidance.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** RECOVERY; app/UI compile and folder-options tests.
**完成門檻：** Apply/OK clears all recovered incidents and global disable while retaining every package switch; failed persistence rejects Apply.

- [x] 2.2.1 Clear recovered Safe Mode incidents only from explicit Extensions Apply/OK.
- [x] 2.2.2 Re-enable the global gate in the same desired-state transaction while retaining individual package choices.
- [x] 2.2.3 Present Safe Mode restart guidance on the Extensions page.

## 3. Installer, documentation, and validation

### 3.1 Product integration

**目的：** Ship and document directory autoload without fixed shortcut arguments.
**輸入：** Existing SDK build/package scripts, NSIS plugin payload list, and safety documentation.
**產出：** Bundled `.sepack` payloads, argument-free shortcuts, updated safety behavior, passing validation.
**依賴：** 1.1 through 2.2.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** FINAL; command results recorded in handoff.
**完成門檻：** Installer packages and installs eight `.sepack` archives with no fixed Plugin argument list, formatting/check/tests pass, and OpenSpec validates strictly.

- [x] 3.1.1 Build/package and install all bundled `.sepack` archives while removing fixed Plugin arguments from shortcuts.
- [x] 3.1.2 Update native safety documentation for autoload, settings, panic latch, and recovery.
- [x] 3.1.3 Run formatting, app check, host tests, focused app/UI tests, and strict OpenSpec validation.

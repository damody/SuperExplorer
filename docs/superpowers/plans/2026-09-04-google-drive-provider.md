# Native Google Drive Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add first-class Google Drive My Drive browsing to SuperExplorer through the existing `RemoteProvider` stack, using Cyberduck's Drive semantics (OAuth, file ids, Docs export, trash).

**Architecture:** Independent `GdriveProvider` in `explorer-remote` talks Drive API v3 over an injectable HTTP transport. `explorer-app` owns OAuth loopback, profile JSON, and Credential Manager. UI reuses remote navigation/transfer; locations display paths and carry `provider_entry_key` file ids.

**Tech Stack:** Rust workspace crates `explorer-model`, `explorer-remote`, `explorer-app`, `explorer-ui`, `explorer-search`; reqwest blocking + rustls; PKCE (sha2 + base64); Windows Credential Manager.

**Spec:** `docs/superpowers/specs/2026-09-04-google-drive-provider-design.md` and `openspec/changes/add-native-google-drive-provider/`.

## Global Constraints

- No live Google API in CI; mock HTTP only.
- No refresh/access tokens in JSON, session, history, bookmarks, Debug, or error strings.
- Do not use Cyberduck's OAuth client id.
- Do not call `rclone` or `curl.exe`.
- Remote delete for Drive is trash, not `files.delete`.
- First release is My Drive only.

---

### Task 1: Model contract

**Files:**
- Modify: `crates/explorer-model/src/remote.rs`
- Modify: `crates/explorer-model/src/domain.rs`
- Modify: `crates/explorer-model/src/navigation.rs`
- Modify: `crates/explorer-model/src/lib.rs`

- [ ] Add `Gdrive` provider kind, email-authority parser, `GdriveProfile`, `provider_entry_key`, capabilities, column bit.
- [ ] Tests: parse `gdrive://you@gmail.com/Work`, reject password-bearing URIs, profile JSON has no token fields, `ColumnFileSystems::REMOTE` contains Gdrive, `from_bits(32)` is None.

### Task 2: Drive protocol + OAuth + provider

**Files:**
- Create: `crates/explorer-remote/src/gdrive.rs`
- Modify: `crates/explorer-remote/src/lib.rs`, `Cargo.toml`

- [ ] Mock-transport tests for list (including shortcuts and duplicate names), Docs export download, binary upload, mkdir, rename, trash, 401 refresh, missing export type.
- [ ] PKCE challenge/token parse tests without a browser.
- [ ] Implement `GdriveProvider` to pass those tests.

### Task 3: App and UI integration

**Files:**
- Modify: `crates/explorer-app/src/remote_service.rs`, `application.rs`
- Modify: `crates/explorer-ui/src/navigation_pane.rs`, `lib.rs`, `chrome.rs`, `state.rs`, `icons.rs`, `actions.rs`
- Modify: `crates/explorer-search/src/address.rs`

- [ ] Persist `gdrive-profiles.json`; Credential Manager target `SuperExplorer/GDRIVE/<email>`.
- [ ] Nav section + Connect synthetic location `super-explorer:gdrive-connect`.
- [ ] Address `gdrive://` starts OAuth; `gdrive://email/path` navigates.
- [ ] Trash confirmation copy; hide 新增捷徑; bookmark icon; remote provider-id matches include `gdrive`.

### Task 4: Verification

- [ ] `cargo test -p explorer-model --lib remote`
- [ ] `cargo test -p explorer-remote --lib gdrive`
- [ ] Focused UI tests for nav + address
- [ ] `openspec validate add-native-google-drive-provider --strict`
- [ ] User-perspective checklist from the spec scenarios

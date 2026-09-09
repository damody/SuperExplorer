# WSL Linux Navigation Pane Implementation Plan

> **For agentic workers:** Execute inline in this session. Do not pause for confirmation.

**Goal:** SuperExplorer's left navigation pane shows a Linux root with installed WSL distros underneath, matching Windows File Explorer (penguin + `Ubuntu-24.04`).

**Architecture:** Reuse the existing navigation-pane section/child pattern (Phones/ADB, This PC/drives). Linux is a Shell namespace row; distros are filesystem UNC roots discovered from the per-user Lxss registry. Expanding a distro reuses Shell/filesystem child discovery. No new remote provider.

**Tech Stack:** Rust crates `explorer-ui`, `explorer-app`, `explorer-i18n`; Windows Shell CLSID; `\\wsl.localhost\<distro>\`.

**Spec:** Conversation design 2026-09-08. Screenshot contract: expandable Linux row, folder-style distro children.

## Global Constraints

- Match the Explorer screenshot: Linux parent + distro children in the left pane. No This PC capacity bars.
- Linux identity is `shell:::{B2B4A4D1-2754-4140-A2EB-9A76D9D7CDC6}`.
- Distro paths are `\\wsl.localhost\<DistributionName>\`.
- Hide the Linux row when no distros are configured/discovered.
- Penguin comes from Shell `icon_location`, not `NavigationIcon::Folder` (Folder forces the generic folder texture).
- Distro children use `NavigationIcon::Folder` so they get chevrons like Explorer.
- Do not duplicate Shell-discovered distro roots under Linux when static rows already exist.
- Nested paths under a distro keep that distro row selected (`\\wsl$` and `\\wsl.localhost` are the same distro).
- `nav-linux = Linux` in every locale catalog (Explorer keeps the English word).
- Discover distros from `HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss\*\DistributionName`.
- Refresh the list on a timer like ADB devices.
- No new remote protocol; browsing uses existing filesystem/Shell listing.

---

### Task 1: Navigation presentation + tests

**Files:**
- Modify: `crates/explorer-ui/src/navigation_pane.rs`
- Modify: `crates/explorer-ui/src/chrome.rs`
- Modify: `crates/explorer-ui/src/icons.rs`

**Produces:**
- `NavigationIcon::Linux`
- `LINUX_NAMESPACE: &str = "shell:::{B2B4A4D1-2754-4140-A2EB-9A76D9D7CDC6}"`
- `WslNavigationDistribution { name, label, available }`
- `configure_wsl_navigation_distributions(Vec<WslNavigationDistribution>)`
- `wsl_distribution_root_path(name) -> PathBuf`
- `wsl_distribution_root_name(location) -> Option<String>`
- Linux section + distro rows in `windows_navigation_items_with_pins`

- [ ] Failing tests first, then implementation:
  - no distros → no `linux` row
  - configured `Ubuntu-24.04` → `linux` then `linux-distro-Ubuntu-24.04`
  - `network` < `linux` < `phones`
  - Linux location is the CLSID parsing name; icon is `Linux`; kind is Section
  - distro location is `\\wsl.localhost\Ubuntu-24.04\`
  - `is_selected` true for `\\wsl.localhost\Ubuntu-24.04\home` and `\\wsl$\Ubuntu-24.04\etc`
  - `should_render_discovered_child` false for Linux parent + distro UNC root
  - zh-TW/en labels are `Linux`
- [ ] Linux row uses Shell icon_location (not Folder).
- [ ] Distro rows are Folder + depth 1 + available/unavailable like other remotes.
- [ ] Flattening Linux also suppresses discovered distro roots (`item.id == "linux"`).
- [ ] `NavigationIcon::Linux` fallback art in `icons.rs` (group with Computer/Network).
- [ ] Linux as Section already has a chevron; do not add Linux to the Folder-only empty-slot match.

### Task 2: i18n

**Files:**
- Modify: every `crates/explorer-i18n/locales/*/chrome.ftl`

- [ ] Add `nav-linux = Linux` after `nav-network` in all 19 locales.
- [ ] `cargo test -p explorer-i18n --test catalog_complete` passes.

### Task 3: Distro discovery + app wiring

**Files:**
- Modify: `crates/explorer-app/src/remote_service.rs`
- Modify: `crates/explorer-app/src/application.rs`

- [ ] `discover_wsl_navigation_distributions()` reads Lxss `DistributionName` values, skips empty, sorts case-insensitively, maps to `WslNavigationDistribution { available: true }`.
- [ ] Non-Windows returns empty.
- [ ] Unit test: names → nav structs; empty names dropped.
- [ ] On Windows, live registry discovery includes this machine's `Ubuntu-24.04` when present.
- [ ] `run_gpui_with_initial_path` configures WSL distros at startup.
- [ ] Existing ADB refresh loop also refreshes WSL distros every 5s.

### Task 4: Verification

- [ ] `cargo test -p explorer-ui --lib navigation_pane`
- [ ] `cargo test -p explorer-ui --lib chrome navigation`
- [ ] `cargo test -p explorer-i18n --test catalog_complete`
- [ ] `cargo test -p explorer-app --lib remote_service`
- [ ] User-perspective: real `Ubuntu-24.04` is discovered; tree contract matches the screenshot (Linux parent, distro child, UNC path, penguin via Shell location).

## Why

Folder Options → View currently mixes view behavior with cache and MFT memory budgets. Users cannot turn MFT off to reclaim RAM, and plugins that need MFT keep running anyway.

## What Changes

- Add a Folder Options tab **空間設定 / Space** after View and before Extensions.
- Move cache usage/limits, MFT resource budgets, folder-size TTL, and Clear thumbnail cache onto Space.
- Add a persisted MFT master switch (default **on**). Apply stops MFT queries immediately, unloads MFT memory caches, and keeps disk indexes.
- Plugins may declare host capability `mft`. Official folder-size column and Size Map do so.
- Turning MFT off auto-disables those plugins. Enabling one while MFT is off shows a confirmation that MFT must be turned on and will increase memory use.
- When MFT is off, the MFT search engine is unavailable.

## Capabilities

### New Capabilities

- `folder-options-space-settings`: Space tab, moved cache/MFT budgets, MFT toggle, persistence.
- `mft-host-feature-gate`: Host `mft` capability, plugin auto-disable, enable confirmation, MFT service memory unload, MFT search unavailability.

### Modified Capabilities

- `extension-package-and-feature-lifecycle`: Features MAY declare host capability `mft`; the host treats it as a gated host feature rather than an always-on capability.

## Impact

Affects `explorer-model` view/session settings, `explorer-ui` Folder Options, `explorer-i18n` catalogs, `explorer-extension-host` capability binding, official folder-size plugins, `explorer-app` MFT query/service, `explorer-shell-win` search availability, and Folder Options UITEST smoke. SuperDesktop is unchanged.

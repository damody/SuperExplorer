## Why

SuperDesktop already defaults taskbar buttons to the left and exposes Left/Center alignment in settings, but its Start menu is always centered. This makes the shell visually inconsistent and prevents the existing setting from producing Windows-compatible Start positioning.

## What Changes

- Make the Start popup follow the current taskbar Left/Center alignment.
- Keep Left as the default for new, missing, or invalid settings.
- Read alignment when Start opens so saved settings apply immediately without restarting SuperDesktop.
- Clarify in Taskbar settings that alignment affects both taskbar buttons and Start.
- Verify mouse and shell-hotkey opening paths use the same DPI-aware, monitor-local geometry.

## Capabilities

### New Capabilities

- `start-menu-alignment`: Defines the configurable, monitor-aware relationship between taskbar alignment and Start popup placement.

### Modified Capabilities

None.

## Impact

The change affects SuperDesktop Start window geometry and the existing Taskbar settings UI/model. It does not change the settings schema, native Explorer configuration, public APIs, dependencies, or Start menu contents. Existing saved `left` and `center` values remain compatible.

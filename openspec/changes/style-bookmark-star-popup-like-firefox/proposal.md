# Change: Style bookmark star popup like Firefox

## Why

The bookmark star currently opens the generic large editor, which does not communicate the quick add/edit workflow users expect from a browser-style star control.

## What Changes

- Present the bookmark editor as a compact Firefox-inspired transient card.
- Emphasize name, destination, Save, and Cancel controls.
- Select and focus the name on open; Enter saves and Escape cancels.
- Preserve the existing bookmark model, destination folders, persistence, and advanced Lua payload editing.

## Non-goals

- Copying Firefox assets or implementation.
- Changing bookmark storage or navigation behavior.

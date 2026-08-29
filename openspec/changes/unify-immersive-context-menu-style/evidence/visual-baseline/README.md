# Context-menu visual baseline

`schema.json` is the mandatory record shape. `fixtures.json` maps every required
visual/menu state to deterministic smoke or controlled-HMENU fixtures. A capture is not
accepted merely because an image exists: every required theme/DPI cell must contain the
matching File Explorer and SuperExplorer crop, SHA-256, measurements, and tolerances.

The current runner provides Windows 11 build 26200 and a 168-DPI image pipeline. The
repository intentionally records other DPI/theme cells as missing instead of treating a
token projection or resized bitmap as physical-runner evidence.

## 1. Drag effect contract

- [x] 1.1 Add deterministic same-volume/cross-volume default effect resolution with Ctrl/Shift precedence tests.
- [x] 1.2 Stop publishing a false Move preference for unmodified native OLE drags while preserving explicit Ctrl Copy and Shift Move.

## 2. UI gesture and target routing

- [x] 2.1 Allow Shift-selected rows to start one left-button drag after the Windows threshold.
- [x] 2.2 Negotiate effects from live modifiers plus the real source/destination paths for folder-row and background targets.
- [x] 2.3 Reject self, descendant, read-only, and no-op Move destinations before queuing a typed transfer.

## 3. Regression verification

- [x] 3.1 Add UI reducer tests for Shift initiation, folder/background routing, modifier effects, invalid targets, and terminal cleanup.
- [x] 3.2 Add Shell tests for native preferred-effect publication and real Copy/Move disk outcomes.
- [x] 3.3 Register deterministic and headful left-drag Move/Copy/Cancel coverage in UTIT.
- [x] 3.4 Run formatting, targeted tests, OpenSpec strict validation, UTIT validation, and the application build.

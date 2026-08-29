# Final traceability and open gates

| Requirement area | Design decisions | Gate | Task range | Current evidence/status |
|---|---|---|---|---|
| Runtime-gated popup and fallback | 1, 2, 6, 11 | G4 | 2.1–2.2 | passed; typed unsupported and permanent native fallback |
| Scoped cleanup and HMENU identity | 3–5, 11 | G5–G8 | 3.1–4.1 | passed; lifecycle, dynamic submenu, replay and identity tests |
| Rollout, privacy and provenance | 6, 7, 9–11 | G9–G10 | 4.2–4.3 | passed; no P0/P1 and clean-room review retained |
| Accepted Local visual baseline | 8–9 | G11 | 5.1 | blocked; required light/dark 100/125/150/200 capture matrix is incomplete |
| Typed remote visual contract | 8 | G12 | 5.2 | passed; all governed fields tokenized and listing colors isolated |
| ADB/SFTP parity | 8–9 | G13 | 5.3 | blocked; current light/background ADB and SFTP cells pass, full item/folder/theme/DPI matrix is incomplete |
| Installed Shell compatibility | 2–6, 11 | G14 | 6.1 | passed; built-in, 7-Zip, WinRAR, TortoiseGit, VS Code and Defender evidence retained |
| Lifecycle/DPI/accessibility recovery | 2–7 | G15 | 6.2 | blocked; all available recovery tests pass, but a real mixed-DPI multi-monitor runner is unavailable |
| Repository integration | 9–10 | G16 | 7.1 | blocked by G11/G13/G15 and therefore traceability task 7.1.8 remains open |

No gap is hidden or converted to success. The unresolved rows are environment/evidence gates,
not known product failures; the conditional rollout therefore remains opt-in.

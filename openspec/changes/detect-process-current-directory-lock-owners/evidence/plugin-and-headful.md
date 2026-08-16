# G4 plugin and headful status

The production example test passed 2/2 offline. Its English README already documented ancestry, privacy-safe false negatives, shared cancellation/deadline, TTL/F5, and offline commands. The Traditional Chinese README was replaced with valid UTF-8 Traditional Chinese containing the same normative content.

The headful script and UTIT manifest contain native/WOW64 setup, `IsWow64Process2` verification, exact/parent screenshots, exit+F5 clearing, real file-lock clearing, and stale refresh/folder/tab/feature assertions. The selected manifest validation passes with all 648 OpenSpec requirements recognized. Source hashes: script `83DFB7BEB3BB3DDCEC52502A502460663F717A9FFB1CB6012780A96B3ABE4C7F`; manifest `595FE924CF743D76D5CC5220F92649AE4A3AE1901E1ADC0B87B38243B687B86D`.

The formal runner now repeatedly reaches the production native and WOW64
current-directory checks. Run
`target/uitest-runs/1786705711-9a19f0bad6db478892e63cb92fe22706`
proved the native nested row, native visible parent row, WOW64 nested row, WOW64
visible parent row, and exit-plus-F5 clearing before the later Folder Options
input step failed. The evidence hashes are:

- `lock-owner-cwd-native-nested.png` — `465086B7E57EED5CA700EC03C1BC9EA24BD58D14B0D4F64E6248A26F9213295A`
- `lock-owner-cwd-native-parent.png` — `221C1BB7ADBA387062787CF339D30DDFBDF866406075A46600D7D2FF03F1431A`
- `lock-owner-cwd-wow64-nested.png` — `400C882A907412C87B3DF2A2D82C88AAB75D31D9E395683D3A47D1249395C64A`
- `lock-owner-cwd-wow64-parent.png` — `221C1BB7ADBA387062787CF339D30DDFBDF866406075A46600D7D2FF03F1431A`
- `lock-owner-cwd-cleared.png` — `DD443124E52C93E4FDB9270C7B5FC7999E36EE16FC7290454B2A964A22828699`

The formal `rust-lock-owner-headful` case remains open because synthetic input
to the separate native Folder Options window does not complete its Confirm
action on this 175% DPI desktop; the runner correctly exits nonzero and is not
claimed as passing.

The documented plugin test passed 2/2. The documented direct locked/offline
`x86_64-pc-windows-msvc` example build also passed on 2026-08-14. Validation
failed closed with `SESDK-INPUT-001`: the published SDK inventory differs from
current SDK files because the checked-out GPUI source is not the approved SDK
snapshot. Consequently the validation and package wrapper gates remain open;
the inventory was not regenerated against an unapproved dependency snapshot.
The stale manifest selectors were corrected and global coverage validation
passes. Current script hash:
`4AB3668A92FBCF8DAF3190C42B4034FA96DDE7F719A94900D2C252C97E88F85C`.

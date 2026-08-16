# Release and headful evidence

The locked Release artifacts used by the installed headful runs have these SHA-256 identities:

- `target/release/SuperExplorer.exe`: `5EE54E72B79C03D68C330DE7B06FABA13B65607EE2159FA3D9314AAE4412BCE2`
- `target/release/superexplorer-mft-service.exe`: `75166282448BCB970CB97C5964402ECC4E41D7BDD1A3B17295BED965952FB567`
- test installer recorded by the installing run: `28BF85CFEE3B49755BE46ADC1BFB35945B757B8E78844BF8257D4C4C87EB85F3`

Reusable immutable installed evidence:

- `../independent-cache-max-editors/evidence/cache-budget-editors-installed-representative/report.json`: PASS; fourteen independent editors, representative icon/extension/GPU/BC7 values, Apply/OK/Cancel persistence.
- `../independent-cache-max-editors/evidence/folder-options-installed-service/report.json`: PASS; dedicated native Folder Options behavior and cache-section screenshots.
- `../independent-cache-max-editors/evidence/folder-options-installed-service/folder-options-cache-controls.png`, SHA-256 `BF6CB8AD0CB4AC843227ECAAC1AD91AA97AF3875D87A961D7FB3E0A176931162`.
- `../independent-cache-max-editors/evidence/folder-options-installed-service/folder-options-cache-telemetry.png`, SHA-256 `61533D3DDF55AF6F056C294E0C15524B4C9B6F2AE0F9C59402BAE74A649601BD`.
- `../independent-cache-max-editors/evidence/folder-options-installed-service/folder-options-cache-mft.png`, SHA-256 `61533D3DDF55AF6F056C294E0C15524B4C9B6F2AE0F9C59402BAE74A649601BD`.

The deterministic headful seam uses the stable `folder-options-cache-usage` selector in `scripts/smoke_folder_options_extensions_scroll_escape.ps1`. Pending/unavailable distinction is additionally covered by model/UI tests. The unavailable-Service screenshot leaf is superseded by the later `refine-cache-telemetry-availability` change, which owns that presentation distinction.

WebP/RGBA performance comparison is intentionally not asserted against the current BC7 binary. The replacement performance evidence belongs to `bc7-icon-thumbnail-caches` G-PERF.

# Evidence lineage review

- Early installer hashes for 5.2.1 and 5.2.2 are historical. Later records explicitly supersede the earlier package records; none are presented as the current 2026-08-14 package.
- The partial-scope Folder Options run for 5.1.3 is superseded by `cache-budget-editors-final30/report.json`.
- The installed editor reports named `installed-final` and `installed-representative` are immutable successful runs. Repeated 2026-08-14 UIA retries are diagnostic attempts and do not supersede those PASS records.
- The current installed package identity is installer `28BF85CFEE3B49755BE46ADC1BFB35945B757B8E78844BF8257D4C4C87EB85F3`, app `5EE54E72B79C03D68C330DE7B06FABA13B65607EE2159FA3D9314AAE4412BCE2`, and service `75166282448BCB970CB97C5964402ECC4E41D7BDD1A3B17295BED965952FB567`.
- `installed-partial-final-2/report.json` is the current passed Size Map partial record. `installed-partial-final-3` is a failed diagnostic attempt showing that the installed Details surface did not expose a partial cell within 60 seconds; it does not supersede the passed Size Map half.
- Task leaves remain atomic: 5.2.5 is intentionally open until both required installed surfaces pass in one bounded lineage.

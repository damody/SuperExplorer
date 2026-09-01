# Read-only ADB extension scan

Device: `emulator-5554` (`ASUSAI2501B`)

Procedure: `adb -s emulator-5554 shell "find / -type f 2>/dev/null"`; only returned path strings were grouped locally. No remote file content was opened or copied.

High-frequency or semantically relevant suffixes observed include `so` (2,591), `ogg` (224), `kl` (168), `ttf` (162), `apk` (120), `pb` (101), `xml` (97), `jar` (75), `rc` (67), `config` (60), `bin` (51), `log` (51), `otf` (41), `art` (35), `oat` (34), `bc` (30), `vdex` (29), `txt` (22), `policy` (19), `cil` (16), `prop` (14), `json` (11), `dat` and `o` (7 each), `idc` and `kcm` (7 and 6), `png` and `conf` (5 each), `prof`, `gz`, `sh`, and `ko` (3 each), `ttc`, `zip`, `pdf`, `bprof`, and `perfetto-trace` (2 each), plus `obb`, `idx`, `service`, and `sha256` examples.

Many pseudo-files under proc/sys/cgroup appear extensionless or have state-like suffixes (`pressure`, `stat`, `events`, `procs`). Those are deliberately not treated as common filename extensions; unknown and extensionless entries retain the generic document fallback.

The implemented taxonomy contains 9 exact compound mappings and 265 unique final-extension mappings (274 declared suffix rules total), including all common actionable suffixes above.


# Representation supersession

The independent settings, telemetry, disk accounting, MFT diagnostics, and Folder Options work in this change remains the active baseline. The WebP representation is not the current production architecture.

The later approved change `bc7-icon-thumbnail-caches` replaces WebP with a private BC7 container and explicitly requires legacy WebP files to be treated as bounded lazy misses. Production source now uses `RGXBC7C1` and `.bc7cache`; restoring WebP would regress that approved architecture.

Terminal task mapping:

- `3.1.4`, `3.1.5`: superseded by `bc7-icon-thumbnail-caches` G-ENCODER and bounded-container safety gates.
- `3.2.1`-`3.2.4`, `3.2.6`, `3.2.7`: their historical WebP implementation is superseded by BC7 tasks `2.1.*` and `2.2.*`. `3.2.5` remains satisfied by the representation-neutral independent disk statistics API.
- `3.3.1`-`3.3.4`: WebP migration is superseded by BC7 task `2.2.5`; the current code treats both `.rgba` and legacy representation files as derived-data misses within scoped cleanup.
- `5.1.2`'s WebP wording is superseded by the current shell disk-cache suite, which validates the BC7 implementation.
- `5.2.3`-`5.2.5`: representation-specific Release profiling and final cache-limit gates transfer to BC7 tasks `5.3.*` so measurements describe the shipped format.

These leaves are terminally resolved as `superseded`, not represented as WebP passes.

# Traceability and terminal disposition

| Scope | Design/spec implementation | Task disposition | Evidence |
|---|---|---|---|
| Independent settings/session | `CacheBudgetSettingsV1`, session projection and Folder Options actions | passed | `automated-validation.md` G-SETTINGS |
| Independent memory limits | icon/base-icon and thumbnail budget application; byte-bounded thumbnail LRU | passed | G-MEMORY-LRU |
| Host telemetry | bounded immutable snapshot, Host-owned reporters, partial/unavailable states | passed | G-HOST-REPORTERS and G-FOLDER-OPTIONS |
| Disk sampling | registered roots, latest-completed snapshot, single-flight/cancellation | passed | focused Folder Options tests and affected compile |
| WebP persistence | replaced by later approved BC7 architecture | superseded | `supersession.md` |
| MFT diagnostics | fixed-size aggregate protocol, bounded client, local ACL, remote rejection | passed | G-MFT-DIAGNOSTICS |
| Headful settings | independent editors and cache sections | passed; unavailable screenshot transferred | `release-and-headful.md` |
| Representation profile | current shipped representation is BC7 | superseded | `bc7-icon-thumbnail-caches` tasks `5.3.*` |

Artifact scan found one intentional historical contradiction: this change's proposal/spec/task prose says WebP while the current production source and later approved change say BC7. It is documented as supersession rather than silently rewriting historical approval. No placeholder (`TODO`, `TBD`, fake hash, or unlinked conditional leaf) is used as completion evidence.

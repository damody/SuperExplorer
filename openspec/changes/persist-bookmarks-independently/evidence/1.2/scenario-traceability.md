# Scenario-to-test traceability

| Requirement scenario | Automated evidence target |
|---|---|
| Valid current bookmark document | `bookmark_store::tests::independent_current_wins_over_legacy` |
| Intentionally empty collection | `bookmark_store::tests::empty_independent_document_is_authoritative` |
| Payload exceeds bound | `bookmark_store::tests::oversized_current_falls_back_without_unbounded_read` |
| First launch after upgrade | `bookmark_store::tests::missing_store_migrates_legacy_once` |
| Migration write unavailable | `bookmark_store::tests::migration_failure_returns_legacy_without_mutation` |
| Corrupt current with valid backup | `bookmark_store::tests::corrupt_current_recovers_backup_and_repairs_current` |
| Both artifacts corrupt | `bookmark_store::tests::corrupt_artifacts_preserve_unrelated_files` |
| Reset saved session | `session_lifecycle::tests::session_reset_does_not_touch_bookmark_store` |
| Reset all saved state | `session_lifecycle::tests::all_state_reset_does_not_touch_bookmark_store` |
| Successful mutation | `session_lifecycle::tests::flush_writes_independent_bookmarks` |
| Transient failure | `session_lifecycle::tests::bookmark_failure_retries_latest_snapshot` |
| Upgrade or repair | `product_identity::installer_preserves_independent_bookmark_store` |
| Uninstall and reinstall | `product_identity::installer_preserves_independent_bookmark_store` |
| Privacy-safe diagnostic | Bookmark-store error tests plus final source review prohibiting payload formatting in errors |

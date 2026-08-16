# G-ROLLOUT decision and rollback

Icon and thumbnail BC7 settings are independent and default to `false` for new, legacy, and restarted sessions. Disabling a gate prevents new compressed admission and rechecks before late compressed publication; requests fall back to provider RGBA without altering the sibling gate.

Focused rollout coverage passes: the Shell gate test proves independent enable/disable and sibling isolation; the thumbnail test captures an in-flight compressed request, disables only thumbnail BC7, and proves late publication is rejected while the icon gate remains enabled; model default/migration/round-trip tests prove prior-session and restart behavior. Thumbnail source hash: `3C5D29DF2D29DB41D9BB52045C971F8DA4F2862E825A678F5AAA18573E15F778`.

Operator rollback is to disable the affected icon or thumbnail setting and restart only if already-visible GPU resources must be discarded immediately. `.bc7cache` files are derived data: they may be retained for later re-enable or deleted within the registered cache root. Re-enable requires rerunning evidence whose executable, source, adapter, driver, or fixture hashes changed.

Default-enable decision: **disabled for both kinds**. G-ICON-QUALITY, G-THUMB-QUALITY, and G-PERF are unresolved, so task 4.2.5 remains open and no optimistic default is applied.

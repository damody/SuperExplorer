# Context-menu soak lineage

The required count is 1,000 real menu open/cancel cycles. Failed runs are retained; none is
silently replaced by a passing summary.

1. Per-cycle canceller thread: failed measurement hygiene. Handles grew `163 → 483` and
   private bytes `1,810,432 → 62,459,904`; the test itself created 1,000 OS threads.
2. One persistent canceller, one warm-up: handles were stable (`494 → 485`) but private bytes
   grew `9,121,792 → 61,845,504`.
3. One persistent canceller, 100 warm-ups: handles were stable (`504 → 489`) but private bytes
   grew `15,679,488 → 66,830,336`. This isolated a Shell-query or popup high-water slope.
4. Controlled one-row HMENU, same application-owned host, 100 warm-ups + 1,000 cycles:
   passed. Handles `260 → 261`; private bytes `6,402,048 → 15,282,176` (+8.9 MiB).
5. Real Shell A/B, 100 warm-ups + 1,000 cycles each: application-owned handles `504 → 489`
   and private bytes `14,389,248 → 66,801,664`; native `TrackPopupMenuEx` handles `491 → 512`
   and private bytes `72,327,168 → 111,366,144`. The run failed only because an absolute
   native-handle threshold was applied even though the gate is comparative. The replacement
   run uses owned-vs-native slope bounds and is recorded below when complete.

The controlled result proves popup window/GDI/shadow cleanup is bounded. The real A/B result
also proves much of the private-commit slope is shared Shell extension query behavior rather
than application-owned presentation alone.

6. Replacement A/B run passed after correcting the comparative gate. Application-owned:
   handles `504 → 489`, private bytes `14,372,864 → 66,600,960` (+52,228,096). Native:
   handles `491 → 512`, private bytes `72,331,264 → 111,267,840` (+38,936,576). The owned
   path used 13,291,520 additional private bytes, within the explicit 16 MiB allowance, and
   its handle slope was lower than native. Result: passed.

# UI invalidation batcher contract fixture

This fixture is intentionally data-only. It describes one deterministic
1,000-result stream: one accepted result per millisecond, a 20 ms invalidation
window, and a path-free slow-callback identity. The contract script consumes
the fixture, checks the deterministic upper bound (50 redraw batches), and
then runs the production batcher, runtime/bridge, provider-timing, and GPUI
service-loop seam tests.

The fixture does not emulate GPUI or invent a second implementation. The Rust
Rust tests are the authority for the host behavior; this file makes the
performance, race-safety, thread-affinity, and privacy assertions reproducible
in CI and in UITEST evidence. The mandatory production vertical test also
requires the bounded first viewport to be admitted before enrichment, FIFO
visible-priority service, bounded redraws, and one-turn cancellation delivery.

# G2/G3 host composition evidence

The ABI-facing adapter resolves the complete bounded item list, constructs one live cancellation predicate and one absolute deadline, invokes the internal batch service once, catches callback panics, and preserves item/location generations in an empty `HOST_ERROR` fallback. No native path or handle crosses the ABI.

Passed focused locked/offline tests:

- `explorer-app`: 5 initial lock-owner tests plus 4 composition tests. The table test covers cancellation/deadline dominance in both source positions; Ready paired with Ready/Empty/Unavailable/HostError; HostError and Unavailable ownerless precedence; both Empty; PID+creation-time deduplication with Restart Manager precedence; deterministic sort and post-sort truncation.
- `explorer-extension-host`: 11 tests for declared/revoked authority, bounded result/name projection, item bounds, one service invocation for the 128-item maximum, cancellation observed inside a running callback, one shared absolute deadline whose remaining duration decreases across two items and both discovery-source phases, and callback/native-reader/composition panic containment with preserved generations, resource cleanup, plus subsequent service usability.
- `explorer-extension-api`: 1 bounded, generation-scoped, discover-only contract test.

Hashes: application `AC4A3AAB191EE9B7F139BB3D99FDBBA94F601AFBC6F509B0D9C7516D8ACB0474`; extension host `9136698995D40210F0D9337BA07A490797DCAE8332EE57AFBC040C67FC745138`.

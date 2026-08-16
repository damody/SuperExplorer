# G-CONTAINER evidence

Schema 6 uses a fixed little-endian header containing magic, schema, endian marker, content kind, BC7 UNORM marker, reserved byte, invalidation digest, logical and padded dimensions, row pitch, payload length, and checksum. Parsing validates all fields and the complete file length before admitting the payload.

`cargo test -p explorer-shell-win bc7_codec::tests --lib` passed, including odd-size padding, exact padded 25% ratio, malformed/excessive input rejection, deterministic representative fixtures, a maximum-single-axis 16384x4096 layout at the 64 MiB payload bound, and rejection of a 16384x16384 layout that exceeds that byte bound.

`cargo test -p explorer-shell-win icon_disk_cache::tests --lib` passed 11 tests. The corruption table rejects and removes changes to magic, schema, endianness, kind, format, reserved field, identity, zero width, padded width, pitch, length, checksum, and trailing bytes.

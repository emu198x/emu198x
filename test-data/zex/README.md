# ZEX Z80 exerciser binaries

Frank Cringle's ZEXDOC and ZEXALL Z80 exerciser programs, originally CP/M
`.com` files. Used by `crates/zilog-z80/tests/zex_tests.rs` to regression-test
the Z80 core's documented (ZEXDOC) and undocumented (ZEXALL) instruction
behaviour against precomputed CRCs.

These binaries are checked into the repo so CI can run the exercisers
without external fixture provisioning. They are ~8.7 KB each, ~17 KB total.

## Provenance and licence

Frank Cringle, 1996. Originally distributed under "yaze" (Yet Another Z80
Emulator). Since redistributed for ~30 years across every major Z80 emulator
project (Fuse, MAME, RetroArch, ZEsarUX, etc.) as standard regression
fixtures. Effectively public domain by long-standing redistribution.

The binaries here are byte-identical to those shipped with most emulator
test suites.

## Files

| File | Bytes | SHA-256 |
|---|---|---|
| `zexdoc.com` | 8 704 | `34923a7ed82285d3038b2d54bd64899e12173eebb61f9d07b4fc72e78af2ae8f` |
| `zexall.com` | 8 704 | `6e2da55147a04f28d303d5da6a1e6b771557ac244653590a0f24a2d39c8537e8` |

## How tests find them

`crates/zilog-z80/tests/support/mod.rs::find_zex_binary` searches in order:

1. `$EMU198X_ZEX_DIR` (if set, override for ad-hoc local runs)
2. This directory (`<repo>/test-data/zex/`)

CI and local both hit step 2 by default.

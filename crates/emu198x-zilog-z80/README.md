# emu198x-zilog-z80

Zilog Z80.

The CPU of the ZX Spectrum, Amstrad CPC, MSX, Game Boy's ancestor line, and
a generation of arcade hardware. Ticked in half-cycles so that signals which
change mid-cycle — MREQ, IORQ, RFSH, and the contended-memory behaviour
those drive — are observable at the right instant.

```rust
use emu198x_zilog_z80::Z80;

let mut cpu = Z80::new();
cpu.tick(); // one half-cycle
```

Undocumented flags and instructions are implemented, and the core is
validated against the Zilog documentation, FUSE's test suite, and Tom Harte's
single-step corpus.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of cycle-accurate
retro-computing emulator cores. This crate is published so siblings and outside
projects can use the chip on its own; it is versioned independently of the
Emu198x suite and bumps only when it changes.

Licensed GPL-2.0-or-later.

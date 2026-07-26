# Decision: Anchor Sinclair 128K contention to HALT2INT128

**Date:** 2026-07-25

## The decision

The Sinclair 7K010E contention window begins at the pre-display
`/Border` boundary. Its delay-table index is derived from that logical
coordinate with a one-ULA-pixel offset:

```text
phase = (contention_pixel + 1) & 0x0f
```

This is a table-coordinate conversion, not an additional CPU T-state.
The alternating Z80 clock level selects the applicable half of each
two-pixel CPU clock cell.

HALT phantom M1 cycles continue to present the byte after the HALT
opcode on the address bus. This is independently load-bearing for a
HALT at `$BFFF`: when RAM bank 7 is paged at `$C000`, the phantom fetch
must participate in contention.

## Evidence

Mark Woodmass's GPL-2.0 `HALT2INT v3` archive contains the
`halt2int128.asm` source and its assembled TAP. The program executes
HALT at six boundary addresses, at each of the two candidate first
contended T-states, with RAM banks 0 and 7 paged at `$C000`. It records
24 refresh-register results and prints `HALT: Early` only when every
byte equals its early-128K table. Any other result is reported as
`Late` or `Unknown`.

With the selected table origin, Emu198x produces `HALT: Early` and all
24 source-defined results. The semantic regression runs the original
TAP and decodes the completed screen rather than comparing against an
Emu198x-generated image.

The retained Spectron checkout at revision
`387a72a2f4932a5e77c6b51c67703027ed01db70` supplies an independent
emulator comparison. Its `Halt2IntTests` integration test runs the same
128K TAP and pins the completed framebuffer hash. Its retained
`halt2int_129.png` capture also reports the early profile with the same
24 values.

This establishes a compatibility target. The retained material does
not contain a direct 128K logic-analyser trace, so this decision does
not claim a new physical measurement of the individual clock edges.

## Consequences

- Contention starts before the video-fetch latch opens. It must not be
  gated by `UlaEngine::video` or `border_active`.
- The phase-origin unit test exercises the previous-line pixels before
  the first active scanline.
- `halt2int128_runs_to_completion` is the authoritative regression for
  the complete timing classification. The obsolete self-generated
  `halt2int128.png` golden is removed.
- A change to HALT address-bus behaviour, interrupt alignment, paged
  RAM contention or the delay-table origin must preserve all 24
  diagnostic results.

## Related documents

- [Spectrum architecture review](spectrum-architecture-review.md)
- [ULA first-fetch T-state offset](ula-first-fetch-tstate-offset.md)
- [Z80 interrupt snapshot identity](z80-interrupt-snapshot-identity.md)

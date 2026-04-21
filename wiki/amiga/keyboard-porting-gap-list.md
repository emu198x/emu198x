# Amiga keyboard — port-gap analysis (2026-04-21)

Phase 1 gap list for tasks #173–#175, following the archive-port
methodology proven on CIA, Paula, Agnus, Blitter, Denise, and Floppy.

## What the Amiga keyboard subsystem is

The Amiga keyboard is a self-contained peripheral built around a
6500/1 microcontroller. It shifts keycodes serially over two CIA-A
pins:

- **KCLK** → CIA-A CNT (clock)
- **KDAT** → CIA-A SP (data, active-low)

Each transmission is an 8-bit rotated keycode. The host CIA-A SDR
latches the byte after 8 CNT clocks and raises ICR bit 3 (SP). The
host acknowledges by pulling CRA bit 6 (SPMODE) high briefly, telling
the keyboard it can send the next byte.

## Current-tree coverage

| Area | Current state |
| --- | --- |
| CIA-A `receive_serial_byte(byte)` — sets SDR + ICR bit 3 | ✅ in `mos-cia-8520` |
| CIA-A `cra()` accessor — tracks SPMODE bit | ✅ |
| Host ICR SP flag raised on byte arrival | ✅ |
| Keyboard controller state machine | ❌ absent from live machine |
| Power-up init sequence ($FD, $FE) | ❌ never emitted |
| Key queueing + rotated encoding | ❌ absent |
| Handshake detection (CRA bit 6 rising edge) | ❌ not observed |

The machine boots without keyboard activity. ROM code that depends
on the keyboard power-up handshake (e.g. certain disk strap paths)
can't advance until the keyboard sends $FD + $FE.

## Archive coverage (`crates/peripheral-commodore-amiga-keyboard-archive/`)

| Area | Archive state |
| --- | --- |
| `pub struct AmigaKeyboard` with 7-state machine | ✅ ~170 LoC |
| `tick()` at E-clock rate returns `Option<u8>` encoded byte | ✅ |
| `handshake()` — advances the state machine on CRA bit 6 rise | ✅ |
| `key_event(keycode, pressed)` — queues key press/release | ✅ |
| `encode_keycode` — WinUAE-equivalent rotate+invert | ✅ |
| Power-up delay (~200ms), byte interval (~1ms), handshake timeout (~143ms) | ✅ at 709 kHz E-clock |
| Resend on handshake timeout for $FD and $FE | ✅ |
| Key-byte handshake timeout drops in-flight byte | ✅ (by design — matches WinUAE) |
| In-crate tests | ✅ 7 state-machine tests |

## HRM + Amiga ROM cross-check

**Rotate+invert encoding.** Matches WinUAE `disk.cpp` /
`keybuf.cpp` where `kbcode = ~((keycode << 1) | (keycode >> 7))`.
Archive's `encode_keycode(byte) = !byte.rotate_left(1)` is identical.

**Power-up sequence.** Per the Amiga Hardware Reference Manual
appendix "Keyboard Communications": keyboard issues `$FD` ("initiate
power-up stream") then `$FE` ("terminate power-up stream") after
initialisation. ROM uses this to probe the keyboard before enabling
the disk subsystem.

**Handshake signal.** On CIA-A CRA bit 6 (SPMODE) rising edge, the
host tells the keyboard "I have read your byte; send the next one."
Real keyboards also time out after ~143ms and resend; archive
preserves this behaviour byte-for-byte with WinUAE.

## Known divergences / simplifications

1. **Timer units are E-clock ticks** (709,379 Hz) — approximates real
   µs-scale timing within ~0.1%. Sufficient for all known ROM code.

2. **Key-byte handshake timeout drops the in-flight byte** — matches
   the archive's internal test; WinUAE does likewise. Only triggers
   if the host never reads SDR, which should never happen in normal
   operation.

3. **No CAPS LOCK LED** — the keyboard protocol supports an LED
   control byte from host. Out of scope.

4. **Single keyboard only** — no multi-keyboard setup modelled (the
   real Amiga doesn't support one anyway).

## Per-phase plan

### Phase 1 — characterisation tests (#173)

- **Protocol tests:** power-up delay + $FD + $FE sequence, encode
  round-trip across all 256 keycodes, encode matches WinUAE formula,
  init/term byte resend after handshake timeout, key queueing + key-up
  bit 7, key-byte timeout drops in-flight byte.

Archive's 7 internal unit tests already cover this; Phase 1 promotes
them to integration tests in the live crate's `tests/` directory so
the spec is frozen before Phase 2.

### Phase 2 — port (#174)

- Move the archive crate into a live `peripheral-commodore-amiga-
  keyboard` path (or keep the archive name unchanged — the package
  is already `peripheral-commodore-amiga-keyboard`).
- Machine holds `keyboard: AmigaKeyboard` field.
- At E-clock rate: `if let Some(byte) = kb.tick() { cia_a.receive_
  serial_byte(byte) }`.
- Track CIA-A CRA bit 6 between E-clock ticks; on rising edge call
  `kb.handshake()`.
- Public `AmigaOcs::key_event(keycode, pressed)` routes to
  `kb.key_event(...)`.

### Phase 3 — integrate + retire (#175)

- Rename `peripheral-commodore-amiga-keyboard-archive` →
  `peripheral-commodore-amiga-keyboard`.
- Integration test: fresh machine advanced through ~200ms of E-clock
  ticks with a dummy Kickstart that handshakes on ICR SP → verifies
  the $FD + $FE sequence reaches CIA-A SDR correctly encoded.

## Conclusion

Smallest of the remaining Amiga ports — the keyboard is a self-
contained state machine that plugs straight into CIA-A's existing
serial-byte input API. Blast radius is one field on `AmigaOcs`, one
E-clock callback, one CRA-edge check.

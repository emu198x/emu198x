# Amiga CIA accesses wait for the E clock

Lane: engineering frontier (Amiga timing). Steve asked on 2026-10-22 for
E-clock timing, after a note that Amiga interrupts wait for it.

## What the hardware does

Gary asserts VPA for the CIAs, so each CIA access is a 6800-style cycle:
the 68000 waits for its E clock (a tenth of the CPU clock: six low, four
high) before the access completes. WinUAE's cycle-exact model
(`cia_wait_pre`, `cia_wait_post`, `get_cia_sync_cycles` in `cia.cpp`)
waits for E-clock position 2, then four clocks, the access, and six more:
10 to 19 clocks in all, against 4 for an ordinary cycle. The core gave CIA
accesses 4.

vAmiga agrees (`Agnus::syncWithEClock`, called for every CIA access): it
waits for E-clock position 2, at least six clocks away, and the access
then takes its usual four, so 10 to 19 clocks again, every access ending
at the same point in the E period. The two models differ only in where
they number the E period from, which is arbitrary against the core's own
E-clock counter.

The interrupt acknowledge is **not** an E-clock cycle on the Amiga, despite
the note. Gary decodes `$F80000-$FFFFFF` as ROM whatever the function code,
so the acknowledge reads its vector byte from the top of the Kickstart ROM,
whose last bytes are `$0019`-`$001F` (checked in Kickstart 1.2, 1.3 and
2.04). WinUAE's `iack_cycle` reads it as an ordinary four-clock access.
Minimig asserts VPA for acknowledge instead, but that is its own core's
convenience. The 68000 interrupt timing change therefore stands as it
is.

## Change

`common-commodore-amiga`'s driver holds a CIA access (`$BFD000-$BFEFFF`,
the window its bus dispatch serves) until the E-clock position WinUAE's
model reaches: the response comes two clocks before the cycle ends, at the
second position-2 point at or after the cycle began. It is worked out from
the CPU's own cycle count and the driver's existing E-clock phase counter,
so no state is added and snapshots are unaffected.

Out of scope:

- machines behind an accelerator bridge (the GVP A530), and processor
  clocks that are not a whole multiple of the system clock, keep their
  timing;
- the CIA decode stays the `$BFDxxx`/`$BFExxx` window; Gary's wider
  `$A00000-$BFFFFF` mirror is a separate question;
- WinUAE's interrupt-sampling delay after a CIA access.

## Verification

- `tests/cia_e_clock.rs` times back-to-back `TST.B $BFE001` from a
  synthetic ROM: 22 to 31 clocks each, all ending at one E-clock position.
  On main each takes 16.
- The Amiga machine, runtime and front-end tests, the ROM-backed tests and
  goldens, and the six Dizzy games.

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

Three steps, each its own PR.

1. **The wait** (#2003). `common-commodore-amiga`'s driver holds a CIA
   access until the E clock reaches the point the references agree on,
   worked out from the CPU's own cycle count and the driver's existing
   E-clock phase counter, so no state is added and snapshots are
   unaffected.

2. **Where the wait ends, and CIA space.**
   - #2003 placed the end point arbitrarily, two clocks after the CIAs
     count. It is not arbitrary: vAmiga runs its CIAs on whole E periods
     from power-on (`CIA::wakeUp`), syncs an access to position 2 and ends
     it at position 6; WinUAE counts its timers from the last synced
     access (`CIA_update_check`) and ends the access six clocks after.
     Both therefore end the access six clocks after the count it
     coincides with, and the read sees that count. The access now ends
     there (`CIA_ACCESS_END_PHASE`).
   - CIA space follows the Hardware Reference Manual (appendix F): the
     whole `$A00000-$BFFFFF` region, CIA-A on data bits 7-0 when A12 is
     low, CIA-B on bits 15-8 when A13 is low, both when both are low,
     A11-A8 the register (`cia::decode_cia_select`). Gayle boards (A600,
     A1200) pass only the `$xxDxxx` and `$xxExxx` pages, as WinUAE's
     `isgaylenocia` does. A byte read takes the CIA on its lane; a lane no
     selected CIA drives returns the prefetch word, as WinUAE does on the
     68000. A written byte goes to every selected CIA, since the 68000
     puts it on both lanes; a word gives CIA-B the high byte and CIA-A the
     low. The E-clock wait covers all of CIA space, including addresses
     that select neither CIA.
   - The debugger and test backdoors keep the narrow `$BFDxxx`/`$BFExxx`
     decoders.

3. **The GVP A530** keeps its timing until the bridge counts motherboard
   time for the wait; that needs a field in the bridge's saved state.
   This also covers the only processor clock that is not a whole multiple
   of the system clock.

Out of scope: WinUAE's interrupt-sampling delay after a CIA access.

## Verification

- `tests/cia_e_clock.rs`:
  - back-to-back `TST.B $BFE001` from a synthetic ROM take 22 to 31
    clocks each, all ending at one E-clock position (16 before #2003);
  - a free-running timer A read in a loop: every read ends six clocks
    after the timer counts and returns the value it then holds (fails with
    the end point #2003 used);
  - CIA space: writes through `$A0E301` and `$B4D300`, a word read at
    `$BFD300`, and a both-selected read and write at `$BFC2xx`/`$BFC3xx`.
- `cia::tests` pins the decode, including Gayle's narrower pages.
- The Amiga machine, runtime and front-end tests, the ROM-backed tests and
  goldens, and the six Dizzy games.

# Atari 800XL — boot to BASIC READY (resolved)

**Status: SOLVED.** The 800XL now cold-boots all the way to the BASIC `READY.`
prompt. With the MiSTer Atari800 OS + BASIC ROM bundle
(`~/.emu198x/roms/atari-800xl/atarixl.rom` + `ataribas.rom`) the OS sizes RAM,
detects built-in BASIC, attempts and times out the SIO disk boot, runs the
BASIC cartridge, and prints `READY` (screen-RAM codes `32 25 21 24 39`) by
~frame 300, then idles in the keyboard-input wait. Guarded by
`crates/machine-atari-800xl/tests/basic_boot_probe.rs::boots_to_basic_ready`.

## The chain of bugs (each its own commit)

The boot traversed several distinct hardware-emulation bugs, most of them the
same "a register's bits were assigned to the wrong source" shape:

| Area | Bug | Fix |
|---|---|---|
| PIA (`machine-atari-800xl` wiring) | Atari cross-wires CPU A0↔A1 into the PIA RS pins; `$D300/01/02/03` = PORTA/PORTB/CRA/CRB | `bus_to_pia_addr` swaps address bits 0/1 |
| ANTIC NMIEN | VBI/DLI enable bits swapped (VBI = bit 6, DLI = bit 7) | corrected bit tests |
| ANTIC DL instruction | DLI/LMS bits swapped (DLI = bit 7, LMS = bit 6) | corrected decode |
| ANTIC hi-res text colour | modes 2/3/F: bg = COLPF2, fg = COLPF2 hue + COLPF1 lum | dedicated compose path |
| ANTIC CHACTL | inverse/blank bits swapped (inverse = bit 1, blank = bit 0) | corrected; cursor renders |
| GTIA CONSOL | read returned the speaker write-latch, not the switches → OPTION looked held → BASIC disabled | split read (switches) from write (speaker) |
| POKEY serial port | no serial transmit → SIO disk boot hung, never timed out to BASIC | two-stage edge-latched serout model |

## POKEY serial — the final piece (the one that needed care)

The built-in BASIC cartridge control byte `$BFFD = $05` sets bit 0 ("boot
peripherals"), so the OS attempts a disk/cassette boot over SIO before running
the cartridge. With no drive it must transmit the command frame, get no ACK,
and time out — falling through to BASIC.

POKEY transmits each byte in **two observable stages**, each surfaced as a
separate IRQST flag, and both are **edge** events the CPU clears by toggling
IRQEN:

* **bit 4** ("serial output data needed") — asserts when the holding register
  empties (ready for the next byte).
* **bit 3** ("serial output transmission finished") — asserts when the shift
  register empties.

The decisive detail: the OS IRQ dispatcher (`$C052` priority scan over the
mask table at `$C0CF`) services **bit 4 (X=6) at a higher priority than
bit 3 (X=5)**. So bit 4 must be a true one-shot — assert once per
holding-empty, then let the dispatcher's IRQEN-toggle ack clear it. An earlier
attempt held bit 4 asserted as a *level*; the dispatcher then looped on bit 4
forever and never reached the bit-3 "transmission done" handler, so the frame
never completed. (That attempt was reverted before the edge model landed.)

Implementation (`crates/atari-pokey/src/lib.rs`):

* `serout_hold_delay` → bit 4 edge (≈ `SEROUT_HOLD_TICKS`).
* `serout_shift_delay` → bit 3 edge (≈ `SEROUT_SHIFT_TICKS`).
* A SEROUT write arms both timers and marks both bits busy.
* Writing SKCTL with bit 5 (`SKCTL_SEROUT_ENABLE`, the OS uses `$23`) while
  idle primes bit 4, so the polled command-frame send at `$CF2D` and the first
  interrupt byte can start.
* The serial *input* side needs nothing: with no device, bit 5 never asserts,
  so the OS's higher-level CDTMV3/TIMFLG timeout fires and SIO returns failure,
  and the coldstart runs `JMP ($BFFA)` into BASIC at `$A000`.

## OS landmarks (OS-XL ROM, for future reference)

- `$C052` IRQ priority scan; `$C0CF` mask table (bit 4 highest, then bit 3).
- `$CF2D` polled serial-output transmit wait (bit 4).
- `$EA9E` interrupt-driven serial-output handler; `$EACD` enables bit 3 after
  the last byte.
- `$CEBB` serial-input (ACK) wait (bit 5 / BREAK).
- `$C3C4` cart-present test (`RAMSIZ < $B0` and `$BFFC == 0` → `$06 = 1`).
- `$C49A` BASIC enable: CONSOL OPTION released → clear PORTB bit 1.

## Verification

`cargo test -p machine-atari-800xl --test basic_boot_probe -- --ignored`
runs both gated tests: `basic_boot_programs_antic_and_gtia` (render) and
`boots_to_basic_ready` (BASIC LOMEM/VNTP non-zero + "READY" in screen RAM).

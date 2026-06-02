# Atari 800XL — SIO disk-boot timeout → BASIC READY (handoff)

**One-line summary**: The 800XL now boots cleanly, renders the GR.0 screen
correctly, and the OS coldstart correctly detects built-in BASIC. The only
thing between us and a `READY.` prompt is emulating POKEY's serial port well
enough that the OS's disk-boot attempt transmits its command frame, gets no
ACK, and times out — falling through to run the BASIC cartridge. This needs a
faithful **two-stage POKEY serial-transmit model plus the OS serial-IRQ
dispatch order**, which is more than a quick patch.

## What is already fixed and committed

| Commit | Fix |
|---|---|
| `069ea850` | Boot: PIA register addressing (A0↔A1 cross-wire) + ANTIC NMIEN bit order |
| `6d7d3efe` | Render: ANTIC DL LMS/DLI bit order, hi-res text colour, CHACTL bit order |
| `098bc701` | GTIA CONSOL split (read = switches, write = speaker latch) → BASIC detected |

After these, with the MiSTer Atari800 OS + BASIC ROM bundle
(`~/.emu198x/roms/atari-800xl/atarixl.rom` + `ataribas.rom`):

- CPU never executes an illegal opcode; RTCLOK advances via VBI.
- Display: black border, blue COLPF2 GR.0 field, light-blue cursor at the
  left margin (column 2 = LMARGN). Pixel-correct.
- Coldstart keeps BASIC enabled: PORTB stays `$FD`, RAM sizes to `$A0`
  (RAMSIZ `$A0`, correct for a BASIC-enabled XL), and the cartridge check
  detects built-in BASIC (`$06 = $01`).

## Why there is no READY yet

The built-in BASIC cartridge control byte `$BFFD = $05` has **bit 0 set
("boot peripherals")**, so the XL coldstart attempts a disk/cassette boot over
SIO *before* running the cartridge. On real hardware with no drive, that SIO
operation times out (after retries) and the OS falls through to run BASIC. Our
POKEY does not emulate the serial port, so the SIO send never completes.

### The OS SIO send/receive path (OS-XL ROM)

- `$CF2D` — **polled** serial-output transmit wait: `LDA #$10; BIT $D20E;
  BNE` — spins until IRQST **bit 4** (serial-output-data-needed) clears.
  Used for the command frame. *(This is where the CPU parked before any
  serial work.)*
- `$EA9E`–`$EADB` — **interrupt-driven** serial-output handler: on each
  bit-4 IRQ it sends the next buffer byte via `STA $D20D` (SEROUT),
  advances the pointer (`$32/$33` vs end `$34/$35`), and on the last byte
  sends the checksum and (`$EACD`) enables IRQST **bit 3**
  (transmission-finished) in POKMSK (`$10`).
- `$CEBB` — serial-input (ACK) receive wait: `LDA #$20; BIT $D20E; BPL
  break; BNE` — exits only on bit 5 (serial-input-ready) or bit 7 (BREAK).
  **No timeout check in this loop** — the timeout is enforced at the
  higher SIO level via CDTMV3 (`STA $0226` at `$EDE4`) → TIMFLG (`$0317`,
  checked at `$EB1D/$EBC6/$EBF0/$ED45/$ED6C`).

### POKEY IRQST serial bits (confirmed vs `pokey.vhdl`)

- bit 3 (`$08`) = serial output transmission finished (`serout_active` on read)
- bit 4 (`$10`) = serial output data needed (holding register empty)
- bit 5 (`$20`) = serial input data ready

## What was tried (and reverted)

A coarse serial-output model in `atari-pokey`:

1. **Single-stage** (`serout_delay`): on SEROUT write set bit 4 busy; clear it
   after N ticks. Result: the OS got *past* `$CF2D` and ran the full SIO
   command-frame send + receive + retry cycle (real progress — `$EA9E`,
   `$CEC3`, and the `$EA9E` retry logic all became live). But it looped in the
   send handler forever and never reached BASIC.
2. **Two-stage** (`serout_hold_delay` → bit 4, `serout_shift_delay` → bit 3):
   modelled holding-empty and shift-done separately. Still looped in `$EADB`.

Both reverted (an infinite SIO loop is worse than a clean park). The clean
checkpoint is the three commits above.

## The actual remaining blocker

POKEY's "output data needed" (bit 4) must be a **latched one-shot**, not a
level:

- It should assert (0) once when the holding register empties (and once when
  serial output is first enabled, to prime the polled `$CF2D` send and the
  first interrupt byte).
- It must **not** re-assert every tick — a level made the bit-4 IRQ fire
  continuously, and the OS handler at `$EACD` enables bit 3 *without* clearing
  bit 4 from POKMSK, so a level-asserted bit 4 re-enters the handler forever.
- After the checksum byte, the transmitter must raise bit 3 (done) so the
  OS's **serial-output-complete** handler runs, disables the serial IRQs, and
  lets SIO proceed to the ACK wait (`$CEBB`), which then times out via CDTMV3.

The open question that decides the design: **the OS IRQ dispatcher's priority
between bit 3 (done) and bit 4 (needed).** If it services bit 4 before bit 3,
a bit-4 that stays asserted in the done window starves bit 3 and never
terminates. Find the serial-IRQ dispatch order in the OS ROM (the IRQ vector
routine) before finalising the latch/ack semantics — model bit 4 as a true
edge-latched interrupt cleared by SEROUT-write or IRQEN-ack, matching what the
dispatcher expects.

## Suggested next-session plan

1. Disassemble the OS IRQ dispatcher; establish bit-3 vs bit-4 service order
   and exactly how bit 4 is acknowledged (SEROUT write vs IRQEN write vs
   IRQST read).
2. Implement POKEY serial output as edge-latched bit 4 + bit 3, primed on
   serial-output enable (gate on SKCTL serial mode), with a "no device"
   serial input (bit 5 never asserts).
3. Verify the OS: command frame transmits → ACK times out (CDTMV3/TIMFLG) →
   SIO returns error → coldstart runs `JMP ($BFFA)` into BASIC `$A000` →
   `READY.` appears in screen RAM (screen moves to ~`$9C40` with RAMTOP
   `$A0`; "READY" screen codes are `32 25 21 24 39`).
4. Add a regression test asserting BASIC runs (LOMEM/VNTP non-zero, or
   "READY" present) within a frame budget.

## Reproduction harness

The throwaway diagnostics used this session (boot a 600–3600 frame run,
follow SDLST→LMS for the live screen address, scan for "READY" screen codes,
histogram park PCs, check `$06`/`$02E4`/PORTB and BASIC `$80/$82` zero-page)
were deleted but are trivial to recreate from this doc. Park-PC histogram +
"PC ever in `$A000-$BFFF`?" is the fastest signal that BASIC ran.

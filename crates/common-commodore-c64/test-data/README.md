# CIA1 keyboard port reads

`vice-matrix-2x2.bin` contains 20,736 PA/PB byte pairs from VICE 3.10's
unmodified `read_ciapa` and `read_ciapb` functions. No ROM or guest code is
included. The regeneration script and source/compiler hashes are in the
Emu198x docs repo, `plans/2026-10-10-c64-keyboard-ghosting/reference.py` and
its adjacent evidence.

Order: 16 masks of four contacts (PA0/PB0, PA0/PB1, PA1/PB0, PA1/PB1),
then the 81 base-three combinations of PA0/PA1/PB0/PB1 direction and latch
state (input, output-low, output-high; PB1 varies fastest), then four
joystick-2 low masks and four joystick-1 low masks. Other pins are inputs,
their latches high, and other keys released. Each result is PA followed by PB.

SHIFT LOCK is disabled and timers do not drive PB6/PB7. This verifies the
digital matrix and VICE's ordinary-key output-contention rules. It is not
an original-hardware or analogue-network measurement. The independent native
guest additionally checks selected configurations in the complete emulators.

`vice-shift-lock.bin` adds 20,736 pairs for the separate SHIFT LOCK contact.
Order: lock open/closed, 16 ordinary-key masks (PA1/PB1, PA1/PB7, PA2/PB1,
PA2/PB7), 81 driver states (PA1/PA2/PB1/PB7, PB7 fastest), four joystick-2
low masks on PA1/PA2, then two joystick-1 masks on PB1. A closed lock adds
PA1/PB7 independently of the ordinary keys and supplies VICE's lock flag.
Timer outputs remain disabled. Generate it with the adjacent docs plan's
`2026-10-10-c64-shift-lock/verify.py` and `sweep.py`; the plan records exact
commands and source hashes. The fixture SHA-256 is
`18ebbbac5069ce38e3a98b7b666e80c9b8605f4dae73f9dec1a639fb7b924649`.
The native lock guest compares Emu198x with these compiled VICE functions;
VICE's KEYBOARD snapshot does not preserve its separate lock flag.

## VIC-II unused registers

`vic-unused-registers.prg` is an original 6502 guest loaded at `$0800`. It
reads `$D02F–$D03F` in each of the sixteen mirrors through `$D3FF`, writes a
varying byte, and reads again. The 544 results at `$6000–$621F` must all be
`$FF`; `$6220 = $A5` proves completion. IRQs, display and sprites are disabled.

The exact guest returns all 544 expected bytes on native VICE 3.10's PAL,
NTSC, old NTSC, PAL C64C and NTSC C64C models. The machine regression runs it
on all four shipping C64 board profiles; the chip regression additionally
covers the old NTSC VIC-II. The generator, native output and input hashes
are in the docs repo under `plans/2026-10-11-vicii-unused-registers/`.
These are digital register-read observations, not physical-hardware captures.

## C64C PRA bank switching

`vic-bank-transient-00.prg` and `vic-bank-transient-ff.prg` are original CPU
programs loaded at `$0800`. They fill identical screen matrices, enable text
display and alternate banks 1/2 through 8,192 PRA stores. Only bank 3's character
zero differs between the programs. Both finish with `$6000/$D7FF = $A5`.

Native VICE's discrete-glue controls produce identical images; custom glue
exposes bank-3 glyph bits for one fetch after a bank 1/2 swap. Captures on five
chip profiles qualify the store-to-pixel relation, including shifted-window
negative controls. The generator (`bank_transient.py`), native captures and
phase verifier (`bank_phase.py`) are in docs PR 89 under
`plans/2026-10-11-c64-phi1-observation/`. These are reference-emulator observations,
not new physical captures. DDRA's separate transient behaviour is not covered.

The CPU regression checks every framebuffer pixel against the traced stores
on all four shipping boards, through both normal and IEC clock paths. It uses
synthetic reset-vector firmware, so the regression needs no external ROMs.

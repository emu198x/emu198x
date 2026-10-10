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

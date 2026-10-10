# CIA1 ordinary-key port reads

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

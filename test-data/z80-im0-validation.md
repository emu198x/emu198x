# Z80 interrupt mode 0 validation

IM 0 executes the device-supplied instruction through the shared decoder,
including operands and prefixes. Opcode/prefix fetches use six-T interrupt
acknowledgements; operand bytes use ordinary read cycles without advancing PC.
The device must supply those bytes on the exposed pins. Undriven FF still
executes RST 38h. NOP and JP(HL) take 6T without pushing the stack; CALL takes
19T and pushes the interrupted PC.

The basis is Zilog's Z80 CPU User Manual, Interrupt Response section (IM 0
permits any instruction, including CALL), and
[interrupt research](https://github.com/redcode/Z80/wiki/Interrupts).
The shared private reference note is
`reference/by-topic/cpu-z80/z80-im0-instruction-stream.md` in the umbrella.

## Independent comparison

Original pin-driving adapters compared 25 streams with the die-derived Perfect
Z80 simulator, hoglet67/perfect6502 branch Z80 revision
`9b0d2e5e826c3a5fae3b5c6669bba1cd5d3b4217`. PC/SP/WZ/AF/BC/HL, byte consumption
and response duration agree in all 25. Cases include NOP, JP, RST, RET, PUSH,
INC, immediate LD, CALL and conditional branches, repeated/overridden prefixes,
CB/ED, indexed BIT/rotate, EI/DI/HALT and a repeating LDIR iteration.

The netlist writes some registers during the next M1: the probe records the
fetch address and read-strobe time, forces NOP, then waits seven half-cycles
before reading registers. The strobe is one half-cycle beyond the emulator's
response boundary. This explicitly accounted-for difference is not extra
interrupt latency. Reproduction adapters and derived observations are retained
in the private umbrella at `ops/experiments/z80-im0/`; third-party source and
binaries remain local. This is simulation, not new physical-chip measurement.

## Regression coverage

`crates/emu198x-zilog-z80/tests/im0.rs` covers injected instructions and operand
addresses, prefix timing/R increments, snapshots at every response half-cycle,
wait extension, observer stack-flow events, EI inhibition, NMI deferral and Q
preservation. It also runs OTIR → injected JP(HL) → BIT/PUSH end to end, exposing
the repeated-output WZ value through flags and the stack. The old implementation
fails the new instruction/operand regressions.

Serialized prefix tags retain their existing values. Old snapshots taken inside
the former RST-only response continue that legacy response; newly accepted IM 0
interrupts use the shared decoder. Forward loading new variants in older builds
is not promised.

Validation on 2026-09-24: 193 ordinary Z80 tests pass; all 1,604,000 Tom Harte
cases match exactly; FUSE retains 1,350 exact cases and six pinned discrepancies
out of 1,356, with no unexpected failures. Formatting and Z80 all-target Clippy
pass. All six pinned Rak 1.2a tape exercisers also pass (251.64 seconds).
These instruction corpora do not themselves exercise injected interrupts;
the dedicated regressions and differential probe do.

Earlier full cold ZEX results remain tied to their recorded revision; they are
not claimed as a rerun of this change. The probe does not establish every bus
edge, every instruction/operand combination, or CMOS-specific behaviour.

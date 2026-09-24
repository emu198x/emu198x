# WZ at repeated output boundaries

For a non-final OTIR/OTDR iteration, WZ becomes the instruction-start PC+1,
wrapping at 16 bits. On the terminating iteration it remains post-decrement
BC+1 for OTIR or BC-1 for OTDR. Output address, data, flags and cycle counts
are unaffected by this correction.

## Evidence

David Banks' [OTIR investigation](https://github.com/hoglet67/Z80Decoder/issues/2#issuecomment-1062812573)
uses a die-derived Z80 simulation to expose WZ between iterations: an IM 0
device supplies `JP (HL)`, which preserves WZ, followed by `BIT 0,(HL)` to
observe its undocumented flag bits. His [probe and decoded traces](https://github.com/hoglet67/Z80Decoder/issues/2#issuecomment-1062908413)
distinguish the PC-derived value from the port-derived value.

On 2026-09-24, the upstream `z80otir.c` at
[`hoglet67/perfect6502`, revision `9b0d2e5`](https://github.com/hoglet67/perfect6502/blob/9b0d2e5e826c3a5fae3b5c6669bba1cd5d3b4217/z80otir.c)
was built and run locally. After 250 half-cycle calls it reported WZ `000e`,
BC `fe01`, PC `0028`, AF `5510`. The instruction starts at `000d`, while
BC-derived WZ would be `fe02`. Changing opcode `b3` to `bb` and initial HL
`0020` to `0022` preserved the interrupt destination and yielded the same
register results for OTDR; its BC-derived alternative would be `fe00`.
The only build portability change renamed the unused netlist enum constant
`wait` to `wait_node` to avoid a macOS declaration collision. No node or
transistor connection changed.

SpecIde's `Z80Otir.h`/`Z80Otdr.h` case 8 also assigns the rewound PC+1.
FUSE 1.7.0 retains BC-derived WZ at this boundary. The die-derived probe
supports correcting the core rather than preserving that FUSE value.

These are simulation observations, not new physical-chip measurements.
The umbrella source note is `reference/by-topic/cpu-z80/z80-output-repeat-memptr.md`.

## Regression and coverage

`block_output_wz_at_repeat_and_termination_boundaries` fails on the previous
core and passes after the correction. It samples both opcodes, B values
0/1/2/255, C values 0/255 and PC values `000d`/`28ff`/`ffff`. It checks WZ,
PC and B after one 21T repeat or 16T terminal iteration, then continues
repeating cases through termination. This covers B wraparound, PC+1 wrap,
and final BC±1 wrap without relying on a corpus exception.

The full Tom Harte corpus now requires exact state matches with no WZ
exception. FUSE's existing OTIR/OTDR exceptions explicitly pin the new WZ
disagreement as well as their existing AF disagreement.

Rak 1.2a's output MEMPTR tests observe termination, so both intermediate
WZ rules can pass them. Its input self-modifying cases expose the repeat
value before termination. A passing output MEMPTR test alone is therefore
insufficient evidence for the repeat rule.

Emu198x currently handles only RST opcodes in IM 0; the exact upstream
interrupt demonstration is not an emulator regression. The direct boundary
test covers WZ without claiming broader IM 0 support or transistor-level
agreement on the internal update half-cycle.

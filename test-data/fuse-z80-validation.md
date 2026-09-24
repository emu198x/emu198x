# FUSE Z80 validation coverage

Observed on 2026-09-24 using the vendored FUSE 1.7.0 corpus: **1,356 cases
executed, 1,350 exact matches, six accepted disagreements, zero unexpected
failures**. This is compatibility with a specific reference corpus, not a
claim of perfect hardware accuracy.

| Fixture | SHA-256 |
|---|---|
| `tests.in` | `9f36e866f22e72ff1f8bf2100bf70ffbf58edd97b453500aab60acf1f403ebbb` |
| `tests.expected` | `15a6946f4addcf97e137b5bdd1d5fdb08124ff91f1b169f36a8bf4afe4bab6e4` |

The [harness](../crates/emu198x-zilog-z80/tests/z80_fuse.rs) compares bus events,
final registers, memory and T-state totals. Accepted cases must disagree on
exactly the named fields. Their values are printed for diagnosis, but the
allowlist does not pin those values: a different value within an accepted
field can still pass. Empty corpora, unmatched named cases and zero case limits
fail. A single selected case is valid partial coverage.

## Measured disagreements

Values below are emulator / fixture; AF includes A in the high byte.

| Case | Instruction | Difference | Explanation supported by source inspection |
|---|---|---|---|
| `76` | HALT | PC `0001 / 0000` | FUSE decrements PC on HALT and increments it when leaving HALT for an interrupt. The exposed halted-PC representation differs. |
| `edb2_1` | INIR | AF `8a00 / 8a0c`; WZ `0001 / 0a41` | Flag XOR `0c`: X and P/V. Repeat adjustment replaces X/Y with PC-high bits and adjusts parity. WZ uses PC+1 here, original BC+1 in FUSE. |
| `edb3_1` | OTIR | AF `3403 / 3417` | Flag XOR `14`: H and P/V, both changed by repeat adjustment. |
| `edb9_2` | CPDR | AF `ffaf / ffa7` | Flag XOR `08`: X. The repeating compare takes X/Y from PC high `7a`; FUSE retains the subtraction-derived bits. |
| `edba_1` | INDR | WZ `0001 / 069e` | PC+1 here, original BC-1 in FUSE. For this input, the flag adjustment leaves F unchanged. |
| `edbb_1` | OTDR | AF `0903 / 0917` | Flag XOR `14`: H and P/V, both changed by repeat adjustment. |

All five block-repeat fixtures observe one repeating iteration at 21 T-states,
not instruction termination. CPDR is a block compare, not block I/O.

## Reference comparison

FUSE 1.7.0 `z80/z80_ed.c` calculates ordinary block-operation flags but does
not apply our repeat adjustment. SpecIde's `source/src/Z80Inir.h`, `Z80Indr.h`,
`Z80Otir.h` and `Z80Otdr.h` apply the same X/Y, H and parity transformation as
our `repeat_block_io_flags`; `Z80Cpdr.h` also replaces X/Y from PC high on
repeat. These source paths are in the umbrella's read-only emulator snapshots.

INDR's apparent exception follows from its input: B becomes 5, PC high is
zero, carry is clear, and parity of `5 & 7` is even. The transformation changes
neither X/Y nor P/V for that case. Agreement for INDR and disagreement for
its siblings therefore does not, by itself, demonstrate an inconsistent core.

This explains the flag differences between implementations; it does not
establish which implementation matches every hardware observation. Full
SpecIde differential execution was not performed. No CPU behaviour or accepted
case list changed in this investigation.

## Repeated-input WZ control experiment

With the pinned z80test 1.2a MEMPTR tape (SHA-256
`444582ddfa4d05711b6235e743ddf68295231e97ba92cc838c5d09a106c9a10f`),
temporarily retaining BC-derived WZ in repeated INIR/INDR instead of assigning
PC+1 fails exactly cases 102 (`INIR->NOP'`) and 103 (`INDR->NOP'`): 158/160.
Restoring the original code passes all 160 cases. Both runs used the same ROM
and tape, with strict fixture mode enabled.

Matching FUSE's repeated-input WZ value is therefore not a safe isolated fix.
The experiment constrains that proposed change; it does not prove that every
intermediate half-cycle in the current model is correct. A next investigation
needs equivalent observation points and a trace through the repeat boundary,
with both corpus versions pinned. The six accepted FUSE cases do not yet
establish a CPU defect that can be fixed without conflicting evidence.

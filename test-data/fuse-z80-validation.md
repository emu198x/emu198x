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
exactly the named fields and match both the observed and reference values
listed below. A changed value, extra mismatch or resolved disagreement fails
until its evidence and exception are reviewed. Values are printed for diagnosis. Empty corpora, unmatched named cases and zero case limits
fail. A single selected case is valid partial coverage.

## Measured disagreements

Values below are emulator / fixture; AF includes A in the high byte.

| Case | Instruction | Difference | Explanation supported by source inspection |
|---|---|---|---|
| `76` | HALT | PC `0001 / 0000` | FUSE decrements PC on HALT and increments it when leaving HALT for an interrupt. The exposed halted-PC representation differs. |
| `edb2_1` | INIR | AF `8a00 / 8a0c`; WZ `0001 / 0a41` | Flag XOR `0c`: X and P/V. Repeat adjustment replaces X/Y with PC-high bits and adjusts parity. WZ uses PC+1 here, original BC+1 in FUSE. |
| `edb3_1` | OTIR | AF `3403 / 3417`; WZ `0001 / 02e1` | Flag XOR `14`: H and P/V, both changed by repeat adjustment. WZ uses PC+1 here, decremented BC±1 in FUSE. |
| `edb9_2` | CPDR | AF `ffaf / ffa7` | Flag XOR `08`: X. The repeating compare takes X/Y from PC high `7a`; FUSE retains the subtraction-derived bits. |
| `edba_1` | INDR | WZ `0001 / 069e` | PC+1 here, original BC-1 in FUSE. For this input, the flag adjustment leaves F unchanged. |
| `edbb_1` | OTDR | AF `0903 / 0917`; WZ `0001 / 033a` | Flag XOR `14`: H and P/V, both changed by repeat adjustment. WZ uses PC+1 here, decremented BC±1 in FUSE. |

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

The [output-repeat investigation](z80-output-repeat-wz.md) reproduces a
die-derived probe supporting PC+1 at the repeat boundary. A five-case
SpecIde/Emu198x probe also compared completed 21T iterations. This is not a
full SpecIde differential corpus. The six accepted FUSE case names remain
the same; OTIR/OTDR now explicitly pin WZ as well as AF.

## Die-derived adjudication (2026-09-24)

Original pin-driving adapters reproduced the instruction-relevant inputs of all
five disputed block-repeat fixtures with Perfect Z80, hoglet67/perfect6502 branch
Z80 revision `9b0d2e5e826c3a5fae3b5c6669bba1cd5d3b4217`. All five agree with
Emu198x on AF, BC, HL, WZ and next-fetch address after 21T. In particular they
confirm AF `8a00`, `3403`, `ffaf`, `2500`, `0903` for INIR, OTIR, CPDR, INDR,
and OTDR respectively. The six precisely constrained exceptions remain; no
CPU behaviour was changed to match the corpus.

A minimal HALT-at-0000 probe fetches from 0001 after 4T and NMI pushes return
address 0001, agreeing with our post-HALT address convention. This does not
require FUSE's internal halted-PC representation to be the same.

The simulator overlaps register writeback with the following M1. The adapter
captures its fetch address and timing, forces NOP, then allows seven half-cycles
for writeback before reading registers. Timing is measured between read strobes;
the settling cycles are excluded. The adapter supplies the port-address high
byte on input, as this harness does. These are selected fixture-shaped inputs,
not a full-state fixture replay, exhaustive waveform check or new physical-chip
measurement. Exact internal flag/WZ writeback timing remains a separate question.

Reproduction and derived observations are retained privately in the umbrella at
`ops/experiments/z80-fuse-disagreements/`, with the shared reference note
`reference/by-topic/cpu-z80/z80-fuse-disagreement-evidence.md`. No third-party
source or binaries are added to this repository. The ordinary integration tests
`disputed_repeat_flags_match_die_derived_observations` and
`halted_fetch_and_nmi_return_use_post_halt_address` preserve these observations
without requiring external fixture files.

## Repeated-input WZ control experiment

With the pinned z80test 1.2a MEMPTR tape (SHA-256
`444582ddfa4d05711b6235e743ddf68295231e97ba92cc838c5d09a106c9a10f`),
temporarily retaining BC-derived WZ in repeated INIR/INDR instead of assigning
PC+1 fails exactly cases 102 (`INIR->NOP'`) and 103 (`INDR->NOP'`): 158/160.
Restoring the original code passes all 160 cases. Both runs used the same ROM
and tape, with strict fixture mode enabled.

Matching FUSE's repeated-input WZ value is therefore not a safe isolated fix.
The experiment constrains that proposed change; it does not prove that every
intermediate half-cycle in the current model is correct. The output-repeat investigation supplies separate boundary evidence for
OTIR/OTDR; it does not change the repeated-input rule.

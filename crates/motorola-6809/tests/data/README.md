# MC6809 register-transfer vectors

`mixed-transfers.txt` contains 800 register-value cases: all 100 ordered pairs
of named registers, both TFR and EXG, and four initial register sets. Of these,
384 use different widths; 416 cover same-width controls. Each case specifies
all nine input and output registers, including the PC after the postbyte read.

The vectors come from compiling the unchanged EXG/TFR switch bodies in XRoar's
`src/mc6809/mc6809.c`, with a small wrapper retaining the D/A/B alias. This is
an independent implementation comparison, not a hardware trace. The generator
and provenance are in the [evidence plan](https://github.com/emu198x/docs/blob/main/plans/2026-10-11-6809-mixed-transfers.md).

- XRoar repository: `https://www.6809.org.uk/git/xroar.git`
- XRoar revision: `b8491ab945d3e83fc1e98d62359813cf8a537d1d`
- XRoar source SHA-256: `1fd3c2b9379dfc630963c207f8d835c80f43491f5f887be7e0fd84375a31b639`
- Generated wrapper SHA-256: `9d54dc35cbdf0169c2931018cc739beced37fc72143b3e25770e2f1cae0e461a`
- Vector SHA-256: `7e7b608f5941919b025d76b8d23cbc2ecbf996f332dfa410e94c5087f655d88b`

The Rust regression executes instructions through the CPU's public pins. It
checks every register, the next opcode-fetch address/signals, and the existing
six-cycle TFR/eight-cycle EXG schedule. The reference wrapper checks values and
write order only: it does not validate bus timing or NMI arming.
Undefined register selectors remain outside this fixture.

## Published hardware observations

[Ciaran Anscomb's original Dragon article](https://www.6809.org.uk/dragon/illegal-opcodes.shtml)
reports low-byte preservation and upper-byte fill for mixed widths.
[Tim Lindner's CoCo 2B experiment](https://tlindner.macmess.org/?p=945)
provides test source, binary and a real-hardware result image. Use the 6809
CoCo 2B result, not the 6309 result on the same page. It supports CC/DP
asymmetry and D/A/B alias ordering as well as ordinary mixed transfers.
The explicit hardware-example test uses its rows 6-10; LDX clears V after
CC is set to 3 in the original program, so those CC exchanges begin with 1.

These observations support this MC6809 behaviour; they are not a survey of
all manufacturers or silicon revisions. No new physical measurement was made.

## Inherent opcode aliases

`inherent-aliases.txt` contains 480 bus traces from the unchanged full XRoar
MC6809 core at the revision above. It covers NOP, NEGA/B and LSRA/B and their
five aliases (1B, 41/51, 45/55), six operands, four CC seeds and PCs 4000/FFFF.
The regression checks cycle count, A/B/CC/PC, unchanged other registers,
operand memory and every read/write address and byte. The discarded second
read uses the next PC, including wrap to 0000.

The exact aliases are supported by [David Banks's hardware-validated record](https://github.com/hoglet67/6809Decoder/wiki/Undocumented-6809-Behaviours).
The byte-level traces are reference-implementation evidence, not new physical
captures. The [plan and full diagnostic matrix](https://github.com/emu198x/docs/blob/5c9c554dadd4bc98935a876be766c3e930a0828e/plans/2026-10-11-6809-opcode-aliases.md)
retain the memory cases that still require a staged timing correction.
The generator is a memory/trace wrapper around the full external CPU source;
it stops at the instruction post-hook. It does not reproduce opcode semantics.

## Register-instruction discarded reads

`register-reads.txt` contains 3,072 full-reference traces: 32 opcode encodings,
eight A values (B is A XOR A5), four CC seeds and PCs 4000/FFFE/FFFF. It covers
the ten earlier encodings and DAA, SEX, ABX, MUL, ANDCC, ORCC, plus A/B forms of
COM, ROR, ASR, ASL, ROL, DEC, INC and CLR. The regression compares ordered bus
reads/bytes, cycle counts, register results, operand memory and next-fetch pins.

The [evidence and complete diagnostic](https://github.com/emu198x/docs/blob/5e7456a/plans/2026-10-11-6809-register-reads.md)
retain 3,264 rows, including two explicitly unresolved reference differences.
TSTA/TSTB are excluded from this fixture because XRoar's three-cycle execution
conflicts with Motorola's two-cycle opcode map. Only DAA V is masked in result
comparisons: Motorola leaves it undefined, and the reference's hardware-derived
calculation needs separate qualification. All other flags compare exactly.
These limits do not mask any bus event for the 32 included encodings.

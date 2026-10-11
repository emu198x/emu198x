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

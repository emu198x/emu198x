# POKEY controller reference captures

`controller-reference.json` records the unchanged Altirra source hashes and
capture provenance. The complete executable investigation is in
[Emu198x docs](https://github.com/emu198x/docs/tree/main/plans/2026-10-11-atari5200-controller-evidence).
These are generated observations, not original hardware measurements.

The frozen 2,048-row scanner capture is
`crates/emu198x-atari-pokey/tests/data/keyboard-scan.csv`. Its probe supplies
one event per 114 clocks, so the production test shifts its capture origin
by 33 clocks to align with the 81-clock startup after a real SKCTL write.
A separate machine test verifies this timing through an executing 6502.

`crates/machine-atari-5200/src/controller-matrix.bin` contains all 32,768
physical key subsets projected from the frozen 65,536-row reference capture.
Both upper-fire states have identical KR1 connectivity. Each two-byte word
is little endian and gives all 16 physical matrix intersections; the source
held-key mask omits the nonexistent key at position 0, so it is shifted left
by one for the board test. All 64 mirrored addresses are checked.

The legacy-envelope comparison retains the original 800XL v21 and 7800 v6
captures. A new unattached scanner contributes exactly one zero Option byte,
and the approved envelope version advances. Every other serialized byte,
complete frame and audio packet must still match the frozen source capture.
The old production envelopes are rejected before decoding; this test does
not implement or promise a legacy reader. Its negative controls reject
changed history and missing, nonzero or extra scanner bytes.

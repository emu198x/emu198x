# Interrupt circuit vectors

`interrupt-vectors.txt` contains 300 distinct retained-circuit input/output
traces covering 32,832 cases and 16,416 pulses. The first column is case
multiplicity; the second encodes 239 phase observations as hexadecimal bytes.
Bits 0–4 are MARIA input phase, observed CPU falling edge, asserted HALT pad,
DLI request and expected asserted NMI. Other bits are zero.

These are digital circuit replay results, not physical pin captures. The
original layout and full extracted network are not distributed here.

Generator and source hashes:
[interrupt completion evidence](https://github.com/emu198x/docs/tree/main/plans/2026-10-12-maria-interrupt-completion).
Timing choice and physical-validation limits:
[adjudication](https://github.com/emu198x/docs/blob/main/plans/2026-10-12-maria-interrupt-completion/adjudication.md).

Vector SHA-256:
`a842c4b04d35f3554246f1350ed96b9ab1a1fa93a9e4d3c441e0e5c310f1e23b`.

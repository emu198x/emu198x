# Format success component evidence

Twenty-four declared cases passed in two fresh source-build test executions.
A deliberately incorrect expected result R failed the exact Format-result
assertion (exit101); the original expectation was restored before both passes.
This is not full Format Track or machine qualification.

The case definition is in `../2026-10-21-format-success-matrix.md` and the test
is `crates/nec-upd765a/tests/format_success.rs`. Single-sector cases use an ID
size different from command N and an R that wraps on increment. The nine-sector
case uses non-sequential physical IDs. Every payload byte, metadata replacement,
untouched neighbouring track, byte/result direction, IRQ and idle recovery is
checked. FM/ND0 component cases do not imply a new supported +3 configuration
or a DMA handshake implementation.

`verification.json` records commands, source/log hashes and source-build
identity. `positive-1.log` and `positive-2.log` each print the required nonzero
case count. `wrong-result.log` retains the rejected expectation. `chip-suite.log`,
`fmt.log` and `clippy.log` retain the owning checks. All subprocesses had a
180-second host timeout; each controller request/result wait has a one-million
tick bound. No firmware was used or retained.

No independent emulator, ROM loader, release binary, native checkpoint or
export/reopen ran in this matrix. Those remain separate requirements of
umbrella237, alongside active READY and applicable Fault adjudication.

Reproduce from the Emu checkout:

```sh
cargo test -p nec-upd765a --test format_success -- --nocapture
```

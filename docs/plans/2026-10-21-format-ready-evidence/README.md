# Format initial READY verification

The bounded fix rejects Format Track before execution when the supplied READY
level is false, even with a writable image mounted. It does not establish
active READY-loss semantics or qualify the complete operation.

- `chip-red.log`: both final chip tests fail without the eight-line fix.
- `native-red.log`: native checkpoint test fails without that fix.
- `focused-chip.log`: two tests, eight initial mode/protection combinations
  plus actual formatted payload after READY is asserted and a fresh command.
- `focused-native.log`: one test, twenty ND/checkpoint combinations, each
  continued both directly and after native restore using blank ROMs and
  authored in-memory media. No firmware or physical disk was executed.
- `tests.log`: owning chip/runtime suites report 562 passed, zero failed,
  62 ignored across 64 result summaries including empty doctest suites.
  Ignored cases are not evidence. This is regression coverage, not a claim
  that every runtime fixture or full ROM boot ran.
- `fmt.log`, `clippy.log`: final owning checks exit zero.
- `verification.json`: source/log hashes, source-build identity, command
  outcomes and limitations. The broad suite ran before final comment and
  rustfmt-only edits; focused checks, fmt and clippy then ran on final code.

Reference: NEC December1978 printed14, Format A Track, states initially
missing READY gives abnormal termination. Its ST0 table printed17 supplies
NR for not-ready conditions. Primary-library locator:
`reference/by-topic/fdc-765/upd765-data-sheet-dec78.md` in the umbrella.
Vendored MAME commit9391e1eff80eb04abd58d48218a1d3b9643c1ea8,
`src/devices/machine/upd765.cpp::format_track_start`, independently checks
READY before protection and enters result with NR|FAIL. That code was read,
not executed as an independent runtime in this fix.

Active preamble READY loss was separately observed continuing on the old
model; an immediate C8 result is not adjudicated by these references.
Retain that gap in Emu1962 and umbrella237.

Reproduce the focused checks from an Emu198x checkout:

```sh
cargo test -p nec-upd765a --test format_ready
cargo test -p runtime-sinclair-zx-spectrum --lib format_initial_not_ready
```

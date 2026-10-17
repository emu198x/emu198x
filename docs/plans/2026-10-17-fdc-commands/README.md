# Five missing FDC commands within the sector-image model

All fifteen documented command opcodes now have paths. **Full controller
conformance remains open.** This is source-build controller/runtime evidence,
not a new released combination or a source-to-ROM qualification for the new
opcodes. The existing released v0.35.5 ROM campaign retains its own scope.

| New command | Implemented behavior | Direct checks |
| --- | --- | --- |
| Write Deleted Data / 09 | Existing paced write engine, selected DDAM, partial CRC, protection/EOT/MT, normal write clears DDAM | Seven controller groups; five native state offsets, EDSK export/reparse and fresh-runtime Read Deleted Data |
| Read Track / 02 | N-controlled represented payloads in physical order; EOT counts fields; recorded errors continue; MT/SK do not change track | Four controller groups; four native state offsets; two fresh MAME order/count cases |
| Scan Equal / 11 | Bytewise equality, SH/SN latch, STP, CM/SK, MT, errors and paced requests | Shared nine controller groups and native state offsets |
| Scan Low or Equal / 19 | Every disk byte must be <= processor byte; equality alone sets SH | Same groups, separate relation cases |
| Scan High or Equal / 1D | Every disk byte must be >= processor byte; equality alone sets SH | Same groups, separate relation cases |

Each new test group failed against the implementation before its command was
added: five Write Deleted Data, four Read Track and six initial Scan groups.
Further Scan groups cover MT/missed EOT, N=0 and literal FF, and media ejection.
The separate [zero-byte write fix](https://github.com/emu198x/emu198x/issues/1815)
records the selected mark and incomplete CRC at preamble completion, before first
RQM. Two regressions failed before correction and pass afterwards; two guards
recheck missing media/protection. No timing constants or stored fields change.
The combined source suite has 99 controller tests, 112 runtime library tests,
six machine tests and one explicitly executed local-ROM menu boot. Clippy on
all affected targets and workspace formatting pass. No proprietary firmware,
general machine states or dependencies are added.

Write Deleted Data retains the existing WriteData tag and opcode in cmd_buf.
ReadTrack and Scan append internal tags after FormatTrack; existing tags and
fields retain their order. The frozen released-controller postcard round trip
still passes. New active-command states require a reader implementing their
new tags; no forward compatibility with older binaries is claimed.

## Independent reference

[Two fresh MAME 0.289 runs](reference/verification.json) each execute sixteen
bounded public-port cases. They inject a DI/JP idle loop so ROM code cannot
compete for the FDC. **This is not ROM loading.** Each case is bounded to two
emulated seconds and 45 host seconds. Firmware identities, executable hash,
commands, environment, media hashes, logs and full observed data/results are
retained in `reference/final-{1,2}.json`; temporary firmware/media are removed.

MAME independently agrees on Write Deleted payload/DDAM read-back and Read
Track order/count. Stock MAME's Scan defects produce ten failed semantic
comparisons, retained as failures despite process exits of zero. See
[Emu #1813](https://github.com/emu198x/emu198x/issues/1813): opcode bit08 causes
spurious CM, inequalities reverse, the hit condition cannot fire, and STP is
ignored. The source implementation follows NEC December 1978 printed p15/Table4
and Fuse's bytewise comparison/latch instead. N=0/STP and exact EOT result
ST0/EN/CHRN remain separately unqualified; SH alone with a CRC error is not a
successful command.

[Write no-TC result metadata #1814](https://github.com/emu198x/emu198x/issues/1814)
remains unresolved: MAME retains C, while Fuse and the existing engine advance
it. NEC Table2 describes processor termination and does not settle this case.
The code citation now states that limit. Payload and mark checks remain
separate from disputed CHRN; no reference has been patched to make a pass.

Verify retained reference evidence and its rejection controls:

```sh
python3.14 docs/plans/2026-10-17-fdc-commands/reference/verify.py
python3.14 -m unittest discover \
  -s docs/plans/2026-10-17-fdc-commands/reference -p 'test_*.py' -v
cargo test --locked -p nec-upd765a
cargo test --locked -p runtime-sinclair-zx-spectrum --lib
```

## Limits and next work

Read Track does not fabricate raw gaps, flux or angular/index timing absent
from DSK/EDSK. Read transfers still use the pre-existing immediate-buffer model.
Scan uses the existing write engine's bounded pre-data/byte request cadence;
physical search, head-load and exact overrun timing are not newly qualified.

All fifteen operations remain partial. Next work is independent Scan conformance,
no-TC write-result adjudication, byte pacing/read deadlines, physical drive
signals/search and interruption/error matrices. The named PAL +3 configuration
does not qualify other geometries, drives, firmware, models, clones or expansions.

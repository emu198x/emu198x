# Keep an ID CRC error when Read Track is interrupted

[Emu198x #1849](https://github.com/emu198x/emu198x/issues/1849) is reproduced
and corrected. Read Track could read a bad ID, transfer payload, then lose that
ID error when a missed byte request ended the command. Both ID and data status
were deferred until payload completion. The result now preserves DE from an
encountered ID-only CRC error while leaving an incomplete data CRC deferred.

The fix derives the bit from existing progress at result assembly. It does not
change byte clocks, interfaces, dependencies or the Spectrum v6 state layout.
It also works with pre-fix v6 checkpoints inside the field: their pending status
and field-entry flag already carry the information. A prepared future field
cannot contribute its CRC error. DD-set metadata does not independently encode
whether its ID was also corrupt; this fix does not invent that distinction.

## What passed

The parent v0.38.1 source failed both a public-register regression and a native
save/restore regression. Both logs retain actual assertion failures. Corrected
source `3bab241b99600ebfad0945e15fc073358e4a41ef` passes 474 controller,
runtime and +3 tests; 63 ignored tests receive no qualification credit. Five
new controller tests cover ID/data/clean CRC controls, 0/1/179 accepted bytes,
next-command recovery, an unvisited future ID and media removal. Thirty-six
native checkpoints cover clean/ID/data cases, low/high RQM and uninterrupted or
interrupted continuation. Each continuation also restores a partially drained
result. Source bytes and live exported media stay unchanged. Clippy, rustfmt
and the normal locked headless release build pass.

The reviewed evidence commit corrects only a comment's Fuse function name from
`start_read_track` to `start_read_diag` after the executed source commit. No
executable Rust code differs; both identities are recorded rather than calling
the later commit the executed source.

An independent unmodified Fuse controller probe ran 32 fresh cases: sixteen
matching repeat pairs, independently checking raw ID/data CRCs and MFM missing
clock marks. After one or seventeen bytes, bad-ID/clean-data cases retain DE
and clear DD; clean-ID or data-only CRC cases clear both until data completion.
The retained full results differ: Fuse's bad-ID interrupted result also has ND,
and its fully drained result R/status differs from our represented model.
We compare the encountered ID error only. This executes Fuse controller/drive
source with an authored host adapter and ASan/UBSan, not the complete Fuse app
or physical hardware. Each case has a 15-second host and two-second emulated
limit. No upstream Fuse report is made.

Two fresh cold boots loaded the captured diagnostic through the normal +3 ROM
Loader and FDC. Both passed 519 native-format, eight firmware, three loading
and seven execution checks. The signature changed from 0000 to 5A5B through CPU
writes outside the loaded CODE, with the expected screen RAM. An expected
5A5C failed with `signature mismatch: expected [90, 92], observed [90, 91]`
after loading passed. Wrappers exit 0/0/1; all three emulator processes exit 0,
showing why process success alone is insufficient.

These name PAL +3/40077/128 KiB, English v4.0 four-ROM firmware, drive A and a
standard protected 180 KiB DSK. Exact configuration, firmware/tool/binary,
fixture, recipe, command, environment and artifact identities are retained in
`evidence/runtime-rom.tar.gz`. The runtime is a source build with suite manifest
0.38.1, not a distributed v0.38.1 binary containing this later fix. Assembly and
mastering are captured reference replay; runtime execution is fresh. These
records deliberately retain `qualified=false`. No firmware, host executable or
native machine state is committed.

## Reproduce

From this repository:

```sh
cargo test --locked -p nec-upd765a --test read_progress
cargo test --locked -p runtime-sinclair-zx-spectrum --lib read_track_id_crc_survives
python3.14 docs/plans/2026-10-19-read-track-id-crc/verify.py
python3.14 -m unittest discover -s docs/plans/2026-10-19-read-track-id-crc -v
python3.14 -m unittest discover -s docs/plans/2026-10-19-read-track-id-crc/reference -v
```

The Python checks inspect captured evidence, including negative mutations;
they do not rerun emulators. `reference/run.py` captures fresh observations
using the local unchanged Fuse source and Clang; its `runs` directory must not
already exist. No firmware is needed for that controller probe. Exact fresh
ROM regression commands are in the archive's `checks.json`; that existing
umbrella wrapper requires legitimate local firmware and the umbrella fixture.

## What remains

This closes one encountered-error loss, not FDC qualification. Exact
part/revision-specific RQM-to-overrun deadlines remain open in
[#1831](https://github.com/emu198x/emu198x/issues/1831). Rotation/index/head-load,
raw gaps and flux, drive behaviour and other variants remain unqualified.
Completed Read Track data-CRC status differences need their own fully drained
experiment. Basic ROM loading does not qualify those features or upgrade prior
FORMAT/export evidence to this binary. Umbrella artifact-retention
[#76](https://github.com/stevehill1981/198x/issues/76) remains a separate hosted
storage blocker; it does not invalidate the retained local runs.

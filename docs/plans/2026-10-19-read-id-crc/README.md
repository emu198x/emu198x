# Read ID rejects represented ID CRC errors

Read ID previously reported success for a sector marked with an ID-field CRC error. The ReadId arm ignored sector status. The narrow fix reports abnormal termination, ST1.DE and ST1.ND (`40 24 00`), while preserving the encountered CHRN and physical ID progression. Data-field errors do not affect Read ID. This fixes #1867.

The regression failed on unchanged production source with `00 00 00 00 00 07 02` instead of `40 24 00 00 00 07 02`. It now passes through ten successive commands over two head/drive combinations, including error-to-clean recovery, wrap, data-CRC isolation and source metadata/payload immutability. All 141 controller tests pass, none ignored; Clippy and formatting pass.

## Reproduce

```sh
cargo test -p nec-upd765a --test read_id_errors
cargo test -p nec-upd765a
cargo clippy -p nec-upd765a --all-targets -- -D warnings
cargo fmt -p nec-upd765a -- --check
python3.14 docs/plans/2026-10-19-read-id-crc/verify.py
```

`verification.json` records exact source hashes, commands, environment and tool identities. `audit-controls.json` records three deliberately rejected mutations under Python optimization: empty positive checks, a wrong source hash and a wrong archive hash. `checks.tar.gz` retains exact raw red/green/full-test/Clippy/format logs; no firmware is included. This is fresh source controller-test evidence, **qualified=false**. It does not provide a new complete source-to-media or released-binary qualification.

## Why these result bits

Primary NEC December 1978 data sheet, printed p14: Read ID returns the first encountered ID; p17: DE marks an ID/data CRC error and ND is set when Read ID cannot read an ID without error; p18: DD concerns only the data field. Primary source: `reference/by-topic/fdc-765/upd765-data-sheet-dec78.md` in the umbrella library, original `/Volumes/Data/Reference-PDFs/by-topic/fdc-765/upd765-data-sheet-dec78.pdf`.

Unchanged vendored Fuse `peripherals/disk/upd_fdc.c`, `read_id` and `start_read_id`, sets DE|ND and abnormal termination on bad ID CRC, preserving CHRN. The independent raw-MAME study #1860 distinguishes real ID-only and data-only corruption; MAME additionally sets MA on the Read ID ID-CRC case. That extra MA remains a reference disagreement, not an instruction to copy an unexplained bit.

The separately captured raw reference packet is `docs/plans/2026-10-19-read-track-raw-crc/evidence.tar.gz`, SHA-256 `3662226019e03309fd3ef4c5b7cd7cb94e39ac04e93fec3dcd87e52170cad386` (26,652 bytes), tracked by #1860 and [PR #1869](https://github.com/emu198x/emu198x/pull/1869), reviewed at `35bf6252b96d0b5b425fec37f440adb907d205d9`. Its `campaign.json` member retains six fresh Read ID observations (clean, actual ID-only CRC, actual data-only CRC, each repeated), raw outputs and firmware/input identities. Installed MAME 0.289 `(unknown)` executable SHA-256 `6d40df87963c58cdd3857d203e186af854fea49ae03b2ab555e30ef1029636d9` reports `40 25 00` only for ID damage; clean/data-only cases report `00 00 00`. This fix consumes that captured reference evidence; the Fuse comparison above is source inspection, not a fresh Fuse execution. The installed MAME source identity is unknown.


The sector model represents ID CRC with DE set and DD clear. DE+DD establishes data CRC; it cannot distinguish data-only damage from simultaneous ID+data damage. No additional bad ID is invented in that ambiguous case. Missing IDs retain the existing MA|ND path. Physical index/search timing, partial IDs, ready pins and silicon revisions remain unqualified. Read ID as a whole remains partial.

## Fresh normal-ROM runtime regression

After the controller correction, the ordinary locked headless release build at source `2ecf2a8eee85ecd94f45ea3089818afb9649b826` succeeded. This is a source build whose suite manifest says 0.38.3, not the public v0.38.3 executable. Binary SHA-256: `c9a5dd2628450d81c8a2b21a46775ea3b6b4a2b533fc952816cf5bdf33d2606f`.

Two fresh cold boots through the actual +3 ROM/FDC disk loader pass: native structure 519 assertions, firmware identity eight, loading three, execution seven per run. CPU writes change the external completion signature from `00 00` to `5A 5B`; screen RAM matches the expected hash. The wrong expected signature `5A 5C` fails after loading passes, although the runtime process exits successfully. Wrapper exits are 0/0/1.

These runs use the existing captured native diagnostic and captured wrapper sources from the Read Data CRC regression. Assembly/mastering remain explicitly `not_run`, with fixture origin `captured_reference_replay`; runtime execution is fresh. Configuration remains PAL +3/40077, 128 KiB, English v4.0 four-ROM bundle, internal drive A, standard 180 KiB DSK. All four legitimate local ROM hashes are recorded; no firmware is archived. Every record remains `qualified=false`.

`runtime-rom.tar.gz` retains exact session scripts, output/logs, artifact hashes, source/executable/compiler identity, original captured wrappers and recipe, copied configuration inventory, build log and invocation. It also retains an initial preparation failure caused by the isolated wrapper missing its inventory file; no runtime executed in that failed attempt. Copying and hashing the inventory resolved that preparation prerequisite before the successful runs. Runtime calls retain their existing emulated-time/event bounds and host timeout, plus a 240-second outer wrapper timeout.

`runtime_verify.py` adapts the existing Read Data runtime auditor and executes its checks even under Python optimization. Ten controls validate the packet and reject missing/duplicate artifacts, missing runtime logs, wrong source, zero assertions, skipped loading, false transition/qualification and an incorrectly successful negative-control wrapper.

```sh
python3.14 -O -m unittest discover -s docs/plans/2026-10-19-read-id-crc -p test_runtime_verify.py -v
```

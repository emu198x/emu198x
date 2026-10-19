# Read Track size and field boundary study

**All 15 FDC operations remain partial.** This study narrows Read Track; it does not qualify a machine or establish silicon timing.

39 named cases; 212 successful captures; 2 bounded capture failures; 20 explicitly inapplicable rows. 26 Emu/MAME case pairs agree on every payload and result byte, repeated twice.

Fresh Emu source d92b7ca3828efc438f335a2dbdb337c80cf7f346 plus the archived test harness; installed MAME 0.289 (source identity unknown); unmodified Fuse source 0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2 with a bounded register/event host. Emu crate version 0.38.3 is a source build, not the released executable. MAME uses the PAL English +3 and the four locally supplied English v4.0 ROM pages, whose hashes are in the campaign. Register hosts bypass normal guest loading; qualified=false.

## What the checks establish

- Valid 250 kbit/s MFM raw fields N0–N5 fit a 6250-byte revolution. Both repetitions drain all seven result bytes, verify idle and issue a subsequent Sense Drive Status command. Source media remain unchanged.
- N0 DTL=0,1,127,128,255 and a nonzero-N contrast are distinct cases. Commanded and recorded N remain separate inputs. Duplicate IDs have distinct payloads; both physical orders and EOT below/equal/above physical field count are retained.
- The raw fixture has its CRC after the authored actual data length. Independent bit-serial CRC and independently decoded MFI clocks/bytes are checked. Emu receives a sector representation; zero padding of a short/empty representation is not an on-disk field, and is not evidence for a missing data address mark.
- N6/N7 cannot fit this raw revolution, and N8/255 are invalid size-code controls. These and absent media have fresh Emu observations only; reference rows say not applicable, not pass.
- Capture pass means bounded payload/result drain and recovery succeeded. It does not mean the observed bytes match hardware. Expectations pin captured observations for evidence integrity; they are not independently established silicon oracles. Nominal Emu payload/order checks are separately asserted.

## What remains open

- DTL: Fuse transfers 1/127 bytes for those N0 settings; Emu and MAME transfer 128. The NEC December 1978 sheet p.12 describes Read Data DTL and p.14 describes Read Track separately. This disagreement needs command- and revision-specific evidence before a behavioural fix.
- Command/recorded size and actual raw-field length mismatches expose CRC/gap bytes in raw emulators. Sector-image padding cannot model that stream faithfully. Keep these as capability/input-boundary observations, not false equivalence.
- Empty/unformatted track, physical CHRN mismatches, duplicate IDs and EOT edge cases remain explicit observations in the table. EOT=0 fails the bounded Fuse drain; its assertion log is retained. No upstream defect claim is made.
- A missing physical track within an otherwise present image is not separately represented by this matrix. MT/SK opcode contrasts, applicable native-state interruption checkpoints and fresh released guest execution for the new boundaries remain separate work. Physical search/rotation/service deadlines remain #1831; drive signal wiring remains #1876.
- No production controller code, dependencies, snapshot schemas or release boundaries change in this packet. A passing default does not qualify the family.

## Reproduce

```sh
python3.14 -B -O docs/plans/2026-10-19-read-track-field-boundaries/verify.py
python3.14 -B -O -m unittest discover -s docs/plans/2026-10-19-read-track-field-boundaries -p "test_*.py" -v
```

For fresh capture, remove or move only this packet’s previous `runs/` directory first, then run `python3.14 -u docs/plans/2026-10-19-read-track-field-boundaries/run.py`. It requires the family’s vendored Fuse source, local +3 firmware, Rust, Clang and installed MAME 0.289. The runner refuses to reuse an existing result directory. A missing firmware home is tested without moving any real firmware. `capture-source.tar.gz` retains the exact pre-format recipe used by this capture; current scripts include subsequent lint cleanup and automatic capture-source retention for future runs. `evidence.tar.gz` retains commands, environment, raw inputs, complete observations and failure logs. No ROM or snapshot is included.

## Every case

| Case | Emu: payload / ST0 ST1 ST2 C H R N | MAME: payload / result | Fuse: payload / result | Interpretation |
| --- | --- | --- | --- | --- |
| n0 | 128 B / 40 80 00 00 00 01 00 | 128 B / 40 80 00 00 00 01 00 | 128 B / 00 00 00 00 00 02 00 | Emu/MAME agree |
| n1 | 256 B / 40 80 00 00 00 01 01 | 256 B / 40 80 00 00 00 01 01 | 256 B / 00 00 00 00 00 02 01 | Emu/MAME agree |
| n2 | 512 B / 40 80 00 00 00 01 02 | 512 B / 40 80 00 00 00 01 02 | 512 B / 00 00 00 00 00 02 02 | Emu/MAME agree |
| n3 | 1024 B / 40 80 00 00 00 01 03 | 1024 B / 40 80 00 00 00 01 03 | 1024 B / 00 00 00 00 00 02 03 | Emu/MAME agree |
| n4 | 2048 B / 40 80 00 00 00 01 04 | 2048 B / 40 80 00 00 00 01 04 | 2048 B / 00 00 00 00 00 02 04 | Emu/MAME agree |
| n5 | 4096 B / 40 80 00 00 00 01 05 | 4096 B / 40 80 00 00 00 01 05 | 4096 B / 00 00 00 00 00 02 05 | Emu/MAME agree |
| n0-dtl-0 | 128 B / 40 80 00 00 00 01 00 | 128 B / 40 80 00 00 00 01 00 | 128 B / 00 00 00 00 00 02 00 | Emu/MAME agree |
| n0-dtl-1 | 128 B / 40 80 00 00 00 01 00 | 128 B / 40 80 00 00 00 01 00 | 1 B / 00 00 00 00 00 02 00 | Emu/MAME agree |
| n0-dtl-127 | 128 B / 40 80 00 00 00 01 00 | 128 B / 40 80 00 00 00 01 00 | 127 B / 00 00 00 00 00 02 00 | Emu/MAME agree |
| n0-dtl-128 | 128 B / 40 80 00 00 00 01 00 | 128 B / 40 80 00 00 00 01 00 | 128 B / 00 00 00 00 00 02 00 | Emu/MAME agree |
| n2-dtl-1 | 512 B / 40 80 00 00 00 01 02 | 512 B / 40 80 00 00 00 01 02 | 512 B / 00 00 00 00 00 02 02 | Emu/MAME agree |
| command-n0-recorded-n2 | 128 B / 40 84 00 00 00 01 00 | 128 B / 40 A4 20 00 00 01 00 | 128 B / 00 00 00 00 00 02 00 | Different inputs or behaviour; unresolved |
| command-n2-recorded-n0 | 512 B / 40 84 00 00 00 01 02 | 512 B / 40 A4 20 00 00 01 02 | 128 B / 00 00 00 00 00 02 02 | Different inputs or behaviour; unresolved |
| command-n1-recorded-n2 | 256 B / 40 84 00 00 00 01 01 | 256 B / 40 A4 20 00 00 01 01 | 256 B / 00 00 00 00 00 02 01 | Different inputs or behaviour; unresolved |
| command-n3-recorded-n2 | 1024 B / 40 84 00 00 00 01 03 | 1024 B / 40 A4 20 00 00 01 03 | 512 B / 00 00 00 00 00 02 03 | Different inputs or behaviour; unresolved |
| represented-0 | 512 B / 40 80 00 00 00 01 02 | 512 B / 40 A0 20 00 00 01 02 | 512 B / 00 20 20 00 00 02 02 | Different inputs or behaviour; unresolved |
| represented-128 | 512 B / 40 80 00 00 00 01 02 | 512 B / 40 A0 20 00 00 01 02 | 512 B / 00 20 20 00 00 02 02 | Different inputs or behaviour; unresolved |
| represented-512 | 512 B / 40 80 00 00 00 01 02 | 512 B / 40 80 00 00 00 01 02 | 512 B / 00 00 00 00 00 02 02 | Emu/MAME agree |
| represented-768 | 512 B / 40 80 00 00 00 01 02 | 512 B / 40 A0 20 00 00 01 02 | 512 B / 00 20 20 00 00 02 02 | Different inputs or behaviour; unresolved |
| empty-track | 0 B / 40 01 00 00 00 01 02 | 0 B / 40 01 00 00 00 01 02 | 0 B / 40 85 00 00 00 01 02 | Emu/MAME agree |
| absent-media | 0 B / 48 00 00 00 00 01 02 | not_applicable | not_applicable | Safety/absence control only |
| id-mismatch-c | 512 B / 40 84 00 00 00 01 02 | 512 B / 40 84 00 00 00 01 02 | 512 B / 00 04 10 00 00 02 02 | Emu/MAME agree |
| id-mismatch-h | 512 B / 40 84 00 00 00 01 02 | 512 B / 40 84 00 00 00 01 02 | 512 B / 00 04 00 00 00 02 02 | Emu/MAME agree |
| id-mismatch-r | 512 B / 40 84 00 00 00 01 02 | 512 B / 40 84 00 00 00 01 02 | 512 B / 00 04 00 00 00 02 02 | Emu/MAME agree |
| id-mismatch-n | 512 B / 40 84 00 00 00 01 02 | 512 B / 40 84 00 00 00 01 02 | 512 B / 00 20 20 00 00 02 02 | Emu/MAME agree |
| ids-12-eot-1 | 512 B / 40 80 00 00 00 01 02 | 512 B / 40 80 00 00 00 01 02 | 512 B / 00 00 00 00 00 02 02 | Emu/MAME agree |
| ids-12-eot-2 | 1024 B / 40 84 00 00 00 01 02 | 1024 B / 40 84 00 00 00 01 02 | 1024 B / 00 00 00 00 00 03 02 | Emu/MAME agree |
| ids-12-eot-3 | 1536 B / 40 80 00 00 00 01 02 | 1536 B / 40 80 00 00 00 01 02 | 1536 B / 00 04 00 00 00 04 02 | Emu/MAME agree |
| ids-21-eot-1 | 512 B / 40 84 00 00 00 01 02 | 512 B / 40 84 00 00 00 01 02 | 512 B / 00 04 00 00 00 02 02 | Emu/MAME agree |
| ids-21-eot-2 | 1024 B / 40 80 00 00 00 01 02 | 1024 B / 40 80 00 00 00 01 02 | 1024 B / 00 04 00 00 00 03 02 | Emu/MAME agree |
| ids-21-eot-3 | 1536 B / 40 84 00 00 00 01 02 | 1536 B / 40 84 00 00 00 01 02 | 1536 B / 00 04 00 00 00 04 02 | Emu/MAME agree |
| ids-11-eot-1 | 512 B / 40 80 00 00 00 01 02 | 512 B / 40 80 00 00 00 01 02 | 512 B / 00 00 00 00 00 02 02 | Emu/MAME agree |
| ids-11-eot-2 | 1024 B / 40 80 00 00 00 01 02 | 1024 B / 40 80 00 00 00 01 02 | 1024 B / 00 04 00 00 00 03 02 | Emu/MAME agree |
| ids-11-eot-3 | 1536 B / 40 80 00 00 00 01 02 | 1536 B / 40 80 00 00 00 01 02 | 1536 B / 00 04 00 00 00 04 02 | Emu/MAME agree |
| eot-0 | 0 B / 40 01 00 00 00 01 02 | 1024 B / 40 00 00 00 00 01 02 | fail | Different inputs or behaviour; unresolved |
| safety-n6 | 8192 B / 40 84 00 00 00 01 06 | not_applicable | not_applicable | Safety/absence control only |
| safety-n7 | 16384 B / 40 84 00 00 00 01 07 | not_applicable | not_applicable | Safety/absence control only |
| safety-n8 | 0 B / 40 01 00 00 00 01 08 | not_applicable | not_applicable | Safety/absence control only |
| safety-n255 | 0 B / 40 01 00 00 00 01 FF | not_applicable | not_applicable | Safety/absence control only |

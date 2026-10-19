# Read Track missing entry and opcode contrasts

16 cases repeated twice: 68 complete accepted-command captures, 12 complete early-rejection captures, 16 explicitly inapplicable reference rows.

**All 15 FDC operations remain partial.** This closes a finite observation gap. It does not qualify a Spectrum configuration, multi-track/skip operation or physical timing.

## What now works

- A present sector image with no track entry is exercised separately from a present empty track and from absent media. Emu returns seven bytes, drains to idle and accepts the next command. Its media remain present and unchanged. No raw-media equivalence is asserted for this container condition.
- The same [1,2] and [2,1] physical fields, distinct payloads and empty raw track are exercised with opcodes 42, 62, C2 and E2. Each accepted command has all nine command bytes, its complete payload, all seven result bytes and subsequent recovery checked.
- Fuse rejects C2/E2 immediately with one invalid-command byte 80. The host sends only the opcode and never feeds the eight unsent parameters into subsequent commands. This rejection has zero payload and real next-command recovery; it is not a timed-out seven-byte transfer.
- Emu and installed MAME accept all four opcode variants and remain on the represented single side. This records observed compatibility only. It cannot establish supported multi-track or skip functionality.
- Independent bit-serial raw CRC and independently decoded MFI clock/byte audits are reused. Commands, environment, source/recipe/executable identities, firmware hashes, full observations, repeatability and time bounds are retained without ROM bytes.

## Evidence and limits

Local verification: 143 controller tests pass with zero failures/ignores, and 18 auditor/prerequisite controls pass under optimized Python. Clippy, Rust format and packet Ruff checks pass. Full workspace and supported-build checks remain CI gates; no pass is inferred from a queued job.

Fresh source Emu at 7222f23075ccec1db5ecb431421ef72139640e4f plus the archived probe edits (crate version 0.38.3); unchanged Fuse source 0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2 with the existing bounded host; installed MAME 0.289 with unknown source commit. Inspected MAME source is recorded separately. The MAME machine is the PAL English +3 with internal single-sided drive and four local English v4.0 firmware pages, identified by ordered hashes. The Emu/Fuse controller hosts use no firmware. MAME uses direct Lua port operations after a RAM idle loop, not ordinary ROM disk loading. `qualified=false`.

NEC uPD765 December 1978 printed p8 fixes MT=0 but shows SK as variable in the command table; p14 says multi-track and skip operations are not allowed. Those observations do not prove whether a particular silicon revision rejects or ignores every combination. Fuse and Emu/MAME differ on acceptance; no upstream defect report or speculative production fix follows.

Expected vectors in expectations.json pin these captured observations for regression and integrity. They are not physical-hardware oracles. The raw payload/order expectations are separately computed from authored sector order. A preparation capture used an inaccurate fixed-SK label; it is retained only as preparation, not credited as this matrix. The final capture uses corrected labels and exact archived sources.

The prior 39-case size/DTL packet remains unchanged. Its DTL, command-vs-recorded size, field-length and EOT=0 differences remain unresolved. Applicable interruption/native-state coverage and fresh released guest execution for boundary cases remain separate, as does #1831 timing and #1876 drive wiring.

## Next finite gate

Current snapshot.rs already defines tests for N2 clean/CRC reads, two-sector boundaries, RQM high/low, overruns and result prefixes. Extend that existing harness to N0/N5 and the newly exposed short/empty/long represented fields, duplicate/reversed IDs and EOT wrapping. Compare uninterrupted vs restored continuation, result bytes and immutable media. Do not repeat the existing N2 gate or claim exact physical deadlines. The bounded follow-up is #1880.

## Reproduce

```sh
python3.14 -B -O docs/plans/2026-10-19-read-track-opcode-track/verify.py
python3.14 -B -O -m unittest discover -s docs/plans/2026-10-19-read-track-opcode-track -p "test_*.py" -v
```

Fresh capture: in the complete source checkout, move the packet’s previous runs/ directory aside and run `python3.14 -u docs/plans/2026-10-19-read-track-opcode-track/run.py`. It reuses the preceding runner, whose identity and source are included with the exact capture recipe. Requires existing local firmware, vendored clean Fuse sources, Clang, Rust and installed MAME 0.289. An existing output directory is rejected. The missing-firmware test uses an empty temporary home and invokes no emulator.

## Every case

| Case | Emu | MAME | Fuse |
| --- | --- | --- | --- |
| ids-12-op-42 | 9 command / 1024 payload / 40 84 00 00 00 01 02 | 9 command / 1024 payload / 40 84 00 00 00 01 02 | 9 command / 1024 payload / 00 00 00 00 00 03 02 |
| ids-12-op-62 | 9 command / 1024 payload / 40 84 00 00 00 01 02 | 9 command / 1024 payload / 40 84 00 00 00 01 02 | 9 command / 1024 payload / 00 00 00 00 00 03 02 |
| ids-12-op-c2 | 9 command / 1024 payload / 40 84 00 00 00 01 02 | 9 command / 1024 payload / 40 84 00 00 00 01 02 | 1 command / 0 payload / 80 |
| ids-12-op-e2 | 9 command / 1024 payload / 40 84 00 00 00 01 02 | 9 command / 1024 payload / 40 84 00 00 00 01 02 | 1 command / 0 payload / 80 |
| ids-21-op-42 | 9 command / 1024 payload / 40 80 00 00 00 01 02 | 9 command / 1024 payload / 40 80 00 00 00 01 02 | 9 command / 1024 payload / 00 04 00 00 00 03 02 |
| ids-21-op-62 | 9 command / 1024 payload / 40 80 00 00 00 01 02 | 9 command / 1024 payload / 40 80 00 00 00 01 02 | 9 command / 1024 payload / 00 04 00 00 00 03 02 |
| ids-21-op-c2 | 9 command / 1024 payload / 40 80 00 00 00 01 02 | 9 command / 1024 payload / 40 80 00 00 00 01 02 | 1 command / 0 payload / 80 |
| ids-21-op-e2 | 9 command / 1024 payload / 40 80 00 00 00 01 02 | 9 command / 1024 payload / 40 80 00 00 00 01 02 | 1 command / 0 payload / 80 |
| empty-track-op-42 | 9 command / 0 payload / 40 01 00 00 00 01 02 | 9 command / 0 payload / 40 01 00 00 00 01 02 | 9 command / 0 payload / 40 85 00 00 00 01 02 |
| empty-track-op-62 | 9 command / 0 payload / 40 01 00 00 00 01 02 | 9 command / 0 payload / 40 01 00 00 00 01 02 | 9 command / 0 payload / 40 85 00 00 00 01 02 |
| empty-track-op-c2 | 9 command / 0 payload / 40 01 00 00 00 01 02 | 9 command / 0 payload / 40 01 00 00 00 01 02 | 1 command / 0 payload / 80 |
| empty-track-op-e2 | 9 command / 0 payload / 40 01 00 00 00 01 02 | 9 command / 0 payload / 40 01 00 00 00 01 02 | 1 command / 0 payload / 80 |
| missing-track-op-42 | 9 command / 0 payload / 40 01 00 00 00 01 02 | N/A: container-entry condition | N/A: container-entry condition |
| missing-track-op-62 | 9 command / 0 payload / 40 01 00 00 00 01 02 | N/A: container-entry condition | N/A: container-entry condition |
| missing-track-op-c2 | 9 command / 0 payload / 40 01 00 00 00 01 02 | N/A: container-entry condition | N/A: container-entry condition |
| missing-track-op-e2 | 9 command / 0 payload / 40 01 00 00 00 01 02 | N/A: container-entry condition | N/A: container-entry condition |

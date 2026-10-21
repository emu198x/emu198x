# All-skipped Read Data status remains unresolved

The fresh reference runs did not perform an all-skipped transfer: every run delivered 512 bytes from R1. They therefore cannot establish the final status of a command that skips every eligible sector. Tracked in [Emu198x#1949](https://github.com/emu198x/emu198x/issues/1949). This packet records a research dependency, not a qualification or an emulator correction.

| Command | Executed Fuse model | Distributed MAME model |
|---|---|---|
| Read Data/Read Deleted, SK off | 512 R1 bytes; `00 00 40 00 00 01 02` | Same |
| Read Data/Read Deleted, SK on | 512 R1 bytes; `00 00 40 00 00 01 02` | 512 R1 bytes; `40 80 40 00 00 01 02` |

Each cell includes both a single R1 track and the exact native-owner two-sector track, repeated twice in fresh processes: 16 Fuse and 16 MAME processes. Seven result bytes drain, MSR returns to `80`, and a subsequent Sense Drive Status returns to idle. Input media remain unchanged; the Fuse live raw track also remains unchanged. The payload is exactly R1's nonuniform pattern, never R2's.

## Exact scope

MFM, N=2, C=H=0, physical track/head 0/0, R=1, EOT=1, GPL=`2A`, DTL=`FF`, logical drive 0, non-DMA Specify `03 DF 03`, no asserted terminal count. Real commands are `46`/`66` and `4C`/`6C`. R1 has the mismatching data mark; optional R2 has the matching mark beyond EOT. Both have valid CRCs and 512-byte fields. Byte i is `((seed + (i & 255)) & 255) ^ (i >> 8)`, with seeds 33/34 for R1/R2. This standard N=2 case is separate from the N=0/DTL=0 ambiguity.

Fuse's unmodified public-register controller is configured as UPD765A/8MHz, on an authored raw MFM track. MAME runs `specpls3 -bios en` with an authored EDSK. The host installs an idle loop and directly drives controller ports. This is controller research, not ROM-loader qualification or source-to-disk execution evidence. Local firmware hashes are recorded; no ROM, snapshot, proprietary media or reference executable is retained.

Current Emu's retained native convention is zero payload and `40 04 00 00 00 01 02` for SK on, and `40 00 40 00 00 01 02` for SK off. Those are comparison inputs from the native owner, not fresh Emu observations in this packet. The independent observations neither validate that convention nor authorize changing it.

## Primary and reference limits

[NEC's December 1978 datasheet](https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf), printed p12 (local extraction lines1040–1043), says Read Data with SK=1 skips a deleted-mark sector and reads the next. Printed p13 gives the converse for Read Deleted Data. It does not explicitly supply all-skipped-at-EOT ST0/ST1/CHRN. The p12/table2 processor-termination case and p16 Scan/STP discussion cannot silently become this no-TC Read Data oracle. Part/revision applicability remains an explicit research question; reference implementations are precedent, not silicon authority.

Executed Fuse source is `0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2`. In `peripherals/disk/upd_fdc.c`, line149 decodes SK from command D5, but line1332 overwrites it from the first parameter's bit5. That parameter is logical select0 in these recorded commands. The skip path at lines677–681 consequently does not establish an all-skipped result here. This explains this particular source-build observation; it is not a universal claim about Fuse.

Inspected MAME source `9391e1eff80eb04abd58d48218a1d3b9643c1ea8`, `src/devices/machine/upd765.cpp` lines1139–1142, pushes data when TC is absent without checking SK; its SECTOR_READ path around2030 reaches EOT with SK set. This is source inspection only. The executed Homebrew MAME 0.289 binary has an unknown source commit, so the packet does not attest it was built from that checkout. No upstream issue or production workaround is proposed.

Source-built Fuse executable SHA256: `e6c043b51a1d6be0834c7cfaa11b52fadb528d29cc1a3155e0d9d4165ffed350`. Distributed MAME executable SHA256: `6d40df87963c58cdd3857d203e186af854fea49ae03b2ab555e30ef1029636d9`. Full source, tool, firmware, command, environment, bounds and artifact identities are in `runs.tar.gz`.

## Reproduce and inspect

From this checkout, the read-only captured-evidence audit is:

```sh
python3 docs/plans/2026-10-21-read-all-skipped/audit.py
```

`capture.py` reruns the bounded fresh study using the existing local Fuse checkout, Apple clang, Homebrew MAME and legitimate local +3 ROMs. It refuses to replace an existing `runs` directory. Each Fuse phase is limited to two emulated seconds and each process to15 host seconds; MAME has10 total emulated seconds and45 host seconds. No new dependency is required. Exact commands and exit statuses are retained. `audit.py` validates the archive and deliberately rejects altered evidence; `verification.json` records that audit, not a hardware pass.

The next requirement is a part-applicable primary trace or independent implementation that actually suppresses host payload for this case, with exact final status, CHRN and recovery measured. Until then this boundary remains required and blocked by research.

## Raw-observation audit repair

Review reproduced a semantic audit gap: the old auditor accepted altered Fuse raw payload after archive hashes were refreshed, because it trusted the unchanged summary. Original MAME observation JSON had been parsed but not retained. `audit-gap-before.json` records the fail-first result through an actual rehashed archive and the exact old audit commit.

`legacy-summary-only-runs.tar.gz` preserves the original archive byte-for-byte; `legacy-verification.json` preserves its old audit output. Those legacy records lack retained MAME raw JSON and are not complete independently auditable observation proof. They are not inputs to the current successful audit.

The corrected capture retains MAME JSON as well as Fuse stdout. A fresh bounded matrix was executed with the corrected source and executable identities in `runs.tar.gz`; no raw JSON was reconstructed from old summaries. Current `audit.py` reparses each engine's raw observation and requires exact equality with its row summary before checking expected payload/status. Two additional controls alter raw payload while leaving the summary intact, refresh artifact hashes, write real archives, traverse the ordinary loader, and require rejection specifically for raw/summary disagreement. Existing observation conclusions and unresolved hardware limits remain unchanged.

# Fully drained Read Track: payload agrees; status differences remain

All 78 fresh observations transfer exactly 512 or 1024 bytes in physical order,
drain all seven result bytes, return to idle and execute a subsequent Sense
Drive Status command. Each of 13 cases repeats twice on Emu, unchanged Fuse and
installed MAME. No production controller behavior changes in this study.
This is controller-level source/installed-binary evidence, **qualified=false**;
it adds no ROM-loading, physical-hardware or distributed-Emu qualification.

Emu and installed MAME agree on all seven result bytes in the eight comparable
clean/data-CRC cases (sixteen executions per engine). The five MAME DSK ID-error
cases cannot establish ID-only CRC behavior: MAME reports DD as well as DE.
The separately inspected MAME DSK loader constructs a bad data CRC from either
ST1.DE or ST2.DD and sets bad_addr_crc=false. Its source identity is not the
unknown source identity of the installed binary. Keep this input limitation
separate from controller disagreement.

## Repeated observations

Result bytes are ST0/ST1/ST2/C/H/R/N, in hexadecimal. The table shows repeat 1;
repeat 2 agrees exactly in payload, result, idle status and recovery result.
Command is `42 00 00 00 01 02 EOT 2A FF`; EOT is the field count (one or two),
R stays 1, and no terminal-count signal is supplied. Each field has C0/H0/N2.

| Physical IDs | Fixture error | Emu | Unchanged Fuse | Installed MAME |
| --- | --- | --- | --- | --- |
| [1] | clean | `40 80 00 00 00 01 02` | `00 00 00 00 00 02 02` | `40 80 00 00 00 01 02` |
| [1] | ID CRC, first | `40 A0 00 00 00 01 02` | `00 24 00 00 00 02 02` | `40 A0 20 00 00 01 02` † |
| [1] | data CRC, first | `40 A0 20 00 00 01 02` | `00 20 20 00 00 02 02` | `40 A0 20 00 00 01 02` |
| [1, 2] | clean | `40 84 00 00 00 01 02` | `00 00 00 00 00 03 02` | `40 84 00 00 00 01 02` |
| [1, 2] | ID CRC, first | `40 A4 00 00 00 01 02` | `00 00 00 00 00 03 02` | `40 A4 20 00 00 01 02` † |
| [1, 2] | ID CRC, last | `40 A4 00 00 00 01 02` | `00 24 00 00 00 03 02` | `40 A4 20 00 00 01 02` † |
| [1, 2] | data CRC, first | `40 A4 20 00 00 01 02` | `00 00 20 00 00 03 02` | `40 A4 20 00 00 01 02` |
| [1, 2] | data CRC, last | `40 A4 20 00 00 01 02` | `00 20 20 00 00 03 02` | `40 A4 20 00 00 01 02` |
| [2, 1] | clean | `40 80 00 00 00 01 02` | `00 04 00 00 00 03 02` | `40 80 00 00 00 01 02` |
| [2, 1] | ID CRC, first | `40 A0 00 00 00 01 02` | `00 04 00 00 00 03 02` | `40 A0 20 00 00 01 02` † |
| [2, 1] | ID CRC, last | `40 A0 00 00 00 01 02` | `00 24 00 00 00 03 02` | `40 A0 20 00 00 01 02` † |
| [2, 1] | data CRC, first | `40 A0 20 00 00 01 02` | `00 04 20 00 00 03 02` | `40 A0 20 00 00 01 02` |
| [2, 1] | data CRC, last | `40 A0 20 00 00 01 02` | `00 24 20 00 00 03 02` | `40 A0 20 00 00 01 02` |

† Requested DSK metadata only; these are not independent ID-only CRC observations.

DE is ST1 bit 5; DD is ST2 bit 5; ND/EN are ST1 bits 2/7. For the represented
cases Emu retains an encountered CRC error across a subsequent clean field,
reports EN at the requested field count and returns commanded CHRN. Fuse clears
DE on the next ID search while retaining earlier data DD, omits EN/abnormal ST0
at completion and increments result R. Reversed order exposes its different ND
bookkeeping too. These differences reproduce; they do not, by themselves,
establish which chip revision behaves that way. No upstream Fuse issue is filed.

The held NEC December 1978 p14 describes continued reading after CRC errors and
termination by the requested number of fields. It does not settle all of these
status-lifetime/result-register differences, and is an original 765 document,
not a clock/revision-identified physical +3 capture. The [source inspection](inspection.json)
records its exact identity and the MAME loader limitation. [Emu #1860](https://github.com/emu198x/emu198x/issues/1860) tracks the next bounded
raw-input MAME ID-error comparison; a DSK flag is insufficient evidence
that an independent runtime saw an ID CRC error.

## What actually ran

- Emu: unchanged controller at d39df56143246efcef945e52afbc9b78f7b1cfd9,
  plus the hashed authored Rust integration-test probe. Twenty-six fresh
  controller instances in one test process; not twenty-six fresh host processes.
- Fuse: unchanged controller/drive/CRC source at
  0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2, twenty-six fresh ASan/UBSan processes.
  Authored ready raw-track fixture, 8 MHz UPD765A selection and the inherited
  3,546,900-T-state event host; no motor-start/container/ROM claim.
- MAME v0.289(unknown), executable SHA256
  6d40df87963c58cdd3857d203e186af854fea49ae03b2ab555e30ef1029636d9,
  twenty-six fresh specpls3/English BIOS processes. Legitimate local firmware
  identities are recorded; private copies are removed. Lua installs an idle
  CPU loop and accesses controller ports directly. This is deliberately not ROM
  loading qualification. No proprietary firmware or snapshots are retained.

Every engine checks unchanged source media. Fuse fixtures have independently
verified ID/data CRCs and all A1 missing-clock marks. Corruption flips one bit
only in the selected ID or data CRC; other bytes remain identical to the clean
track. MAME uses the existing standard-DSK generator, whose source hash is
recorded separately. Full source/executable/compiler identities, commands,
environment, per-command host times and raw stdout/stderr are preserved in
`evidence.tar.gz`. Runtime host limits are 15s (Fuse), 30s (Emu), 45s (MAME);
all reference invocations completed within 13.78 host seconds. Execution bounds
are 2 million Emu ticks, 2 emulated Fuse seconds and 2 emulated MAME seconds for the
command, with a 10-second MAME whole-machine limit. These are experiment bounds,
not qualified hardware service deadlines.

## Reproduce and audit

Offline, without firmware or emulators:

```sh
python3.14 docs/plans/2026-10-19-read-track-drained-crc/verify.py
python3.14 -m unittest discover -s docs/plans/2026-10-19-read-track-drained-crc -p test_verify.py -v
```

All 140 controller tests pass; targeted Clippy, Rust formatting and Ruff pass.
The fifteen auditor tests include a valid packet and fourteen controls. Recomputed
artifact hashes cannot hide missing/duplicate observations, zero payload,
truncated results, lost recovery, altered raw CRCs/clock marks, false status
agreement, invented source identity, patched reference or qualification claims. The source
guard also rejects dirty references under Python optimisation; its regression
was demonstrated red against the previous script and green after correction.

For a fresh capture in a new checkout of this PR, run:

```sh
python3.14 docs/plans/2026-10-19-read-track-drained-crc/run.py
```

It uses
only existing Cargo, Clang, installed MAME and the recorded local reference/ROM
paths; it refuses an existing runs directory. The exact local commands are in
the archive. The test/probe source hashes distinguish authored adapters from
unchanged reference code. No new dependencies or production interfaces exist.

## Limits and follow-up

This closes the bounded evidence question in #1851, not Read Track support.
All fifteen FDC operations remain partial. Raw gaps/flux/index timing, physical
service cutoffs, A/B differences, combined ID+data damage, terminal count,
N/DTL/length variants and wider Spectrum configurations remain unqualified.
Scan #1813, no-TC Write CHRN #1814 and read timing #1831 remain open.
The earlier public v0.38.3 basic ROM-loading packet retains its own claims;
this study neither replaces nor broadens it.

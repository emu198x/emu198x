# Obtainable evidence confirms programmed Seek spacing

**Both references respond to SRT. They differ in first/final timing. No operation or silicon behavior is qualified by this packet.** This informs [Emu#1970](https://github.com/emu198x/emu198x/issues/1970), alongside the current Emu reproducer in [#1971](https://github.com/emu198x/emu198x/pull/1971).

There are64 operation observations across10 fresh processes: eight source-built Fuse processes (four SRT values, twice) and two installed MAME processes (all16 cases at1us and32us polling). Journeys are0→1,1→3,3→1 and1→1. Every completion returned exact SIS `20,target`; every result drained to idle. These observations do not claim ROM-loaded diagnostic execution.

| SRT | Primary spacing at4MHz | Fuse observed adjacent steps, host ticks | MAME one-step completion,1us polling | MAME two-step/reverse completion |
|---:|---:|---:|---:|---:|
| 0 | 32ms | 113500 | 32.013ms | 64.015ms |
| 1 | 30ms | 106406 | 30.013ms | 60.015ms |
| E | 4ms | 14187 | 4.013ms | 8.015ms |
| F | 2ms | 7093 | 2.013ms | 4.015ms |

Fuse's zero-distance completion is immediate; MAME's is observed at10us. The32us polling run brackets the same MAME completion times. The exact last no-completion and first completed SIS samples are retained; these are not fabricated IRQ edges or internal step transitions.

## What actually ran

Fuse commit `0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2`: unchanged `upd_fdc.c`, `fdd.c`, `crc.c` and their headers, with the existing authored standalone event host. The host supplies an initialized ready drive, without media transfers or motor/firmware claims. It uses3546895 host ticks/second, matching the current Emu PAL host frequency; the earlier study's authored host used3546900. Only that host constant changed. Address/undefined-behavior sanitizers were enabled and emitted no diagnostics.

Fuse steps immediately when the final Seek command parameter is written. It updates physical cylinder/PCN at that instant, schedules another step after its programmed interval, and signals completion one interval after its final step. Its integer calculation truncates `(milliseconds*3546895)/1000` per interval. This differs slightly from an exact rational clock and is preserved as observed reference behavior, not adopted as an accuracy requirement.

MAME0.289 installed binary SHA256 `6d40df87963c58cdd3857d203e186af854fea49ae03b2ab555e30ef1029636d9`; its source commit is unknown. A local English +3 firmware bundle is identified by hashes only. The existing direct-port Lua approach writes an authored idle loop and sets PC, supplies an authored disk and enables the motor. **This is injection plus direct register access, not ROM-loader qualification.** The private firmware directory is removed on exit, and no firmware or RAM/native snapshot is archived. The authored source disk remained unchanged.

MAME completion is observed by repeated public SIS commands. An incomplete Seek returns no-pending SIS `80`; a completed Seek returns `20,target`. This is an active polling experiment, at1us and32us resolutions, rather than passive IRQ/pin observation. The independent case matrix returns consistent brackets at both resolutions. MAME's total durations fit one programmed interval per step, plus2.5us per step and10us final delay. That agrees with the inspected source precedent but does not establish every internal edge in the installed binary.

## What this settles and leaves open

The [NEC December1978 datasheet](https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf), printedpp15–16, defines adjacent-step spacing as `(16−SRT)` milliseconds at8MHz, doubled at4MHz. The executed Fuse step events and MAME distance-dependent completion brackets support this requirement. A fixed256-host-tick whole-journey completion, unchanged by SRT or distance, is inconsistent with the primary requirement and both executed references.

First-step timing, final completion offsets, exact pulse widths, READY sampling at step boundaries, concurrent/aliased drive motion, Recalibrate's search and HUT/HLT remain outside these observations. In particular, the immediate Fuse zero-distance result and MAME's10us result disagree. We have no identified hardware observation to choose an exact offset. Do not silently turn that disagreement into an all-silicon contract or a complete SPECIFY/Seek/Recalibrate qualification.

## Reproduce and audit

Read-only captured-observation audit, requiring no ROMs or reference executables:

```sh
python3 -B docs/plans/2026-10-21-seek-reference-timing/audit.py
```

Fresh execution requires the identified local Fuse checkout, clang/SDK, installed MAME and legitimate local firmware:

```sh
python3 -B docs/plans/2026-10-21-seek-reference-timing/capture.py
```

Capture refuses an existing `runs/` directory. Fuse is bounded to one emulated second per journey and15 host seconds per process. MAME is bounded to250ms per journey,10 emulated seconds overall and45 host seconds per process. Compilation has a60-second host timeout. At most250001 SIS samples per MAME journey can be retained under these bounds; actual1us samples total 340228. Raw logs, commands, environment, compiler/executable/source/firmware identities and exact observation rows are retained in `runs.tar.gz`. `producer-inputs.tar.gz` preserves the authored producer files exactly as hashed by the capture.

The audit validates archive hashes, exact/unique case and producer-process receipts, raw/summary equality, programmed Fuse step events and MAME completion brackets. Six semantic negative controls reject a wrong interval with a matching altered summary, raw/summary disagreement, missing/duplicate cases, false qualification and invented MAME source identity. These controls test the semantic checker, not a claim of hostile archive or compiler attestation. `verification.json` records the result with `qualified=false`.

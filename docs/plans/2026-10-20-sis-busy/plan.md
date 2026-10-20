# Preserve logical drive busy until SIS reads ST0

Lane: Spectrum launch-hardening. Parent frozen closure gate: SIS register phases.

The controller currently exposes drive busy only during its staged seek countdown.
NEC A/B datasheet Seek says the logical busy bits clear through SIS. MAME
`upd765.cpp` lines644–653 clears the selected drive after the first SIS result
byte (ST0), while PCN is still unread. Other pending drives remain busy. Fuse
clears busy on seek completion instead; retain this disagreement explicitly.

1. Public-register red tests: complete Seek/Recalibrate and observe retained
   busy; dispatch SIS and retain busy; read ST0 and clear only that logical bit;
   read PCN and return to idle. Include two logical pending drives and reset.
2. Derive retained busy from pending seek statuses and the unread first SIS
   result, adding no state. Do not alter READY or the parent's command gate.
3. Correct older completion helpers/assertions that equated busy with mechanical
   motion; keep bounded tick limits and actual pending/status assertions. Run all
   controller tests, Clippy and formatting. Retain logs and push parent patch.

Sources: shared primary reference `reference/by-topic/fdc-765/upd765-data-sheet-dec78.md`
(Seek/Recalibrate/SIS printedpp15–16); later NEC A/B transcription Seek section
https://www.tiffe.de/Robotron/Bausteinuebersicht/FDC/necfdc.htm#Seek ; read-only
MAME `emulators/multi-system/mame/src/devices/machine/upd765.cpp`, Fuse
`emulators/zx-spectrum/fuse-emulator-fuse/peripherals/disk/upd_fdc.c`.

## Result and remaining prerequisite

Three public-register tests fail before the fix. Afterward all150 controller
tests pass, none ignored; all-target Clippy and workspace format pass. Busy
retention uses existing saved state, so partial SIS restoration needs no newfield.

A separate confirmed readiness bug remains in this branch: initially-not-ready
Recalibrate returns D0, although NECp16 applies the Seek READY rule and MAME
recalibrate_start returns NR|FAIL|SE (68). D0 does not contain SE or NR. Parent
READY integration must correct that classification before claiming not-ready
Recalibrate busy/phase coverage. This patch does not edit that concurrent work.

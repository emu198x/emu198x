# Isolate Scan Equal early termination before EOT

Lane: Spectrum launch-hardening. Scope: Emu #1813, the first matching normal
512-byte sector with EOT at sector 2. No controller behavior/schema/API change
is planned. No new dependency or general reference framework.

1. Read current rules/decisions, current controller and existing Scan tests,
   held NEC primary, unmodified Fuse controller/drive source and retained MAME
   probe. Retain exact revisions/hashes. Do not treat old observations as new.
2. Add a bounded standalone C reference probe under `reference/`, compiling
   unmodified Fuse `upd_fdc.c`, `fdd.c`, `crc.c` with their production headers.
   A reviewed host adapter supplies allocation/types, event scheduling and
   inert UI/startup wiring only. Actual controller/drive parsing, comparisons,
   CRC and phase transitions remain upstream code. Author a small raw-track
   fixture with two normal 512-byte sectors, correct marks/CRCs and distinct
   bytes. No ROM or full machine state is needed or redistributed.
3. Run first-sector hit, later-sector hit and unsatisfied controls twice.
   Count host bytes until the actual result phase. Keep exact result CHRN
   separately: only early termination is being settled. Check source media
   immutability and preserve build commands/executable/source identities.
4. Rerun the existing MAME direct-port probe in the same logical cases and
   run the existing Emu Scan register regression. Retain failures as failures;
   no patched reference or zero-exit-only verdict. If new Emu bugs appear,
   reproduce and send a separate fix session before feature work.
5. Add an offline auditor with deliberate wrong byte-count/missing-result/
   identity controls. Write a concise report distinguishing source-controller
   evidence, installed runtime evidence and physical-hardware unknowns.
   Review, check, push/merge the owning Emu PR, link #1813, update the umbrella
   remaining-work view if justified. Keep broader conformance open.
6. Remove disposable builds/worktrees only after remote retention and exact
   safe artifact verification. Do not touch canonical or other-session work.

Stop if the adapter must implement Scan behavior or replace the real drive's
read path. Reduce scope and report that blocker rather than manufacturing an
independent reference pass.

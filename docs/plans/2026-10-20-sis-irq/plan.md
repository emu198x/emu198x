# Clear the interrupt signal when Sense Interrupt Status is issued

Lane: Spectrum launch-hardening. Base: e8f5728a58e84d1ad057e33b2e5b5d89c262e4a0.

SIS currently recomputes IRQ from remaining pending seek statuses. The NEC
December 1978 datasheet, printed page 16, instead says issuing SIS resets the
interrupt signal. Pending statuses and the interrupt signal are distinct state.
MAME `upd765.cpp` clears IRQ at `fifo_w` entry and retains other drive records.
Fuse `peripherals/disk/upd_fdc.c` retains IRQ until its seek records are drained;
that disagreement is retained rather than silently treating consensus as proof.

1. Add a public-input regression in `crates/nec-upd765a/tests/sis_interrupt.rs`:
   complete two seeks; issue SIS; require IRQ low immediately; drain both ST0/PCN
   records and verify phase boundaries; show a later completion raises IRQ again.
   Retain a failing log before changing production code.
2. Change only SIS interrupt acknowledgement in `crates/nec-upd765a/src/lib.rs`.
   Preserve the per-drive pending records and both result bytes. No public API,
   saved fields, snapshot schema, READY model or timing changes.
3. Run the focused test, all controller tests, Clippy with warnings denied and
   formatting checks. Retain commands and outputs here; push an owning issue/PR
   for parent review. This fixes one bug; it does not qualify the whole operation.

Primary source: umbrella `reference/by-topic/fdc-765/upd765-data-sheet-dec78.md`,
PDF `/Volumes/Data/Reference-PDFs/by-topic/fdc-765/upd765-data-sheet-dec78.pdf`.
Read-only precedent: umbrella `emulators/multi-system/mame/src/devices/machine/upd765.cpp`
(SIS discussion and `fifo_w`) and Fuse `peripherals/disk/upd_fdc.c` (SIS handling).

Task build cache: umbrella `output/qualification/sis-irq-target`; remove after
review and commit retention, together with this task's linked worktree.

## Result

Owning issue: https://github.com/emu198x/emu198x/issues/1888 .
The public-input regression fails before the fix at `issuing SIS must acknowledge IRQ`.
Two older assertions pinned the incorrect association between pending status and IRQ;
both are corrected while retaining their status-byte checks. All 142 controller tests
pass with zero ignored; all-target Clippy and workspace formatting pass. The initial
post-fix run that exposed the second old assertion is retained as `old-expectation.log`.
Exact commands, versions, source reference hashes and log hashes are in `verification.json`.
No firmware or reference emulator process ran in these focused controller tests.
No API, serialized fields or snapshot version changed. READY support remains separate.

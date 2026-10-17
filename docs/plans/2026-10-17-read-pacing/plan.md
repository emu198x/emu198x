# Pace controller read bytes and report missed requests

Lane: Spectrum launch-hardening. Approved approach: read pacing and overrun,
using existing controller clock/state and public-register tests, then a real
ROM-loading regression. Leave provisional Fuse Scan work unsubmitted.

1. Inspect current read/write/Scan engines, NEC December 1978 read/status
   descriptions and vendored Fuse/MAME controllers. Reproduce unpaced reads and
   incorrect EN-on-abandon with failing tests in
   `crates/nec-upd765a/tests/read_pacing.rs`.
2. Change `crates/nec-upd765a/src/lib.rs` to gate data consumption on RQM,
   publish bytes on the existing FM/MFM byte clock, and terminate missed read
   requests with OR. Reuse the serialized countdown and RQM; no field/layout
   change. Keep initial sector search, inter-sector gaps, DMA/ND, rotational
   timing and exact interrupted-read CHRN explicitly outside this slice.
3. Adapt existing controller test hosts to poll/tick between reads. Retain
   their payload/status assertions. Exercise both read opcodes and Read Track,
   early/wrong-direction access, late service, missed first/later requests,
   fixed cadence, complete draining and command recovery. Check restore at
   RQM-high/low through existing runtime mechanisms where practical.
4. Run targeted red/green tests, full controller tests, relevant machine/runtime
   tests, Clippy and fmt. Execute independent bounded MAME read probes, recording
   disagreement rather than using a patched reference. Reuse existing real ROM
   regression for two passes and a wrong-signature failure with local firmware.
5. Retain safe evidence, exact commands/source/tool identities and limitations.
   Push an owning-repo PR, inspect CI/review and merge when ready. Follow normal
   release workflow for shipping behaviour. Update the umbrella remaining-work
   view and owning issues with actual results. Preserve unrelated checkouts.
6. Remove task-created builds/worktree only once commits and safe evidence are
   retained. Report any precise blocker and the smallest remaining step.

Reference inspection: Fuse read path checks RQM and reports OR on its coarse
400ms timeout; it does not independently establish byte cadence. MAME has
clocked data delivery but a shared 16-byte FIFO, so its read-overrun deadline
must be measured and kept separate from a one-byte hardware service window.

# Correct the Amstrad contention signal

Engineering-frontier continuation of #2265. Implementation and snapshot approval
are pending: this is the concrete design for review, not an adopted decision.

## Reproduction

The current Cybernoid +3 disk entry stays in a corrupted loader state and records
FDC overruns on early transfers into the contended memory bank. First-parent
bisection identifies b5ef8c86b518cee2090656ad4c6c5b8b0c70c397 (paced disk reads)
as the first commit exposing the lost transfer. Its parent reaches the menu;
the October 6 recapture commit exactly matches both original catalogue hashes.
Do not remove correct byte pacing or enlarge its deadline to hide the fault.

The same original disk/firmware boots in Fuse 1.7.0, including with Speedlock
detection disabled, and MAME 0.289. MAME's transparent native I/O taps preserve
all three complete screenshot files and record all 49,152 fast-loader bytes.
Our original core overruns the first four 4 KiB chunks. Disabling contention as
an explicitly unqualified diagnostic control restores the menu with no overruns.

A smaller ROM-free reproduction maps RAM [4,5,6,3], executes NOP/JP from bank 3
at FF00, and changes only I. With refresh also in bank 3 the loop takes 70
master ticks throughout. With I=0, refresh addresses bank 4 and the same loop
also takes 80, 90 and 113 ticks. There are no contended instruction or operand
fetches. The 40077 currently gates the CPU clock on MREQ, including refresh;
it does not drive the CPU's WAIT input. Z80 refresh does not sample WAIT.

## Primary evidence and precedent

- Held Amstrad +3 manual, PDF p275 (Part 30 reference information): explicitly
  specifies wait-state control for contended RAM. PDF p201 distinguishes the
  older Spectrum refresh interaction from the +3, where it does not occur.
  Source record: umbrella `reference/by-system/sinclair-zx-spectrum/`
  `zx-spectrum-plus-3-manual-amstrad.source.json`.
- Existing primary-library `ula-timing-expanded.md`, Memory Contention, already
  states that Ferranti clock stretching must not be carried across to Amstrad.
- Held +3 service manual circuit diagram pp18-19: CPU clock and WAIT wiring.
  Original archive SHA-256:
  1542ce2ce5eed4da4c6dbb901f8c3cd2ab4eb65663c25d4be28b7bbc13ae14c2.
- Vendored SpecIde `ULA::generateVideoDataGa` drives SIGNAL_WAIT_, with MREQ
  qualification; the Sinclair path uses clock contention. This is implementation
  precedent, not primary fact authority.

## Recommended design

1. Qualify the existing Z80 WAIT input at memory/I/O/interrupt edges before
   relying on it. Preserve whole native T-states and inactive memory strobes;
   WAIT must not stall an M1 refresh. If this requires any additional shared
   CPU snapshot layout or public API change, stop for a separate concrete
   compatibility decision rather than extending the Amstrad approval silently.
2. In the standalone amstrad-ula-40077 repository, change
   `crates/emu198x-amstrad-ula-40077/src/lib.rs` to keep the CPU clock running
   and expose the physical active WAIT output. Carry its live output explicitly
   in the chip state. Add pin/phase tests including contended refresh, memory
   reads/writes and I/O/internal cycles; retain rendering and interrupt tests.
   Publish the changed chip contract before the machine consumes it. Existing
   consumers must wire WAIT rather than use it to suppress CPU clock edges.
3. In `crates/common-sinclair-zx-spectrum-amstrad-class/src/core.rs`, feed that
   output to the existing Z80 WAIT input at each scheduled edge. Keep the master
   oscillator, FDC byte clock, video and audio clocks independent of CPU waits.
   Add the ROM-free refresh-bank regression and original-firmware disk boot
   regression, plus contended/uncontended read/write/refresh controls.
4. Write an explicit narrow decision for the 40077: override RULES.md clock
   rules 2-3 only where they imply suppressing CPU edges on this chip. The
   physical WAIT mechanism leaves the CPU clock running; no catch-up clocks,
   guessed frame lengths or instruction-level shortcuts are introduced.
5. In `crates/runtime-sinclair-zx-spectrum/src/snapshot.rs`, write version 20
   for +2A/+2B/+3 and reject v19 before payload decoding. Preserve all original
   v19 fixtures as rejection controls. Existing physical 48K boards remain v20,
   Timex v18 and the other models v19. Test mid-contention native continuation,
   atomic rejection and independent board/profile identity.
6. Verify the fixed published chip and ordinary machine boot through normal CI.
   Recapture every one of the 103 catalogue entries using the retained inputs,
   with explicit per-entry results and negative controls. Keep every expected
   hash unchanged until each changed waypoint is explained and qualified; do
   not bless a boot failure or unexplained phase merely to obtain green tests.

Alternative: retain the current implementation and leave #2265 explicitly open.
Disabling contention or weakening FDC overrun detection is not a candidate fix.

## Resources and evidence

Evidence lives in /private/tmp/198x-runtime-state-evidence until archived durably.
Historical worktree/cache: /private/tmp/198x-plus3-bisect and
/private/tmp/198x-plus3-bisect-target. Headless Fuse/libspectrum reference build:
/private/tmp/198x-fuse-reference-build. Upstream chip clone:
/private/tmp/198x-amstrad-contention. Main task worktree/cache:
/private/tmp/198x-runtime-state-correctness and /private/tmp/198x-runtime-state-target.
All task processes and temporary source modifications must be removed before
completion. Completed BBC, Aquarius, staged-cartridge, CPC and shared release
changes have already merged independently; they do not depend on this design.

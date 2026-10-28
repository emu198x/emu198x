# Native frame requests (#2136)

Lane: engineering-frontier. Base: 6f84ad5c9. Exact shared contract approved by the user.

## Cause and design

`HeadlessSession::run_frames` asks `run_until` for a nominal frame budget.
SNES advances whole native frames until it crosses the target. Its initial
partial field and variable field lengths can require two output frames.
Changing hardware timing or dividing the reported count would hide the fault.

Adopt an additive `MachineCore::run_frame` operation. Unsupported
runtimes refuse before advancing. The session uses the exact operation when
available without a nominal-time fallback.
All current runtimes implement the native-boundary operation. Frame-granular
runtimes reuse their delivery path; helpers that previously used CPU budgets
now have a separate operation driven by the existing chip event or fixed-raster
clock phase. Time-budget requests retain
their present meaning. No serialized state or hardware timing changes.

## Steps and verification

1. Retain fresh failures with the existing original SNES cartridge probe in
   `docs/plans/2026-10-27-small-issue-fixes/snes-probe.py`.
2. Agree the additive contract and update
   `knowledge/decisions/runtime-internal-shape.md` before implementation.
3. Add `StopReason::NoFrame` for bounded attempts without a completed field.
   Add the operation in `crates/emu198x-shell/src/machine.rs`; route normal,
   traced and recording frame requests in `src/session.rs` through it.
   Shell tests cover exact calls, explicit refusal, early stop, zero frames,
   sink errors, inputs, and recording/trace parity.
4. Implement the operation across every `runtime-*/src/runtime.rs` and the
   C64/Spectrum/Amiga family dispatchers, including `runtime-nintendo-snes/src/runtime.rs`,
   `runtime-sega-mega-drive/src/runtime.rs` and `runtime-snk-neo-geo/src/runtime.rs`.
   Integration tests compare emitted frames, machine counters and elapsed time
   over multiple counts, all existing regional profiles, and successive calls.
5. Run the retained CLI reproducer, script controls, affected suites, strict
   Clippy and formatting. Demonstrate the frame regression fails without the
   production correction. Push, obtain green CI, rebase-merge and verify #2136.
6. Verify remote retention and remove the temporary worktree/build cache.

## Related authorised tasks

- #320: separate Game Boy test-only change: identify local game images and
  compare deterministic checkpoints with independently run SameBoy before
  retaining hashes. Cover identified model/firmware choices and fail explicitly
  for configured missing/wrong fixtures. No production state/API change.
- #242: separate MSX change after consulting primary PPI wiring, local machine
  split work and reference emulator. Agree any audio/state extension before
  implementation. Keep cassette transport under #240 and avoid broadening the
  shared MSX machine's model dispatch.

The archive paths named by rule 25 are absent on this machine. Frozen donor
trees remain reference-only under the umbrella rule; no migration from them.

## Audit and results

The expanded audit includes all concrete runtimes and the family dispatchers.
The native UI now uses this operation for whole fields and turbo/refresh, while
sub-field UI slices keep their time-budget path. Wall pacing uses actual elapsed
master time. No chip divider, bus cycle, contention rule or serialized layout is
changed. Transient VDP/CRTC observation latches are skipped by serde and cleared
for each request. Existing `run_until` time requests retain their separate path.

Boundary sources: C64/C128 tick completion; Amiga completed-field projection;
Spectrum driver's half-cycle wrap; SNES PPU / Mega Drive VDP / Neo Geo LSPC;
TMS/Sega VDP wrap in MSX, ColecoVision, SG-1000, SMS/Game Gear, Sord and SVI;
BBC/CPC/PET CRTC, Electron/Atari fixed raster, VIC/Atom/Jupiter/ZX80/ZX81 video
completion, Dragon's existing beam wrap and Game Boy's PPU frame-ready event.
This is execution-boundary verification of the current models, not blanket
qualification of every video model's physical accuracy. The existing renderer
limitations remain separate issues.

The TIA's defensive roll and Game Boy's LCD-off timeout are not counted as
frames. Bounded attempts return `NoFrame`; CPU/audio progress is retained and no
completed-frame packet is sent. Stale frame-ready flags cannot satisfy a fresh
operation. Unsupported external runtime implementations refuse explicitly.

The retained CLI probe fails before the change and passes all six cases after
it. The script control verifies 2 + 0 + 3 frames with equal CLI clocks. Independent
machine paths verify clock/count invariants across every existing SNES/Mega Drive
profile and Neo Geo with tiny, nominal and oversized pacing hints. Partial-step
VDP/raster/CRTC/C64 controls and Dragon/Game Boy replay pass. Reintroducing the
nominal session budget fails the SNES invariant test (exit 101); restoring the
exact path passes. `verification.json` retains positive test counts and results.

All 61 affected library suites pass: 1,899 passed, 0 failed, 25 ignored.
Strict all-target Clippy passes for all 61 affected packages. Formatting and
`git diff --check` pass. An earlier broad run was stopped after its code was
superseded; it spent almost nine minutes in firmware-backed Amiga goldens and is
not presented as a completed verification. Full integration CI remains required.

### Files

- `crates/emu198x-atari-tia/src/lib.rs`
- `crates/emu198x-shell/src/launch.rs`
- `crates/emu198x-shell/src/machine.rs`
- `crates/emu198x-shell/src/query.rs`
- `crates/emu198x-shell/src/script.rs`
- `crates/emu198x-shell/src/session.rs`
- `crates/emu198x-ui/src/host_core.rs`
- `crates/emu198x-ui/src/lib.rs`
- `crates/machine-acorn-atom/src/lib.rs`
- `crates/machine-acorn-bbc-micro/src/lib.rs`
- `crates/machine-acorn-electron/src/lib.rs`
- `crates/machine-amstrad-cpc/src/lib.rs`
- `crates/machine-atari-2600/src/lib.rs`
- `crates/machine-atari-5200/src/lib.rs`
- `crates/machine-atari-7800/src/lib.rs`
- `crates/machine-atari-800xl/src/lib.rs`
- `crates/machine-coleco-colecovision/src/lib.rs`
- `crates/machine-commodore-pet/src/lib.rs`
- `crates/machine-commodore-vic-20/src/lib.rs`
- `crates/machine-dragon-32/src/lib.rs`
- `crates/machine-jupiter-ace/src/lib.rs`
- `crates/machine-mattel-aquarius/src/lib.rs`
- `crates/machine-msx/src/lib.rs`
- `crates/machine-nintendo-game-boy/src/lib.rs`
- `crates/machine-nintendo-nes/src/lib.rs`
- `crates/machine-sega-master-system/src/lib.rs`
- `crates/machine-sega-sg-1000/src/lib.rs`
- `crates/machine-sinclair-zx80/src/lib.rs`
- `crates/machine-sinclair-zx81/src/lib.rs`
- `crates/machine-snk-neo-geo/src/lib.rs`
- `crates/machine-sord-m5/src/lib.rs`
- `crates/machine-spectravideo-svi-328/src/lib.rs`
- `crates/machine-tatung-einstein/src/lib.rs`
- `crates/runtime-acorn-atom/src/runtime.rs`
- `crates/runtime-acorn-bbc-micro/src/runtime.rs`
- `crates/runtime-acorn-electron/src/runtime.rs`
- `crates/runtime-amstrad-cpc/src/runtime.rs`
- `crates/runtime-atari-2600/src/runtime.rs`
- `crates/runtime-atari-5200/src/runtime.rs`
- `crates/runtime-atari-7800/src/runtime.rs`
- `crates/runtime-atari-800xl/src/runtime.rs`
- `crates/runtime-atari-st/src/runtime.rs`
- `crates/runtime-coleco-colecovision/src/runtime.rs`
- `crates/runtime-commodore-amiga/src/runtime.rs`
- `crates/runtime-commodore-amiga/src/variants.rs`
- `crates/runtime-commodore-c64/src/family_runtime.rs`
- `crates/runtime-commodore-c64/src/runtime.rs`
- `crates/runtime-commodore-c64/tests/lifecycle.rs`
- `crates/runtime-commodore-pet/src/runtime.rs`
- `crates/runtime-commodore-vic-20/src/runtime.rs`
- `crates/runtime-dragon/src/runtime.rs`
- `crates/runtime-jupiter-ace/src/runtime.rs`
- `crates/runtime-mattel-aquarius/src/runtime.rs`
- `crates/runtime-memotech-mtx/src/runtime.rs`
- `crates/runtime-msx/src/runtime.rs`
- `crates/runtime-nintendo-game-boy/src/runtime.rs`
- `crates/runtime-nintendo-nes/src/runtime.rs`
- `crates/runtime-nintendo-snes/src/runtime.rs`
- `crates/runtime-nintendo-snes/tests/runtime.rs`
- `crates/runtime-oric-atmos/src/runtime.rs`
- `crates/runtime-sega-master-system-class/src/runtime.rs`
- `crates/runtime-sega-mega-drive/src/runtime.rs`
- `crates/runtime-sega-mega-drive/tests/variants.rs`
- `crates/runtime-sega-sg-1000/src/runtime.rs`
- `crates/runtime-sinclair-zx-spectrum/src/family_runtime.rs`
- `crates/runtime-sinclair-zx-spectrum/src/runtime.rs`
- `crates/runtime-sinclair-zx80/src/runtime.rs`
- `crates/runtime-sinclair-zx81/src/runtime.rs`
- `crates/runtime-snk-neo-geo/src/runtime.rs`
- `crates/runtime-snk-neo-geo/tests/runtime.rs`
- `crates/runtime-sord-m5/src/runtime.rs`
- `crates/runtime-spectravideo-svi-328/src/runtime.rs`
- `crates/runtime-tatung-einstein/src/runtime.rs`

Headless reports now distinguish the session's verified native request count
(`frames_run`) from the retained machine/lifetime diagnostic
(`machine_frame_counter`). `run_result` exposes an early stop such as `NoFrame`.
The final shell/UI rerun after this reporting change passes separately in the
verification ledger. Machine clocks and snapshot layouts are unchanged.

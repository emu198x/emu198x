# Small issue fixes in priority order

Lane: engineering-frontier validation and host reliability.

Work from current main in an isolated checkout. Fix confirmed bugs before
small documentation corrections. Each change gets its own tested commit;
related small commits can share one review/CI run. No new dependencies,
public interfaces, snapshot schemas or hardware timing models are planned.

1. **Einstein malformed DSK input (#277).** Reproduce the count/shift/payload
   faults, then validate track/descriptor/data bounds and representable geometry
   in `crates/machine-tatung-einstein/src/lib.rs` before allocation/mounting.
   Add public-API malformed-input and unchanged-mounted-disk controls in
   `crates/machine-tatung-einstein/tests/dsk_validation.rs`. Run machine/runtime
   tests and Clippy. Keep container consolidation separately open if needed.
2. **C64 MCP PRG startup (#1858).** Inspect the shared launcher's startup-media
   contract, then reuse the existing importer in `crates/emu198x-c64/src/app.rs`.
   Add an original PRG regression that checks memory and real-ROM RUN if present;
   run the owning app tests and Clippy. Do not duplicate startup media loading.
3. **CoCo firmware and shifted text (#2134/#2135).** Trace the direct harness's
   firmware composition and key scheduling in `crates/emu198x-dragon/src/script.rs`
   and existing runtime keyboard helpers. Reproduce each separately, fix through
   the existing contracts, test both Dragon and CoCo with model-matched ROMs.
4. **SNES frame accounting (#2136).** Reproduce with an original cartridge;
   compare emitted frames and elapsed native ticks before correcting the existing
   launcher/runtime boundary. Run NTSC/PAL and script controls, no clock shortcut.
5. **Small truthful documentation (#2137/#2147/#2143).** Correct the finite UI
   audit scope, source-backed A2000 chip wording and unimplemented SC-3000 claims
   in their owning files. Preserve historical changelog context. Verify each
   against the registry/actual adapters or held primary sources; no hardware
   changes in a prose fix.

Recheck issue/PR state before each step. Preserve other active worktrees.
Push tested commits, require actual completed CI before merge, verify issue
closure or explicitly retain residual scope. Save useful evidence before
removing task worktrees, build caches and temporary processes.

## Verified first fixes

Einstein: valid standard/extended sector-ID ordering is preserved; malformed
counts/shifts/extents and unsupported nonuniform geometry return errors before
mounting. The regression failed before the fix; four validation tests and the
owning machine/runtime suites pass afterward. Strict owning Clippy passes.
#277 retains the separate neutral-container consolidation scope.

C64: the actual MCP subprocess regression returned untouched RAM before the
fix. The shared startup importer now waits for boot and imports the PRG for
both UI and MCP; MCP diagnostics go to stderr. The real-ROM regression passes:
all loaded bytes match and typed RUN executes the original POKE program.
The app suite, real-ROM script-loader control and strict headless Clippy pass.
No startup tape/disk handling is duplicated in the MCP constructor.

CoCo firmware: the regression fails on catalogue-named 8 KiB chips before the
fix and passes for both models afterward. The harness reuses coco_rom_image,
retains Color BASIC-only and explicit combined-image inputs, and rejects bad
sizes. The help now distinguishes the Dragon-32 direct harness from the shared
runtime paths. The full headless app suite and strict Clippy pass. Original
MAME Color BASIC 1.2/1.3 plus Extended BASIC 1.1 boot both models and accept
PRINT 6 through --type-command; hashes/logs are retained separately.

Dragon/CoCo symbols: the old harness mapped quotes specially and sent `*`/`+`
as nonexistent named keys. The unit regression failed, and original-ROM runs
on Dragon 32, CoCo 1 and CoCo 2 echoed PRINT 67 and returned 67 for PRINT 6*7.
Both traced and untraced paths now use the runtime keyboard's existing chords,
with unchanged hold/release timing and explicit errors for unknown characters.
Fresh ROM runs on all three machines echo the requested commands exactly:
PRINT 6*7 returns 42; PRINT 6+7 returns 13. Owning tests/Clippy pass.

A2000 prose: Commodore's Release 2 Getting Started chip table explicitly lists
8361/8367 for the A1000 and German A2000; PN 314135-01 identifies U101 as
8370 NTSC/8371 PAL, while rev6 PN 314135-02 lists 8372. Profile/chip comments
now distinguish these boards; this does not select a new emulated board or
change chip behaviour. Formatting/diff checks pass.

## Smaller scope corrections and remaining frame issue

#2137 now names the UI audit's finite 30-adapter scope and the four later
native adapters; all 30 rows and local links were checked. #2143 removes
SC-3000 from three active Cargo descriptions. Cargo metadata confirms the
shipped descriptions; historical release text remains with an Unreleased
clarification. SC-3000 implementation stays in #224/#225.

#2136 is reproduced using an original 32 KiB LoROM and no firmware. Requests
1/2/5 emit 2/4/10 NTSC frames and 2/3/6 PAL frames. The first machine frame
ends at the PPU's V=240 output boundary, before the nominal full-frame target;
NTSC's alternating short field also falls below that fixed budget. The runtime
keeps calling run_frame until its time target is reached, while the shared
session assumes a fixed budget per requested frame. Source inspected:
HeadlessSession::run_frames, SnesRuntime::run_until, Snes::run_frame and the
PPU counter/line_start. Ares's SFC counter and the existing machine tests retain
variable field timing. Do not change silicon timing, halve N or change only the
reported count. A correct shared native-frame stepping contract needs separate
design; this issue remains open. The bounded diagnostic is retained beside
this plan, outside the ordinary passing test suite.

[Verification receipts and fixture identities](2026-10-27-small-issue-fixes/verification.json)
and adjacent logs retain the before/after results. No copyrighted ROM bytes
are committed. The SNES reproducer accepts explicit binary/output paths and
exits nonzero for extra frames; its output directory must be new.

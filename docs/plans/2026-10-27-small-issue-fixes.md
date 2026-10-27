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

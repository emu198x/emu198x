# emu198x-script-amiga — port-gap analysis (2026-04-21)

Phase 1–3 gap list for task #181, following the archive-port
methodology. Like the Runtime port (#179), script-amiga is a
bridge crate; the "archive" ran against the pre-restart runtime,
and the port rewires it to the ported runtime. Because the
runtime's public API held the shell-facing shape stable, the
script binary ends up needing **zero source changes** — only the
workspace include path moves.

## What script-amiga is

A minimal headless Amiga runner CLI. Takes a Kickstart ROM,
optionally an ADF image, optionally a script file, and drives the
machine through `emu198x_shell::HeadlessSession` + `AmigaRuntime`.
Outputs PNG screenshots, WAV audio captures, and JSON query
reports.

## Current-tree coverage vs archive

| Area | Current (archive) | Post-port |
| --- | --- | --- |
| Depends on `runtime-commodore-amiga` | ✅ already | ✅ (now points at ported runtime) |
| CLI: `--kickstart`, `--disk`, `--script`, `--wait-for-boot`, `--frames`, `--screenshot`, `--audio-capture`, `--print-query` | ✅ | ✅ unchanged |
| `boot.detected` / `boot.reason` queries | ✅ | ✅ — these were restored in the runtime during this task |
| Screenshot capture (PNG) | ✅ via `HeadlessSession::save_screenshot` | ✅ (runtime emits RGBA frames the shell captures) |
| Audio capture (WAV) | ✅ via `HeadlessSession::save_audio_capture` | ✅ (runtime currently emits empty AudioPackets; WAV has no samples yet) |
| ROM-directory auto-discovery | ✅ | ✅ unchanged |
| In-crate tests (CLI parse + PNG/WAV capture) | ✅ 2 tests | ✅ still pass against ported runtime |

## Changes required by the port

1. **Runtime query-path restoration.** During the Runtime port
   (#179) the `boot.detected` / `boot.reason` / `boot.row` paths
   were dropped from the runtime's query provider. script-amiga
   uses `HeadlessSession::wait_for_boot` which queries
   `boot.detected`, so those paths had to be re-added. The
   heuristic is the archive's: "non-white pixel count > 1000 with
   a known first active row means the Kickstart insert-disk screen
   or beyond is painted."

2. **Workspace include.** The `-archive` directory rename brings
   the binary crate into the live build. No source change.

3. **Cargo.toml path bump.** The archive's path dep on
   `runtime-commodore-amiga` still resolves after the Runtime port
   because that crate now lives at the live path (`../runtime-
   commodore-amiga`). Nothing to change in script-amiga's manifest.

## Known simplifications

1. **Empty audio capture.** Until Paula grows a runtime-facing
   resample buffer (separate follow-up), `--audio-capture` writes
   a zero-sample WAV. PNG capture works fully because Denise's
   framebuffer is live.

2. **No CI-driven ROM test.** The archive had an `#[ignore]`'d
   test that expected `/Users/stevehill/.emu198x/roms/commodore-
   amiga/kick13.rom`. Kept intact — not a port concern.

## Per-phase execution

Single-commit port matching the Runtime pattern:
1. Ensure the Runtime exposes `boot.detected` / `boot.reason` /
   `boot.row` so `wait_for_boot` can resolve them.
2. Pull script-amiga into the workspace member list and remove
   from `exclude`.
3. Rename directory `emu198x-script-amiga-archive/` →
   `emu198x-script-amiga/`.
4. Run the 2 in-crate tests against the ported runtime —
   both pass unchanged.

## Conclusion

This is the smallest of the Amiga ports by net change: a path
rename plus restoring three query paths in the runtime. The
existing `HeadlessSession` contract gave script-amiga's source
complete stability across the runtime rewrite.

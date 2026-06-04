# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/runtime-sinclair-zx-spectrum-v0.2.0) - 2026-06-04

### Added

- *(spectrum)* wire Pentagon / Scorpion / Timex variant dispatch

### Fixed

- *(spectrum)* step via retirement counter, matching the Z80 machines
- *(spectrum)* three Code198x-authoring speed bumps
- *(spectrum-plus3)* refresh boot golden after FDC drive detection fix

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- rustfmt the workspace clean
- *(spectrum)* restore criterion frame-throughput bench harness
- cargo fmt + clippy clean across the workspace
- watch_ay_* — AY register-write tracer
- port_read / port_write — direct bus-level Z80 I/O
- step / run_until_pc / disasm — second half of Z80 debug suite
- query_cpu — read every Z80 register in one MCP call
- full-trace memory_read / poke / watch_memory tools
- Spectrum follow-ups: generalise autoload helpers + frame-tick hook
- Spectrum MCP at family level — SpectrumRuntimeKind + set_machine
- cargo fmt --all across the workspace
- Open Emu198x for public release
- 13 new tests covering MachineCore impl + helpers
- Sinclair Interface 2 keyboard-matrix routing
- Float48K strict assertion un-gated (architecture review Seam 5 #2)
- Boot invariants: INT timing on 128K and Pentagon (Seam 5 #1b, #1c)
- Update Unclean/Reference asset paths to assets/
- Boot invariants: contention table waypoint (Seam 5 #9)
- Boot invariants: 4 new 128K-family waypoints (Seam 5 expansion)
- Boot invariants: 5 new Seam 5 waypoint assertions for the 48K runtime
- Rename BoardIssue → UlaRevision with explicit revision variants
- FDC disk survives snapshot restore + serde_skip audit (Seam 3)
- Kempston joystick input routing (Seam 2)
- Bump png 0.17 → 0.18
- Apply cargo fmt to three files that had drifted
- Speedlock silent-music: root cause was missing post-load keypress sequence, not an emulator bug
- Speedlock silent-music: watchpoint test, plus correction — the 2-byte divergence is just stack-visible BC, not a checksum
- Restore CI: cargo fmt + clippy --all-targets clean across the workspace
- Unblock Green Beret with a working 1986 Imagine rip
- Acquit IO contention model: IN A,(FE) costs 13T avg, matches spec
- Localise Green Beret shortfall: chip's fill takes 1424ms vs 1336ms target
- Pin chip tolerance: passes 1555ms, wedges 1335ms — ~220ms shortfall
- Survey Speedlock-7 calibration pauses: Green Beret is a rip outlier
- Rule out byte-decoder cost as Green Beret's Speedlock-7 timing-slip cause
- confirm Green Beret timing skew via pause-extension test
- Green Beret bug is TIMING, not bit-decoding
- pin Green Beret to 16 garbage bytes at \$90ef-\$90fe
- pin Green Beret's wipe-write to $feaf checksum check
- map the three anti-tamper wipe paths
- seven more Speedlock-7 48K titles — Spectrum reaches 80 entries
- shares Speedlock-7's loader code, fails on post-load verify
- Verify Speedlock-2 / Speedlock-5 status after the partial-byte fix
- Fix TZX partial-last-byte parsing — unblocks Speedlock-7 tape loading
- Trace Speedlock-7 byte decoder without FUSE
- pinpoint wipe trigger at $fd6c (L=$28, want $3A)
- RAM-dump harness + tape-edge timing ruled out
- result-phase R reflects abort sector vs EOT+1 correctly
- Add five new +3 catalogue entries: seven distinct loader paths
- Carry per-sector ST1/ST2 + DDAM through the EDSK pipeline
- Author five +3 disk catalogue entries spanning three protections
- +3 disk Loader now runs end-to-end (architecturally)
- Match +2 boot banner on a single rendered row
- Wire Spectrum+ into the catalogue runner
- Reattach Spectrum ULA timing config on snapshot restore
- Cross-variant Spectrum format-load matrix: 33 green, no #[ignore]
- Wire portable .sna / .z80 snapshot import; rename State menu honestly
- Update K_CUR alongside E_LINE so the editor inserts into the right buffer
- Tap ENTER past the copyright banner before pressing the K prompt
- Add load_basic_program runtime helper with system-variable fix-up
- Prepare Spectrum runtime for native-menu Phase 2 machine swap
- Track 1C Phase 1: native Machine menu shell + AppCommand channel
- Lock golden boot screens for the 8 in-scope Spectrum variants
- Apply cargo fmt updates from Rust toolchain 1.95.0
- Lift Kempston joystick to a Peripheral, migrate all hosting machines
- Complete Spectrum SOLID variant coverage via class layer crates
- Apply cargo fmt across in-tree edits + refresh Cargo.lock
- Expose AY-3-8912 register file as runtime queries
- Document Scorpion / TS2068 banner blockers from probe results
- Paging-aware glyph reader: 4 more Spectrum banners confirmed
- Verify Spectrum boot banners: TC2048 confirmed, 5 variants blocked
- Generalise SpectrumSessionQueryProvider across all 7 variants
- Post-track tidy: rustfmt sweep + motorola-68000 doc accuracy
- directed-test passes across the runtime family
- Split Spectrum runtime (hybrid shape) + decision record
- Add boot_invariants.rs for the four anchor families
- add native channel controls
- commit mechanical cleanup across diagnostics
- Run rustfmt across the workspace
- Tidy Spectrum runtime layering and file layout
- Consolidate Spectrum per-machine boilerplate
- Clean up Spectrum family architecture
- Wrap every Spectrum variant in a generic MachineCore runtime
- Add Timex TC2048 + TC2068/TS2068 machines, extend runtime catalogue
- Add Spectrum tape autoload workflow
- Add Jet Set Willy Spectrum regression
- Fix Spectrum tape loading and add Manic Miner regression
- Add Spectrum UI verifier shell
- Add shared boot wait workflow
- Add Spectrum boot detection query surface
- Add Spectrum machine timing integration checks
- Add Spectrum family query namespace
- Add shared headless session and JSON scripts
- Formalize firmware bootstrap and media transport control
- Add Spectrum snapshots and headless runner
- Add Spectrum media runtime and beeper audio
- Add stable Rust CI
- Bootstrap workspace and documentation baseline

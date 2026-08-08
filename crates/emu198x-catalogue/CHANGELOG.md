# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Run all 13 C64 entries through byte-fixed-point snapshot re-encoding plus
  frame/audio replay. The coverage includes firmware-only boot, D64, D81,
  writable raw-GCR G64, TAP, three cartridge types, and 1541/1571/1581 drive
  routes.
- Report the first differing offset, values and total byte count when snapshot
  re-encoding is not canonical.
- Requalify Bomb Jack's audio oracle after VICE-pattern cold RAM changed its
  deterministic music stream; ordinary capture and snapshot replay agree on
  the replacement hash while the title frame remains unchanged.
- Honour an entry's explicit `media.writable` setting while keeping archive
  media read-only by default.
- Add optional, bounds-checked boot-frame ignore rectangles. Selected RGBA
  pixels are zeroed only for hashing, while capture PNGs remain complete for
  review.
- Restrict the Workbench 1.3 exclusion to its allocator-derived free-memory
  field; every pixel outside that 50 x 18 rectangle remains exact.
- Add strictly tagged, sequential `entry.startup` actions for passing guest
  release screens, trainers, selectors, and prompts through ordinary emulated
  input. Every wait and input hold is bounded, and manifests cannot mix this
  form with legacy absolute-frame `entry.script` steps.
- Advance one native frame after every startup input release so adjacent
  actions cannot collapse their release and next press into one host batch.
- Move the Arkanoid Ackerlight intro handoff to the bounded startup navigator
  at its corrected-MFM input point, retaining the reviewed golden waypoint.
- Move the Barbarian, 1943, Bad Dudes and Banshee release-screen and trainer
  handoffs to the same bounded navigator, using input points reviewed after
  the corrected MFM pacing.
- Move Banshee's post-release capture from an active dissolve to the midpoint
  of matching 100-frame samples across an 800-frame POWERUPS-page span.
- Requalify the reviewed Barbarian, Bad Dudes, State of the Art and Alien
  Syndrome visual/audio waypoints after corrected disk, Copper and blitter
  arbitration shifted their deterministic animation phases.
- Route A600 ECS and A1200 AGA entries through the Amiga catalogue runner,
  using each runtime profile's native frame timing for capture and audio
  windows.
- Add Workbench 3.1, Banshee, State of the Art and Alien Syndrome entries,
  including an A500 OCS NTSC firmware route.
- Run every Amiga catalogue entry through the shared save-state fixed-point,
  frame-replay and audio-replay gate.
- Requalify all ten Amiga entries under the corrected final arbitration core;
  each entry now passes its exact frame/audio oracle and snapshot/replay gate.
- Requalify the Workbench 3.1 and Banshee AGA frame baselines after the Lisa
  bitplane/DIW phase correction. Banshee's overlapping playfield is exactly a
  two-host-sample translation; Workbench retains its complete pointer image.

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/emu198x-catalogue-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt --all across the workspace
- Add cargo-dist release workflow for the six native shells
- Open Emu198x for public release
- C64 + NES Seam 4: catalogue oracle integrity
- Tree housekeeping: project relocation paths + Cargo.lock
- Kempston-routing verification entry (Sabre Wulf, Seam 2)
- re-capture SpeedLock cluster at FRAME_ROUTING_VERSION=3
- re-capture +3 mirrors at FRAME_ROUTING_VERSION=3
- re-capture +2B mirrors at FRAME_ROUTING_VERSION=3
- re-capture +2A mirrors at FRAME_ROUTING_VERSION=3
- re-capture +2 mirrors at FRAME_ROUTING_VERSION=3
- re-capture 128K slice at FRAME_ROUTING_VERSION=3
- re-capture 16K entries at FRAME_ROUTING_VERSION=3
- re-capture Plus duplicates at FRAME_ROUTING_VERSION=3
- re-capture 48K vanilla slice at FRAME_ROUTING_VERSION=3
- capture-mode bypasses routing-version check
- stage Arkanoid floating-bus entry, revert manifest version
- AOLatch border granularity (Smith Ch 14)
- two-stage shifter pipeline (Seam 1 of architecture review)
- routing-version gating for hash re-capture discipline
- Remove EMU198X_POST_LOAD_KEY env var — script's `input` step already does this
- Recapture 21 catalogue hashes that drifted from the AY mix + HALT fix + AY port-A pull
- Migrate five Speedlock catalogue entries from TAP to TZX + Speedlock reference + SkoolKit byte-diff finding
- Speedlock silent-music: pin Rainbow Islands' loader stall to TAP/TZX
- Speedlock silent-music: pin the mechanism to the loader's decoy trap
- Restore CI: cargo fmt + clippy --all-targets clean across the workspace
- +2A and +2B entries — 10 each, all PASS
- Unblock Green Beret with a working 1986 Imagine rip
- seven more Speedlock-7 48K titles — Spectrum reaches 80 entries
- document why Speedlock attract waypoints are silent
- four Speedlock-tape entries unlocked by the partial-byte fix
- static analysis of Op Wolf loader architecture
- model exec-phase read timeout + rotational ReadID
- Add Microsphere + Bleepload + Speedlock-cluster sweep
- pause=0 in data blocks means "no pause", not "stop"
- Add four Speedlock-6 +3 catalogue entries
- Add five new +3 catalogue entries: seven distinct loader paths
- Carry per-sector ST1/ST2 + DDAM through the EDSK pipeline
- Author five +3 disk catalogue entries spanning three protections
- Add 9 new 128K + 10 +2 catalogue entries (50/51 SNAP-PASS)
- Wire +2 / +2A / +2B into the catalogue runner
- Push 48K, Spectrum+, 16K to 10/10 (32/32 SNAP-PASS)
- Wire 16K into the catalogue runner
- Add Spectrum+ catalogue entries (16/16 SNAP-PASS)
- Wire Spectrum+ into the catalogue runner
- Build the Spectrum save-state catalogue harness
- Fix new clippy lints introduced by Rust 1.95.0
- Apply cargo fmt updates from Rust toolchain 1.95.0
- Complete Spectrum SOLID variant coverage via class layer crates
- Document Spectrum +3 disk subsystem gap is not a regression
- Round 5 close: WB 2.04 hash bump, arkanoid + bad-dudes-ecs committed
- Round 4: joystick FIRE + Amiga real-game titles + Aztec rescued
- Round 3: Spectrum +3 disk entry + mouse-click script support
- Round 2: 2 C64 SID showcases (Wizball + Monty on the Run)
- Round 1: ECS Amiga + 2 OCS real-game entries
- Wire Amiga runtime + add Workbench 1.3 desktop entry
- Add C64 disk-game wins + tape autoload
- Wire C64 runtime + support disk-loaded games via scripted RUN
- Add 4 NES entries across UxROM / MMC1 / NROM / MMC3
- Wire NES runtime, add Super Mario Bros, validate cross-system
- Wire 128K runtime, extend 48K bench, validate cross-variant dispatch
- Land catalogue harness with Manic Miner end-to-end

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add machine-level pointer-advance regressions for clean-idle
  register-equal DDF windows on original and Fat Agnus

### Fixed

- Preserve original Agnus's phase-shifted short-line terminal result
  through machine timing and runtime snapshot restore so the following
  pre-`$18` DDF start remains inhibited
- Preserve original Agnus's carried horizontal DDF hard-start gate
  through machine arbitration and runtime snapshots
- Enforce Fat Agnus 8372A's default `$D8` bitplane-DMA stop in
  OCS-shaped machines while retaining `HARDDIS` and the other enhanced
  horizontal-limit bypasses
- Preserve original Agnus's `$D8` terminal fetch through machine
  arbitration and snapshots while the Fat Agnus `HARDDIS` path retains
  its available post-`$DF` grants
- Preserve each line's observed ordinary DDFSTOP and terminal fetch
  endpoint through OCS and mixed Fat Agnus arbitration
- Preserve each line's DDFSTRT comparator match through OCS and mixed
  Fat Agnus arbitration and rendering
- Pass the installed Agnus revision's vertical display-window state to
  Denise output, including OCS wrapping and Fat Agnus extended windows
- Route base display-window writes through the installed Agnus
  revision so Fat Agnus updates its extended vertical-DIW latch
- Select Fat Agnus 8372A explicitly for matching OCS-shaped profiles,
  expose its identity, enhanced sprite comparators, extended blitter
  registers, DIWHIGH DMA gating and programmable timing/blanking, and
  enforce each Agnus revision's chip-RAM ceiling while retaining OCS Denise

## [0.2.0](https://github.com/emu198x/emu198x/releases/tag/machine-commodore-amiga-ocs-v0.2.0) - 2026-06-04

### Other

- *(release)* independent per-machine versioning, baseline 0.2.0
- cargo fmt + clippy clean across the workspace
- A1200 Stage AE-j: correct chipset identification across OCS / ECS / AGA
- A1200 Stage AE-h + AE-i: investigation tooling — chipset write log + CPU instruction trace
- A1200 Stage AE-e: mirror BPLCON0 / palette / chipset-read tracers onto OCS + ECS
- cargo fmt --all across the workspace
- Open Emu198x for public release
- Amiga Seam 1.7: move copper.rs to common-commodore-amiga
- Amiga Seam 1.6: Denise wrapper goes generic over DeniseChip
- Amiga Seam 1.4: move memory.rs into common-commodore-amiga
- Amiga Seam 1.3: move cia.rs into common-commodore-amiga
- Amiga Seam 1.2: move rtc.rs into common-commodore-amiga
- Fix more clippy lints from Rust 1.95.0
- Apply horizontal DIW gate to Denise output — fixes KS 2.04 wraparound
- Amiga NTSC: chip-layer line alternation + 5 NTSC OCS variants
- Move disk read DMA state machine into Paula
- Refactor Amiga service_cpu_bus into BusTransaction/BusResponse
- Add Amiga postcard snapshots across the chip stack
- add amiga joystick controls
- add native channel controls
- wire native mouse input
- Apply mechanical Rust formatting cleanup
- resolve remaining diagnostic cleanup
- commit mechanical cleanup across diagnostics
- fix workspace clippy and test hygiene
- fix ks12 slow-ram probe and add a1000 goldens
- add real a1000 bootstrap WOM support
- land wb13 boot investigation and fixes
- fix ADKCON bit constants — WORDSYNC was silently the wrong bit
- WB 1.3 diag: pinpoint the silent failure — chained QBlits never run
- Revert DMA cursor-rewind and gap-fill — KS trackdisk needs the wrap
- MFM encoder: rectify boundary clock bits + gap-fill post-track DMA
- Phase A: narrow WB 1.3 boot hang to MFM format compat with trackdisk
- Complete disk DMA transfers so trackdisk's DSKBLK fires
- paint COLOR00 through the full viewport for the border
- framebuffer origin follows the Standard viewport, not DDF
- fetch the trailing DDF block (+1 per line)
- Route copper MOVEs through the machine-wide custom-register dispatch
- Fix DMACONR byte-read: upper byte on even addresses
- Pin OCS insert-disk regression to KS 1.3 clearing BPLEN + SPREN
- Configurable RAM + fast-RAM autoconfig: runtime presets (step 3 of 3)
- Zorro-II autoconfig fast RAM (step 2 of 3)
- Configurable chip + slow RAM sizes (step 1 of 3)
- Cross-cutting boot integration tests (task #180)
- Retire commodore-gary-archive: the archive is now the live crate
- Gary Phase 2: wire address decoder into the machine
- Retire peripheral-commodore-amiga-keyboard-archive: archive is now live
- Keyboard Phase 2: wire controller into the machine
- Retire peripheral-commodore-amiga-floppy-archive: the archive is now the live crate
- Floppy Phase 2: wire DF0 drive into the machine
- Retire commodore-denise-ocs-archive: the archive is now the live crate
- Denise OCS Phase 2c: wire LACE + sprite DMA
- Denise OCS Phase 2b: delegate pixel pipeline to commodore-denise-ocs
- Denise OCS Phase 2a: absorb BPLCON1/2 + colour palette
- Port Blitter into the machine (tasks #134–#147)
- Retire commodore-agnus-ocs-archive: the archive is now the live crate
- Port Agnus OCS into the machine (tasks #139, #140, #141, #148)
- Retire commodore-paula-8364-archive: the archive is now the live crate
- Implement Paula POTGO + POTxDAT + POTGOR analog-input registers (task #129)
- Implement Paula serial UART — SERDAT/SERPER/SERDATR + TBE/RBF IRQs (tasks #128, #121)
- Wire Paula disk-completion + MFM-sync IRQ paths (task #127)
- Port Paula disk register storage into the machine (task #126)
- Port Paula audio DMA engine + AUDx IRQs (task #125)
- Port Paula audio register storage into the machine (task #124)
- Port Paula INTENA/INTREQ/ADKCON into the machine (task #123)
- Tighten CIA-8520 API: hide fields, fold duplication, name the bits
- Retire mos-cia-8520-archive: the archive is now the live crate
- Port CIA-8520 archive into machine; add Phase 1 + Phase 2 tests
- Resolve task #96: copper CDANG halt fixes chip-only KS 1.3 boot
- Task #96: prove the copper is chip-only's corruption source
- Task #96: identify chip-only deadlock at DoIO(TD_CHANGESTATE)
- Task #96: identify romboot as the diverging routine
- Nail down why chip-only's LOFlist stays at ExecBase
- Wire CIA-A disk pins to empty-drive state; boot reaches WAITBLIT
- Add 8520 one-shot auto-start on TxHI write
- Trap timer.device VBL + CIA handler entry points
- Prove ROM contains no "start Timer B" code outside timer.device init
- Lock in CIA-A Timer B behaviour with two regression tests
- Track CIA-A register writes; confirm TB never started for MICROHZ
- Pin trackdisk's exact wait: timer.device Port 2 (500ms MICROHZ)
- Pin the actual parked PC: trackdisk waits on timer.device delay
- Pin strap's DoIO-blocked-in-WaitIO state via BeginIO traps
- Add Intuition LVO trap + early screen-setup trap
- Add message-port LVO trap (PutMsg / GetMsg / DoIO / ...)
- Add Paula disk register storage + write log
- Add exec.library Wait/Signal/Cause trap
- Add graphics.library LVO trap diagnostic
- Add GfxBase / View diagnostic for insert-disk deadlock
- Add Amiga copper COPJMP1/2 strobe dispatch
- Lock in M12-step-1 progress with integration tests
- run diag_boot_state against both configs side-by-side
- dump TaskReady + TaskWait lists with names
- Add boot-state snapshot diagnostic (frame 300)
- M12 step 1: CIA-A TOD counter wired to VBL
- Level-sensitive /VERTB and /IRQ with Paula edge latches
- Floating bus: track last-driven value on the chip bus
- Copper WAIT: honour the 3rd memory cycle (6 CCKs, not 4)
- Migrate primary tick to master/4 (68000 CPU clock = lores pixel)
- CPU chip-RAM bus arbitration + palette-index guard
- Copper yields to DMA: odd-CCK slot discipline
- Denise reads DDFSTRT / DDFSTOP / DIWSTRT / DIWSTOP from registers
- Copper WAIT/SKIP: implement IR2 mask field per HRM
- Fix autovectored interrupts + tighten PAL timing + stale comment
- Trace INTENA write sequence, reframe chip-only bug
- Add differential MemList trace: chip-only vs slow-RAM
- Make Denise per-CCK cycle-accurate (M11.1 fix)
- Amiga restart M11.2: slow RAM at $C00000 (trapdoor expansion)
- Amiga restart M11.1: bitplane DMA fetch + decode
- Amiga restart M11: Denise pixel pipeline (background only)
- Amiga restart M10: Copper coprocessor
- track INTENA peak + write count
- Amiga restart M9: CIA-B stub
- register read counter + extended long-run
- M8 status + diagnostic shows next blocker
- Amiga restart M8: CIA-A timers + ICR + CIA→Paula IRQ
- full ROM inventory now available
- Amiga restart M7: chipset read fidelity (VPOS/VHPOS + CIA-A inputs)
- Amiga restart M6: beam counter + VBL interrupt
- Diagnostic test: long-run boot state checkpoints
- Amiga restart M5: bootstrap ExecBase placement (regression check)
- Amiga restart M4: chip-RAM aliasing for the size probe
- Amiga restart M3: OVL clear via CIA-A
- Amiga restart M2: custom-register storage
- Amiga restart M1: chip RAM + CPU bus integration
- Amiga restart: archive old chipsets, ship M0 (CPU + ROM + OVL)

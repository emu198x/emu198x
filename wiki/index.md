# Wiki Index

## Chips
- [Zilog Z80](chips/zilog-z80.md) — half-cycle signal-level state machine, MStep walker, 100% Tom Harte
- [Ferranti 6C001E](chips/ferranti-6c001e.md) — Spectrum 48K ULA, 14 MHz crystal, memory+I/O+internal contention
- [Sinclair 7K010E](chips/sinclair-7k010e.md) — Spectrum 128K/+2 ULA, 17.7 MHz crystal, phase 1 contention
- [Amstrad 40077](chips/amstrad-40077.md) — Spectrum +2A/+3 gate array, MREQ-only contention, no floating bus
- [GI AY-3-8912](chips/gi-ay-3-8912.md) — PSG, 3 tone + noise + envelope, /8 prescaler, Bresenham downsampling
- [NEC µPD765A](chips/nec-upd765a.md) — floppy disk controller, DSK/EDSK via `format-amstrad-dsk`, used in Spectrum +3
- [MOS 6502](chips/mos-6502.md) — C64 / NES / BBC / Apple II / Atari / etc CPU, pipelined pin bus, stock + 2A03 variants, 2 × 2.47M Tom Harte validated
- [MOS 6526 CIA](chips/mos-cia-6526.md) — C64 complex interface adapter, two timers + TOD + I/O ports, ported with 23 tests
- [MOS 6581 / 8580 SID](chips/mos-sid-6581.md) — C64 sound chip, three voices + ADSR + state-variable filter, ported with 9 tests
- [MOS 6569 / 6567 VIC-II](chips/mos-vic-ii.md) — C64 video chip, text/bitmap/sprites, badline + sprite DMA BA assertion, raster IRQ, ported with 23 tests
- [Ricoh 2C02 PPU](chips/ricoh-ppu-2c02.md) — NES PPU, dot-level rendering, loopy scroll, sprite overflow bug, NMI timing, A12 mapper notification, ported with 20 tests
- [Ricoh 2A03 APU](chips/ricoh-apu-2a03.md) — NES APU, two pulse + triangle + noise + DMC, non-linear mixer, 48 kHz downsample, lifted from archive with 21 tests

## Systems
### Commodore 64
- [Overview](systems/commodore-c64.md) — live 6502/CIA/VIC-II/SID board loop, **KERNAL boots to `READY.` prompt end-to-end**, runtime + headless CLI now ship PNG screenshots, mono audio, and runtime snapshots via the shared shell

### Nintendo NES
- [Overview](systems/nintendo-nes.md) — 2A03 CPU + 2C02 PPU + APU + Mapper wired, master-clock tick loop, **nestest 8991/8991 passing**, **Super Mario Bros. renders**, System trait integrated
- [Clock topology](decisions/nes-clock-topology.md) — master-clock-driven tick loop, 1 CPU : 3 PPU dot ratio, pin contracts for PPU/CPU/Mapper
- `format-nintendo-nes-ines` — iNES / NES 2.0 header parser + Mapper trait + NROM (mapper 0). Other 47 mappers deferred.

### Commodore Amiga
- [Overview](systems/commodore-amiga.md) — fresh A500 OCS PAL headless baseline, Kickstart-visible output, DF0 `ADF` insertion, screenshots, stereo audio, and scripted keyboard input
- [Port plan](decisions/amiga-port-plan.md) — the archive-to-fresh-workspace port plan this baseline came from

### ZX Spectrum
- [Overview](systems/spectrum/overview.md) — 11 variants (16K, 48K, 128K, +2, +2A, +2B, +3, Pentagon, Scorpion, TC2048, TS2068), ULA-drives architecture
- [Contention](systems/spectrum/contention.md) — three ULA implementations, I/O contention, internal contention
- [Variants](systems/spectrum/variants.md) — memory maps, paging, I/O ports, ROMs, floating bus
- [Signal Part 3](systems/spectrum/signal-part-3.md) — acid test demo, AY discovery, IM 2 vector chain

## Concepts
- [Clock trees](concepts/clock-trees.md) — master oscillator principle, verified Spectrum clock values
- [Bus protocols](concepts/bus-protocols.md) — signal-level CPU interface, per-T-state bus methods
- [Audio mixing](concepts/audio-mixing.md) — beeper + AY chain, EAR/MIC feedback, Bresenham downsampling
- [Tape formats](concepts/tape-formats.md) — TAP/TZX → pulse sequences, motor timing
- [Test methodology](concepts/test-methodology.md) — Tom Harte, ZEXDOC/ALL, FUSE, system-level tests

## Decisions
- [ULA-drives model](decisions/ula-drives-model.md) — timing chip owns the clock, CPU is subordinate
- [No Bus trait](decisions/no-bus-trait.md) — CPU exposes signals, machine handles transactions
- [Half-cycle signals](decisions/half-cycle-signals.md) — half-cycle granularity for accurate signal timing
- [Fresh start rationale](decisions/fresh-start-rationale.md) — why full rewrite, what carried forward
- [Crate naming](decisions/crate-naming.md) — manufacturer-chipname convention
- [Product roadmap](decisions/product-roadmap.md) — 35+ systems long-term, four by October 2026, wave plan, chip reuse map
- [Native UI strategy](decisions/native-ui-strategy.md) — platform-specific frontends (SwiftUI/GTK4/WinUI), SDL2+native menus for October
- [Save state format](decisions/save-state-format.md) — serde + postcard, derive on everything from day one
- [System-specific run loops](decisions/system-specific-run-loops.md) — no universal tick pattern, each system matches its hardware
- [SpectrumDriver](decisions/spectrum-driver.md) — one shared run loop across the Spectrum family via a provided-method trait
- [Peripheral trait](decisions/peripheral-trait.md) — static dispatch for edge-connector devices, typed fields per machine
- [Hotkey modifier policy](decisions/hotkey-modifier-policy.md) — Alt only, never Ctrl/Shift (they're SYMBOL SHIFT / CAPS SHIFT)
- [Archives as source](decisions/archives-as-source.md) — port from `~/Projects/Emu198x-archive*` first, lifecycle is port → evaluate → cleanup
- [CPU bus interface](decisions/cpu-bus-interface.md) — pin-level for *every* CPU, no Bus trait ever, supersedes the old Z80-specific framing
- [NES clock topology](decisions/nes-clock-topology.md) — master clock drives the loop, PPU every dot, CPU every 3rd dot, mapper observes CPU pins
- [Amiga port plan](decisions/amiga-port-plan.md) — 9-phase plan, OCS (A500) first, 68000 pin conversion is the long pole, ~35K lines in archive
- [Amiga architecture review](decisions/amiga-architecture-review.md) — five seams to tighten before the Amiga scales (disk DMA, CPU bus, byte-lane, merge latch, boot invariants); spine unchanged

## Tests
- [Spectrum](tests/spectrum.md) — Z80 100% Tom Harte, ZEXDOC/ALL pass, 11 variants boot, Signal Part 3 working

## References
- [Emulators](references/emulators.md) — SpecIde, FUSE, zxsp, WinUAE, VICE, and others

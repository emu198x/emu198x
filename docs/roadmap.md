# Roadmap

Status, gaps, and next steps for the four primary systems (Spectrum, C64, NES,
Amiga). For systems beyond the core four, see [future-systems.md](future-systems.md).

---

## 1. Architectural Principles

### Core traits

`emu-core` provides the shared abstractions:

- **`Bus`** — 8-bit read/write with `u32` address, tick, fetch, contention hooks
- **`WordBus`** — extends `Bus` with 16-bit read/write (68000, SH-2)
- **`IoBus`** — extends `Bus` with Z80-style I/O port read/write
- **`Cpu<B: Bus>`** — step, reset, interrupt, nmi, pc (returns `u32`)
- **`Tickable`** — advance by one or more master clock ticks
- **`Machine`** — run_frame, render, generate_audio, key_down/up, joystick, load_file

### Crate naming

- **One crate per IC**: each chip is independently testable, never depends on
  other chip crates
- **Manufacturer prefix**: `mos-6502`, `zilog-z80`, `motorola-68000`,
  `gi-ay-3-8910`
- **One crate per chip variant**: distinct revisions get separate crates. Config
  within a single crate is for pin/jumper differences on the same die (e.g.
  NTSC/PAL crystal, LFSR polynomial)
- **One crate per media format**: parsing ADF, D64, TAP etc. is pure logic with
  no hardware dependencies
- **One crate per peripheral**: floppy drives, keyboards, controllers are
  separate from their controlling chips
- **Machine crates are thin orchestrators**: wire components, own the master
  clock, derive chip clocks. Minimal logic.

### Clock domain model

Each machine crate counts master oscillator ticks and fires component ticks at
integer-derived ratios. This is a pattern, not a trait:

```
Amiga PAL:    28.37516 MHz / 4 = 7.09379 MHz (CCK) / 2 = 3.54690 MHz (CPU)
Spectrum 48K: 14.000 MHz / 4 = 3.500 MHz (CPU), / 2 = 7.000 MHz (pixel)
C64 PAL:      17.734475 MHz × 4/9 = 7.882 MHz (dot) / 8 = 0.985 MHz (CPU)
NES NTSC:     21.477272 MHz / 12 = 1.790 MHz (CPU), / 4 = 5.369 MHz (PPU)
```

---

## 2. Crate Inventory

### CPUs

| Crate | CPU | Status |
|-------|-----|--------|
| `mos-6502` | 6502 | Complete (2,560,000 single-step tests) |
| `zilog-z80` | Z80A/B | Complete (1,604,000 single-step tests) |
| `motorola-68000` | 68000–68040 family | Complete (317,500 DL + 5,335,000 Musashi tests across 8 variants) |
| `motorola-68010` | 68010 | Thin wrapper (572,500 Musashi tests) |
| `motorola-68020` | 68020 | Thin wrapper (600,000 Musashi tests) |

### Amiga custom chips

| Crate | Chip | Status |
|-------|------|--------|
| `commodore-agnus-ocs` | Agnus 8361/8367/8370/8371 | Complete |
| `commodore-agnus-ecs` | Super Agnus 8372A | Complete (wraps OCS) |
| `commodore-agnus-aga` | Alice (AGA Agnus) | Complete (wraps ECS; FMODE fetch, 8-plane lowres) |
| `commodore-denise-ocs` | Denise 8362 | Complete |
| `commodore-denise-ecs` | Super Denise 8373 | Complete (wraps OCS) |
| `commodore-denise-aga` | Lisa (AGA Denise) | Complete (wraps ECS; 24-bit palette, HAM8, FMODE sprites) |
| `commodore-paula-8364` | Paula 8364 | Complete |
| `mos-cia-8520` | CIA 8520 | Complete |

### Amiga support chips

| Crate | Chip | Status |
|-------|------|--------|
| `commodore-gayle` | Gayle (IDE + PCMCIA) | Complete |
| `commodore-fat-gary` | Fat Gary | Stub in `machine-amiga` |
| `commodore-ramsey` | Ramsey (DRAM controller) | Stub in `machine-amiga` |
| `commodore-gary` | Gary (address decode) | Not started (logic inline) |
| `commodore-buster` | Buster (Zorro II) | Not started |
| `commodore-super-buster` | Super Buster (Zorro III) | Not started |
| `commodore-dmac` | DMAC 390537 (A3000 SCSI) | Not started |
| `commodore-akiko` | Akiko (CD32) | Not started |

### Amiga peripherals

| Crate | Device | Status |
|-------|--------|--------|
| `drive-amiga-floppy` | 3.5" DD 880KB floppy | Complete |
| `peripheral-amiga-keyboard` | Keyboard controller | Complete |
| `drive-amiga-hd` | IDE/SCSI hard drive | Not started |

### Shared chips

| Crate | Chip | Used By | Status |
|-------|------|---------|--------|
| `gi-ay-3-8910` | AY-3-8910/8912 | Spectrum 128+ | Complete |
| `mos-sid-6581` | SID 6581/8580 | C64 | Complete (both models) |
| `mos-via-6522` | VIA 6522 | 1541 drive | Complete |
| `nec-upd765` | NEC 765 FDC | Spectrum +3 | Complete |
| `sinclair-ula` | Spectrum ULA | Spectrum | Complete |

### Format crates

| Crate | Format | Status |
|-------|--------|--------|
| `format-adf` | Amiga Disk File | Complete |
| `format-ipf` | Interchangeable Preservation Format | Complete |
| `format-d64` | Commodore D64 disk image | Complete |
| `format-gcr` | Commodore 1541 GCR encoding | Complete |
| `format-c64-tap` | C64 TAP tape image | Complete |
| `format-spectrum-tap` | Spectrum TAP tape image | Complete |
| `format-tzx` | TZX tape image | Complete |
| `format-prg` | C64 PRG file loader | Complete |
| `format-sna` | Spectrum SNA snapshot | Complete |
| `format-z80` | Spectrum .Z80 snapshot | Complete |
| `nes-cartridge` | iNES cartridge + 14 mappers | Complete |

### System crates

| Crate | System | Status |
|-------|--------|--------|
| `emu-spectrum` | ZX Spectrum (48K, 128K, +2, +2A, +3) | Complete |
| `emu-c64` | Commodore 64 (PAL, NTSC) | Feature-complete |
| `emu-nes` | NES/Famicom (NTSC, PAL) | Complete |
| `machine-amiga` | Amiga (A500–A4000) | OCS/ECS working, AGA crates extracted (boot in progress) |

---

## 3. System Status

### Cross-system feature matrix

| Category | Spectrum | C64 | NES | Amiga |
|----------|----------|-----|-----|-------|
| CPU | 100% | 100% | 100% | ~99% (68000 + 68020 extensions) |
| Video | 100% | 100% | ~98% | ~97% (OCS/ECS/AGA bitplanes, HAM, sprites) |
| Audio | 100% (beeper + AY) | ~97% (6581/8580 filter) | ~95% (all 5 channels) | ~93% (hardware LPF + DAC non-linearity) |
| Storage | TAP + TZX + SNA + Z80 + DSK | PRG + CRT (7) + TAP + D64 (r/w) | 14 mappers + battery | ADF r/w + IPF read |
| Peripherals | Keyboard + Kempston | Keyboard + joystick + REU | 4-player + Zapper | Keyboard + mouse |
| Model variants | 48K, 128K, +2, +2A, +3 | PAL + NTSC | NTSC + PAL | A500, A500+, A1200 |

### ZX Spectrum

No blocking gaps. 48K, 128K, +2, +2A, and +3 PAL are production-grade. TZX
handles turbo loaders via real-time EAR signal simulation. The +3 FDC (NEC
uPD765) supports DSK/EDSK with read and write.

**Content capture TODO:** One timing-sensitive visual demo, one hero screenshot,
one audio capture, one lesson draft.

### Commodore 64

No blocking gaps. All six VIC-II display modes, sprite DMA cycle stealing, fine
scrolling. SID 6581/8580 with 32-point piecewise-linear filter lookup. Seven CRT
cartridge types. 1541 drive with read/write and half-track positioning. REU
(128/256/512 KB). TAP turbo loaders via CIA1 FLAG.

**Accuracy gap:** SID per-chip filter calibration needs measured data from
specific revisions; current table captures the 6581 kink shape from reSID die
analysis.

**Content capture TODO:** One badline visual demo, one SID audio example, one
hero visual, one lesson draft.

### NES

No blocking gaps for NTSC/PAL. 14 mappers cover ~89% of the licensed library.
DMC DMA cycle stealing with correct OAM DMA interaction. Battery-backed PRG RAM
for RPGs.

**Blocking broader compatibility:** FDS (Famicom Disk System) not implemented.

**Content capture TODO:** One pipeline-focused visual demo, one sprite/timing
capture, one lesson draft.

### Amiga

Boots all OCS/ECS Kickstart versions to insert-disk screen. Workbench 1.3
reaches full desktop. AGA display pipeline functional (8 bitplanes, 24-bit
palette, HAM8, FMODE). IPF copy-protected disks supported.

**Blocking broader compatibility:** WHDLoad needs IDE/filesystem/autoconfig
infrastructure.

#### Kickstart boot status

| ROM | Model | Status |
|-----|-------|--------|
| KS 1.0 | A1000 | Yellow screen (no slow RAM) |
| KS 1.2 | A500, A2000 | Insert-disk ✅ |
| KS 1.3 | A500, A2000 | Insert-disk ✅ |
| KS 2.04 | A500+ | Insert-disk ✅ |
| KS 2.05 | A600 | Insert-disk ✅ |
| KS 3.1 | A500, A600, A2000 | Insert-disk ✅ |
| KS 2.02/3.1 | A3000 | STRAP reached, stalls on device init |
| KS 3.0/3.1 | A1200, A4000 | AGA display incomplete |
| WB 1.3 | A500 | Full desktop ✅ |

**Content capture TODO:** One Copper/Blitter visual demo, one audio DMA example,
one hero capture, one lesson draft.

---

## 4. What's Next

### Chip extractions

Chips still embedded in system crates need extracting into standalone crates.
Extraction is the primary accuracy diagnostic — until a chip has its own crate
and pin-level test harness, you can't tell which behaviours are chip-level and
which are system-level assumptions baked in by the host. The Amiga chip
extractions (Agnus, Denise, Paula) proved this: separating them exposed timing
assumptions that were invisible when everything lived in one struct.

**Chip crates (accuracy-critical):**

| Chip | Current location | Target crate | Why first |
|------|-----------------|--------------|-----------|
| ~~AGA Agnus (Alice)~~ | ~~`machine-amiga`~~ | ~~`commodore-agnus-aga`~~ | Done |
| ~~AGA Denise (Lisa)~~ | ~~`machine-amiga`~~ | ~~`commodore-denise-aga`~~ | Done |
| ~~VIC-II (6569/6567)~~ | ~~`emu-c64`~~ | ~~`mos-vic-ii`~~ | Done (PAL+NTSC in one crate via `VicModel`) |
| ~~CIA 6526~~ | ~~`emu-c64`~~ | ~~`mos-cia-6526`~~ | Done (`external_b` pattern for keyboard) |
| ~~PPU 2C02~~ | ~~`emu-nes`~~ | ~~`ricoh-ppu-2c02`~~ | Done (closure-based CHR access, `Mirroring` enum lives here) |
| ~~APU 2A03~~ | ~~`emu-nes`~~ | ~~`ricoh-apu-2a03`~~ | Done (`ApuRegion` enum, raw getters, 20 tests) |

**Format crates (all extracted):**

| Format | Target crate | Status |
|--------|-------------|--------|
| ~~D64~~ | `format-d64` | Done |
| ~~GCR~~ | `format-gcr` | Done |
| ~~C64 TAP~~ | `format-c64-tap` | Done |
| ~~Spectrum TAP~~ | `format-spectrum-tap` | Done |
| ~~TZX~~ | `format-tzx` | Done |
| ~~PRG~~ | `format-prg` | Done |
| ~~SNA~~ | `format-sna` | Done |
| ~~Z80~~ | `format-z80` | Done |
| ~~iNES~~ | `nes-cartridge` | Done |

### Amiga completion

AGA chip extraction is done. Remaining work is boot pipeline and peripheral stubs.

| Item | Notes |
|------|-------|
| A1200/A4000 boot | AGA crates extracted; debug the AGA boot pipeline to reach insert-disk |
| A3000 device init | SCSI controller ($DD0000) / Super Buster stubs needed for insert-disk |
| WHDLoad | IDE, filesystem, autoconfig — enables hard-drive game installs |

### Amiga model configs

| Model | Status |
|-------|--------|
| A500 | ✅ |
| A2000 | ✅ |
| A500+ | ✅ |
| A600 | ✅ |
| A1000 | Partial (yellow screen — no slow RAM) |
| A1200 | Partial (AGA display incomplete — crates extracted, boot pipeline needs work) |
| A3000 | Partial (STRAP reached, stalls on device init) |
| A4000 / CDTV / CD32 | Not started |

### Tooling & frontend

The emulators serve an educational mission. The features below turn them from
headless engines into teaching tools. Design specs live in `docs/features/`.

| Item | Status | Impact |
|------|--------|--------|
| Save states | Not started | Lesson checkpoints, instant replay, MCP `save_state`/`load_state` |
| WASM per-system builds | Not started | Web-embedded lessons — each system is a separate JS/WASM package |
| Breakpoint conditions | Not started | Expression-based breakpoints (`a == 0 && pc > $C000`) beyond address-only |
| Observable trait | Not started | Structured `snapshot()` / `query(path)` API across all chips, replacing ad-hoc MCP queries |
| Trace recording | Not started | Step-by-step execution history — instruction, memory, register, interrupt events with tick stamps |
| Visual debugger | Not started | Registers, disassembly, memory hex view, video/audio state — the education core |
| Launcher UI | Not started | Variant/option selection screen before boot (per-system) |
| MCP event notifications | Not started | `breakpoint_hit`, `frame_complete` push events for AI agent workflows |
| Input configuration UI | Not started | Keyboard mapping (positional/symbolic), joystick/gamepad binding |
| Media panel widgets | Not started | Tape deck, disk drive, cartridge slot — visual controls with drag-and-drop |

### New systems

See [future-systems.md](future-systems.md) for the full catalogue. Not in scope
until the four primary systems are complete.

### Prioritised work items

**Amiga completion:**

1. **A1200/A4000 boot** — debug AGA boot pipeline to reach insert-disk (crates extracted)
2. **A3000 SCSI stubs** — unblock insert-disk screen
3. **WHDLoad support** — IDE/filesystem/autoconfig infrastructure

**Chip extractions (accuracy + reuse):**

4. ~~**VIC-II extraction**~~ — Done (`mos-vic-ii`, closure-based VRAM access, 28 tests)
5. ~~**CIA 6526 extraction**~~ — Done (`mos-cia-6526`, `external_b` pattern, 19 tests)
6. ~~**Format crate extractions**~~ — Done (9 crates: format-d64, format-gcr, format-c64-tap, format-spectrum-tap, format-tzx, format-prg, format-sna, format-z80, nes-cartridge)
7. ~~**NES PPU extraction**~~ — Done (`ricoh-ppu-2c02`, closure-based CHR access, `Mirroring` owns here, 9 tests)
8. ~~**NES APU extraction**~~ — Done (`ricoh-apu-2a03`, `ApuRegion` enum, raw getters, 20 tests)

**Tooling:**

7. **Save states** — required for lesson checkpoints and MCP workflow
8. **WASM builds** — required for web-embedded lessons
9. **Observable trait + trace recording** — structured state API for education
10. **Visual debugger** — registers, disassembly, memory — makes lessons possible
11. **Breakpoint conditions** — needed for non-trivial debugging sessions

---

## 5. Testing Strategy

### Per-chip unit tests

Every chip crate has `#[cfg(test)]` modules testing behaviour in isolation with
mock buses/inputs: instruction correctness, cycle counting, interrupt timing,
waveform output, pixel output, DMA slot allocation.

### Per-format tests

Load/save round-trip for every format. Reject corrupt/truncated files. Validate
checksums where applicable (MFM CRC, iNES header).

### Integration tests

Crystal-to-frame timing verification. DMA contention cycle counts. Cross-chip
timing (copper writes at exact beam position).

### Test ROM/program suites

- **NES**: nestest, blargg APU/PPU tests
- **Amiga**: SysTest, DiagROM
- **Spectrum**: FUSE test suite
- **C64**: Lorenz test suite, VICE test programs

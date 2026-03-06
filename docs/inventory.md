# Inventory

Architecture notes and crate inventory for the current codebase. This is
reference material, not the active-work list.

For active priorities, see [roadmap.md](roadmap.md). For current support state,
see [status.md](status.md).

---

## Architecture Notes

### Core Traits

`emu-core` provides the shared abstractions:

- **`Bus`** - 8-bit read/write with `u32` address, tick, fetch, and contention hooks
- **`WordBus`** - extends `Bus` with 16-bit read/write support
- **`IoBus`** - extends `Bus` with Z80-style I/O port read/write
- **`Cpu<B: Bus>`** - `step`, `reset`, `interrupt`, `nmi`, and `pc` (returns `u32`)
- **`Tickable`** - advance by one or more master clock ticks
- **`Machine`** - `run_frame`, `render`, `generate_audio`, `key_down/up`, `joystick`, `load_file`

### Crate Conventions

- Default to one crate per IC, media format, or reusable peripheral
- Use manufacturer-prefixed crate names where that improves grep-ability
- Split revisions into separate crates when behavior diverges enough to justify separate tests; keep crystal, jumper, or small configuration differences inside one crate when the die is otherwise the same
- Keep format crates pure parsing and encoding logic with no hardware dependencies
- Keep peripheral crates separate from their controlling chips when they are independently reusable
- Keep machine crates thin: wire components, own the master clock, and avoid logic that belongs in chip or peripheral crates

### Naming Direction

- `machine-<vendor>-<system>` is the intended name for pure emulation-core crates
- When there is no meaningful single-vendor prefix, use the full platform name directly, for example `machine-msx-1`
- `emu-<system>` is the intended name for runnable host packages that can expose headed and headless modes from the same crate
- `runner-*` is transitional and should collapse into `emu-*`
- Chip crates stay manufacturer-first rather than adding `chip-` prefixes; many chips are mixed-function and do not sort cleanly into `video`, `audio`, or `io`
- Format crates should prefer `format-<platform>-<format>` when an extension is platform-specific or overloaded across ecosystems, for example `format-amstrad-cpc-dsk` or `format-commodore-64-tap`
- Inventory tables below keep current package names where they already exist in the repository, even if they predate this normalized naming direction

### Clock Domain Model

Each machine crate counts master oscillator ticks and fires component ticks at
integer-derived ratios. This is a pattern, not a trait:

```text
Amiga PAL:    28.37516 MHz / 4 = 7.09379 MHz (CCK) / 2 = 3.54690 MHz (CPU)
Spectrum 48K: 14.000 MHz / 4 = 3.500 MHz (CPU), / 2 = 7.000 MHz (pixel)
C64 PAL:      17.734475 MHz x 4/9 = 7.882 MHz (dot) / 8 = 0.985 MHz (CPU)
NES NTSC:     21.477272 MHz / 12 = 1.790 MHz (CPU), / 4 = 5.369 MHz (PPU)
```

## Crate Inventory

### CPUs

| Crate            | CPU                | Status                                                            |
| ---------------- | ------------------ | ----------------------------------------------------------------- |
| `mos-6502`       | 6502               | Complete (2,560,000 single-step tests)                            |
| `zilog-z80`      | Z80A/B             | Complete (1,604,000 single-step tests)                            |
| `motorola-68000` | 68000-68040 family | Complete (317,500 DL + 5,335,000 Musashi tests across 8 variants) |
| `motorola-68010` | 68010              | Thin wrapper (572,500 Musashi tests)                              |
| `motorola-68020` | 68020              | Thin wrapper (600,000 Musashi tests)                              |

### Amiga Custom Chips

| Crate                  | Chip                      | Status                                                    |
| ---------------------- | ------------------------- | --------------------------------------------------------- |
| `commodore-agnus-ocs`  | Agnus 8361/8367/8370/8371 | Complete                                                  |
| `commodore-agnus-ecs`  | Super Agnus 8372A         | Complete (wraps OCS)                                      |
| `commodore-agnus-aga`  | Alice                     | Complete (wraps ECS; FMODE fetch, 8-plane lowres)         |
| `commodore-denise-ocs` | Denise 8362               | Complete                                                  |
| `commodore-denise-ecs` | Super Denise 8373         | Complete (wraps OCS)                                      |
| `commodore-denise-aga` | Lisa                      | Complete (wraps ECS; 24-bit palette, HAM8, FMODE sprites) |
| `commodore-paula-8364` | Paula 8364                | Complete                                                  |
| `mos-cia-8520`         | CIA 8520                  | Complete                                                  |

### Amiga Support Chips

| Crate                    | Chip                     | Status                                                |
| ------------------------ | ------------------------ | ----------------------------------------------------- |
| `commodore-gayle`        | Gayle (IDE + PCMCIA)     | Complete                                              |
| `commodore-fat-gary`     | Fat Gary                 | Stub in current Amiga machine crate (`machine-amiga`) |
| `commodore-ramsey`       | Ramsey (DRAM controller) | Stub in current Amiga machine crate (`machine-amiga`) |
| `commodore-gary`         | Gary (address decode)    | Not started (logic inline)                            |
| `commodore-buster`       | Buster (Zorro II)        | Not started                                           |
| `commodore-super-buster` | Super Buster (Zorro III) | Not started                                           |
| `commodore-dmac-390537`  | DMAC 390537 (A3000 SCSI) | Stub complete (10 tests)                              |
| `commodore-akiko`        | Akiko (CD32)             | Not started                                           |

### Amiga Peripherals

| Crate                       | Device               | Status      |
| --------------------------- | -------------------- | ----------- |
| `drive-amiga-floppy`        | 3.5" DD 880KB floppy | Complete    |
| `peripheral-amiga-keyboard` | Keyboard controller  | Complete    |
| `drive-amiga-hd`            | IDE/SCSI hard drive  | Not started |

### Shared Chips

| Crate            | Chip             | Used By       | Status                 |
| ---------------- | ---------------- | ------------- | ---------------------- |
| `gi-ay-3-8910`   | AY-3-8910/8912   | Spectrum 128+ | Complete               |
| `mos-sid-6581`   | SID 6581/8580    | C64           | Complete (both models) |
| `mos-vic-ii`     | VIC-II 6567/6569 | C64           | Complete               |
| `mos-cia-6526`   | CIA 6526         | C64           | Complete               |
| `mos-via-6522`   | VIA 6522         | 1541 drive    | Complete               |
| `nec-upd765`     | uPD765 FDC       | Spectrum +3   | Complete               |
| `ricoh-ppu-2c02` | PPU 2C02         | NES/Famicom   | Complete               |
| `ricoh-apu-2a03` | APU 2A03         | NES/Famicom   | Complete               |
| `sinclair-ula`   | Spectrum ULA     | Spectrum      | Complete               |

### Format Crates

Current crate names in this table reflect the existing repository. Several of
them predate the normalized `format-<platform>-<format>` naming policy above.

| Crate                 | Format                              | Status   |
| --------------------- | ----------------------------------- | -------- |
| `format-adf`          | Amiga Disk File                     | Complete |
| `format-ipf`          | Interchangeable Preservation Format | Complete |
| `format-d64`          | Commodore D64 disk image            | Complete |
| `format-gcr`          | Commodore 1541 GCR encoding         | Complete |
| `format-c64-tap`      | C64 TAP tape image                  | Complete |
| `format-spectrum-tap` | Spectrum TAP tape image             | Complete |
| `format-tzx`          | TZX tape image                      | Complete |
| `format-prg`          | C64 PRG file loader                 | Complete |
| `format-sna`          | Spectrum SNA snapshot               | Complete |
| `format-z80`          | Spectrum Z80 snapshot               | Complete |
| `nes-cartridge`       | iNES cartridge + 14 mappers         | Complete |

### Core Machine Crates

Target names in this table reflect the intended architecture split, even where
the current repository still uses older package names.

| Crate                           | System                               | Status                                     |
| ------------------------------- | ------------------------------------ | ------------------------------------------ |
| `machine-sinclair-zx-spectrum`  | ZX Spectrum (48K, 128K, +2, +2A, +3) | Planned split from `emu-spectrum`          |
| `machine-commodore-64`          | Commodore 64 (PAL, NTSC)             | Planned split from `emu-c64`               |
| `machine-nintendo-nes`          | NES/Famicom (NTSC, PAL)              | Planned split from `emu-nes`               |
| `machine-commodore-amiga`       | Amiga (A500-A4000)                   | Currently `machine-amiga`; rename planned  |

### Runnable Packages

| Crate           | Role                                                   | Status                        |
| --------------- | ------------------------------------------------------ | ----------------------------- |
| `emu-spectrum`  | Runnable Spectrum package; headed and headless modes   | Complete                      |
| `emu-c64`       | Runnable C64 package; headed and headless modes        | Feature-complete              |
| `emu-nes`       | Runnable NES package; headed and headless modes        | Complete                      |
| `amiga-runner`  | Runnable Amiga package; intended to become `emu-amiga` | Transitional                  |

## Testing Strategy

### Per-Chip Unit Tests

Every chip crate has `#[cfg(test)]` modules testing behavior in isolation with
mock buses and inputs: instruction correctness, cycle counting, interrupt
timing, waveform output, pixel output, and DMA slot allocation.

### Per-Format Tests

Load and save round-trip for every format. Reject corrupt and truncated files.
Validate checksums where applicable.

### Integration Tests

Crystal-to-frame timing verification. DMA contention cycle counts. Cross-chip
timing such as Copper writes at exact beam position.

### Test ROM And Program Suites

- `NES`: `nestest`, blargg APU and PPU tests
- `Amiga`: SysTest, DiagROM
- `Spectrum`: FUSE test suite
- `C64`: Lorenz test suite, VICE test programs

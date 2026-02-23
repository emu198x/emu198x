# emu198x: Comprehensive Multi-System Emulator Architecture

## Vision

A cycle-accurate emulator framework covering 8-bit, 16-bit, and 32-bit home
computers and consoles from the late 1970s through the mid 1990s. Every system
is modeled from its master crystal oscillator down, with each IC as an
independent, reusable crate.

---

## Architectural Principles

1. **Crystal-clock-driven timing** -- every system derives all clocks from its
   master oscillator. The emulator models this propagation explicitly: master
   clock -> dividers -> chip clocks. No frame-based approximations.

2. **One crate per IC** -- each chip is independently testable and reusable
   across machines. Chips never depend on other chip crates.

3. **One crate per media format** -- parsing ADF, D64, TAP, SNA, iNES etc. is
   pure logic with no hardware dependencies.

4. **One crate per peripheral device** -- floppy drives, keyboards, controllers
   are separate from the chips that control them.

5. **Machine crates are thin orchestrators** -- they wire components together,
   implement the `Machine` trait, own the master clock, and derive all chip
   clocks from it. Minimal logic.

6. **One crate per chip variant** -- each distinct chip revision gets its own
   crate, named with its full manufacturer prefix. Where one generation extends
   another (OCS → ECS → AGA), the later crate wraps and composes the earlier
   one. Config within a single crate is reserved for pin/jumper differences on
   the same die (e.g. NTSC/PAL crystal selection, LFSR polynomial).

---

## Part 1: Core Framework (`emu-core`)

### 1.1 Existing Traits

- `Bus` -- 8-bit read/write with `u32` address, tick, fetch, contention hooks
- `IoBus` -- extends `Bus` with Z80-style I/O port read/write
- `Cpu<B: Bus>` -- step, reset, interrupt, nmi, pc
- `Machine` -- run_frame, render, generate_audio, key_down/up, joystick, load_file

### 1.2 Required Changes

**Widen `Cpu::pc()` to `u32`:**
The 68000 has a 24-bit address space, the 65C816 has 24-bit, ARM7TDMI has
32-bit. Change `fn pc(&self) -> u16` to `fn pc(&self) -> u32`. Update
implementations in `zilog-z80` and `mos-6502` (return `self.pc as u32`).

**Add `WordBus` trait:**
```rust
/// Extension for 16-bit data bus systems (68000, SH-2).
/// The 68000 also does byte accesses (CIA registers), so this extends Bus.
pub trait WordBus: Bus {
    fn read_word(&mut self, address: u32) -> u16;
    fn write_word(&mut self, address: u32, value: u16);
}
```

**Add `Tickable` trait (formerly `Clocked` in this plan draft):**
```rust
/// A component that can be advanced by clock ticks.
pub trait Tickable {
    /// Advance by one master clock tick.
    fn tick(&mut self);

    /// Advance by multiple ticks.
    fn tick_n(&mut self, count: Ticks) {
        for _ in 0..count.get() {
            self.tick();
        }
    }
}
```

### 1.3 Clock Domain Model

Each machine crate owns a `MasterClock` that counts oscillator ticks and
derives all component clocks via integer dividers:

```rust
pub struct ClockDomain {
    pub master_freq_hz: u64,      // e.g., 28_375_160 for Amiga PAL
    pub dividers: &'static [u32], // chain of dividers to reach this domain
}
```

This is not a trait -- it's a design pattern. Each machine crate implements
clock derivation in its `run_frame()` loop by counting master ticks and
firing component ticks at the appropriate ratios.

**Examples:**
```
Amiga PAL:   28.37516 MHz / 4 = 7.09379 MHz (color clock) / 2 = 3.54690 MHz (CPU)
Spectrum 48K: 14.000 MHz / 4 = 3.500 MHz (CPU), / 2 = 7.000 MHz (pixel)
C64 PAL:     17.734475 MHz * 4/9 = 7.882 MHz (dot) / 8 = 0.985 MHz (CPU)
NES NTSC:    21.477272 MHz / 12 = 1.790 MHz (CPU), / 4 = 5.369 MHz (PPU)
Genesis:     53.693175 MHz / 7 = 7.671 MHz (68000), / 15 = 3.580 MHz (Z80)
```

---

## Part 2: CPU Crates

### 68000 Family

| Crate | CPU | Extends | Used By |
|---|---|---|---|
| `motorola-68000` | 68000 | (base) | A1000, A500, A2000, Atari ST, Genesis, Neo Geo |
| `motorola-68010` | 68010 | `motorola-68000` (thin wrapper today) | A2000 accelerators |
| `motorola-68020` | 68020 | `motorola-68000` (thin wrapper today) | Accelerator cards |
| `motorola-68ec020` | 68EC020 | `motorola-68020` (subset) | A1200, CD32 |
| `motorola-68030` | 68030 | `motorola-68020` | A3000, A4000/030, Atari TT/Falcon |
| `motorola-68040` | 68040 | `motorola-68030` | A4000/040 |
| `motorola-68060` | 68060 | `motorola-68040` | Accelerator cards |

Apollo 68080 → `apollo-68080` (stretch goal).

**`motorola-68000` Detail:**

68000 emulation is working locally. Bring it in as a workspace crate.

Current status: `CpuModel`/capability scaffolding exists in `motorola-68000`,
and `motorola-68010` / `motorola-68020` exist as thin wrapper crates that pin
the shared core to a later model while model-specific semantics are added.

**Requirements:**
- Implement `Cpu<B: Bus>` with `pc() -> u32`
- Support all 68000 instructions with cycle-accurate bus timing
- Model the 2-word prefetch queue -- bus stalls happen mid-instruction
- The `Bus` implementation handles DMA contention (same pattern as Spectrum ULA
  contention), so the CPU doesn't know about DMA

Each later crate composes the previous one and adds instructions, addressing
modes, and features (MMU, FPU, caches) on top.

### 6502 Family

| Crate | CPU | Extends | Used By |
|---|---|---|---|
| `mos-6502` | 6502 | (base) | Atari 8-bit, BBC Micro, Apple II, 1541 |
| `mos-6510` | 6510 | `mos-6502` + I/O port | C64 |
| `mos-8502` | 8502 | `mos-6510` + 2 MHz | C128 |
| `ricoh-2a03` | 2A03 | `mos-6502` − decimal mode | NES |
| `wdc-65c02` | 65C02 | `mos-6502` + extra ops | Apple IIe/IIc, PC Engine |
| `wdc-65c816` | 65C816 | `wdc-65c02` + 16-bit | SNES, Apple IIGS |

### Z80 Family

| Crate | CPU | Extends | Used By |
|---|---|---|---|
| `zilog-z80` | Z80A/B | (base) | Spectrum, CPC, MSX, SMS, Game Gear, Genesis sound |
| `sharp-sm83` | SM83 | (separate — Z80/8080 hybrid) | Game Boy, GBC |
| `ascii-r800` | R800 | `zilog-z80` + extras | MSX turboR |

### Other CPUs

| Crate | CPU | Used By |
|---|---|---|
| `sony-spc700` | SPC700 | SNES audio |
| `hitachi-sh2` | SH-2 | 32X, Saturn |
| `arm-arm7tdmi` | ARM7TDMI | GBA |
| `hudson-huc6280` | HuC6280 | PC Engine |

---

## Part 3: Amiga -- All Variants

### 3.1 The Amiga Family

| Model | Year | Chipset | CPU | Chip RAM | Unique Hardware |
|---|---|---|---|---|---|
| A1000 | 1985 | OCS | 68000 @ 7.09 MHz | 256KB | WCS (writable control store for Kickstart) |
| A500 | 1987 | OCS | 68000 | 512KB (1MB w/ 8372) | Trapdoor expansion, Gary |
| A2000 | 1987 | OCS/ECS | 68000 | 512KB-1MB | Zorro II slots, CPU slot, video slot, Buster |
| A500+ | 1991 | ECS | 68000 | 1MB | Built-in ECS, battery-backed RTC |
| A3000 | 1990 | ECS | 68030 @ 25 MHz | 2MB | Zorro III, SCSI (DMAC+33C93), Ramsey, Super Buster, Amber flicker fixer |
| A600 | 1992 | ECS | 68000 | 1MB (2MB) | Gayle (IDE + PCMCIA), no Zorro |
| A1200 | 1992 | AGA | 68EC020 @ 14 MHz | 2MB | Gayle (IDE + PCMCIA), trapdoor, clock port |
| A4000 | 1992 | AGA | 68030/040 | 2MB | Zorro III, IDE (via Gayle/A4000 IDE), Ramsey, Super Buster |
| A4000T | 1994 | AGA | 68040 @ 25 MHz | 2MB | Tower, SCSI (NCR53C710), Zorro III |
| CDTV | 1991 | ECS | 68000 | 1MB | CD-ROM drive, IR remote, DMAC for CD |
| CD32 | 1993 | AGA | 68EC020 @ 14 MHz | 2MB | Akiko (chunky-to-planar + CD), gamepad |

### 3.2 Chipset Generations

#### OCS (Original Chip Set)

| Chip | Part Numbers | Function |
|---|---|---|
| Agnus | 8361 (NTSC), 8367 (PAL), 8370 (NTSC Fat), 8371 (PAL Fat) | DMA controller, copper, blitter, beam counter. 8361/8367: 512KB max. 8370/8371: 1MB (Fat Agnus). |
| Denise | 8362 | Video output, sprites, playfields, collision. 12-bit color (4096 colors), 32 palette registers, up to 6 bitplanes (HAM/EHB for more). |
| Paula | 8364 | 4-ch 8-bit audio, disk controller, interrupt controller. Unchanged across all chipset generations. |

#### ECS (Enhanced Chip Set)

| Chip | Part Number | New vs OCS |
|---|---|---|
| Super Agnus | 8372A | 1MB chip RAM, improved beam counter (more VHPOS bits), programmable display size |
| Super Denise | 8373 | Productivity modes (640x480, 1280x256 super-hires), borderless display, improved sprite resolution, scan doubling support, genlock |
| Paula | 8364 | Unchanged |

#### AGA (Advanced Graphics Architecture)

| Chip | Name | New vs ECS |
|---|---|---|
| Alice | (replaces Agnus) | 32-bit chip RAM bus, 2MB chip RAM, improved blitter |
| Lisa | (replaces Denise) | 256 palette registers from 16.7M colors (24-bit RGB), up to 8 bitplanes (256 color planar), HAM8 (262144 colors), 64-bit fetch for bitplanes, sprite improvements |
| Paula | 8364 | Still unchanged |

### 3.3 Amiga Support Chips

Each of these is a separate crate:

| Crate | Chip | Found In | Function |
|---|---|---|---|
| `commodore-gary` | Gary | A500, A2000 | Address decoding glue logic, overlay control, ROM select |
| `commodore-fat-gary` | Fat Gary | A3000, A4000 | Enhanced address decode, bus timeout, power supply control |
| `commodore-gayle` | Gayle | A600, A1200, CD32 | IDE controller (active-4 ATA), PCMCIA controller (Type I/II), interrupt management |
| `commodore-ramsey` | Ramsey | A3000, A4000 | DRAM controller, refresh, configurable RAM size/type |
| `commodore-buster` | Buster | A2000 | Zorro II bus controller, autoconfig |
| `commodore-super-buster` | Super Buster | A3000, A4000 | Zorro III bus controller (32-bit DMA capable) |
| `commodore-dmac` | DMAC (390537) | A3000, CDTV | DMA controller for SCSI (WD33C93) and CD-ROM |
| `commodore-akiko` | Akiko | CD32 | Chunky-to-planar hardware converter, CD-ROM subcode controller |
| `commodore-amber` | Amber | A3000 | Flicker fixer / scan doubler (converts interlaced output to progressive) |

### 3.4 Amiga Custom Chip Crates

**Agnus** -- DMA controller, copper, blitter, beam counter. Three crates with
layered composition:

- `commodore-agnus-ocs` -- 8361/8367/8370/8371. NTSC/PAL and 512KB/1MB as
  config (same die, pin differences). Contains copper, blitter, DMA sub-modules.
- `commodore-agnus-ecs` -- Super Agnus 8372A. Wraps OCS, adds 1MB chip RAM,
  improved beam counter, programmable display size.
- `commodore-agnus-aga` -- Alice. Wraps ECS, adds 32-bit bus, 2MB chip RAM,
  improved blitter.

**Denise** -- Video output, sprites, playfields. Three crates:

- `commodore-denise-ocs` -- 8362. 12-bit color, 32 palette regs, 6 bitplanes.
- `commodore-denise-ecs` -- Super Denise 8373. Wraps OCS, adds productivity
  modes, scan doubling, genlock.
- `commodore-denise-aga` -- Lisa. Wraps ECS, adds 24-bit color, 256 regs, 8
  bitplanes, HAM8.

**Paula** -- Audio, disk, interrupts (unchanged across generations):

- `commodore-paula-8364`

**CIA** -- Two per Amiga (CIA-A: keyboard/overlay/LED, CIA-B: disk/serial):

- `mos-cia-8520` -- Binary TOD, timer auto-start on high-byte write.

### 3.5 Amiga Peripheral & Expansion Crates

| Crate | Device | Notes |
|---|---|---|
| `drive-amiga-floppy` | 3.5" DD 880KB floppy | MFM encoding, 80 cylinders × 2 heads × 11 sectors |
| `drive-amiga-hd` | IDE/SCSI hard drive | Block device abstraction |
| `drive-cdrom` | CD-ROM drive | For CDTV and CD32, ISO 9660 + CD-DA |
| `peripheral-amiga-keyboard` | Keyboard controller | Serial protocol via CIA-A |
| `peripheral-amiga-mouse` | Mouse | Quadrature encoding via JOYxDAT |
| `peripheral-amiga-gamepad` | CD32 gamepad | 7-button via shift register |
| `expansion-zorro` | Zorro II/III bus | Autoconfig protocol, slot management |
| `expansion-pcmcia` | PCMCIA interface | Type I/II cards (via Gayle) |

### 3.6 Amiga Expansion Cards (Virtual)

| Crate | Card | Function |
|---|---|---|
| `card-rtg` | Virtual RTG graphics card | Chunky framebuffer, P96/CGX compatible, arbitrary resolution/depth |
| `card-accelerator` | Virtual accelerator | Faster CPU + fast RAM, uses appropriate `motorola-*` CPU crate |
| `card-ethernet` | Virtual network card | For networking support |
| `card-ram-expansion` | RAM expansion | Fast RAM on Zorro II/III |

### 3.7 Amiga Machine Crate

```rust
pub struct Amiga<C: ChipsetConfig> {
    // Master clock
    master_clock: u64,                    // 28.37516 MHz (PAL) or 28.63636 MHz (NTSC)
    color_clock_divider: u32,             // /4 from master

    // Custom chips
    cpu: M68k,                            // motorola-68000 (or -68ec020/-68030/-68040)
    agnus: Agnus,                         // commodore-agnus-{ocs,ecs,aga}
    denise: Denise,                       // commodore-denise-{ocs,ecs,aga}
    paula: Paula,                         // commodore-paula-8364
    cia_a: Cia8520,                       // mos-cia-8520
    cia_b: Cia8520,                       // mos-cia-8520

    // Support chips (model-dependent)
    gary: Option<Gary>,                   // A500, A2000
    gayle: Option<Gayle>,                 // A600, A1200, CD32
    ramsey: Option<Ramsey>,               // A3000, A4000
    akiko: Option<Akiko>,                 // CD32 only

    // Memory
    chip_ram: Vec<u8>,                    // 256KB-2MB
    fast_ram: Vec<u8>,                    // 0-8MB+ (Zorro/trapdoor)
    kickstart: Vec<u8>,                   // 256KB or 512KB ROM

    // Peripherals
    floppy: [Option<AmigaFloppyDrive>; 4],
    ide: Option<IdeDrive>,
    keyboard: AmigaKeyboard,

    // Expansion
    zorro_slots: Vec<Box<dyn ZorroCard>>,
    pcmcia: Option<PcmciaSlot>,

    // Output
    framebuffer: Vec<u8>,
    audio_buffer: Vec<(f32, f32)>,
}
```

**Model presets:**
```rust
pub type Amiga1000 = Amiga<A1000Config>;   // OCS, 68000, 256KB, WCS
pub type Amiga500  = Amiga<A500Config>;    // OCS, 68000, 512KB
pub type Amiga2000 = Amiga<A2000Config>;   // OCS/ECS, 68000, Zorro II, CPU slot
pub type Amiga500Plus = Amiga<A500PlusConfig>; // ECS, 68000, 1MB
pub type Amiga3000 = Amiga<A3000Config>;   // ECS, 68030, Zorro III, SCSI
pub type Amiga600  = Amiga<A600Config>;    // ECS, 68000, IDE, PCMCIA
pub type Amiga1200 = Amiga<A1200Config>;   // AGA, 68EC020, IDE, PCMCIA
pub type Amiga4000 = Amiga<A4000Config>;   // AGA, 68030/040, Zorro III, IDE
pub type AmigaCdtv = Amiga<CdtvConfig>;    // ECS, 68000, CD-ROM
pub type AmigaCd32 = Amiga<Cd32Config>;    // AGA, 68EC020, CD-ROM, Akiko
```

### 3.8 Vampire / SAGA (Stretch Goal)

The Vampire is an FPGA-based accelerator with:
- **Apollo 68080 CPU** -- 68060-compatible + AMMX (128-bit SIMD), 64-bit regs
- **SAGA chipset** -- AGA-superset with chunky display modes, RTG-like framebuffer,
  hardware sprite scaling, 16-bit audio
- **Variants:** V2 (plugs into A500/A600/A1200), V4 (standalone board)

Modeled as:
- `apollo-68080` CPU crate (extends `motorola-68060`, adds AMMX instructions)
- `commodore-saga` crate (AGA superset with chunky modes)
- `card-vampire` expansion crate

This is explicitly deferred to later phases.

---

## Part 4: Existing Systems -- All Variants

### 4.1 ZX Spectrum Family

**Already implemented.** Needs variant expansion.

#### Clock Trees

| Variant | Crystal(s) | Pixel Clock | CPU Clock | T/Line | Lines | T/Frame |
|---|---|---|---|---|---|---|
| 48K | 14.000 + 4.4336 MHz | 7.000 MHz | 3.500 MHz | 224 | 312 | 69,888 |
| 128 / +2 grey | 17.7345 MHz | 7.094 MHz | 3.547 MHz | 228 | 311 | 70,908 |
| +2A / +2B / +3 | 17.7345 MHz | 7.094 MHz | 3.547 MHz | 228 | 311 | 70,908 |
| Pentagon 128 | 14.000 MHz | 7.000 MHz | 3.500 MHz | 224 | 320 | 71,680 |

#### ULA Variants

| Chip | Models | Emulation Notes |
|---|---|---|
| 5C102E / 5C112E | 48K Issue 1-2 | Buggy I/O contention |
| 6C001E-7 | 48K Issue 3-6A | Standard 48K contention model |
| 7K010E-5 / Amstrad 40056 | 128K, +2 grey | 228 T-state line, different contention |
| Amstrad 40077 gate array | +2A/B, +3 | Not a ULA -- different contention again |
| Pentagon discrete logic | Pentagon | **No contention** |
| Timex SCLD | TC2068/TC2048 | Extended video modes (512×192, 8×1 attr) |

#### Variants to Add

| Variant | New Chips Needed | Notes |
|---|---|---|
| +3 | `nec-upd765` (uPD765A) | 3" floppy drive, +3DOS, port $1FFD paging |
| Pentagon 128/256/512 | None (discrete logic) | No contention, 320-line frame, different INT timing |
| Scorpion ZS-256 | None | Different memory mapping, turbo modes |
| Timex 2068 | `timex-scld` | Extended video modes, different AY ports, cartridge |

#### Expansions to Add

| Expansion | New Crate | Notes |
|---|---|---|
| Interface 1 | `peripheral-if1` | Microdrive, RS-232, ZX Net, shadow ROM |
| Interface 2 | `peripheral-if2` | Cartridge ROM overlay, joystick ports |
| Kempston joystick | (config in machine) | Port $1F, trivial |
| DivIDE/DivMMC | `peripheral-divide` | IDE/SD, shadow ROM paged on M1 traps |
| Multiface | `peripheral-multiface` | NMI button, shadow ROM/RAM |
| Beta 128 (TR-DOS) | `peripheral-beta128` | `western-digital-wd1770` (WD1793), up to 4 drives |
| AY board (for 48K) | (config in machine) | Add AY-3-8910 at 128-compatible ports |

### 4.2 NES / Famicom Family

**Already implemented.** Needs variant expansion.

#### Clock Trees

| Variant | Master Crystal | CPU Divider | PPU Divider | CPU Clock | PPU:CPU | Lines |
|---|---|---|---|---|---|---|
| NTSC (2A03/2C02) | 21.477272 MHz | /12 | /4 | 1.790 MHz | 3:1 | 262 |
| PAL (2A07/2C07) | 26.601712 MHz | /16 | /5 | 1.663 MHz | 3.2:1 | 312 |

#### PPU Variants

| Chip | System | Notes |
|---|---|---|
| RP2C02 | Famicom, NES NTSC | Standard |
| RP2C07 | NES PAL | 312 lines, 3.2:1 ratio |
| RC2C03B/C | VS. System | RGB output, standard palette |
| RP2C04-0001..0004 | VS. System | 4 different scrambled palettes |
| RC2C05-01/03/04 | VS. System | Swapped $2000/$2001, ID in $2002 |

#### Variants to Add

| Variant | Changes Needed | Notes |
|---|---|---|
| PAL NES | PPU variant config | 26.6 MHz crystal, 312 lines, 3.2:1 PPU:CPU ratio |
| Famicom | Audio expansion support | 60-pin cart with expansion audio pins |
| Famicom Disk System | `nintendo-fds-2c33` | 32KB RAM, 8KB CHR RAM, wavetable audio, QD drive |
| VS. System | PPU variant config | Scrambled palettes, coin-op hardware, DIP switches |
| PlayChoice-10 | Z80 supervisor | Dual screen, timer system |

#### Mapper Priority

**Tier 1 (~90% of licensed library):**
NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4), AxROM (7)

**Tier 2 (expansion audio -- Famicom only):**
VRC6 (24/26), VRC7 (85), Namco 163 (19), Sunsoft 5B (69), MMC5 (5)

Each expansion audio mapper contains a sound chip that could be its own crate:
- `konami-vrc6-audio` -- 2 pulse + 1 sawtooth
- `konami-vrc7-audio` -- YM2413 derivative (6-ch FM)
- `namco-163-audio` -- 8-ch wavetable
- `sunsoft-5b-audio` -- YM2149 variant (3-ch PSG)

### 4.3 Commodore 64 Family

**Already implemented.** Needs variant expansion.

#### Clock Trees

```
PAL:  17.734475 MHz * 4/9 = 7.882 MHz (dot) / 8 = 0.985249 MHz (CPU)
NTSC: 14.318181 MHz * 4/7 = 8.182 MHz (dot) / 8 = 1.022727 MHz (CPU)
```

VIC-II generates the CPU clock. Without VIC-II, nothing runs.

#### VIC-II Variants

| Chip | System | Cycles/Line | Lines | Total Cycles | Notes |
|---|---|---|---|---|---|
| 6567R56A | NTSC early | 64 | 262 | 16,768 | Rare |
| 6567R8 | NTSC common | 65 | 263 | 17,095 | Standard NTSC |
| 8562 | NTSC C64C | 65 | 263 | 17,095 | HMOS, grey dots bug |
| 6569R1 | PAL early | 63 | 312 | 19,656 | 5 luminance levels |
| 6569R3/R5 | PAL common | 63 | 312 | 19,656 | 9 luminance levels |
| 8565 | PAL C64C | 63 | 312 | 19,656 | HMOS, grey dots bug |
| 6572 | PAL-N | 65 | 312 | 20,280 | Rare |

#### SID Variants

| Feature | 6581 (NMOS) | 8580 (HMOS) |
|---|---|---|
| Filter | Beefy, inconsistent (±30%) | Clean, consistent |
| Combined waveforms | Unique behavior (exploited by composers) | Different mixing |
| Volume register audio | **Loud** (4-bit DAC exploit) | Nearly silent |
| Supply | +12V | +9V |

#### CIA Variants

| Feature | 6526 (pre-1987) | 6526A (post-1987) |
|---|---|---|
| Timer IRQ | Standard timing | Fires 1 cycle later |
| Impact | Some copy protection relies on exact CIA timing | |

#### Variants to Add

| Variant | Changes | Notes |
|---|---|---|
| SX-64 | No Datasette port, built-in 1541, 60Hz TOD | Different default colors |
| C64 GS | Cartridge-only, no keyboard | Game console variant |
| PAL-N (6572) | `mos-vic-ii-paln` crate | 65 cycles/line, 312 lines |

#### Expansions to Add

| Expansion | New Crate | Notes |
|---|---|---|
| 1541 drive | `drive-1541` (extract) | Own 6502 + 2× VIA 6522, GCR encoding |
| 1541-II | variant of `drive-1541` | External PSU, same logic |
| 1581 drive | `drive-1581` | 3.5" DD, `western-digital-wd1770` + `mos-cia-8520` |
| REU 1700/1750/1764 | `mos-reu-8726` | DMA engine, 128KB-2MB, I/O at $DF00 |
| CMD SuperCPU | Uses `wdc-65c816` | 65816 @ 20 MHz, 16MB RAM |
| EasyFlash | `cart-easyflash` | 1MB flash, bank-switched |
| Action Replay | `cart-action-replay` | Freezer/trainer cartridge |
| SwiftLink | `mos-acia-6551` | RS-232 up to 38.4 kbps |
| 1351 Mouse | (config in machine) | Proportional mouse via SID pots |

### 4.4 Commodore 128 Family

**Already implemented.** Needs variant expansion.

#### Clock Tree
```
Same crystal as C64 (17.734475 MHz PAL / 14.318181 MHz NTSC)
Same 8701 clock generator
CPU at 1 MHz or 2 MHz (software selectable)
Z80 at ~4 MHz input, ~2 MHz effective (bus sharing)
VDC: SEPARATE 16 MHz crystal (asynchronous to system bus!)
```

#### VDC Variants

| Chip | Models | VRAM | Notes |
|---|---|---|---|
| 8563 | C128, C128D | 16KB (upgradeable to 64KB) | Original |
| 8568 | C128DCR | 64KB standard | Integrated glue logic, different pinout, register-compatible |

#### Variants to Add

| Variant | Changes | Notes |
|---|---|---|
| C128D | Built-in 1571, metal case | Same electronics |
| C128DCR | `mos-vdc-8568` with 64KB VRAM | Cost-reduced |

#### Expansions to Add

Same as C64 plus:
- 1571 drive burst mode (in C128 native mode)
- VDC RAM expansion (16KB → 64KB)
- CP/M 3.0 support (Z80 mode)

---

## Part 5: Future Systems

### 5.1 8-Bit Computers

| System | CPU Crate | Key Chip Crates | Priority |
|---|---|---|---|
| Amstrad CPC | `zilog-z80` | `motorola-mc6845`, `general-instrument-ay-3-8910`, `intel-ppi-8255`, `nec-upd765` | High |
| MSX1 | `zilog-z80` | `texas-instruments-tms9918a`, `general-instrument-ay-3-8910`, `intel-ppi-8255` | High |
| MSX2 | `zilog-z80` | `yamaha-v9938` (wraps `texas-instruments-tms9918a`), `general-instrument-ay-3-8910`, `intel-ppi-8255` | High |
| MSX2+ | `zilog-z80` | `yamaha-v9958` (wraps `yamaha-v9938`), `general-instrument-ay-3-8910`, `intel-ppi-8255`, `yamaha-ym2413` | High |
| MSX turboR | `ascii-r800` | `yamaha-v9958`, `yamaha-ym2149`, `intel-ppi-8255`, `yamaha-ym2413` | High |
| BBC Micro | `mos-6502` | `motorola-mc6845`, `texas-instruments-sn76489`, `mos-via-6522`, `western-digital-wd1770` | Medium |
| Atari 8-bit | `mos-6502` | `atari-antic`, `atari-gtia`, `atari-pokey`, `motorola-pia-6520` | Medium |
| Apple II | `mos-6502` (+`wdc-65c816` for IIGS) | `ensoniq-doc-5503` (IIGS) | Medium |
| ZX81 | `zilog-z80` | `ferranti-ula-zx81` | Low |

### 5.2 16-Bit Computers

| System | CPU Crate | Key Chip Crates | Priority |
|---|---|---|---|
| Atari ST/STE | `motorola-68000` | `yamaha-ym2149`, `motorola-mc6850`, `motorola-mc68901`, `western-digital-wd1770`, `atari-shifter` | High |
| Atari TT | `motorola-68030` | Above + `atari-tt-shifter` | Low |
| Atari Falcon | `motorola-68030` | `atari-videl`, `motorola-dsp56001` | Low |
| Sharp X68000 | `motorola-68000` | `yamaha-ym2151`, `oki-msm6258`, `sharp-x68k-video` | Low |

### 5.3 Consoles

| System | CPU Crate(s) | Key Chip Crates | Priority |
|---|---|---|---|
| Sega Master System | `zilog-z80` | `sega-315-5124`, `texas-instruments-sn76489` | High |
| Game Gear | `zilog-z80` | `sega-315-5124` (viewport as config), `texas-instruments-sn76489` | High (w/ SMS) |
| Sega Genesis | `motorola-68000` + `zilog-z80` | `sega-genesis-vdp`, `yamaha-ym2612`, `texas-instruments-sn76489` | High |
| SNES | `wdc-65c816` + `sony-spc700` | `nintendo-snes-ppu`, `nintendo-snes-dsp` | High |
| Game Boy / GBC | `sharp-sm83` | `sharp-gb-ppu`, `sharp-gb-apu` | High |
| PC Engine | `hudson-huc6280` | `hudson-huc6270` (VDC), `hudson-huc6260` (VCE) | Medium |
| Game Boy Advance | `arm-arm7tdmi` + `sharp-sm83` | `nintendo-gba-ppu` | Medium |
| Neo Geo | `motorola-68000` + `zilog-z80` | `snk-lspc`, `yamaha-ym2610` | Medium |
| Mega CD | `motorola-68000` (×2) | `ricoh-rf5c164`, `sega-asic-mcd` | Low |
| 32X | `hitachi-sh2` (×2) | `sega-32x-vdp` | Low |

---

## Part 6: Shared Chip Crates -- Cross-System Reuse

### 6.1 Sound Chips

| Crate | Chip | Systems |
|---|---|---|
| `general-instrument-ay-3-8910` | AY-3-8910/8912 | Spectrum 128+, CPC, Mockingboard |
| `yamaha-ym2149` | YM2149F | MSX, Atari ST, Sunsoft 5B mapper |
| `mos-sid-6581` | SID 6581 | C64, C128 |
| `mos-sid-8580` | SID 8580 | C64C, C128CR |
| `texas-instruments-sn76489` | SN76489 | BBC Micro, SMS, Game Gear, Genesis |
| `yamaha-ym2612` | YM2612 (OPN2) | Genesis |
| `yamaha-ym2610` | YM2610 (OPNB) | Neo Geo |
| `yamaha-ym2151` | YM2151 (OPM) | X68000, arcade |
| `yamaha-ym2413` | YM2413 (OPLL) | MSX2+, SMS Japan |
| `atari-pokey` | POKEY | Atari 8-bit |
| `ensoniq-doc-5503` | Ensoniq DOC | Apple IIGS |

**Shared internal module: `ym-fm-core`** -- common Yamaha FM operator logic
(envelope generator, phase generator, sine table, feedback, LFO) shared by
`yamaha-ym2612`, `yamaha-ym2610`, `yamaha-ym2151`, `yamaha-ym2413`.

### 6.2 Video Chips

| Crate | Chip | Systems |
|---|---|---|
| `texas-instruments-tms9918a` | TMS9918A | MSX1, Colecovision, SG-1000 |
| `yamaha-v9938` | V9938 | MSX2 (wraps `texas-instruments-tms9918a`) |
| `yamaha-v9958` | V9958 | MSX2+ (wraps `yamaha-v9938`) |
| `sega-315-5124` | Sega VDP | SMS, Game Gear (viewport as config) |
| `motorola-mc6845` | MC6845 CRTC | CPC, BBC Micro (pure timing, no pixel generation) |
| `mos-vic-ii-pal` | VIC-II PAL | C64 PAL, C128 PAL |
| `mos-vic-ii-ntsc` | VIC-II NTSC | C64 NTSC, C128 NTSC |
| `ferranti-ula-spectrum` | Spectrum ULA | Spectrum (extract from machine-spectrum) |
| `atari-antic` | ANTIC | Atari 8-bit (display list DMA processor) |
| `atari-gtia` | GTIA | Atari 8-bit (color generation + sprites) |

### 6.3 Support Chips

| Crate | Chip | Systems |
|---|---|---|
| `mos-cia-6526` | CIA 6526/6526A | C64, C128 |
| `mos-cia-8520` | CIA 8520 | Amiga |
| `mos-via-6522` | VIA 6522 | BBC Micro, Apple II, 1541 drive |
| `intel-ppi-8255` | PPI 8255 | CPC, MSX, X68000 |
| `motorola-mc6850` | ACIA 6850 | Atari ST (MIDI + keyboard) |
| `motorola-mc68901` | MFP 68901 | Atari ST (interrupts, timers, serial) |
| `motorola-pia-6520` | PIA 6520/6821 | Atari 8-bit |
| `nec-upd765` | NEC 765 FDC | CPC, Spectrum +3 |
| `western-digital-wd1770` | WD1770/1772/1793 FDC | Atari ST, BBC Master, Beta 128 |

The 6526 and 8520 differ in TOD format (BCD vs binary) and timer auto-start.
The 6526A is a mask revision -- config within `mos-cia-6526`.

---

## Part 7: Media Format Crates

| Crate | Format | Systems | Status |
|---|---|---|---|
| `format-adf` | Amiga Disk File (880KB) | Amiga | New |
| `format-hdf` | Amiga Hard Disk File | Amiga | New |
| `format-ipf` | Interchangeable Preservation Format | Amiga, Atari ST | New (SPS/CAPS) |
| `format-d64` | 1541 disk image | C64, C128 | Extract |
| `format-d71` | 1571 disk image | C128 | Extract |
| `format-d81` | 1581 disk image | C64, C128 | New |
| `format-tap` | Tape (Spectrum/C64) | Spectrum, C64 | Extract |
| `format-tzx` | TZX tape format | Spectrum | New |
| `format-sna` | SNA snapshot | Spectrum | Extract |
| `format-z80` | Z80 snapshot | Spectrum | New |
| `format-prg` | C64 program | C64, C128 | Extract |
| `format-t64` | T64 tape archive | C64 | New |
| `format-ines` | iNES/NES 2.0 ROM | NES | Extract |
| `format-fds` | FDS disk image | Famicom Disk System | New |
| `format-dsk` | Amstrad/+3 disk | CPC, Spectrum +3 | New |
| `format-cas` | MSX cassette | MSX | New |
| `format-rom-msx` | MSX ROM | MSX | New |
| `format-sms` | SMS/GG ROM | SMS, Game Gear | New |
| `format-md` | Genesis ROM | Genesis | New |
| `format-sfc` | SNES ROM | SNES | New |
| `format-gb` | Game Boy ROM | GB, GBC | New |
| `format-gba` | GBA ROM | GBA | New |
| `format-st` | Atari ST disk | Atari ST | New |
| `format-atr` | Atari 8-bit disk | Atari 8-bit | New |
| `format-ssd-dsd` | BBC disk | BBC Micro | New |

---

## Part 8: Component Reuse Matrix

How many systems benefit from each shared crate:

| Crate | System Count | Systems |
|---|---|---|
| `zilog-z80` | **9+** | Spectrum, ZX81, CPC, MSX, SMS, Game Gear, Genesis, C128, Neo Geo |
| `mos-6502` | **4** | Atari 8-bit, BBC Micro, Apple II, 1541 drive |
| `mos-6510` | **1** | C64 |
| `ricoh-2a03` | **1** | NES |
| `motorola-68000` | **5+** | Amiga (OCS/ECS), Atari ST, Genesis, Neo Geo, X68000 |
| `motorola-68ec020` | **2** | A1200, CD32 |
| `motorola-68030` | **3** | A3000, A4000/030, Atari TT/Falcon |
| `general-instrument-ay-3-8910` | **3** | Spectrum 128+, CPC, Mockingboard |
| `yamaha-ym2149` | **3** | MSX, Atari ST, Sunsoft 5B |
| `texas-instruments-sn76489` | **4** | BBC Micro, SMS, Game Gear, Genesis |
| `mos-cia-6526` | **2** | C64, C128 |
| `mos-cia-8520` | **1** | Amiga |
| `intel-ppi-8255` | **3** | CPC, MSX, X68000 |
| `motorola-mc6845` | **2** | CPC, BBC Micro |
| `mos-via-6522` | **2+** | BBC Micro, Apple II, 1541/1571 drives |
| `nec-upd765` | **2** | CPC, Spectrum +3 |
| `mos-vic-ii-pal` | **2** | C64 PAL, C128 PAL |
| `mos-vic-ii-ntsc` | **2** | C64 NTSC, C128 NTSC |
| `mos-vic-ii-paln` | **1** | C64 PAL-N |
| `mos-sid-6581` | **2** | C64, C128 |
| `mos-sid-8580` | **2** | C64C, C128CR |

---

## Part 9: Implementation Roadmap

### Phase 1: Core Framework (Immediate)

1. Widen `Cpu::pc()` to `u32`
2. Add `WordBus` trait to `emu-core`
3. Add `Tickable` trait to `emu-core` (this replaced the earlier `Clocked` wording)

### Phase 2: Amiga Foundation (Immediate)

1. `motorola-68000` -- bring in 68000 code as workspace crate (done, with `CpuModel` scaffolding)
2. `mos-cia-8520` -- Amiga CIA
3. `commodore-paula-8364` -- audio + disk + interrupts
4. `commodore-agnus-ocs` -- DMA + copper + blitter (OCS)
5. `commodore-denise-ocs` -- video output (OCS)
6. `format-adf` -- Amiga disk format
7. `drive-amiga-floppy` -- floppy drive mechanism
8. `peripheral-amiga-keyboard` -- keyboard controller
9. `machine-amiga` -- wire together as Amiga 500 (currently implemented as `emu-amiga-rock`)
10. `amiga-runner` -- runner binary

Current status note: thin `motorola-68010` and `motorola-68020` wrapper crates
already exist to support staged 68k family expansion, and KS1.3 boot-screen
regression assertions are in place for the Amiga baseline.

### Phase 3: Amiga Variants

1. `commodore-agnus-ecs` (Super Agnus 8372A, wraps OCS)
2. `commodore-denise-ecs` (Super Denise 8373, wraps OCS)
3. `commodore-agnus-aga` (Alice, wraps ECS)
4. `commodore-denise-aga` (Lisa, wraps ECS)
5. `commodore-gayle` -- IDE + PCMCIA (A600, A1200, CD32)
6. `commodore-gary`, `commodore-fat-gary` -- address decoding
7. `commodore-akiko` -- chunky-to-planar + CD (CD32)
8. `commodore-ramsey`, `commodore-super-buster` -- A3000/A4000
9. `motorola-68ec020`, `motorola-68030`, `motorola-68040` CPU crates
10. Model configs: A1000, A2000, A500+, A600, A1200, A3000, A4000, CDTV, CD32
11. `card-rtg` -- virtual RTG graphics card
12. `expansion-zorro` -- Zorro II/III bus + autoconfig

### Phase 4: Existing System Variants

1. Spectrum +3 (add `nec-upd765`, +3DOS support)
2. Spectrum Pentagon (no contention model, 320-line frame)
3. NES PAL support (2A07/2C07 variants, 3.2:1 ratio)
4. Famicom Disk System (`nintendo-fds-2c33`, wavetable audio)
5. NES expansion audio mappers (VRC6, VRC7, Namco 163, Sunsoft 5B)
6. `mos-sid-6581`, `mos-sid-8580` -- separate SID crates
7. `mos-vic-ii-pal`, `mos-vic-ii-ntsc` -- separate VIC-II crates
8. `mos-cia-6526` -- C64/C128 CIA
9. C128 VDC 64KB support, C128DCR model

### Phase 5: Extract Shared Chips from Existing Machines

1. `general-instrument-ay-3-8910` (from Spectrum 128 code, enables CPC)
2. `yamaha-ym2149` (separate crate, enables MSX/Atari ST)
3. `ferranti-ula-spectrum` (from machine-spectrum)
4. `mos-vdc-8563`, `mos-vdc-8568` (from machine-c128)
5. Format crate extractions (TAP, SNA, D64, PRG, iNES)

### Phase 6: New 8-Bit Systems

1. `machine-cpc` -- Amstrad CPC (`zilog-z80` + `motorola-mc6845` + `general-instrument-ay-3-8910` + `intel-ppi-8255` + `nec-upd765`)
2. `machine-msx` -- MSX family (`zilog-z80` + `texas-instruments-tms9918a` / `yamaha-v9938` / `yamaha-v9958` + `general-instrument-ay-3-8910` + `intel-ppi-8255`)
3. `machine-bbc` -- BBC Micro (`mos-6502` + `motorola-mc6845` + `texas-instruments-sn76489` + `mos-via-6522`)
4. `machine-atari8` -- Atari 800XL (`mos-6502` + `atari-antic` + `atari-gtia` + `atari-pokey`)
5. `machine-zx81` -- ZX81 (`zilog-z80` + `ferranti-ula-zx81`)

### Phase 7: 16-Bit Systems & Consoles

1. `machine-genesis` -- Sega Genesis (`motorola-68000` + `zilog-z80` + `sega-genesis-vdp` + `yamaha-ym2612` + `texas-instruments-sn76489`)
2. `machine-sms` -- Sega Master System (`zilog-z80` + `sega-315-5124` + `texas-instruments-sn76489`)
3. `machine-snes` -- SNES (`wdc-65c816` + `nintendo-snes-ppu` + `sony-spc700` / `nintendo-snes-dsp`)
4. `machine-atarist` -- Atari ST (`motorola-68000` + `yamaha-ym2149` + `atari-shifter` + `motorola-mc68901`)
5. `machine-gb` -- Game Boy / GBC (`sharp-sm83` + `sharp-gb-ppu` + `sharp-gb-apu`)
6. `machine-pce` -- PC Engine (`hudson-huc6280` + `hudson-huc6270` + `hudson-huc6260`)

### Phase 8: Advanced Systems

1. `machine-gba` -- Game Boy Advance (`arm-arm7tdmi`)
2. `machine-neogeo` -- Neo Geo (`motorola-68000` + `zilog-z80` + `snk-lspc` + `yamaha-ym2610`)
3. `machine-x68000` -- Sharp X68000 (`motorola-68000` + `yamaha-ym2151` + `sharp-x68k-video`)
4. Genesis add-ons: Mega CD, 32X
5. Amiga Vampire/SAGA support (`apollo-68080` + `commodore-saga`)

---

## Part 10: Testing Strategy

### Per-chip unit tests

Every chip crate has `#[cfg(test)]` modules testing its behavior in isolation
with mock buses/inputs. Key areas:

- **CPUs:** Instruction correctness (existing fuse-tests for Z80, similar suites
  for 6502/68000), cycle counting, interrupt timing
- **Sound:** Waveform output for known register states
- **Video:** Pixel output for known bitplane/tile/sprite configurations
- **Timers:** Countdown modes, interrupt generation, edge cases
- **DMA:** Slot allocation, contention timing, priority

### Per-format tests

- Load/save round-trip for every format
- Reject corrupt/truncated files gracefully
- Validate checksums where applicable (MFM CRC, iNES header)

### Integration tests

- Crystal-to-frame timing: verify exact T-states/cycles per frame for each
  system variant
- DMA contention: CPU cycle counts under various display configurations
- Cross-chip timing: copper writes affecting Denise at exact beam position

### Test ROM/program suites

- NES: existing test ROMs (nestest, blargg APU/PPU tests)
- Amiga: SysTest, DiagROM
- Spectrum: FUSE test suite (already used)
- C64: Lorenz test suite, VICE test programs
- Genesis: md-test, blastem test ROMs

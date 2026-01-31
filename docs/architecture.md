# Architecture

## Timing Model: Crystal Accuracy

All emulators tick at the **master crystal frequency** of the target system. This is the foundational architectural decision. Everything else derives from it.

### Why Crystal Accuracy

The CPU is not the timing master on vintage systems. The crystal oscillator is. Video chips, audio chips, and CPUs all derive their clocks from division of the master crystal. Emulating at the crystal level means:

- No timing drift between components
- Phase relationships are always correct
- Bus contention emerges naturally
- Interrupt sampling happens at correct moments
- Peripherals with strict timing (disk controllers, serial) work correctly
- Observable state is coherent at any point

### Master Clocks

| System | Crystal (NTSC) | Crystal (PAL) |
|--------|----------------|---------------|
| ZX Spectrum | — | 14.000 MHz |
| Commodore 64 | 14.318182 MHz | 17.734475 MHz |
| NES/Famicom | 21.477272 MHz | 26.601712 MHz |
| Amiga | 28.63636 MHz | 28.37516 MHz |

### Derived Clocks

All component clocks derive from the master crystal by integer division.

**ZX Spectrum (PAL 14 MHz):**
- ÷2 = 7.000 MHz pixel clock (ULA)
- ÷4 = 3.500 MHz CPU clock (Z80)

**Commodore 64 (PAL 17.734475 MHz):**
- ÷18 = 985.248 kHz CPU clock (6510)
- Dot clock ≈ 7.88 MHz (VIC-II)

**Commodore 64 (NTSC 14.318182 MHz):**
- ÷14 = 1.022727 MHz CPU clock (6510)
- Dot clock ≈ 8.18 MHz (VIC-II)

**NES (NTSC 21.477272 MHz):**
- ÷4 = 5.369318 MHz PPU clock
- ÷12 = 1.789773 MHz CPU clock (6502)
- PPU runs at exactly 3× CPU rate

**NES (PAL 26.601712 MHz):**
- ÷5 = 5.320342 MHz PPU clock
- ÷16 = 1.662607 MHz CPU clock

**Amiga (PAL 28.37516 MHz):**
- ÷4 = 7.09379 MHz CPU clock (68000)
- ÷8 = 3.546895 MHz colour clock (DMA slot reference)

**Amiga (NTSC 28.63636 MHz):**
- ÷4 = 7.15909 MHz CPU clock
- ÷8 = 3.579545 MHz colour clock

### Core Loop Pattern

```rust
struct Emulator {
    master_clock: u64,
    region: Region,  // NTSC or PAL
    // ... component state
}

impl Emulator {
    fn tick(&mut self) {
        self.master_clock += 1;
        
        // Components check their phase
        // Example for NES NTSC: PPU every 4 ticks, CPU every 12
        if self.master_clock % self.ppu_divisor() == self.ppu_phase() {
            self.ppu.tick();
        }
        if self.master_clock % self.cpu_divisor() == self.cpu_phase() {
            self.cpu.tick();
        }
        // Audio, disk controller, etc.
    }
    
    fn run_frame(&mut self) {
        let ticks = self.crystal_hz() / self.fps();
        for _ in 0..ticks {
            self.tick();
        }
    }
}
```

### Phase Offsets

Division alone is insufficient. Components may tick on different phases:

```
Crystal:  |0|1|2|3|4|5|6|7|8|9|A|B|
PPU:      |*|.|.|.|*|.|.|.|*|.|.|.|  (every 4, phase 0)
CPU:      |.|.|.|*|.|.|.|.|.|.|.|*|  (every 12, phase 3)
```

The phase relationship affects:
- When data appears on the bus
- Interrupt sampling points
- Memory contention patterns

Each system document specifies correct phase relationships.

### Multi-Cycle Operations

A 68000 instruction can span 70+ crystal ticks. Track state within instructions:

**Option A — Micro-ops:**
Break each instruction into per-tick operations. More accurate, more complex.

**Option B — Busy-until:**
Mark CPU as busy until tick N. Simpler, but may miss mid-instruction events.

For the 8-bit systems, busy-until is usually sufficient. For Amiga, micro-ops may be required for copper/blitter interaction accuracy.

### Bus Contention

"Is this my tick?" is necessary but not sufficient. Also consider:

- Is the bus free?
- Is DMA active?
- Is the video chip stealing cycles?
- Am I in a wait state?

See per-system documents for contention rules.

### Interrupt Timing

When exactly is an interrupt line sampled? This varies per CPU:

| CPU | Sampling Point |
|-----|----------------|
| 6502 | Between instructions, checks IRQ/NMI on final cycle |
| Z80 | End of instruction, during M1 |
| 68000 | Between instructions, priority encoded |

### Clock Counter

At 28 MHz, a u64 gives ~20,000 years before overflow. Use u64 for master_clock.

### Performance

| System | Crystal | Host cycles @ 4GHz | Headroom |
|--------|---------|-------------------|----------|
| Spectrum | 14 MHz | 285 | 285× |
| C64 | 17.7 MHz | 226 | 226× |
| NES | 21.5 MHz | 186 | 186× |
| Amiga | 28.4 MHz | 141 | 141× |

This is more than enough headroom for interpreted emulation. JIT is not required.

---

## Core Traits

### `Clock`

Anything that has timing.

```rust
pub trait Clock {
    /// Crystal frequency in Hz
    fn crystal_hz(&self) -> u64;
    
    /// Current master clock tick
    fn master_clock(&self) -> u64;
    
    /// Advance by one crystal tick
    fn tick(&mut self);
    
    /// Advance by N crystal ticks
    fn run_ticks(&mut self, n: u64) {
        for _ in 0..n {
            self.tick();
        }
    }
}
```

### `Observable`

Anything whose state can be inspected.

```rust
pub trait Observable {
    /// Snapshot current state
    fn snapshot(&self) -> StateSnapshot;
    
    /// Query specific state by path (e.g., "cpu.a", "memory.0xD020")
    fn query(&self, path: &str) -> Option<Value>;
}
```

### `Controllable`

Anything that accepts input or control.

```rust
pub trait Controllable {
    /// Inject input event
    fn input(&mut self, event: InputEvent);
    
    /// Reset to initial state
    fn reset(&mut self);
    
    /// Load state from snapshot
    fn restore(&mut self, snapshot: &StateSnapshot);
}
```

### `Component`

A subsystem within an emulator (CPU, video chip, audio chip, etc.).

```rust
pub trait Component: Observable {
    /// Divisor from master clock
    fn clock_divisor(&self) -> u64;
    
    /// Phase offset (0 to divisor-1)
    fn clock_phase(&self) -> u64;
    
    /// Execute one component tick
    fn tick(&mut self, bus: &mut Bus);
    
    /// Check if this component ticks on the given master clock
    fn should_tick(&self, master_clock: u64) -> bool {
        master_clock % self.clock_divisor() == self.clock_phase()
    }
}
```

### `Emulator`

A complete system.

```rust
pub trait Emulator: Clock + Observable + Controllable {
    /// System identifier
    fn system(&self) -> System;
    
    /// Current region (NTSC/PAL)
    fn region(&self) -> Region;
    
    /// Insert media (disk, tape, cartridge)
    fn insert_media(&mut self, slot: MediaSlot, media: Media) -> Result<()>;
    
    /// Capture current frame
    fn screenshot(&self) -> Image;
    
    /// Current audio buffer
    fn audio_buffer(&self) -> &[f32];
}
```

---

## System Configuration

Systems have variants. Model the axes, not the permutations.

### Configuration Structure

```rust
pub struct SystemConfig {
    pub region: Region,
    pub variant: SystemVariant,
    pub memory: MemoryConfig,
    pub peripherals: Vec<Peripheral>,
}

pub enum Region {
    PAL,
    NTSC,
}
```

### Per-System Variants

**Spectrum:**
```rust
pub enum SpectrumVariant {
    Spectrum48K,
    Spectrum128K,
    SpectrumPlus2,
    SpectrumPlus2A,
    SpectrumPlus3,
}
```

Differences: Memory banking, AY chip, disk interface, ROM set.

**Commodore 64:**
```rust
pub enum C64Variant {
    C64,        // Only one, really
}

pub enum SidVariant {
    Mos6581,    // Original, darker filter
    Mos8580,    // Later, brighter filter
}
```

Main difference is SID revision and region (PAL/NTSC crystal).

**NES:**
```rust
pub enum NesVariant {
    Nes,        // Western
    Famicom,    // Japanese (expansion audio, different controllers)
}
```

Differences: Expansion audio routing, controller pinout, region timing.

**Amiga:**
```rust
pub enum AmigaChipset {
    OCS,    // Original: A500, A1000, A2000
    ECS,    // Enhanced: A500+, A600, A3000
    AGA,    // Advanced: A1200, A4000
}

pub enum M68kVariant {
    M68000,
    M68020,
    M68030,
    M68040,
}

pub struct AmigaConfig {
    pub chipset: AmigaChipset,
    pub cpu: M68kVariant,
    pub chip_ram: usize,
    pub fast_ram: usize,
    pub kickstart: KickstartVersion,
}
```

Accelerators are just "faster CPU + Fast RAM" — they don't change chipset timing.

### Presets

Provide named presets for common configurations:

```rust
pub fn preset_a500() -> AmigaConfig {
    AmigaConfig {
        chipset: AmigaChipset::OCS,
        cpu: M68kVariant::M68000,
        chip_ram: 512 * 1024,
        fast_ram: 0,
        kickstart: KickstartVersion::V1_3,
    }
}

pub fn preset_a1200() -> AmigaConfig {
    AmigaConfig {
        chipset: AmigaChipset::AGA,
        cpu: M68kVariant::M68020,
        chip_ram: 2 * 1024 * 1024,
        fast_ram: 0,
        kickstart: KickstartVersion::V3_1,
    }
}
```

### What Accelerators Change

An accelerator replaces the CPU. It does NOT change:
- Chipset timing (OCS is still OCS)
- Chip RAM access speed (still colour-clock limited)
- DMA behaviour

It DOES change:
- CPU instruction execution speed
- Fast RAM access speed
- Available instructions (68020+ have more)
- Cache behaviour

Model as: `AmigaConfig { cpu: M68030, fast_ram: 8MB, ..preset_a500() }`

The A500 with a 68030 accelerator is still an A500 — same video, same Kickstart compatibility, same software. It just runs faster when not waiting for the bus.

### Educational Targets

Lessons target the baseline:
- Spectrum 48K PAL
- C64 PAL with 6581 SID
- NES NTSC
- Amiga 500 PAL with Kickstart 1.3

Other variants exist for:
- Running user's own software
- Testing compatibility
- Historical accuracy

---

## What Not To Do

These are architectural errors. Do not attempt them.

### ❌ Instruction-level CPU stepping

```rust
// WRONG: This loses cycle accuracy
fn run_frame(&mut self) {
    while !self.frame_complete {
        self.cpu.execute_instruction();
        self.ppu.catch_up(self.cpu.cycles);
    }
}
```

The CPU does not execute complete instructions atomically. Memory accesses happen on specific cycles. The video chip needs to interleave.

### ❌ Floating-point timing

```rust
// WRONG: Float drift will accumulate
let cycles_per_frame: f64 = 29780.5;
```

Use integer master clock ticks. Handle fractional frames by alternating tick counts.

### ❌ Skipping ticks for performance

```rust
// WRONG: Breaks timing accuracy
if self.turbo_mode {
    self.tick(); self.tick(); self.tick(); self.tick();
}
```

If you need to run faster than real-time, run more frames. Each frame must tick correctly.

### ❌ Treating the CPU as timing master

```rust
// WRONG: Video chip is often the true master
fn tick(&mut self) {
    if self.cpu.ready() {
        self.cpu.tick();
    }
    self.ppu.tick();  // PPU should drive timing, not follow
}
```

The CPU waits for the video chip, not the other way around.

# Amiga Emulator Implementation Plan

## Overview

Add a cycle-accurate Amiga 500 (OCS) emulator to the emu198x workspace, using a
**crate-per-component** architecture where each custom chip, memory subsystem,
peripheral device, and media format is an independent crate with well-defined
interfaces. The user already has 68000 CPU emulation working locally -- this
plan assumes that code will be brought in as the `cpu-m68k` crate.

This architecture is designed to be reusable across machines and to eventually
allow factoring out existing components (VIC-II, SID, CIA 6526, ULA, 1541 drive,
D64/TAP formats) from the current machine crates.

### Architectural Principles

1. **One crate per IC** -- each chip is independently testable and reusable
2. **One crate per bus/memory subsystem** -- bus arbitration and memory are
   modeled explicitly, not hidden inside machine crates
3. **One crate per media format** -- parsing ADF, D64, TAP, SNA, iNES etc. is
   pure logic with no hardware dependencies
4. **One crate per peripheral device** -- floppy drives, keyboards etc. are
   separate from the chips that control them
5. **Machine crates are thin orchestrators** -- they wire components together
   and implement the `Machine` trait, but contain minimal logic themselves

---

## Phase 1: Core Trait Changes

### 1.1 Widen `Cpu::pc()` to `u32`

The existing `Cpu` trait hardcodes `pc() -> u16`, but the 68000 has a 24-bit
address space. The `Bus` trait already uses `u32` for addresses.

**File:** `core/src/cpu.rs`

Change `fn pc(&self) -> u16` to `fn pc(&self) -> u32`.

Update the two existing implementations:
- `cpu-6502/src/lib.rs` -- return `self.pc as u32`
- `cpu-z80/src/lib.rs` -- return `self.pc as u32`

Update any callers (grep for `.pc()` usage in machine crates and runners).

### 1.2 Add system bus traits to `emu-core`

New modules in `core/src/`:

**`system_bus.rs` -- Shared data bus abstraction:**

```rust
/// A 16-bit data bus with word-aligned access, as used by the Amiga,
/// and adaptable via wrapper for 8-bit buses (Spectrum, C64, NES).
pub trait SystemBus {
    /// Read a word from the bus. May stall the requestor if the bus is busy.
    fn read_word(&mut self, address: u32) -> u16;

    /// Write a word to the bus. May stall the requestor if the bus is busy.
    fn write_word(&mut self, address: u32, value: u16);

    /// Read a byte (for systems with byte-wide buses or CIA access).
    fn read_byte(&mut self, address: u32) -> u8;

    /// Write a byte.
    fn write_byte(&mut self, address: u32, value: u8);
}
```

**`dma.rs` -- DMA request/response types:**

```rust
/// A DMA transfer request from a chip to the bus arbiter.
pub struct DmaRequest {
    pub address: u32,
    pub write: bool,
    pub data: u16,       // for writes
    pub priority: DmaPriority,
}

pub enum DmaPriority {
    Disk,
    Audio,
    Sprite,
    Bitplane,
    Copper,
    Blitter,
    Refresh,
}
```

**`chip.rs` -- Common chip interface:**

```rust
/// Trait for memory-mapped chips with a register interface.
pub trait Chip {
    fn read_register(&self, reg: u16) -> u16;
    fn write_register(&mut self, reg: u16, value: u16);
}
```

---

## Phase 2: Memory & Bus Crates

### 2.1 `mem-bus` -- Bus Arbitration

A crate that models a shared bus with cycle-level arbitration. This is not
Amiga-specific -- the concept of "multiple requestors sharing a bus with
priority" applies to any system with DMA.

```rust
pub struct BusArbiter<const SLOTS_PER_LINE: usize> {
    slot_owners: [Option<DmaPriority>; SLOTS_PER_LINE],
    current_slot: usize,
}

impl BusArbiter {
    /// Register a DMA request for the current slot.
    /// Returns true if granted (higher priority than any existing claim).
    pub fn request(&mut self, req: &DmaRequest) -> bool;

    /// Is the CPU blocked from the bus this slot?
    pub fn cpu_blocked(&self) -> bool;

    /// Advance to the next slot.
    pub fn advance(&mut self);
}
```

For simpler systems (Spectrum, C64), the bus arbiter degenerates to just
contention delay tables -- the same crate can provide a `ContentionTable`
type that the existing machines could adopt.

### 2.2 `mem-chip-ram` -- Amiga Chip RAM

Models the shared memory accessible by both CPU and DMA channels.

```rust
pub struct ChipRam {
    data: Vec<u8>,        // 512KB or 1MB depending on Agnus variant
    size: usize,
}

impl ChipRam {
    pub fn read_word(&self, address: u32) -> u16;
    pub fn write_word(&mut self, address: u32, value: u16);
    pub fn read_byte(&self, address: u32) -> u8;
    pub fn write_byte(&mut self, address: u32, value: u8);
}
```

This is kept separate from bus arbitration -- ChipRam is pure storage, the bus
arbiter decides *when* it can be accessed.

For other systems this pattern could yield `mem-spectrum-ram`, `mem-c64-ram`
etc., but those are simple enough that a standalone crate may be overkill. The
Amiga's chip RAM warrants separation because of the complex DMA interactions.

### 2.3 `mem-rom` -- ROM storage (shared across systems)

Generic ROM container. Multiple systems need ROM loading (Kickstart, BASIC,
KERNAL, Spectrum ROM).

```rust
pub struct Rom {
    data: Vec<u8>,
    base_address: u32,
}

impl Rom {
    pub fn load(data: &[u8], base: u32) -> Self;
    pub fn read_byte(&self, address: u32) -> u8;
    pub fn read_word(&self, address: u32) -> u16;
}
```

---

## Phase 3: Media Format Crates

Each file format parser is a standalone crate with no hardware dependencies.
Pure data structures and parsing logic.

### 3.1 `format-adf` -- Amiga Disk File

```rust
pub struct Adf {
    tracks: [[u8; TRACK_SIZE]; 160],  // 80 cylinders × 2 heads
}

impl Adf {
    pub fn load(data: &[u8]) -> Result<Self, FormatError>;
    pub fn read_track(&self, cylinder: u8, head: u8) -> &[u8];
    pub fn write_track(&mut self, cylinder: u8, head: u8, data: &[u8]);
    pub fn encode_mfm(&self, cylinder: u8, head: u8) -> Vec<u8>;
    pub fn decode_mfm(mfm_data: &[u8]) -> Result<Vec<u8>, FormatError>;
}
```

### 3.2 Future format crates (refactored from existing code)

| Crate | Format | Currently in |
|---|---|---|
| `format-d64` | Commodore 1541 disk image | `machine-c64` |
| `format-d71` | Commodore 1571 disk image | `machine-c128` |
| `format-tap` | Spectrum/C64 tape | `machine-spectrum`, `machine-c64` |
| `format-sna` | Spectrum snapshot | `machine-spectrum` |
| `format-prg` | C64 program file | `machine-c64` |
| `format-ines` | NES ROM (iNES/NES 2.0) | `machine-nes` |
| `format-adf` | Amiga disk file | new |

These are small crates (typically <500 lines each) but the separation means:
- Format validation is testable without booting an emulator
- Multiple machines can share formats (TAP is used by both Spectrum and C64)
- New format support doesn't touch machine crates

---

## Phase 4: Chip Crates

Each chip is its own crate with dependencies only on `emu-core` for shared
traits. No chip crate depends on another chip crate.

### 4.1 `cpu-m68k` (user's existing code)

Bring in the user's 68000 emulation as a workspace crate.

Key requirements for integration:
- Implement `Cpu<B: Bus>` trait (with the widened `pc() -> u32`)
- Bus access must be word-aligned (68000 reads/writes 16 bits at a time)
- The 68000 prefetch queue (2 words) must be modeled -- bus stalls happen
  mid-instruction when a prefetched bus cycle is delayed by DMA
- Expose per-bus-cycle granularity: the machine crate needs to interleave DMA
  slots between individual 68000 bus cycles, not just between instructions

**CPU-bus interaction (critical for cycle accuracy):**

The `Bus` implementation (owned by the machine crate) handles DMA arbitration
inside `read()`/`write()`. Each CPU bus access calls `bus.read()`, which
internally advances the system by one color clock, checks if DMA needs the slot,
stalls if needed, then returns. This matches the existing pattern used by the
Spectrum (ULA contention in `bus.read()`).

The CPU doesn't need to know about DMA -- it just sees slower bus responses when
accessing chip RAM. Fast RAM and ROM accesses bypass arbitration entirely.

### 4.2 `chip-agnus` -- DMA controller, beam counter

**Responsibilities:**
- Beam position counter (VPOS/HPOS), color clock advancement
- DMA slot arbitration: fixed-slot table for disk/audio/sprite/refresh,
  dynamic scheduling for bitplane DMA based on DDFSTRT/DDFSTOP/BPU
- Bitplane pointer management (auto-increment, modulo addition)
- Sprite DMA pointer management
- DMACON register (master DMA enable, per-channel enables, BLTPRI)

**Contains two sub-components as modules (not sub-crates):**

- **Copper** -- WAIT/MOVE/SKIP instruction pipeline, beam position comparison,
  copper danger bit, dual copper list pointers
- **Blitter** -- channels A-D, barrel shifter, first/last word masks, minterms,
  area fill (inclusive/exclusive), line drawing (Bresenham), ascending/descending

These live inside `chip-agnus` rather than as separate crates because they are
physically part of Agnus and share its internal state (beam position, DMA slot
allocation). The copper and blitter don't exist as independent ICs.

**Public interface:**

```rust
pub struct Agnus { /* ... */ }

impl Agnus {
    /// Advance by one color clock. Returns what happened this slot.
    pub fn tick(&mut self, ram: &mut ChipRam) -> SlotResult;

    /// Read a register (VPOSR, VHPOSR, DMACONR, etc.).
    pub fn read_register(&self, reg: u16) -> u16;

    /// Write a register (DMACON, DIWSTRT, BPLxPT, BLTSIZE, COPxLC, etc.).
    pub fn write_register(&mut self, reg: u16, value: u16);

    /// Is the CPU blocked from chip RAM this cycle?
    pub fn cpu_blocked(&self) -> bool;

    /// Is the blitter busy?
    pub fn blitter_busy(&self) -> bool;
}

/// What happened during this DMA slot.
pub struct SlotResult {
    pub bitplane_data: Option<(u8, u16)>,    // (plane_idx, data) -> feed to Denise
    pub sprite_data: Option<(u8, u16, bool)>, // (sprite, data, is_ctl) -> feed to Denise
    pub copper_write: Option<(u16, u16)>,     // (register, value) -> dispatch to chip
    pub interrupt: Option<u16>,               // interrupt request bits (blitter done, copper)
}
```

**Variant support:** OCS Agnus (8370/8371, 512KB) vs Fat Agnus (8372, 1MB).
Config enum selects chip RAM address mask.

**Files:**
- `chip-agnus/src/lib.rs` -- Agnus struct, register handling, top-level tick
- `chip-agnus/src/dma.rs` -- DMA slot table, fixed slot assignments, bitplane scheduling
- `chip-agnus/src/copper.rs` -- Copper state machine, IR1/IR2 pipeline, WAIT comparison
- `chip-agnus/src/blitter.rs` -- Blitter engine, minterms, fill, line draw

### 4.3 `chip-denise` -- Video output

**Responsibilities:**
- Receive bitplane data words from Agnus, load into shift registers
- Planar-to-chunky conversion (assemble pixel color index from up to 6 bitplanes)
- Display modes: normal, dual playfield, HAM (Hold-And-Modify), EHB (Extra Half-Brite)
- 8 hardware sprites, attached mode for 15-color pairs
- Playfield/sprite priority (BPLCON2)
- Horizontal fine scroll (BPLCON1, 0-15 pixel delay per playfield)
- Collision detection (CLXDAT/CLXCON)
- 32-entry 12-bit color palette (COLOR00-COLOR31)

Denise has **no bus access** -- all data comes from Agnus via the machine crate.
This matches the real hardware.

**Public interface:**

```rust
pub struct Denise { /* ... */ }

impl Denise {
    /// Process one color clock. Returns pixel output.
    pub fn tick(
        &mut self,
        bitplane_data: Option<(u8, u16)>,
        sprite_data: Option<(u8, u16, bool)>,
        display_active: bool,
    ) -> PixelOutput;

    pub fn read_register(&self, reg: u16) -> u16;
    pub fn write_register(&mut self, reg: u16, value: u16);
}

pub struct PixelOutput {
    pub color: u16,          // 12-bit RGB (4:4:4) from palette lookup
    pub is_blank: bool,      // in blanking period
    pub is_border: bool,     // outside display window
}
```

**Files:**
- `chip-denise/src/lib.rs` -- Denise struct, register handling, palette
- `chip-denise/src/playfield.rs` -- Bitplane shift registers, mode rendering
- `chip-denise/src/sprites.rs` -- 8 sprite channels, attached mode, multiplexing
- `chip-denise/src/collision.rs` -- Hardware collision detection matrix

### 4.4 `chip-paula` -- Audio, disk control, interrupts

**Responsibilities:**
- 4-channel 8-bit PCM audio with period counters, volume, inter-channel modulation
- Disk controller: MFM sync detection, DSKLEN double-write safety, read/write DMA
- Interrupt controller: INTENA/INTREQ registers, 14 sources mapped to IPL levels 1-6

**Public interface:**

```rust
pub struct Paula { /* ... */ }

impl Paula {
    /// Advance by one color clock.
    pub fn tick(&mut self);

    /// Does this slot have a fixed audio DMA assignment?
    pub fn audio_dma_slot(&self, hpos: u16) -> Option<DmaRequest>;

    /// Does this slot have a fixed disk DMA assignment?
    pub fn disk_dma_slot(&self, hpos: u16) -> Option<DmaRequest>;

    /// Provide data fetched by Agnus for audio/disk DMA.
    pub fn dma_complete(&mut self, channel: DmaChannel, data: u16);

    /// Get current audio output (left, right) as f32 samples.
    pub fn audio_output(&self) -> (f32, f32);

    /// Current 68000 interrupt priority level (0 = none, 1-6).
    pub fn ipl(&self) -> u8;

    /// Set an interrupt request (from other chips: copper, blitter, vblank).
    pub fn set_interrupt(&mut self, bit: u16);

    pub fn read_register(&self, reg: u16) -> u16;
    pub fn write_register(&mut self, reg: u16, value: u16);
}
```

**Files:**
- `chip-paula/src/lib.rs` -- Paula struct, interrupt controller (INTENA/INTREQ/IPL mapping)
- `chip-paula/src/audio.rs` -- 4 audio channels, period/volume/ADKCON modulation
- `chip-paula/src/disk.rs` -- Disk controller, MFM sync, DSKLEN latch

### 4.5 `chip-cia-8520` -- CIA timer/IO chip (×2 per Amiga)

Models a single MOS 8520 CIA. The machine crate instantiates two.

**Responsibilities per instance:**
- Two 8-bit I/O ports with data direction registers (PRA/PRB/DDRA/DDRB)
- Two 16-bit countdown timers (A/B) with continuous/one-shot/chained modes
- 24-bit binary Time-of-Day counter (binary, unlike 6526's BCD)
- Serial shift register (CIA-A uses this for keyboard protocol)
- 5-source interrupt controller (ICR)
- Clocked by E-clock (~709 kHz), not the main system clock

**Public interface:**

```rust
pub struct Cia8520 { /* ... */ }

impl Cia8520 {
    pub fn tick(&mut self);                        // one E-clock tick
    pub fn read(&mut self, reg: u8) -> u8;         // register 0x0-0xF
    pub fn write(&mut self, reg: u8, value: u8);
    pub fn irq(&self) -> bool;
    pub fn set_port_a_input(&mut self, value: u8);
    pub fn set_port_b_input(&mut self, value: u8);
    pub fn port_a_output(&self) -> u8;             // after DDR masking
    pub fn port_b_output(&self) -> u8;
    pub fn set_flag_pin(&mut self, state: bool);   // active-low /FLAG input
    pub fn set_cnt_pin(&mut self, state: bool);
    pub fn set_sp_pin(&mut self, state: bool);     // serial port data input
    pub fn tod_tick(&mut self);                     // 50/60Hz TOD input
}
```

**Relationship to C64/C128 CIA 6526:** Very similar but 6526 has BCD TOD and
slightly different timer edge cases. Start as separate `chip-cia-8520`; a
future `chip-cia` crate with a variant generic could unify both. Not worth
blocking on.

---

## Phase 5: Peripheral Device Crates

### 5.1 `drive-amiga-floppy` -- Amiga 3.5" DD floppy drive

Models the physical drive mechanism, separate from Paula's disk controller.

```rust
pub struct AmigaFloppyDrive {
    disk: Option<Adf>,       // uses format-adf crate
    cylinder: u8,            // current head position (0-79)
    head: u8,                // 0 or 1
    motor_on: bool,
    rotation_pos: u32,       // position within current track (in bits)
    mfm_track: Vec<u8>,      // MFM-encoded track data
}

impl AmigaFloppyDrive {
    pub fn insert_disk(&mut self, disk: Adf);
    pub fn eject_disk(&mut self) -> Option<Adf>;
    pub fn step(&mut self, direction: bool);       // step in/out
    pub fn select_head(&mut self, head: u8);
    pub fn set_motor(&mut self, on: bool);
    pub fn read_bit(&mut self) -> bool;            // next MFM bit from rotation
    pub fn write_bit(&mut self, bit: bool);
    pub fn is_track0(&self) -> bool;               // /TK0 signal
    pub fn is_write_protected(&self) -> bool;
    pub fn disk_changed(&self) -> bool;            // /DSKCHANGE signal
    pub fn index_pulse(&self) -> bool;             // once per revolution
}
```

The machine crate connects CIA-B port B outputs (step, direction, motor, side,
select) to this drive's methods, and connects the drive's MFM bitstream to
Paula's disk controller input.

### 5.2 `peripheral-keyboard` -- Amiga keyboard controller

Models the keyboard's internal microcontroller protocol.

```rust
pub struct AmigaKeyboard {
    key_states: [bool; 128],
    transmit_queue: VecDeque<u8>,
    handshake_pending: bool,
}

impl AmigaKeyboard {
    pub fn key_down(&mut self, amiga_keycode: u8);
    pub fn key_up(&mut self, amiga_keycode: u8);
    pub fn tick(&mut self);                        // advance internal state
    pub fn serial_data_ready(&self) -> bool;       // connect to CIA-A SP/CNT
    pub fn read_serial(&mut self) -> u8;           // keycode byte
    pub fn handshake(&mut self);                   // CIA-A acknowledges receipt
}
```

### 5.3 Future peripheral crates (refactored from existing code)

| Crate | Device | Currently in |
|---|---|---|
| `drive-1541` | Commodore 1541 floppy drive | `machine-c64` |
| `drive-1571` | Commodore 1571 floppy drive | `machine-c128` |
| `peripheral-datasette` | Commodore tape drive | `machine-c64` |
| `peripheral-spectrum-tape` | Spectrum tape deck | `machine-spectrum` |

---

## Phase 6: `machine-amiga` -- System Integration

The orchestrator crate that wires all components into a working Amiga 500.
Implements the `Machine` trait. Contains minimal logic -- just plumbing.

### 6.1 Module structure

```
machine-amiga/
  src/
    lib.rs          -- pub type Amiga500 = Amiga<OcsChipset>;
    amiga.rs        -- Amiga<C: Chipset> struct, Machine trait impl
    bus.rs          -- Bus impl (address decoding, DMA contention)
    input.rs        -- KeyCode -> Amiga keycode mapping, mouse/joystick
    config.rs       -- Chipset trait, OcsChipset, (future: EcsChipset, AgaChipset)
```

### 6.2 The Amiga struct

```rust
pub struct Amiga<C: Chipset> {
    cpu: M68000,                     // cpu-m68k
    agnus: C::Agnus,                 // chip-agnus (OCS or ECS variant)
    denise: C::Denise,               // chip-denise
    paula: Paula,                    // chip-paula
    cia_a: Cia8520,                  // chip-cia-8520
    cia_b: Cia8520,                  // chip-cia-8520
    chip_ram: ChipRam,               // mem-chip-ram
    fast_ram: Option<Vec<u8>>,       // optional expansion
    kickstart: Rom,                  // mem-rom
    floppy: [Option<AmigaFloppyDrive>; 4],  // drive-amiga-floppy
    keyboard: AmigaKeyboard,         // peripheral-keyboard
    framebuffer: Vec<u8>,            // RGBA output
    audio_buffer: Vec<(f32, f32)>,   // stereo samples accumulated per frame
    e_clock_divider: u8,             // counts to 10 for CIA E-clock
}
```

### 6.3 The core loop: `run_frame()`

Each iteration = 1 color clock = 2 CPU cycles = 1 DMA slot.

```
fn run_frame(&mut self) {
    while !frame_complete {
        // 1. Agnus tick: advance beam, execute DMA slot, run copper/blitter
        let slot = self.agnus.tick(&mut self.chip_ram);

        // 2. Dispatch copper register writes to target chips
        if let Some((reg, val)) = slot.copper_write {
            self.dispatch_register_write(reg, val);
        }

        // 3. Feed bitplane/sprite data from Agnus to Denise
        let pixel = self.denise.tick(
            slot.bitplane_data,
            slot.sprite_data,
            self.agnus.display_active(),
        );

        // 4. Write pixel to framebuffer (if not in blanking)
        if !pixel.is_blank {
            self.write_pixel(pixel.color);
        }

        // 5. Paula tick: audio counters, disk state
        self.paula.tick();
        let (left, right) = self.paula.audio_output();
        self.audio_buffer.push((left, right));

        // 6. Propagate interrupts from slot results
        if let Some(irq_bits) = slot.interrupt {
            self.paula.set_interrupt(irq_bits);
        }

        // 7. CPU gets 2 cycles (bus.read/write handles stalling)
        //    Bus impl checks agnus.cpu_blocked() for chip RAM access
        self.cpu.step(&mut self.bus);

        // 8. E-clock tick (every 10 CPU cycles = every 5 color clocks)
        self.e_clock_divider += 1;
        if self.e_clock_divider >= 5 {
            self.e_clock_divider = 0;
            self.cia_a.tick();
            self.cia_b.tick();
            // Wire CIA outputs to peripherals
            self.update_cia_peripherals();
        }

        // 9. Check interrupts: paula.ipl() -> CPU IPL pins
        self.cpu.set_ipl(self.paula.ipl());
    }
}
```

### 6.4 Bus implementation

The `Bus` impl handles address decoding:

| Address Range | Target | DMA Contention? |
|---|---|---|
| $000000-$0FFFFF | Chip RAM (via bus arbiter) | Yes |
| $C00000-$D7FFFF | "Slow" RAM (trapdoor) | Yes (on chip bus) |
| $200000-$9FFFFF | Fast RAM (expansion) | No |
| $F80000-$FFFFFF | Kickstart ROM | Yes (on chip bus, A500) |
| $DFF000-$DFF1FF | Custom registers | Yes (synced to color clock) |
| $BFE001 | CIA-A (even byte addresses) | E-clock sync latency |
| $BFD000 | CIA-B (odd byte addresses) | E-clock sync latency |

### 6.5 Video output

- **Resolution:** 724×568 (PAL with borders) -- configurable
- **Format:** RGBA framebuffer, same as other machines
- Denise outputs 12-bit palette indices per color clock, machine converts to RGBA

### 6.6 Audio output

- f32 stereo at 44100 Hz
- Paula channels 0+3 left, 1+2 right
- Accumulated per color clock, downsampled in `generate_audio()`

### 6.7 File loading

Uses format crates for parsing:
- `.adf` via `format-adf` -- inserted into `drive-amiga-floppy`
- `.rom` / `.kick` via `mem-rom` -- loaded as Kickstart

---

## Phase 7: `amiga-runner`

Minimal binary. Same pattern as `spectrum-runner` and `c64-runner`.

```
amiga-runner/
  src/
    main.rs     -- load Kickstart ROM, optional ADF, create Amiga500, run()
  Cargo.toml    -- depends on machine-amiga, runner-lib, emu-core
```

---

## Phase 8: Testing Strategy

### 8.1 Per-chip unit tests (in each chip crate)

- **chip-agnus:** Beam position wrapping (PAL/NTSC, long/short frames), DMA slot
  table verification, copper WAIT/MOVE timing, copper danger bit, blitter
  minterm truth tables, blitter area fill, blitter line draw octants
- **chip-denise:** Planar-to-chunky for 1-6 bitplanes, HAM hold-and-modify
  pixel sequence, EHB half-bright, dual playfield with independent scroll,
  sprite priority vs playfield, attached sprite pairs, collision detection
- **chip-paula:** Audio period countdown, volume scaling, channel modulation
  (ADKCON), interrupt priority mapping (14 sources to 6 IPL levels), disk
  sync detection, DSKLEN double-write safety latch
- **chip-cia-8520:** Timer A/B countdown in continuous/one-shot mode, timer B
  counting timer A underflows, TOD counter increment, serial shift register,
  interrupt generation and masking

### 8.2 Per-format unit tests (in each format crate)

- **format-adf:** Load/save round-trip, MFM encode/decode, sector checksum
  validation, reject truncated/corrupt files

### 8.3 Integration tests (in machine-amiga)

- DMA contention: CPU cycle count with 0/2/4/6 bitplanes active
- Copper timing: MOVE to COLOR00 at exact hpos, verify pixel changes
- Blitter + CPU interleave: cycle sharing with BLTPRI=0 vs BLTPRI=1
- CIA timing: keyboard serial protocol, timer interrupt generation
- Disk: ADF load, sync detection, track read via DMA

### 8.4 Test ROM support

- Headless `amiga-test-runner` (like `nes-test-runner`) for automated validation
- Target: SysTest, DiagROM, and custom timing test programs

---

## Phase 9: Future -- Refactor Existing Machines (stretch goal)

Apply the crate-per-component pattern retroactively:

### Chip crates to extract

| Current location | New crate | Reused by |
|---|---|---|
| `machine-spectrum/src/memory/ula.rs` | `chip-ula` | Spectrum 48K, 128K, +2/+3 |
| `machine-c64/src/vic.rs` | `chip-vic-ii` | C64, C128 |
| `machine-c64/src/sid.rs` | `chip-sid` | C64, C128 |
| CIA code in `machine-c64/src/memory.rs` | `chip-cia-6526` | C64, C128 |
| `machine-c128/src/vdc.rs` | `chip-vdc-8563` | C128 |

### Format crates to extract

| Current location | New crate | Reused by |
|---|---|---|
| TAP loading in `machine-spectrum` | `format-tap` | Spectrum, C64 |
| SNA loading in `machine-spectrum` | `format-sna` | Spectrum |
| D64 loading in `machine-c64` | `format-d64` | C64, C128 |
| D71 loading in `machine-c128` | `format-d71` | C128 |
| PRG loading in `machine-c64` | `format-prg` | C64, C128 |
| iNES loading in `machine-nes` | `format-ines` | NES |

### Peripheral crates to extract

| Current location | New crate | Reused by |
|---|---|---|
| Disk drive in `machine-c64` | `drive-1541` | C64 |
| Disk drive in `machine-c128` | `drive-1571` | C128 |
| Tape in `machine-c64` | `peripheral-datasette` | C64, C128 |
| Tape in `machine-spectrum` | `peripheral-spectrum-tape` | Spectrum |

---

## Workspace Changes

New crates for Amiga support:

```toml
members = [
    # ... existing ...
    "cpu-m68k",
    "mem-bus",
    "mem-chip-ram",
    "mem-rom",
    "format-adf",
    "chip-agnus",
    "chip-denise",
    "chip-paula",
    "chip-cia-8520",
    "drive-amiga-floppy",
    "peripheral-keyboard",
    "machine-amiga",
    "amiga-runner",
]
```

---

## Implementation Order

1. **Phase 1** -- Core trait changes (widen `pc()`, add system bus traits)
2. **Phase 2** -- `mem-rom`, `mem-chip-ram`, `mem-bus` (foundational, small)
3. **Phase 3** -- `format-adf` (pure parsing, independently testable)
4. **Phase 4.1** -- Bring in `cpu-m68k` (user's existing code)
5. **Phase 4.5** -- `chip-cia-8520` (simplest chip, good for validating the pattern)
6. **Phase 4.4** -- `chip-paula` (interrupts needed early; audio + disk can start stubbed)
7. **Phase 4.2** -- `chip-agnus` (the big one: DMA + copper + blitter)
8. **Phase 4.3** -- `chip-denise` (video output)
9. **Phase 5** -- `drive-amiga-floppy`, `peripheral-keyboard`
10. **Phase 6** -- `machine-amiga` (wire everything together)
11. **Phase 7** -- `amiga-runner`
12. **Phase 8** -- Testing and validation against real hardware behavior

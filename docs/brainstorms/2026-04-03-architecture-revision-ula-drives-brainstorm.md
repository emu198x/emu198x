# Architecture Revision: ULA-Drives Model

**Date:** 2026-04-03
**Status:** Agreed — supersedes sections 4, 5, 13, and 20 of the original brainstorm
**Prompted by:** SpecIde architecture study revealing that ULA-drives is closer to real hardware

## What Changed

The original brainstorm (2026-04-02) used a **CPU-drives** model: the Z80 calls `bus.read()`, `bus.contend()`, etc., and the Bus implementation advances the master clock. The ULA catches up lazily via `catch_up()`.

After studying SpecIde (the closest reference emulator), we discovered that the **ULA-drives** model is both more accurate and simpler:

- The ULA ticks every half-cycle of the master oscillator (14MHz)
- The ULA gates the CPU's clock signal — contention is implicit (CPU doesn't tick)
- The machine loop handles bus transactions by inspecting Z80 output signals
- The ULA renders video in real-time (no catch_up needed)
- Floating bus and snow effect are free (ULA always knows its fetch state)

## Why This Is Better

On real Spectrum hardware, the Ferranti ULA literally controls the Z80's clock pin. When the ULA needs the bus for a screen fetch, it withholds the clock and the CPU freezes. The CPU doesn't "know" about contention — it just stops.

The CPU-drives model inverted this: the CPU called `bus.contend()` which added delay half-cycles. This was an abstraction that:
1. Required the CPU to know about contention (a machine concern, not a CPU concern)
2. Needed explicit contention lookups on every memory access
3. Required `catch_up()` with event queues for mid-scanline rendering
4. Made floating bus an on-demand computation instead of natural state
5. Required `std::mem::take` to work around Rust's borrow checker

The ULA-drives model eliminates all of these.

## The New Architecture

### Z80 as Signal-Level State Machine

The Z80 is a chip with pins. It exposes output signals (address bus, data bus, control signals) and accepts input signals (data bus for reads, WAIT, INT, NMI). Each `tick()` call advances by one half-cycle of the master clock.

```rust
pub struct Z80 {
    // Registers
    pub regs: Registers,
    
    // Internal state
    state: HalfCycleState,  // e.g., M1_T1_RISE, M1_T1_FALL, M1_T2_RISE, ...
    
    // Output signals (Z80 → machine)
    pub addr: u16,       // Address bus A0-A15
    pub data: u8,        // Data bus D0-D7 (active during write cycles)
    pub mreq: bool,      // Memory request (active low on real hardware, active high here)
    pub iorq: bool,      // I/O request
    pub rd: bool,        // Read
    pub wr: bool,        // Write
    pub m1: bool,        // Machine cycle 1 (opcode fetch)
    pub rfsh: bool,      // Refresh
    pub halt: bool,      // CPU halted
    
    // Input signals (machine → Z80)
    pub data_in: u8,     // Data bus for reads (set by machine before next tick)
    pub wait: bool,      // Wait signal (ULA contention on +2A/+3)
    pub irq: bool,       // Interrupt request
    pub nmi: bool,       // Non-maskable interrupt
    pub nmi_edge: bool,  // NMI edge detection (NMI is edge-triggered)
}

impl Z80 {
    /// Advance one half-cycle. Call this on every master clock edge.
    /// After calling, inspect output signals and perform bus transactions.
    pub fn tick(&mut self) { ... }
}
```

### ULA Interface

The ULA trait changes to reflect its role as the clock master:

```rust
pub trait Ula {
    /// Advance one half-cycle of the master oscillator.
    /// The ULA:
    /// - Advances its pixel counter
    /// - Reads video RAM when needed (sets screen_addr output)  
    /// - Renders pixels to the framebuffer in real-time
    /// - Computes whether the CPU clock is active (contention gating)
    /// - Tracks the current floating bus value
    /// - Generates the interrupt signal at the correct HC
    fn tick(&mut self, memory: &dyn MemoryBus);

    /// Is the CPU clock active this half-cycle?
    /// False = contention (CPU should not tick).
    fn cpu_clock_active(&self) -> bool;

    /// Is the interrupt signal asserted?
    fn interrupt_active(&self) -> bool;

    /// The byte currently on the ULA's data bus (for floating bus reads).
    fn floating_bus(&self) -> u8;

    /// The address the ULA is currently driving for screen fetches.
    /// Used for snow effect detection (conflict with CPU refresh address).
    fn screen_addr(&self) -> u16;

    /// Read port 0xFE (keyboard + EAR bit).
    fn read_fe(&self, port: u16) -> u8;

    /// Write port 0xFE (border + beeper + MIC).
    fn write_fe(&mut self, port: u16, val: u8);

    /// Frame timing constants.
    fn frame_timing(&self) -> &FrameTiming;
    
    /// End of frame housekeeping (flash counter, frame counter).
    fn end_frame(&mut self);
}
```

### Machine Loop

```rust
const FRAME_HC: u32 = 279_552;  // 48K: 69888 T-states × 4 HC per T

impl Spectrum48K {
    fn run_frame(&mut self) {
        for _hc in 0..FRAME_HC {
            // 1. ULA ticks every half-cycle (renders, computes contention)
            self.ula.tick(&self.memory);

            // 2. CPU ticks only when ULA allows
            if self.ula.cpu_clock_active() {
                self.z80.tick();
                self.handle_bus();
            }

            // 3. Feed input signals
            self.z80.irq = self.ula.interrupt_active();
            
            // 4. Tape advance (1 HC at a time)
            if let Some(ref mut tape) = self.tape {
                tape.advance_hc(1);
            }
        }

        self.ula.end_frame();
    }

    fn handle_bus(&mut self) {
        // Memory read
        if self.z80.mreq && self.z80.rd {
            self.z80.data_in = self.memory.read(self.z80.addr);
        }
        // Memory write
        else if self.z80.mreq && self.z80.wr {
            self.memory.write(self.z80.addr, self.z80.data);
        }
        // I/O read
        else if self.z80.iorq && self.z80.rd && !self.z80.m1 {
            self.z80.data_in = self.io_read(self.z80.addr);
        }
        // I/O write
        else if self.z80.iorq && self.z80.wr {
            self.io_write(self.z80.addr, self.z80.data);
        }
        // Interrupt acknowledge
        else if self.z80.iorq && self.z80.m1 {
            self.z80.data_in = 0xFF;  // No device on 48K bus
        }
    }
}
```

### What This Means for the Bus Trait

**The Bus trait is removed.** The Z80 crate exports only the `Z80` struct with its signal-based interface. Each machine provides its own loop that drives the Z80 and handles bus transactions.

For testing (Tom Harte tests), a simple test harness replaces the Bus:

```rust
fn run_test(z80: &mut Z80, memory: &mut [u8; 65536], tstates: u32) {
    let mut hc = 0u32;
    while hc / 4 < tstates {  // 4 HC per T-state for testing
        z80.tick();
        // Handle bus transactions
        if z80.mreq && z80.rd {
            z80.data_in = memory[z80.addr as usize];
        } else if z80.mreq && z80.wr {
            memory[z80.addr as usize] = z80.data;
        } else if z80.iorq && z80.rd {
            z80.data_in = (z80.addr >> 8) as u8;  // FUSE convention
        }
        hc += 1;
    }
}
```

### What This Means for the Z80 Tick Walker

The MStep sequence design is preserved, but each MStep now maps to specific half-cycle states with explicit signal assertions:

| MStep | Half-cycles | Signals |
|---|---|---|
| M1 T1 rise | addr=PC, mreq=true, rd=false, m1=true | Address on bus |
| M1 T1 fall | rd=true | MREQ falls, begin read |
| M1 T2 rise | (data latched from data_in) | Data available |
| M1 T2 fall | mreq=false, rd=false | End of read |
| M1 T3 rise | addr=IR, rfsh=true, mreq=true | Refresh address |
| M1 T3 fall | | Refresh MREQ |
| M1 T4 rise | rfsh=false, mreq=false | End refresh |
| M1 T4 fall | (decode opcode) | Select sequence |

Each MStep from the original design (FetchByte, ReadAddr, WriteAddr, etc.) decomposes into 4-6 half-cycle states with explicit signal transitions.

### What Stays the Same

- ULA as trait (variant-specific implementations)
- Memory as separate trait (address mapping, contention status)
- Crate structure (zilog-z80, spectrum-common, ferranti-ula-6c001e, machine-*)
- Palette-indexed u8 pixel output
- Blip buffer audio
- All test suites
- Clock tree (verified frequencies, half-cycle counting)
- All variant-specific decisions (contention patterns, I/O tables, Timex modes, Next architecture)
- Crate naming convention

### What's Removed

- Bus trait (replaced by signal-based Z80 interface)
- `std::mem::take` pattern (no longer needed — Z80 is not inside the Bus)
- `catch_up()` with event queue (ULA renders in real-time)
- `contend()` / `contend_no_mreq()` / `contend_io()` (contention is implicit — ULA gates the clock)
- `advance_hc()` / `cpu_divisor()` (the loop counts HC, not the bus methods)

### Performance Implications

The loop runs 279,552 iterations per frame instead of 69,888. But each iteration is simpler:
- ULA tick: advance pixel counter, conditional screen fetch, conditional render pixel (~5-10ns)
- CPU tick (when active): advance state machine, set signals (~5ns)
- Bus handler: one branch + one memory access (~3ns)

Estimated frame time: 279K × ~15ns = ~4.2ms. With optimisation (the ULA tick fast-path for non-screen-fetch half-cycles can be very tight), this should come well under 2ms.

The key advantage: **no contention computation overhead**. The CPU simply doesn't tick during contended half-cycles. No lookup table, no branch, no computation. The ULA's pixel counter naturally produces the right gating pattern.

### +2A/+3 WAIT Signal

The Amstrad gate array uses the Z80's WAIT pin instead of clock gating. In this model:
- The `Ula::cpu_clock_active()` always returns true for +2A/+3 (clock is never gated)
- Instead, the Amstrad ULA sets `z80.wait = true` during contended accesses
- The Z80 state machine checks `wait` at the appropriate half-cycle (T2 rise of memory accesses)
- This matches the real hardware precisely

### Pentagon / Scorpion

No contention at all. `cpu_clock_active()` always returns true. `z80.wait` is never asserted. The CPU runs at full speed on every tick.

### Next at 28MHz

The master loop runs at 28MHz. The ULA ticks at 28MHz. At 3.5MHz CPU speed, the CPU fires every 8th half-cycle. At 7/14/28MHz turbo, the divisor decreases. This is natural in the ULA-drives model — the ULA just asserts `cpu_clock_active()` more frequently.

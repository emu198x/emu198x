---
title: "feat: Spectrum 48K boot milestone — cycle-accurate from empty repo to copyright message"
type: feat
date: 2026-04-03
supersedes: docs/plans/2026-04-02-feat-spectrum-48k-boot-milestone-plan.md
brainstorm: docs/brainstorms/2026-04-02-spectrum-variant-aware-architecture-brainstorm.md
revision: docs/brainstorms/2026-04-03-architecture-revision-ula-drives-brainstorm.md
---

# Spectrum 48K Boot Milestone (v2 — ULA-Drives Architecture)

From empty repo to a 48K ZX Spectrum (Issue 3, Ferranti ULA) that boots to "(C) 1982 Sinclair Research Ltd" with cycle-accurate contention, passing FUSE timing tests and 100% Tom Harte Z80 tests.

## Overview

This plan implements the **ULA-drives** architecture: the custom chipset owns the bus and the clock, the CPU is a passive signal-level state machine that gets ticked when the bus master allows it. This is how real hardware works — not just for the Spectrum, but for every system in the Emu198x project.

The milestone proves:
1. The master oscillator loop works (14MHz, ULA ticks every half-cycle, CPU ticks when allowed)
2. Contention is correct (implicit — ULA withholds clock, FUSE timing tests pass)
3. The Z80 is correct (Tom Harte 100%, ZEXDOC passes)
4. The system integrates correctly (ROM boots, copyright message renders)
5. The signal-level CPU pattern is proven and portable to 6502, 6809, 68000

## The Universal Principle: Chipset Drives, CPU Follows

Every retro system has a custom chipset that owns the bus and the clock. The CPU is one of several components fighting for bus time. The chipset decides who gets access.

| System | Bus Master | CPU | How CPU is Gated |
|---|---|---|---|
| ZX Spectrum 48K | Ferranti ULA | Z80 | ULA withholds clock signal during screen fetches |
| ZX Spectrum +2A/+3 | Amstrad gate array | Z80 | Gate array asserts WAIT during screen fetches |
| C64 | VIC-II | 6502 | VIC-II de-asserts BA + AEC to steal bus cycles for sprite/character fetches |
| NES | PPU | 2A03 (6502) | PPU and CPU share a master clock; CPU gets every 3rd master cycle |
| Amiga | Agnus | 68000 | Agnus controls bus — even cycles for DMA, odd cycles for CPU. CPU halted when blitter/copper need all cycles |
| Atari 2600 | TIA | 6502 | TIA generates the master clock, 6502 is directly clocked |
| MSX | VDP (TMS9918) | Z80 | VDP WAITs the Z80 during VRAM access |
| Amstrad CPC | Gate array | Z80 | Gate array gates Z80 clock (similar to Spectrum ULA) |
| Sega Master System | VDP | Z80 | VDP can WAIT the Z80 during VRAM access |

**The architectural principle:** Every CPU in Emu198x is a **signal-level state machine** that exposes its bus signals as outputs and accepts data/control as inputs. The machine loop — driven by the master oscillator — ticks the chipset first, then conditionally ticks the CPU. This is the same pattern regardless of CPU type.

### Implications for All CPU Crates

Each CPU crate exports a struct with:
- **Output signals:** address bus, data bus, control signals (read/write/memory/io/etc.)
- **Input signals:** data bus (for reads), wait, interrupt, reset
- **A `tick()` method** that advances by one half-cycle (or one clock phase)
- **No Bus trait.** The CPU does not "call" bus methods. The machine inspects the CPU's signals and performs transactions.

| CPU | Tick Granularity | Key Signals | Notes |
|---|---|---|---|
| Z80 | Half-cycle (14MHz for Spectrum) | MREQ, IORQ, RD, WR, M1, RFSH, HALT | WAIT input for +2A/+3 |
| 6502 | Half-cycle (PHI1/PHI2) | RW, SYNC, addr, data | RDY input for DMA halt |
| 6809 | Quarter-cycle (E/Q phases) | R/W, BA, BS, addr, data | MRDY for stretching |
| 68000 | Half-cycle (CLK) | AS, UDS, LDS, R/W, FC0-FC2 | DTACK for wait states, BR/BG/BGACK for bus arbitration |

This eliminates the "Bus trait vs signal interface" question for all future systems. There is no Bus trait. There are chips with pins.

## Technical Approach

### Architecture

The master oscillator ticks at the system's crystal frequency. On each tick:
1. The chipset (ULA/VIC-II/PPU/Agnus) ticks — advances its state, performs memory fetches, renders pixels
2. If the chipset allows the CPU to run this cycle, the CPU ticks — advances its internal state machine, drives signals
3. The machine loop inspects signals and performs bus transactions (memory read/write, I/O)
4. Audio chips tick (AY, SID, APU) on their respective divisors

No component "calls" another. They all respond to the clock.

### Z80 Signal-Level Interface (`zilog-z80/src/z80.rs`)

```rust
pub struct Z80 {
    // Registers (public for machine inspection and test setup)
    pub regs: Registers,

    // Half-cycle state machine
    state: HalfCycleState,
    
    // Tick walker state (sequence, step index, staged data, prefix)
    walker: WalkerState,

    // === Output signals (Z80 → machine) ===
    pub addr: u16,       // Address bus A0-A15
    pub data: u8,        // Data bus D0-D7 (active during write cycles)
    pub mreq: bool,      // Memory request active
    pub iorq: bool,      // I/O request active
    pub rd: bool,        // Read active
    pub wr: bool,        // Write active
    pub m1: bool,        // Machine cycle 1 (opcode fetch)
    pub rfsh: bool,      // Refresh cycle active
    pub halt: bool,      // CPU halted (executing phantom fetches)

    // === Input signals (machine → Z80) ===
    pub data_in: u8,     // Data bus for reads (machine sets before next tick)
    pub wait: bool,      // Wait (for +2A/+3 WAIT-based contention)
    pub irq: bool,       // Maskable interrupt request
    pub nmi: bool,       // Non-maskable interrupt (edge-triggered)
}

impl Z80 {
    /// Advance one half-cycle of the master clock.
    ///
    /// After calling:
    /// 1. Inspect output signals (addr, mreq, rd, wr, iorq, etc.)
    /// 2. If mreq && rd: set data_in = memory[addr]
    /// 3. If mreq && wr: memory[addr] = data
    /// 4. If iorq && rd && !m1: set data_in = io_read(addr)
    /// 5. If iorq && wr: io_write(addr, data)
    /// 6. If iorq && m1: set data_in = interrupt_data (IntAck)
    /// 7. Set wait, irq, nmi as appropriate
    pub fn tick(&mut self) { ... }
}

impl Default for Z80 {
    // Reset state: PC=0, SP=0xFFFF, all signals deasserted
}
```

### Half-Cycle State Machine

Each Z80 bus operation decomposes into precise half-cycle states:

**M1 Opcode Fetch (8 half-cycles = 4 T-states):**

| HC | State | Signals Set | Action |
|---|---|---|---|
| 0 | M1_T1_RISE | addr=PC, m1=true | Address on bus |
| 1 | M1_T1_FALL | mreq=true, rd=true | Begin memory read |
| 2 | M1_T2_RISE | (check wait) | Data available — latch data_in |
| 3 | M1_T2_FALL | mreq=false, rd=false | End read, PC++ |
| 4 | M1_T3_RISE | addr=IR, rfsh=true, mreq=true | Refresh address on bus |
| 5 | M1_T3_FALL | | Refresh MREQ active |
| 6 | M1_T4_RISE | mreq=false, rfsh=false | End refresh, R++ |
| 7 | M1_T4_FALL | m1=false, decode | Select MStep sequence |

**Memory Read (6 half-cycles = 3 T-states):**

| HC | State | Signals Set | Action |
|---|---|---|---|
| 0 | MR_T1_RISE | addr=target | Address on bus |
| 1 | MR_T1_FALL | mreq=true, rd=true | Begin read |
| 2 | MR_T2_RISE | (check wait) | Data available — latch data_in |
| 3 | MR_T2_FALL | mreq=false, rd=false | End read |
| 4 | MR_T3_RISE | | Processing |
| 5 | MR_T3_FALL | | M-cycle complete |

**Memory Write (6 half-cycles = 3 T-states):**

| HC | State | Signals Set | Action |
|---|---|---|---|
| 0 | MW_T1_RISE | addr=target | Address on bus |
| 1 | MW_T1_FALL | mreq=true, data=value | Data + MREQ on bus |
| 2 | MW_T2_RISE | wr=true | Begin write |
| 3 | MW_T2_FALL | | Write active |
| 4 | MW_T3_RISE | mreq=false, wr=false | End write |
| 5 | MW_T3_FALL | | M-cycle complete |

**I/O Read (8 half-cycles = 4 T-states):**

| HC | State | Signals Set | Action |
|---|---|---|---|
| 0 | IOR_T1_RISE | addr=port | Port address on bus |
| 1 | IOR_T1_FALL | | |
| 2 | IOR_T2_RISE | iorq=true, rd=true | Begin I/O read |
| 3 | IOR_T2_FALL | (check wait) | I/O device has time to respond |
| 4 | IOR_T3_RISE | | Data available — latch data_in |
| 5 | IOR_T3_FALL | iorq=false, rd=false | End read |
| 6 | IOR_T4_RISE | | Processing |
| 7 | IOR_T4_FALL | | M-cycle complete |

**I/O Write (8 half-cycles = 4 T-states):**

| HC | State | Signals Set | Action |
|---|---|---|---|
| 0 | IOW_T1_RISE | addr=port | Port address on bus |
| 1 | IOW_T1_FALL | data=value | Data on bus |
| 2 | IOW_T2_RISE | iorq=true, wr=true | Begin I/O write |
| 3 | IOW_T2_FALL | | |
| 4 | IOW_T3_RISE | | |
| 5 | IOW_T3_FALL | iorq=false, wr=false | End write |
| 6 | IOW_T4_RISE | | Processing |
| 7 | IOW_T4_FALL | | M-cycle complete |

**Internal (2 half-cycles per T-state, repeated N times):**

| HC | State | Signals Set | Action |
|---|---|---|---|
| 0 | INT_RISE | addr=IR (or context-dependent) | IR on bus, no MREQ |
| 1 | INT_FALL | | Half-cycle complete |

### Contention: How the ULA Gates the CPU

On the 48K, the Ferranti ULA fetches screen data during the active display area. During these fetches, it needs the bus and withholds the CPU's clock. The ULA's `tick()` method:

1. Advances its internal pixel counter
2. If the pixel counter is in a screen-fetch phase: assert screen fetch, deassert `cpu_clock_active`
3. If not in screen-fetch phase: assert `cpu_clock_active`

The contention pattern `[6,5,4,3,2,1,0,0]` emerges naturally: within each 8-T-state (16 HC) character cell, the ULA fetches for 6 T-states and releases for 2. The CPU accumulates a delay of 0-6 T-states depending on when it tries to access contended memory.

But the ULA only gates the CPU when the **CPU is accessing contended memory**. For non-contended accesses, `cpu_clock_active` stays true even during screen fetches. This means the ULA needs to see the CPU's address bus and MREQ signal to decide:

```rust
fn cpu_clock_active(&self) -> bool {
    if !self.in_screen_fetch_phase() {
        return true;  // Not fetching — CPU runs freely
    }
    if !self.cpu_accessing_contended_memory() {
        return true;  // CPU isn't accessing contended RAM — no conflict
    }
    false  // ULA needs the bus, CPU must wait
}
```

The ULA checks `z80.addr` and `z80.mreq` to determine if there's a bus conflict. This is exactly what happens on the silicon: the ULA monitors the address bus and only withholds the clock when both the ULA and CPU want the same memory.

### Crate Structure

```
zilog-z80/                              Z80 half-cycle state machine
  src/
    z80.rs                              Z80 struct, signals, tick()
    registers.rs                        Register file
    walker.rs                           MStep sequence walker
    mcycle.rs                           M-step sequences (~50 static arrays)
    alu.rs                              ALU + flag operations
    decode.rs                           Opcode decode tables

spectrum-common/                        Shared Spectrum types
  src/
    ula.rs                              Ula trait
    memory.rs                           MemoryBus trait
    timing.rs                           FrameTiming, clock constants
    pixels.rs                           Pixel drawing helpers
    palette.rs                          Standard 16-colour palette
    tape.rs                             Tape playback engine (edge state machine)
    audio.rs                            Blip buffer wrapper

ferranti-ula-6c001e/                    Ferranti ULA implementation
  src/
    lib.rs                              FerrantiUla struct
    contention.rs                       Clock gating logic
    video.rs                            Real-time pixel rendering
    interrupt.rs                        Interrupt generation
    floating_bus.rs                     Floating bus tracking
    ear_mic.rs                          EAR/MIC feedback (Issue 1/2/3)

machine-sinclair-zx-spectrum-48k/       48K machine
  src/
    lib.rs                              Spectrum48K struct, run_frame(), handle_bus()
    memory.rs                           Memory48K (ROM + 48K RAM)
    io.rs                               I/O port dispatch

format-tap/                             TAP tape format parser
format-tzx/                             TZX tape format parser

emu-sinclair-zx-spectrum/               Minimal SDL/winit runner
```

### Implementation Phases

#### Phase 1: Workspace + Z80 Half-Cycle State Machine

The Z80 is the longest pole. Build it first, test with Tom Harte.

**1.1 Cargo workspace**

- [ ] Root `Cargo.toml` with `[workspace.dependencies]` and `[workspace.package]`
- [ ] Crate stubs for all crates listed above
- [ ] `test-data/` at workspace root (Tom Harte JSON, FUSE tests, ZEXDOC/ZEXALL)
- [ ] Rust edition 2024, `edition.workspace = true` in each crate
- [ ] Feature flags: `fuse-tests`, `zexall`, `single-step-tests` for slow suites
- [ ] `criterion` or `divan` as workspace dev-dependency for benchmarks from day one
- [ ] `.gitignore` for ROMs, build artefacts, test data

**1.2 Z80 registers (`zilog-z80/src/registers.rs`)**

- [ ] `Registers` struct: AF, BC, DE, HL, AF', BC', DE', HL', IX, IY, SP, PC, I, R, IFF1, IFF2, IM, MEMPTR (WZ)
- [ ] Accessor methods for 8-bit and 16-bit register pairs
- [ ] `impl Default` — Z80 reset state

**1.3 Z80 signals and state (`zilog-z80/src/z80.rs`)**

- [ ] `Z80` struct as specified above (registers + walker state + output signals + input signals)
- [ ] `HalfCycleState` enum — all half-cycle phases for all M-cycle types (M1, MR, MW, IOR, IOW, INT, INTACK)
- [ ] `impl Default` — reset state, all signals deasserted

**1.4 MStep sequences (`zilog-z80/src/mcycle.rs`)**

Port the proven design from the old codebase:

- [ ] `MStep` enum: FetchByte, FetchByteHi, FetchDisp, ReadAddr, ReadAddrHi, WriteAddr, WriteAddrHi, PushHi, PushLo, PopLo, PopHi, IoRead, IoWrite, Internal(u8), IntAck, Execute
- [ ] ~50 static `&[MStep]` sequence arrays for all Z80 instructions
- [ ] Decode functions: `decode_unprefixed`, `decode_ed`, `decode_cb`, `decode_ix`
- [ ] DDCB/FDCB: post-step hooks for sub-opcode and indexed address computation
- [ ] Each MStep knows how many half-cycles it takes (M1=8, MR=6, MW=6, IOR=8, IOW=8, INT=2×N)

**1.5 Half-cycle tick walker (`zilog-z80/src/walker.rs`)**

- [ ] `Z80::tick()` — advance one half-cycle
- [ ] State machine: based on current `HalfCycleState`, set output signals and advance to next state
- [ ] Data latching: when the machine sets `data_in`, the walker picks it up on the correct half-cycle (T2 rise for memory reads, T3 rise for I/O reads)
- [ ] Opcode decode: at M1_T4_FALL, decode the latched opcode byte and select the MStep sequence
- [ ] Prefix handling: CB/DD/ED/FD trigger additional M1 fetches. DD/FD chain. DD+CB enters DDCB mode.
- [ ] Sequence walking: each MStep maps to a sequence of HalfCycleStates
- [ ] Execute step: 0 half-cycles, applies operation using staged data, then moves to next MStep
- [ ] Conditional branches: `walker.done = true` for not-taken path
- [ ] HALT: phantom M1 fetch (address on bus, MREQ active, but result discarded), repeat until interrupt
- [ ] Interrupt check: at end of each instruction. If `irq` and IFF1 set → enter IntAck sequence. If `nmi` edge → enter NMI sequence. EI defers check by one instruction.
- [ ] WAIT handling: if `wait` is asserted on specific half-cycles (T2 rise of MR/MW, TW states), the state machine inserts wait states (stays in current state, doesn't advance). This is for +2A/+3 only — the 48K uses clock gating, not WAIT.

**1.6 ALU and flag operations (`zilog-z80/src/alu.rs`)**

- [ ] All Z80 ALU operations with correct undocumented flag behaviour
- [ ] Block I/O flag formulas (verified against FUSE, z80cpp, SpecIde):

  | Instruction | k formula |
  |---|---|
  | INI | `k = data as u16 + ((C.wrapping_add(1)) as u16)` |
  | IND | `k = data as u16 + ((C.wrapping_sub(1)) as u16)` |
  | OUTI | `k = data as u16 + L_after as u16` (L AFTER HL++) |
  | OUTD | `k = data as u16 + L_after as u16` (L AFTER HL--) |

  Flags: S/Z/bits 3,5 from `B_after`. N from `data bit 7`. H=C from `k > 0xFF`. P = parity of `(k & 7) ^ B_after`.

**1.7 Tom Harte test harness (`zilog-z80/tests/single_step_tests.rs`)**

- [ ] Test driver loop: drives Z80 by calling `tick()` repeatedly, handles bus signals

  ```rust
  fn run_test(z80: &mut Z80, mem: &mut [u8; 65536], max_tstates: u32) {
      let mut hc = 0u32;
      while hc < max_tstates * 2 {  // 2 HC per T-state in test harness
          z80.tick();
          if z80.mreq && z80.rd { z80.data_in = mem[z80.addr as usize]; }
          if z80.mreq && z80.wr { mem[z80.addr as usize] = z80.data; }
          if z80.iorq && z80.rd { z80.data_in = (z80.addr >> 8) as u8; }
          hc += 1;
      }
  }
  ```

  Note: the test harness ticks every half-cycle with no contention. `cpu_clock_active()` is effectively always true. The test harness IS the machine loop — just the simplest possible one.

- [ ] JSON test loader: parse Tom Harte format
- [ ] `#[test] #[ignore] fn run_all()` — 1.6M tests, assert 100%
- [ ] `#[test] #[ignore] fn run_all_ticked()` — same via half-cycle tick interface

**1.8 Benchmarks (`zilog-z80/benches/`)**

- [ ] `bench_tick_nop` — Z80 executing NOPs, measure half-cycles/sec
- [ ] `bench_tick_mixed` — representative instruction mix
- [ ] `bench_decode` — opcode decode throughput

**Phase 1 success criteria:**
- Tom Harte: 1,604,000/1,604,000 (100%)
- Z80 drives correct signals on each half-cycle
- Half-cycle state machine matches expected signal timing for all M-cycle types
- Benchmarks baselined

---

#### Phase 2: Spectrum Common + Ferranti ULA

**2.1 spectrum-common traits and types**

- [ ] `FrameTiming` struct: `halfcycles_per_line`, `lines_per_frame`, `halfcycles_per_frame`, `first_pixel_hc`, `interrupt_start_hc`, `interrupt_length_hc`
- [ ] `TIMING_48K` constant: 448 HC/line, 312 lines, 279552 HC/frame
- [ ] `Ula` trait (revised for ULA-drives model — `tick()`, `cpu_clock_active()`, `interrupt_active()`, `floating_bus()`, etc.)
- [ ] `MemoryBus` trait: `read(addr) -> u8`, `write(addr, val)`, `is_contended(addr) -> bool`
- [ ] Palette: `SPECTRUM_PALETTE: [u32; 16]`
- [ ] Pixel helpers: `draw_standard_cell()`
- [ ] `contention_lookup()` — shared helper for contention gating computation

**2.2 Framebuffer**

- [ ] 352×296 pixels (48+256+48 × 48+192+56+8), 1 byte per pixel (palette index)
- [ ] ULA writes pixels in real-time during `tick()` — no catch_up needed
- [ ] Border pixels: current border colour index (0-7)
- [ ] Screen pixels: palette indices 0-15

**2.3 Ferranti ULA (`ferranti-ula-6c001e/`)**

- [ ] `FerrantiUla` struct: pixel counter (HC position within frame), border colour, beeper state, flash_counter (0-31), current floating bus byte, screen fetch state
- [ ] `tick(&mut self, memory: &dyn MemoryBus, cpu_addr: u16, cpu_mreq: bool, framebuffer: &mut [u8])`:
  - Advance pixel counter
  - If in screen-fetch phase: read screen data byte OR attribute byte from memory (alternating). Track the byte for floating bus.
  - If in active display area: render pixel to framebuffer from current screen data + attribute
  - If in border: render border colour to framebuffer
  - Compute `cpu_clock_active` based on pixel counter phase AND whether CPU is accessing contended memory (`cpu_addr` in 0x4000-0x7FFF AND `cpu_mreq` active)
- [ ] `cpu_clock_active() -> bool`: result of last tick's computation
- [ ] `interrupt_active() -> bool`: true within interrupt window at frame start
- [ ] `floating_bus() -> u8`: the byte the ULA is currently reading (screen or attribute). 0xFF during border/blanking.
- [ ] `write_fe(val: u8)`: update border colour (bits 0-2), beeper state (bit 4), MIC (bit 3)
- [ ] `read_fe(port: u16, keyboard: &[u8; 8]) -> u8`: keyboard rows + EAR bit
- [ ] `end_frame()`: increment flash_counter (mod 32), reset pixel counter
- [ ] Issue 1/2/3: `BoardIssue` enum affecting EAR bit 6 feedback logic. Issue 3 for milestone, all three implemented.
- [ ] Snow effect: detect when Z80's refresh address (visible via `rfsh` signal + addr bus) conflicts with ULA's current screen fetch address. Corrupt the fetched byte.

**2.4 Memory48K**

- [ ] `Memory48K` struct: `rom: [u8; 16384]`, `ram: [u8; 49152]`
- [ ] `read(addr) -> u8`, `write(addr, val)`, `is_contended(addr) -> bool`
- [ ] `load_rom(path)` from `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`
- [ ] `Memory16K` variant (upper RAM reads 0xFF)

**Phase 2 success criteria:**
- ULA `tick()` produces correct pixel output for known screen data
- `cpu_clock_active()` matches FUSE contention tables
- `interrupt_active()` asserts for exactly 32 T-states (64 HC) at frame start
- `floating_bus()` returns correct byte during screen fetch phases

---

#### Phase 3: Machine Integration + FUSE Tests + Boot

**3.1 Spectrum48K machine**

- [ ] `Spectrum48K` struct: `z80: Z80`, `ula: FerrantiUla`, `memory: Memory48K`, `framebuffer: Vec<u8>`, keyboard state, tape state, audio state, `hc: u32`
- [ ] `run_frame()`:

  ```rust
  fn run_frame(&mut self) {
      for _hc in 0..FRAME_HC {
          // 1. ULA ticks — renders, computes clock gating
          self.ula.tick(
              &self.memory,
              self.z80.addr,
              self.z80.mreq,
              &mut self.framebuffer,
          );

          // 2. CPU ticks when ULA allows
          if self.ula.cpu_clock_active() {
              self.z80.tick();
              self.handle_bus();
          }

          // 3. Input signals
          self.z80.irq = self.ula.interrupt_active();

          // 4. Tape advance
          if let Some(ref mut tape) = self.tape {
              tape.advance_hc(1);
          }
      }
      self.ula.end_frame();
  }
  ```

- [ ] `handle_bus()`: inspect Z80 signals, perform memory/IO transactions
  - `mreq && rd` → `z80.data_in = memory.read(z80.addr)`
  - `mreq && wr` → `memory.write(z80.addr, z80.data)`
  - `iorq && rd && !m1` → I/O dispatch: port 0xFE → `ula.read_fe()` with keyboard; unclaimed → `ula.floating_bus()`
  - `iorq && wr` → port 0xFE → `ula.write_fe()` + record beeper delta
  - `iorq && m1` → IntAck: `z80.data_in = 0xFF`

- [ ] I/O contention for 48K (per-T-state, 4 cases):
  
  The ULA's `cpu_clock_active()` already handles memory contention (it sees the address bus and MREQ). For I/O contention, the ULA also needs to see IORQ. The 4-case table:

  | High byte contended? | A0 | Contention behaviour |
  |---|---|---|
  | No | 0 (ULA port) | ULA gates clock for 1 HC at IORQ, then 3 HC during I/O |
  | No | 1 (non-ULA) | No contention (4 T-states uncontended) |
  | Yes | 0 (ULA port) | Contend on address, then contend during I/O |
  | Yes | 1 (non-ULA) | Contend on address only (MREQ never asserts for I/O, but address bus still triggers contention check) |

  The ULA handles this by monitoring both the address bus and the control signals (MREQ, IORQ).

**3.2 FUSE test harness**

1,356 tests at `fuse-emulator-fuse/z80/tests/`. Format documented in the deepened plan v1.

- [ ] Parse `tests.in` + `tests.expected`
- [ ] For each test: fill 64K with `DEADBEEF`, overlay test memory, set registers, run N T-states via the Spectrum48K machine loop (with contention), compare results
- [ ] Verify bus events (MC/MR/MW/PR/PW/PC) match expected sequence
- [ ] `#[test]` per case (1,356 tests)
- [ ] Target: 100% pass

**3.3 ZEXDOC/ZEXALL**

- [ ] Load binary at 0x0100, CP/M BDOS trap at 0x0005
- [ ] `#[test] #[ignore]` — ZEXDOC ~5min, ZEXALL ~30min

**3.4 Boot verification**

- [ ] Load 48K ROM, run ~50 frames
- [ ] Verify copyright message in framebuffer
- [ ] `#[test] #[ignore] fn boot_to_copyright()`

**3.5 Minimal blit window**

- [ ] ~50 lines: open window, blit framebuffer as texture, loop
- [ ] Visual smoke-testing during development

**3.6 Benchmarks**

- [ ] `bench_frame_boot` — one frame of booted ROM
- [ ] `bench_frame_contended` — frame with heavy screen writes
- [ ] `bench_palette_convert` — 104K indexed → RGBA

**Phase 3 success criteria:**
- FUSE: 100% pass (contention timing verified)
- ZEXDOC: pass
- ROM boots to copyright message
- Frame time < 2ms release mode (>500 FPS headroom)

---

#### Phase 4: Audio + Tape

**4.1 Blip buffer audio**

- [ ] `blip_buf` crate integration in `spectrum-common/src/audio.rs`
- [ ] `add_beeper_delta(hc, new_level)` on port 0xFE writes
- [ ] `end_frame(frame_hc) -> &[i16]` for audio callback
- [ ] Beeper amplitude: ±8192

**4.2 Tape engine + format crates**

- [ ] Edge state machine: `next_edge() -> Option<u32>` (T-states until next transition)
- [ ] `advance_hc(1)`: decrement countdown, toggle level at zero
- [ ] TAP parser: `format-tap` crate
- [ ] TZX parser: `format-tzx` crate (blocks 0x10-0x15, 0x20, 0x2A, 0x2B)
- [ ] Signal level tracking: `current_level: bool`, start LOW, toggle per pulse
- [ ] Wire into Spectrum48K: EAR bit from `tape.ear_level()`, suppress EAR/MIC feedback when tape connected

**4.3 Keyboard state**

- [ ] `keyboard: [u8; 8]` — 8 half-rows, active low
- [ ] `read_fe(port)` returns half-row selected by high byte of port address

**Phase 4 success criteria:**
- BEEP command produces clean audio
- TAP/TZX files load via accurate signal generation
- Keyboard matrix works

---

#### Phase 5: Minimal Runner

**5.1 SDL/winit runner**

- [ ] Window: 352×296 (or 2× scaled)
- [ ] Framebuffer blit: palette → RGBA, upload texture
- [ ] Audio: SDL audio callback or cpal, 48kHz
- [ ] Keyboard: PC keyboard → Spectrum 8×5 matrix
- [ ] Frame timing: target 50.08 Hz, sleep between frames

**5.2 Visual verification**

- [ ] Boot screen renders correctly
- [ ] Cursor blinks (FLASH working)
- [ ] Typing at BASIC prompt works
- [ ] BEEP 1,0 produces tone
- [ ] Load a game from TAP

---

## Acceptance Criteria

### Functional

- [ ] Z80: 100% Tom Harte (1,604,000 tests)
- [ ] Z80: ZEXDOC passes
- [ ] Z80: ZEXALL passes (stretch)
- [ ] FUSE contention tests: 100%
- [ ] 48K ROM boots to copyright message
- [ ] Beeper audio works
- [ ] TAP files load via accurate tape signal
- [ ] Keyboard input works

### Non-Functional

- [ ] Frame time < 2ms release mode
- [ ] No `unsafe` (except FFI)
- [ ] Clippy clean
- [ ] All crates compile independently

## Risk Analysis

| Risk | Impact | Mitigation |
|---|---|---|
| Half-cycle Z80 is more complex than T-state | More states, more code | Study SpecIde's Z80.cc for reference. The half-cycle states are mechanical (each T-state decomposes into rise+fall). |
| 279K loop iterations vs 70K — 4× overhead | May not hit 500 FPS | Each iteration is trivial (~10-15ns). 279K × 15ns = 4.2ms worst case. Optimise ULA fast-path (non-fetch HCs). |
| Block I/O repeat flags | 1,996 known failures | Formulas verified against 3 sources. Implement exactly as documented. |
| I/O contention in ULA-drives model | ULA needs to see IORQ | ULA monitors address bus + IORQ + MREQ to compute all 4 contention cases. |
| WAIT-based contention (+2A/+3) differs from clock gating | Two contention mechanisms | Z80 has `wait` input. ULA-drives model handles both: Ferranti gates clock, Amstrad asserts WAIT. |

## References

### Architecture Documents

- Original brainstorm: `docs/brainstorms/2026-04-02-spectrum-variant-aware-architecture-brainstorm.md`
- Architecture revision: `docs/brainstorms/2026-04-03-architecture-revision-ula-drives-brainstorm.md`
- Superseded plan: `docs/plans/2026-04-02-feat-spectrum-48k-boot-milestone-plan.md`

### Reference Emulators

- **SpecIde** (`~/Projects/Emu198x-Unclean/SpecIde/`) — closest to our architecture. Key files: `Spectrum.cc` (main loop), `ULA.cc` (contention), `Z80.cc` (half-cycle state machine)
- **FUSE** (`~/Projects/Emu198x-Unclean/fuse-emulator-fuse/`) — contention tables, test suite
- **z80cpp** (`~/Projects/Emu198x-Unclean/z80cpp/`) — clean Z80 reference

### External

- Tom Harte tests: https://github.com/TomHarte/ProcessorTests
- FUSE: https://fuse-emulator.sourceforge.net/
- Sinclair Wiki: https://sinclair.wiki.zxnet.co.uk/wiki/Contended_memory
- `blip_buf` crate: https://docs.rs/blip_buf/

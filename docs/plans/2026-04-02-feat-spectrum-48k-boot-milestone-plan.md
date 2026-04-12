---
title: "feat: Spectrum 48K boot milestone — cycle-accurate from empty repo to copyright message"
type: feat
date: 2026-04-02
deepened: 2026-04-03
brainstorm: docs/brainstorms/2026-04-02-spectrum-variant-aware-architecture-brainstorm.md
---

# Spectrum 48K Boot Milestone

From empty repo to a 48K ZX Spectrum (Issue 3, Ferranti ULA) that boots to "(C) 1982 Sinclair Research Ltd" with cycle-accurate contention, passing FUSE timing tests and 100% Tom Harte Z80 tests.

## Enhancement Summary

**Deepened on:** 2026-04-03
**Research agents used:** architecture-strategist, performance-oracle, code-simplicity-reviewer, pattern-recognition-specialist, best-practices-researcher (Rust workspace patterns)

### Critical Fix
- **Bus trait defaults must advance the clock** — the original plan had no-op defaults that silently produce incorrect timing. Fixed: defaults advance HC by `cpu_divisor` but skip contention. Requires `fn cpu_divisor(&self) -> u32` on the trait.

### Key Improvements
1. **Performance guardrails:** `#[inline]` on all Bus trait methods, benchmarks from day one, `Option<Z80>` if mem::take overhead exceeds 15% of frame budget
2. **catch_up() call discipline:** call on port 0xFE writes + frame end only, never per-tick (saves 140-210us/frame)
3. **Invariant documented:** Bus methods must never read `self.cpu` during `tick()` — the field holds Default state while CPU is extracted
4. **Shared helpers identified:** `contention_lookup()`, `render_standard_segment()`, `EventQueue<E>` in spectrum-common to prevent duplication across future ULA implementations
5. **Workspace patterns:** `[workspace.dependencies]`, `[workspace.package]` for shared metadata, feature flags for large test suites, test-data/ at workspace root
6. **Minimal blit window pulled to Phase 3** for visual smoke-testing during integration

## Overview

This is the foundational milestone for the Emu198x fresh start. Every architectural decision made in the brainstorm is designed around this target: if the architecture handles Spectrum 48K contention from day one, everything else (128K, +2A/+3, Timex, Pentagon, Scorpion, Next) follows.

The milestone proves:
1. The master oscillator loop works (14MHz, CPU fires every 4th half-cycle)
2. Contention is correct (FUSE timing tests pass)
3. The Z80 is correct (Tom Harte 100%, ZEXDOC passes)
4. The system integrates correctly (ROM boots, copyright message renders)

## Problem Statement

The previous codebase was "fast first, accurate later." Signal Part 3 (Mikropol 1992 demo) proved this approach fails — its interrupt handler is graphics data that only works when cumulative contention over an entire frame is cycle-perfect. Every accuracy retrofit broke something else. The fresh start builds accuracy into the foundation.

## Technical Approach

### Architecture

See the full brainstorm at `docs/brainstorms/2026-04-02-spectrum-variant-aware-architecture-brainstorm.md` for all 20 locked decisions. The key points for this milestone:

- **Master oscillator:** 14MHz crystal, integer half-cycle counting
- **CPU:** Z80 tick walker, 1 T-state per `tick()` call, generic over `Bus` trait
- **Bus trait:** in `zilog-z80` crate, full Z80 bus signals with default no-op contention methods. Spectrum machines override contention. Non-Spectrum machines get free defaults.
- **Machine IS the Bus:** `Spectrum48K` struct implements `Bus`, owns CPU/ULA/Memory as fields
- **ULA as trait:** `FerrantiUla` implements timing, contention, video, interrupt, floating bus
- **Memory as separate trait:** `Memory48K` handles address mapping and contention status
- **Video output:** palette-indexed `u8` per pixel, separate palette-to-RGBA stage
- **Rendering:** event-aware `catch_up()` for mid-scanline border changes
- **Audio:** blip buffer resampling (beeper only for 48K)
- **ROMs:** loaded from `~/.emu198x/roms/sinclair-zx-spectrum-48k/`

### Crate dependency graph

```
zilog-z80                    (Bus trait + Z80 tick walker)
    │
spectrum-common              (Ula trait, Memory trait, FrameTiming, pixel helpers, tape engine)
    │
ferranti-ula-6c001e          (Ferranti ULA: contention, video, interrupt, floating bus)
    │
machine-sinclair-zx-spectrum-48k  (wires Z80 + Ferranti ULA + Memory48K, implements Bus)
    │
emu-sinclair-zx-spectrum     (minimal SDL/winit runner for visual verification)
```

Additionally (independent, built in parallel):
```
format-tap                   (TAP tape format parser)
format-tzx                   (TZX tape format parser)
```

### Implementation Phases

#### Phase 1: Project skeleton + Z80 tick walker (critical path)

This is the longest pole. The Z80 must be correct before anything else can work.

**1.1 Cargo workspace setup**

- [ ] `Cargo.toml` workspace root with `[workspace.dependencies]` for shared dependency versions and `[workspace.package]` for shared metadata (edition, version, license, rust-version)
- [ ] Crate stubs: `zilog-z80`, `spectrum-common`, `ferranti-ula-6c001e`, `machine-sinclair-zx-spectrum-48k`
- [ ] `test-data/` directory at workspace root. Reference from tests via `concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-data/z80/")` for cross-crate access.
- [ ] `.gitignore` for ROMs, build artefacts, test data archives
- [ ] Rust edition 2024, MSRV latest stable. Use `edition.workspace = true` in each crate.
- [ ] Clippy clean, no `#[allow]` without justification
- [ ] Feature flags for slow test suites: `fuse-tests`, `zexall`, `single-step-tests`. Default features include only fast unit tests so `cargo test` stays quick during development.
- [ ] Benchmark harness: add `criterion` or `divan` as a workspace dev-dependency from day one

**1.2 Bus trait (`zilog-z80/src/bus.rs`)**

```rust
/// Z80 bus interface. Each machine provides its own implementation.
/// Time is tracked in half-cycles of the machine's master oscillator.
///
/// INVARIANT: Every method that represents a T-state advances the HC counter
/// by at least `cpu_divisor()` half-cycles. Contention methods may add extra
/// delay BEFORE the base advance (FUSE model: contention first, then T-state cost).
///
/// Default implementations for contention methods advance the clock but skip
/// contention. Machines without contention (MSX, SMS, Pentagon) use the defaults.
/// Spectrum machines override to add contention before the advance.
///
/// CRITICAL INVARIANT: During cpu.tick(&mut bus), the bus's cpu field holds
/// Default state (via std::mem::take). Bus methods must NEVER read self.cpu.
/// All values the bus needs (addresses, data) arrive as method parameters.
pub trait Bus {
    /// Half-cycles per CPU T-state. 4 for 48K Spectrum, 5 for 128K, etc.
    fn cpu_divisor(&self) -> u32;

    /// Current half-cycle position within the frame.
    fn current_hc(&self) -> u32;

    /// Advance the master clock by `hc` half-cycles (internal use).
    fn advance_hc(&mut self, hc: u32);

    /// T1 of a memory M-cycle: apply contention if addr is contended,
    /// then advance 1 T-state. Default: advance without contention.
    #[inline]
    fn contend(&mut self, _addr: u16) {
        self.advance_hc(self.cpu_divisor());
    }

    /// Internal op T-state: contend_no_mreq if applicable,
    /// then advance 1 T-state. Default: advance without contention.
    #[inline]
    fn contend_no_mreq(&mut self, _addr: u16) {
        self.advance_hc(self.cpu_divisor());
    }

    /// Per-T-state I/O contention + advance 1 T-state.
    /// Default: advance without contention.
    #[inline]
    fn contend_io(&mut self, _port: u16, _t: u8) {
        self.advance_hc(self.cpu_divisor());
    }

    /// T2 of M1: read opcode byte + advance 1 T-state.
    fn m1_read(&mut self, addr: u16) -> u8;

    /// T3 of M1: refresh cycle + advance 1 T-state.
    fn refresh(&mut self, addr: u16);

    /// T2 of memory read M-cycle + advance 1 T-state.
    fn read(&mut self, addr: u16) -> u8;

    /// T2 of memory write M-cycle + advance 1 T-state.
    fn write(&mut self, addr: u16, val: u8);

    /// T3 of I/O read M-cycle + advance 1 T-state.
    fn io_read(&mut self, port: u16) -> u8;

    /// T3 of I/O write M-cycle + advance 1 T-state.
    fn io_write(&mut self, port: u16, val: u8);

    /// Data byte during interrupt acknowledge.
    fn interrupt_data(&mut self) -> u8;

    /// Check if a maskable interrupt is pending.
    fn irq_pending(&self) -> bool;

    /// Check if a non-maskable interrupt is pending.
    fn nmi_pending(&self) -> bool;
}
```

**Changes from original plan (post-review):**
- Added `cpu_divisor()` — required for defaults to advance the clock correctly
- Added `advance_hc()` — internal method for clock advancement, used by defaults and overrides
- All contention defaults now advance HC by `cpu_divisor` (not no-ops) — any machine using defaults gets correct timing
- Added `#[inline]` to contention defaults for cross-crate inlining without LTO
- Documented the `self.cpu` invariant: bus methods must never read the CPU field during tick()

For test harnesses (Tom Harte tests), a `FlatBus` implementation provides flat RAM. It can use the default contention methods (which correctly advance HC) — no need to override them.

### Research Insight: Cross-crate inlining

Add `#[inline]` to ALL Bus trait methods in the implementations too, not just the defaults. Without `#[inline]`, Rust does not inline across crate boundaries unless LTO is enabled. Since Bus is in `zilog-z80` and implementations are in machine crates, this annotation is essential for the hot path. Verify in release assembly that `contend()`, `read()`, and `write()` are inlined into the frame loop.

**1.3 Z80 registers and state (`zilog-z80/src/registers.rs`, `zilog-z80/src/state.rs`)**

- [ ] `Registers` struct: AF, BC, DE, HL, AF', BC', DE', HL', IX, IY, SP, PC, I, R, IFF1, IFF2, IM, MEMPTR (WZ)
- [ ] `TickState` struct: current M-step sequence, step index, staged data (data_lo, data_hi, addr), prefix state (none/CB/DD/ED/FD/DDCB/FDCB), done flag
- [ ] `Z80` struct: `Registers` + `TickState` + `halted: bool` + `ei_pending: bool`
- [ ] `impl Default for Z80` — reset state (PC=0, SP=0xFFFF, etc.)

**1.4 M-step sequences (`zilog-z80/src/mcycle.rs`)**

Port the proven design:

- [ ] `MStep` enum: 14 variants (FetchByte, FetchByteHi, FetchDisp, ReadAddr, ReadAddrHi, WriteAddr, WriteAddrHi, PushHi, PushLo, PopLo, PopHi, IoRead, IoWrite, Internal(u8), IntAck, Execute)
- [ ] ~50 static `&[MStep]` sequence arrays for all Z80 instructions
- [ ] Decode functions: `decode_unprefixed(opcode) -> &[MStep]`, `decode_ed(opcode) -> &[MStep]`, `decode_cb(opcode) -> &[MStep]`, `decode_ix(opcode) -> &[MStep]` (DD/FD share, with IX/IY selection)
- [ ] DDCB/FDCB sequences: post-step hooks to save sub-opcode and compute indexed address

**1.5 Tick walker (`zilog-z80/src/tick.rs`)**

- [ ] `Z80::tick<B: Bus>(bus: &mut B)` — processes one T-state of the current M-step
- [ ] M1 opcode fetch: T1 contend, T2 m1_read, T3 refresh+contend, T4 decode → select sequence
- [ ] Prefix detection: CB/DD/ED/FD trigger second M1. DD/FD chain. DD+ED override. DD+CB enters DDCB mode.
- [ ] Sequence walking: each MStep has per-T-state bus calls
- [ ] Execute step: applies operation using staged data, 0 T-states (immediate)
- [ ] Conditional branches: `cs.done = true` for not-taken (JR cc, DJNZ, CALL cc)
- [ ] RET cc: sequence switching after Internal(1) condition check
- [ ] Block repeat ops: switch to non-repeat sequence when done (not cs.done)
- [ ] HALT: phantom M1 fetch (contend at T1, m1_read at T2, advance T3-T4), repeat until interrupt
- [ ] Interrupt check: at end of each instruction (not mid-instruction). Check `irq_pending()` if IFF1 set. Check `nmi_pending()` unconditionally. EI delay: interrupt check deferred by one instruction after EI.
- [ ] Interrupt sequences: IntAck(7T) + Execute(set PC) + PushHi + PushLo. IM2 adds ReadAddr + ReadAddrHi + Execute.
- [ ] NMI: Internal(5) + Execute + PushHi + PushLo

**1.6 ALU and flag operations (`zilog-z80/src/alu.rs`)**

- [ ] All Z80 ALU operations: ADD, ADC, SUB, SBC, AND, OR, XOR, CP, INC, DEC, DAA, RLA, RRA, RLCA, RRCA
- [ ] Rotate/shift: RL, RR, RLC, RRC, SLA, SRA, SRL, SLL (undocumented)
- [ ] Bit operations: BIT, SET, RES
- [ ] Block operations: LDI, LDD, CPI, CPD, INI, IND, OUTI, OUTD + repeat variants
- [ ] Undocumented flag behaviour: bits 3 and 5 of F from various operations, MEMPTR effects on BIT n,(HL)
- [ ] Block I/O flag fixes (the 1,996 Tom Harte failures). The formulas below are verified against FUSE, z80cpp, and SpecIde — all three agree:

  **The key computation** is a value `k` that determines H, C, and P flags:

  | Instruction | k formula |
  |---|---|
  | INI | `k = data as u16 + ((C.wrapping_add(1)) as u16)` |
  | IND | `k = data as u16 + ((C.wrapping_sub(1)) as u16)` |
  | OUTI | `k = data as u16 + L_after as u16` (L AFTER HL++) |
  | OUTD | `k = data as u16 + L_after as u16` (L AFTER HL--) |

  **Flag rules (identical across all 4 instructions):**
  - **S, bits 5, 3:** from `B` after decrement (`b_after & 0xA8`)
  - **Z:** set if `b_after == 0`
  - **N:** bit 7 of the data byte (`data & 0x80`)
  - **H and C:** both set if `k > 0xFF`, both clear otherwise (they are always identical)
  - **P/V:** even parity of `((k as u8) & 0x07) ^ b_after`

  **Critical ordering details:**
  - For INI/IND: B is decremented BEFORE the port read. `data` is the byte from the port. C is unchanged.
  - For OUTI/OUTD: `data` is read from memory at (HL) BEFORE HL changes. B is decremented. HL is incremented/decremented. L in the formula is AFTER the HL adjustment.
  - The repeat variants (INIR, OTDR, etc.) use the same flag logic — they just loop while B != 0.

  Sources: FUSE `z80.pl:236-405`, z80cpp `z80.cpp:667-791`, SpecIde `Z80Ini.h`/`Z80Outi.h`

**1.7 Tom Harte test harness (`zilog-z80/tests/single_step_tests.rs`)**

- [ ] `FlatBus` struct: 64KB RAM array, HC counter, no contention
- [ ] JSON test loader: parse Tom Harte format (initial state → expected state)
- [ ] `#[test] #[ignore] fn run_all()` — iterate all ~1.6M tests, count pass/fail, assert 100%
- [ ] `#[test] #[ignore] fn run_all_ticked()` — same tests via `tick()` interface
- [ ] Per-opcode test filtering for debugging: `cargo test -p zilog-z80 --test single_step_tests -- 0xCB_0x00`

**1.8 Benchmarks (`zilog-z80/benches/`)**

Add from day one using `criterion` or `divan`:

- [ ] `bench_tick_nop` — Z80 executing NOPs in FlatBus, measure ticks/sec (baseline for mem::take overhead)
- [ ] `bench_tick_mixed` — representative instruction mix from FUSE test cases
- [ ] `bench_decode` — opcode decode throughput (verify jump table, not comparison chain)

**Phase 1 success criteria:**
- Tom Harte: 1,604,000/1,604,000 (100%)
- All Z80 instructions decode and execute correctly
- Tick walker advances exactly 1 T-state per `tick()` call
- Benchmarks baselined: tick overhead measured and documented

---

#### Phase 2: Spectrum common + Ferranti ULA

**2.1 spectrum-common traits and types (`spectrum-common/src/`)**

- [ ] `FrameTiming` struct: `halfcycles_per_line`, `lines_per_frame`, `halfcycles_per_frame`, `first_pixel_hc`, `contention_start_hc`, `interrupt_start_hc`, `interrupt_length_hc`, `cpu_divisor`
- [ ] `TIMING_48K` constant: 448 HC/line, 312 lines, 279552 HC/frame, first pixel 14336×4, contention start 14335×4, interrupt at 0, interrupt length 32×4, cpu_divisor 4
- [ ] `Ula` trait (as defined in brainstorm section 2)
- [ ] `MemoryBus` trait: `read(addr) -> u8`, `write(addr, val)`, `is_contended(addr) -> bool`
- [ ] Palette: `SPECTRUM_PALETTE: [u32; 16]` — the 16 standard colours as RGBA
- [ ] Pixel helpers: `draw_standard_cell(pixels: u8, attr: u8, flash_state: bool, output: &mut [u8])` — writes 8 palette indices
- [ ] `EventQueue<E>` — timestamped event queue for mid-scanline state changes (border colour, future copper writes). Use `SmallVec<[E; 8]>` internally — border changes rarely exceed a handful per frame, no heap allocation for the common case.
- [ ] `contention_lookup(hc, start, pattern, divisor) -> u32` — shared helper for contention delay computation, usable by all ULA variants that have contention. The 48K, 128K, and Timex all use the same lookup logic with different pattern arrays and phase offsets.
- [ ] `render_standard_segment(render_hc, target_hc, memory, framebuffer, timing, border_colour, flash_state)` — shared scanline rendering for standard 256×192 display. Used by Ferranti, Sinclair 7K, Amstrad GA, Pentagon, and Scorpion ULAs. Only Timex SCLD overrides for hi-colour/hi-res modes.

**2.2 Framebuffer specification**

The framebuffer covers the full visible area including border:

- [ ] **48K dimensions:** 352×296 pixels (48 border left + 256 screen + 48 border right) × (48 border top + 192 screen + 56 border bottom). This matches the standard PAL visible area.
- [ ] Layout: row-major, 1 byte per pixel (palette index), `352 * 296 = 104,192 bytes`
- [ ] Border pixels: filled with the current border colour index (0-7)
- [ ] Screen pixels: palette indices 0-15 (ink/paper × bright)

**2.3 Ferranti ULA (`ferranti-ula-6c001e/src/`)**

- [ ] `FerrantiUla` struct: border colour, beeper state, flash_counter (0-31, FLASH attribute inverts ink/paper when counter >= 16, toggles every 16 frames = 32-field period), render_hc position, event queue (border change events via `EventQueue`)
- [ ] `contention_delay(hc: u32) -> u32`: lookup in `[6,5,4,3,2,1,0,0]` pattern, phase 0, returns delay in half-cycles (multiply pattern value by `cpu_divisor`)
- [ ] `interrupt_active(hc: u32) -> bool`: true when `hc` is within interrupt window (0..32×cpu_divisor at frame start)
- [ ] `floating_bus(hc: u32, memory: &dyn MemoryBus) -> u8`: per-T-state tracking of ULA fetch cycle. During active display, returns the screen byte or attribute byte the ULA is currently reading. During border/blanking, returns 0xFF.
- [ ] `catch_up(target_hc, memory, framebuffer)`: event-aware incremental rendering
  - Walk from `render_hc` to `target_hc`
  - Check event queue for border-change events before target
  - Render border pixels at current border colour
  - Render screen pixels: compute screen address from HC position, fetch pixel byte + attribute, call `draw_standard_cell()` helper
  - Handle FLASH: invert ink/paper when `flash_counter >= 16` and attribute has FLASH bit set
- [ ] `write_fe(port, val, hc)`: update border colour (bits 0-2), beeper state (bit 4), MIC (bit 3). Push border-change event with HC timestamp.
- [ ] `read_fe(port) -> u8`: keyboard rows (bits 0-4, active low), EAR bit (bit 6). Issue 3: EAR reflects tape signal when connected, else MIC|EAR output feedback.
- [ ] `end_frame()`: increment flash counter (mod 32), reset render_hc, clear event queue
- [ ] Issue 1/2/3 configuration: `BoardIssue` enum affecting EAR bit 6 feedback logic

**2.4 Memory48K (`machine-sinclair-zx-spectrum-48k/src/memory.rs`)**

- [ ] `Memory48K` struct: `rom: [u8; 16384]`, `ram: [u8; 49152]` (3 × 16K banks: bank 5 at 0x4000, bank 2 at 0x8000, bank 0 at 0xC000)
- [ ] `read(addr) -> u8`: ROM at 0x0000-0x3FFF, RAM at 0x4000-0xFFFF
- [ ] `write(addr, val)`: ROM writes ignored, RAM at 0x4000-0xFFFF
- [ ] `is_contended(addr) -> bool`: `addr >= 0x4000 && addr <= 0x7FFF`
- [ ] `load_rom(path: &Path) -> Result<Self>`: load 16K ROM from file. Error if file missing or wrong size.
- [ ] ROM path: `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`
- [ ] 16K model support: `Memory16K` variant with only bank 5 populated, upper RAM reads return 0xFF

**Phase 2 success criteria:**
- `contention_delay` matches FUSE's `ula_contention[]` table for all 69888 T-state positions
- `interrupt_active` returns true for exactly 32 T-states at frame start
- `Memory48K` correctly maps ROM and RAM
- Pixel helpers produce correct palette indices for known attribute/pixel combinations

---

#### Phase 3: Machine integration + FUSE tests

**3.1 Spectrum48K machine (`machine-sinclair-zx-spectrum-48k/src/lib.rs`)**

- [ ] `Spectrum48K` struct: `cpu: Z80`, `ula: FerrantiUla`, `memory: Memory48K`, `hc: u32`, `framebuffer: Vec<u8>`, keyboard state, tape state
- [ ] `impl Bus for Spectrum48K`:
  - `current_hc()` → `self.hc`
  - `contend(addr)` → if `memory.is_contended(addr)` then `hc += ula.contention_delay(hc)`, then `hc += CPU_DIVISOR`
  - `contend_no_mreq(addr)` → same pattern (48K contends internal ops when IR in 0x4000-0x7FFF)
  - `contend_io(port, t)` → 4-case I/O contention table per T-state
  - `m1_read(addr)` → `memory.read(addr)` + advance
  - `refresh(addr)` → contend check on IR + advance
  - `read(addr)` → `memory.read(addr)` + advance
  - `write(addr, val)` → `memory.write(addr, val)` + advance
  - `io_read(port)` → I/O dispatch: port 0xFE (bit 0 = 0) → `ula.read_fe(port)` with keyboard state; unclaimed → `ula.floating_bus(hc, &memory)`
  - `io_write(port, val)` → port 0xFE (bit 0 = 0) → `ula.write_fe(port, val, hc)` + push border-change event + record beeper delta
  - `contend_io(port, t)` → 48K I/O contention per-T-state table (4 cases):

    | High byte contended? | A0 (port bit 0) | T-state pattern |
    |---|---|---|
    | No | 0 (even, ULA) | t=0: advance. t=1: contend+advance. t=2: advance. t=3: advance. |
    | No | 1 (odd) | t=0..3: advance (no contention) |
    | Yes | 0 (even, ULA) | t=0: contend+advance. t=1: contend+advance. t=2: advance. t=3: advance. |
    | Yes | 1 (odd) | t=0..3: contend+advance each |

    "contend" = apply `ula.contention_delay(hc)` before the advance. "advance" = add `cpu_divisor` HC.
  - `interrupt_data()` → `0xFF` (IM1 data bus value on 48K)
  - `irq_pending()` → `ula.interrupt_active(hc)`
  - `nmi_pending()` → `false` (no NMI source on 48K)

- [ ] `run_frame()`:
  ```rust
  fn run_frame(&mut self) {
      while self.hc < FRAME_HC {
          let mut cpu = std::mem::take(&mut self.cpu);
          cpu.tick(self);
          self.cpu = cpu;
      }
      // Render any remaining video up to frame end
      self.ula.catch_up(self.hc, &self.memory, &mut self.framebuffer);
      self.ula.end_frame();
      self.hc -= FRAME_HC;  // carry remainder into next frame
  }
  ```

  **catch_up() call discipline (from performance review):** Do NOT call `catch_up()` per tick — that adds 140-210us/frame of overhead even with an empty event queue (70K calls × 2-3ns bounds check). Instead, call it:
  1. Inside `io_write()` when port 0xFE is written (border colour change) — the event-aware design renders up to the current HC, applies the change, then continues
  2. Once at frame end (to finish rendering the frame)

  During normal operation, port 0xFE writes happen 0-200 times per frame. Cost: effectively zero.

- [ ] Frame boundary mid-instruction: the `while hc < FRAME_HC` check is between `tick()` calls. Since `tick()` does 1 T-state (4 HC + contention), an instruction that spans the frame boundary will complete its current T-state, then the loop exits. The carry-over `hc -= FRAME_HC` handles the overshoot. `catch_up()` must handle HC values up to `FRAME_HC + 24` (worst case: 6 T-states contention × 4 HC = 24 HC overshoot). Document this bound explicitly.

### Research Insight: mem::take performance

The `std::mem::take` pattern copies ~80-100 bytes of Z80 state twice per tick (take + put back). At 70K ticks/frame, that's ~14MB of memcpy per frame — roughly 280-560us, or 15-25% of the 2ms budget.

**Measure this in Phase 1.** Add a benchmark:
```rust
// zilog-z80/benches/tick_overhead.rs
fn bench_frame_nop_tick(c: &mut Criterion) { /* Z80 executing NOPs, measure frame time */ }
fn bench_frame_mixed(c: &mut Criterion) { /* representative instruction mix */ }
```

**If overhead exceeds 15% of frame time**, switch to `Option<Z80>`:
```rust
// Option<Z80> replaces the zero-fill with writing a single discriminant byte
let mut cpu = self.cpu.take().unwrap();
cpu.tick(self);
self.cpu = Some(cpu);
```

This is worth tracking because at 28MHz (Next), the loop runs 4× more iterations — the overhead scales linearly.

**3.2 FUSE test harness (`machine-sinclair-zx-spectrum-48k/tests/fuse_tests.rs`)**

FUSE tests are at `fuse-emulator-fuse/z80/tests/` — two files: `tests.in` (1,356 test cases) and `tests.expected`.

**tests.in format** (per test case):
```
<test_name>                                          ← e.g. "36" or "d3_4"
AF BC DE HL AF' BC' DE' HL' IX IY SP PC MEMPTR       ← 13 hex u16 values
I R IFF1 IFF2 IM halted tstates                      ← I,R hex u8; rest decimal
<addr> <byte1> <byte2> ... -1                         ← memory setup (0 or more lines)
-1                                                   ← end of test case
```

**tests.expected format** (per test case):
```
<test_name>
    <time> <type> <address> [<data>]                  ← bus events (0 or more)
AF BC DE HL AF' BC' DE' HL' IX IY SP PC MEMPTR       ← expected final registers
I R IFF1 IFF2 IM halted tstates                      ← expected final state
<addr> <byte1> <byte2> ... -1                         ← memory CHANGES only
```

**Event types:** `MR` (memory read, has data), `MW` (memory write, has data), `MC` (memory contend, no data), `PR` (port read, has data), `PW` (port write, has data), `PC` (port contend, no data).

**Parsing trick:** `-1` parsed as unsigned hex = `0xFFFFFFFF`. Check `value >= 0x10000` for end-of-memory, `value >= 0x100` for end-of-byte-sequence.

**Background memory:** `coretest.c` fills all 64K with `DE AD BE EF` repeating before each test, then overlays the test's memory lines. Expected memory diff only shows changed bytes.

**Port read stub:** Returns `port >> 8` (high byte of port address). Not real hardware — just a predictable test value.

- [ ] Parse `tests.in` + `tests.expected` into structured test cases
- [ ] For each test case:
  1. Construct `Spectrum48K` with `DEADBEEF`-filled memory + test's memory overlays
  2. Set CPU registers from test initial state
  3. Run until `hc / CPU_DIVISOR >= tstates`
  4. Compare final registers, memory diff, and T-state count against expected values
  5. Optionally verify bus events (MC/MR/MW/PR/PW/PC) against expected event sequence
- [ ] `#[test]` per FUSE test case (1,356 tests — individual test functions are fine at this scale)
- [ ] Track pass/fail count, target: 100%

**3.3 ZEXDOC/ZEXALL harness (`machine-sinclair-zx-spectrum-48k/tests/zex_tests.rs`)**

- [ ] Load ZEXDOC/ZEXALL `.com` binary into memory at address 0x0100
- [ ] Provide a minimal CP/M BDOS trap at address 0x0005 (print character via C register)
- [ ] Run until HALT or completion message appears in output
- [ ] `#[test] #[ignore]` — ZEXDOC takes ~5 minutes, ZEXALL ~30 minutes
- [ ] Parse output for "Tests complete" line and pass/fail count

**3.4 Boot verification (pulled forward from Phase 4)**

The boot test needs only Z80 + ULA + Memory + ROM — no audio, no tape. Running it here (rather than after audio/tape in Phase 4) proves the milestone one phase earlier.

- [ ] Load 48K ROM from `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`
- [ ] Run ~50 frames (~1 second of emulated time)
- [ ] Verify copyright message appears in framebuffer (check known pixel positions)
- [ ] `#[test] #[ignore] fn boot_to_copyright()` — requires ROM file

**3.5 Minimal blit window (pulled forward from Phase 5)**

A bare window that blits the framebuffer helps debug video rendering issues (attribute decoding, screen address calculation) that are hard to catch from unit tests alone. ~50 lines of code.

- [ ] Open a window (SDL2 or winit), blit framebuffer as texture each frame
- [ ] No keyboard, no audio, no tape — just visual output
- [ ] Run `run_frame()` in a loop, display result
- [ ] This is a development tool, not the final runner

**Phase 3 success criteria:**
- FUSE contention tests: 100% pass
- ZEXDOC: all tests pass
- ZEXALL: all tests pass (stretch goal — undocumented flag behaviour)
- `Spectrum48K::run_frame()` completes in reasonable time (~1ms per frame in release mode)
- ROM boots to copyright message (visual verification via blit window)
- Benchmarks baselined: frame time, tick overhead, catch_up cost

---

#### Phase 4: Audio + tape + boot verification

**4.1 Blip buffer audio (`spectrum-common/src/audio.rs`)**

- [ ] Use `blip_buf` crate (Blargg's blip-buf Rust port)
- [ ] `AudioOutput` struct: `BlipBuf`, current amplitude, output sample rate
- [ ] `set_rates(master_hz, sample_rate)` — e.g., `set_rates(14_000_000, 48000)`
- [ ] `add_beeper_delta(hc: u32, new_level: bool)` — compute amplitude change, call `blip.add_delta()`
- [ ] `end_frame(frame_hc: u32) -> &[i16]` — call `blip.end_frame()`, `blip.read_samples()`
- [ ] Beeper amplitude: +/- 8192 (audible but not clipping in 16-bit PCM)

**4.2 Tape engine (`spectrum-common/src/tape.rs`)**

- [ ] `TapePlayer` struct: current block index, edge state machine state, `current_level: bool` (starts LOW), `countdown: u32` (T-states until next edge)
- [ ] `next_edge() -> Option<u32>`: returns T-states until next signal transition. None = tape ended/stopped.
- [ ] `advance(tstates: u32)`: decrement countdown. When zero, toggle `current_level`, fetch next edge duration.
- [ ] `ear_level() -> bool`: returns `current_level`
- [ ] TAP block state machine: pilot tone → sync1 → sync2 → data bits (MSB first, 2 pulses per bit) → pause
- [ ] TZX block state machines: 0x10 (standard), 0x11 (turbo), 0x12 (pure tone), 0x13 (pulse sequence), 0x14 (pure data), 0x15 (direct recording), 0x20 (pause/stop), 0x2A (stop if 48K), 0x2B (set signal level)
- [ ] Signal level tracking: maintain `current_level` across blocks, toggle per pulse, support 0x2B override
- [ ] Pause semantics: pause=0 → stop (set `stopped` flag). Pause>0 → 1ms at current level, then LOW for remainder.

**4.3 Format crates**

- [ ] `format-tap/src/lib.rs`: parse TAP file → `Vec<TapBlock>` (flag byte + data + checksum)
- [ ] `format-tzx/src/lib.rs`: parse TZX file → `Vec<TzxBlock>` (enum per block type)
- [ ] Both crates: no emulation dependency, pure format parsing
- [ ] Test with known TAP/TZX files

**4.4 Integration: wire tape into Spectrum48K**

- [ ] `Spectrum48K` holds optional `TapePlayer`
- [ ] In `io_read` for port 0xFE: if tape connected and playing, bit 6 = `tape.ear_level()`. If not connected, bit 6 = EAR/MIC feedback (issue-dependent).
- [ ] In `run_frame()`: after each `cpu.tick()`, call `tape.advance(tstates_elapsed)` to keep tape in sync with master clock
- [ ] Suppress EAR/MIC feedback when tape is connected

**4.5 Keyboard state**

- [ ] `Spectrum48K` holds a `keyboard: [u8; 8]` array — the 8 half-rows of the keyboard matrix
- [ ] Each byte: bits 0-4 are keys in that half-row (active low: 0 = pressed, 1 = released)
- [ ] `read_fe(port)` returns `keyboard[(port >> 8) as usize]` with appropriate bit masking for half-row selection
- [ ] For FUSE tests: keyboard state is injected directly (tests set specific half-row values)
- [ ] For the runner (Phase 5): PC keyboard mapped to Spectrum matrix

**Phase 4 success criteria:**
- Beeper produces audible, non-aliased square wave output through blip buffer
- TAP files parse correctly (verified against known files)
- TZX files parse correctly (blocks 0x10-0x15, 0x20, 0x2A, 0x2B)
- Tape signal drives EAR bit correctly
- Keyboard matrix correctly maps port reads to key states

---

#### Phase 5: Minimal runner + visual verification

**5.1 SDL/winit runner (`emu-sinclair-zx-spectrum/src/main.rs`)**

- [ ] Window: 352×296 (or 2× scaled: 704×592) with title "ZX Spectrum 48K"
- [ ] Framebuffer blit: convert palette indices → RGBA, upload to GPU texture, draw
- [ ] Audio: SDL audio callback or cpal, 48kHz, 16-bit stereo (mono beeper duplicated to both channels)
- [ ] Keyboard: map PC keyboard → Spectrum 8×5 matrix (at minimum: keys needed to type BASIC commands)
- [ ] Frame timing: target 50.08 Hz (69888 T-states at 3.5MHz), sleep between frames
- [ ] No debugger, no CRT filters, no save states, no menus

### Research Insight: Machine-level benchmarks

Add to `machine-sinclair-zx-spectrum-48k/benches/`:

```rust
fn bench_frame_boot(c: &mut Criterion) { /* one frame of booted ROM */ }
fn bench_frame_contended(c: &mut Criterion) { /* frame with heavy screen writes */ }
fn bench_catch_up_full(c: &mut Criterion) { /* catch_up for full frame render */ }
fn bench_palette_convert(c: &mut Criterion) { /* 104K indexed → RGBA */ }
```

These provide the data to validate every performance assumption. The palette conversion benchmark is especially useful — if it's under 100us (expected), CPU conversion is fine. If the Next's larger framebuffers push it higher, GPU-side palette lookup becomes worthwhile.

**5.2 Visual verification checklist**

- [ ] Boot screen: "(C) 1982 Sinclair Research Ltd" renders correctly
- [ ] Cursor blinks (FLASH attribute working, 16-frame period)
- [ ] Border is white (default)
- [ ] Typing at BASIC prompt produces characters (keyboard input working)
- [ ] `BEEP 1,0` produces audible tone (audio working)
- [ ] `LOAD ""` enters tape loading mode (EAR detection working)
- [ ] Load a simple TAP file (e.g., Manic Miner) — visual confirmation

**Phase 5 success criteria:**
- Window opens, ROM boots, copyright message visible
- Keyboard input works for BASIC interaction
- Audio produces clean beeper sound
- A game loads from TAP file and is playable

---

## Alternative Approaches Considered

1. **Port the old codebase incrementally.** Rejected: the "fast first, accurate later" architecture is fundamentally incompatible with cycle-accurate contention. Every fix was a retrofit.

2. **Start with a simpler machine (e.g., ZX81).** Rejected: the 48K Spectrum is the hardest timing target in the family. If the architecture handles it, everything else follows. Starting simpler would defer the hard problems.

3. **Use an existing Z80 crate.** Rejected: no existing Rust Z80 crate provides per-T-state tick granularity with correct contention bus calls. Our tick walker design is proven against 1.6M tests.

4. **Dynamic dispatch (trait objects) for ULA/Memory.** Rejected: monomorphisation gives zero-cost dispatch. The machine struct is generic over concrete types, not `Box<dyn>`.

## Acceptance Criteria

### Functional Requirements

- [ ] Z80 passes 100% of Tom Harte single-step tests (1,604,000 tests)
- [ ] Z80 passes ZEXDOC (documented Z80 behaviour)
- [ ] Z80 passes ZEXALL (undocumented flag behaviour) — stretch goal
- [ ] FUSE contention timing tests: 100% pass
- [ ] 48K ROM boots to "(C) 1982 Sinclair Research Ltd"
- [ ] Beeper produces correct audio output (BEEP command works)
- [ ] TAP files load via accurate tape signal generation
- [ ] Keyboard input works at BASIC prompt

### Non-Functional Requirements

- [ ] Frame time < 2ms in release mode (>500 FPS headroom)
- [ ] No `unsafe` code (except in FFI boundaries if needed for SDL)
- [ ] Clippy clean, no warnings
- [ ] All crates compile independently (no circular dependencies)

### Quality Gates

- [ ] All `cargo test` pass (non-ignored tests)
- [ ] All `cargo test -- --ignored` pass (Tom Harte, FUSE, ZEX, boot — given ROM availability)
- [ ] `cargo clippy -- -D warnings` clean
- [ ] Each crate has a README with purpose and usage

## Dependencies and Prerequisites

| Dependency | Purpose | Status |
|---|---|---|
| Tom Harte Z80 tests | CPU validation | Available (test-data/) |
| FUSE test files | Contention validation | Available (extract from FUSE source) |
| ZEXDOC/ZEXALL binaries | CPU validation | Available (standard distribution) |
| 48K ROM | Boot verification | User-provided at `~/.emu198x/roms/` |
| `blip_buf` crate | Audio resampling | Available on crates.io |
| SDL2 or winit+wgpu | Window/input/audio | Available on crates.io |
| Rust stable (latest) | Build toolchain | Available |

## Risk Analysis

| Risk | Impact | Mitigation |
|---|---|---|
| Block I/O repeat flags (INIR/OTDR etc.) are hard to get 100% | Delays Tom Harte 100% target | Study z80cpp and FUSE reference implementations. These are the 1,996 failures from the old codebase — known problem, solvable. The undocumented flag formulas involve `temp = data + (C±1)` for IN ops and `temp = data + L` for OUT ops, affecting H and C flags. |
| FUSE test format parsing is non-trivial | Delays contention validation | Parse format early (during Phase 1). The format is text-based with register state + memory dumps + expected T-state counts. Start parsing before the Z80 is complete to de-risk Phase 3. |
| ULA floating bus requires exact per-T-state fetch tracking | Complex to implement correctly | Build alongside contention (they share the same ULA fetch cycle tracking). Not needed for boot, but the architecture is simpler if built together. |
| Audio latency/buffer sizing | Poor audio experience | Use standard 2048-sample buffer at 48kHz (~43ms latency). Acceptable for initial milestone. |
| mem::take overhead at 28MHz (Next) | Frame budget exceeded | At 28MHz the loop runs 4× more iterations — take/put scales linearly. If 15-25% at 3.5MHz, it's 60-100% at 28MHz. Measure in Phase 1 benchmarks. Switch to `Option<Z80>` or pass-individual-refs if needed. |
| Bus trait defaults silently produce incorrect timing | Hard-to-debug timing bugs | **Fixed:** defaults now advance HC. Any machine using defaults gets correct base timing. |
| Cross-crate inlining of Bus methods | 2-5× performance loss if not inlined | Add `#[inline]` to all Bus trait method implementations. Verify in release assembly. |

## Future Considerations

This milestone establishes the foundation. The brainstorm document covers the full roadmap:

- **128K** — Sinclair 7K010E ULA, bank switching, AY-3-8912 sound chip
- **+2A/+3** — Amstrad gate array, different contention, no I/O contention, FDC
- **Timex** — SCLD, 8 video modes, DOCK/EXROM paging
- **Pentagon/Scorpion** — no contention, Beta 128 disk, different frame timing
- **Next** — 28MHz, layer compositor, copper, sprites, DMA, 8K MMU
- **Shell** — debugger, CRT filters, save states, full UI (from old emu198x-shell)

Each is a separate milestone that builds on this foundation.

## References

### Internal References

- Brainstorm: `docs/brainstorms/2026-04-02-spectrum-variant-aware-architecture-brainstorm.md`
- Z80 tick walker design: `memory/project_z80_tick_walker_design.md`
- Spectrum accuracy lessons: `memory/project_spectrum_accuracy_lessons.md`
- Fresh start decisions: `memory/project_fresh_start_decisions.md`

### External References

- Tom Harte Z80 tests: https://github.com/TomHarte/ProcessorTests
- FUSE emulator: https://fuse-emulator.sourceforge.net/
- Sinclair Wiki contention: https://sinclair.wiki.zxnet.co.uk/wiki/Contended_memory
- `blip_buf` crate: https://docs.rs/blip_buf/

### Reference Emulators (~/Projects/Emu198x-Unclean/)

- **SpecIde** — cycle-accurate ULA/Z80 interleaving, closest to our architecture
- **FUSE** — contention tables and test suite (authoritative timing data)
- **zxsp** — broadest variant coverage, per-model ULA subclasses

### Research Insight: SpecIde Architecture Comparison

SpecIde (C++/SFML) is the closest reference. Key files: `Spectrum.cc` (main loop), `ULA.cc` (contention/video), `Z80.cc` (half-cycle state machine).

**Control flow is inverted from ours.** SpecIde's `Spectrum::clock()` is the orchestrator — it clocks the ULA, then conditionally clocks the Z80, and handles all bus transactions by inspecting `z80.state`. Our plan puts the Z80 in control: the CPU calls `bus.read()` etc., and the Bus implementation advances the master clock. Both approaches produce correct timing; ours is more idiomatic Rust (trait-based dispatch vs. external state inspection).

**Contention gating is identical in concept.** SpecIde computes `cpuClock` from `delayTable[pixel & 0x0F]` — a 16-entry boolean table indexed by low 4 bits of the pixel counter. When false, the Z80 doesn't tick. Our `contention_delay()` does the same computation but returns the delay as half-cycles rather than gating a clock signal. Same result, different expression.

**Key lessons from SpecIde:**

1. **The `delayTable[pixel & 0x0F]` pattern** is the cleanest contention lookup. Our `contention_delay()` should use an equivalent approach internally rather than computing from scratch.

2. **+2A/+3 uses the Z80's WAIT_ pin**, not clock gating. SpecIde models both: Ferranti ULA sets `cpuClock = false`, Amstrad GA asserts `SIGNAL_WAIT_`. Our Bus trait abstracts both as "contend adds half-cycles" — simpler but worth documenting the hardware difference.

3. **Snow effect** (ULA reads collide with CPU refresh on the same bus) is modelled per-half-cycle in SpecIde. Not needed for boot milestone, but the architecture shouldn't prevent it. The `refresh(addr)` bus method gives us the hook — the ULA can check if the refresh address conflicts with its current video fetch.

4. **EAR/MIC feedback** uses voltage tables (`voltages[6][4]`) and exponential decay (`vInc = vInc * 93437 / 100000`) for Issue 2/3 differences. More complex than a simple boolean — affects tape loading accuracy for some protected software.

5. **SpecIde's interrupt timing needs a +1 adjustment** because the ULA is clocked before the Z80 (`++interruptStart; ++interruptEnd`). Our inverted control flow (Z80 calls bus, bus queries ULA at current HC) doesn't have this problem — the interrupt check naturally uses the correct HC position.

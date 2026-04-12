# Spectrum Variant-Aware Architecture

**Date:** 2026-04-02
**Status:** Agreed
**First milestone:** 48K boots to "(C) 1982 Sinclair Research Ltd" with correct contention timing

## What We're Building

A ground-up Spectrum emulator core that handles variant differences as a first-class architectural concern — not bolted on later. The target variant family spans the full hardware lineage:

- **Sinclair line:** Issue 1, Issue 2, Issue 3, Spectrum+, 128K, +2 (grey)
- **Amstrad line:** +2A, +2B, +3, +3B
- **Timex:** TC2048, TC2068, TS2068
- **Eastern European clones:** Pentagon 128/256/512/1024, Scorpion ZS-256
- **Modern recreations:** ZX Spectrum Next, TBBlue (future)

The first implementation target is the 48K (Issue 3, Ferranti ULA). But every abstraction boundary is designed to accommodate the full family from day one.

## Why This Approach

The previous codebase was "fast first, accurate later." Every accuracy improvement was a risky retrofit. Signal Part 3 (Mikropol 1992 demo) exposed the fundamental problem: its interrupt handler is graphics data that only works when cumulative contention over an entire frame is cycle-perfect. The architecture must be cycle-accurate from the foundation.

Variant awareness matters because the differences are not cosmetic — they affect the clock tree, contention model, I/O decoding, memory map, and audio subsystem. A 128K isn't a 48K with more RAM. A Pentagon isn't a 128K without contention. The Timex machines add entirely new video modes. Treating these as config flags on a single machine is the path to if-chains and regression.

## Key Decisions

### 1. Master oscillator drives the loop

The tick loop counts **integer half-cycles of the machine's master crystal**. The CPU fires on its divisor. Contention = the CPU's clock slot is skipped. No extra ticks, no catch-up logic, no nanosecond accumulators.

Half-cycles are the finest granularity any component needs. For the 128K family, the AY clock (crystal ÷ 10) is exactly 1 AY tick per 10 half-cycles — no fractional values.

`master_hz` is metadata for audio sample rate conversion only. The tick loop itself is pure integer counting.

#### Verified clock tree (all frequencies from libspectrum + hardware schematics)

| Variant | Crystal (Hz) | CPU ÷ | CPU (Hz) | AY ÷ | AY (Hz) | T/line | Lines | T/frame | 1st pixel T |
|---|---|---|---|---|---|---|---|---|---|
| 48K / TC2048 / Scorpion | 14,000,000 | 4 | 3,500,000 | — | — | 224 | 312 | 69,888 | 14,336 |
| 48K NTSC | 14,110,000 | 4 | 3,527,500 | — | — | 224 | 262 | 58,688 | — |
| TC2068 (PAL) | 14,000,000 | 4 | 3,500,000 | 8 | 1,750,000 | 224 | 312 | 69,888 | 14,336 |
| TS2068 (NTSC) | 14,112,000 | 4 | 3,528,000 | 8 | 1,764,000 | 224 | 262 | 58,688 | 9,169 |
| 128K / +2 | 17,734,475 | 5 | 3,546,895 | 10 | 1,773,447.5 | 228 | 311 | 70,908 | 14,362 |
| +2A / +2B / +3 / +3B | 17,734,475 | 5 | 3,546,895 | 10 | 1,773,447.5 | 228 | 311 | 70,908 | 14,362 |
| Pentagon (all) | 14,336,000 | 4 | 3,584,000 | 8 | 1,792,000 | 224 | 320 | 71,680 | 17,988 |

**Notes:**
- 48K uses a plain 14 MHz crystal. A separate 4.433619 MHz crystal drives the PAL encoder. Two crystals, independent.
- 128K uses 17,734,475 Hz (4× PAL subcarrier = 4 × 4,433,618.75). One crystal replaces both. ÷4 = PAL encoding, ÷5 = CPU. This eliminated the 48K's "dot crawl" artefact.
- libspectrum rounds 128K CPU to 3,546,900 Hz (5 Hz error). We use the exact value 3,546,895 Hz.
- AY clock for 128K family is 1,773,447.5 Hz (÷10 of crystal). Fractional Hz but exact in half-cycle counts: 10 HC per AY tick.
- TS2068's 14.112 MHz crystal chosen so 14,112,000 ÷ 896 = 15,750 Hz (NTSC H-sync). Separate 3.579545 MHz crystal for NTSC colour burst.
- TC2048 first pixel differs by 15 T-states from 48K (14,321 vs 14,336).

### 2. ULA as trait — one implementation per variant

The ULA is the heart of each variant. A `Ula` trait defines the interface. Each ULA variant is a separate implementation — no parameterisation across families, no shared base struct.

```rust
trait Ula {
    /// Compute contention delay in half-cycles at the given frame position.
    fn contention_delay(&self, hc: u32) -> u32;

    /// Is the current interrupt pin asserted?
    fn interrupt_active(&self, hc: u32) -> bool;

    /// Return the byte currently on the ULA's data bus (for floating bus reads).
    /// Tracks per-T-state which byte the ULA is fetching (screen data or attribute).
    fn floating_bus(&self, hc: u32, memory: &dyn MemoryBus) -> u8;

    /// Render video up to the given half-cycle position.
    /// Uses shared pixel-drawing helpers from spectrum-common for standard display mode.
    fn catch_up(&mut self, hc: u32, memory: &dyn MemoryBus, framebuffer: &mut [u8]);

    /// Read from ULA-owned I/O (port 0xFE: keyboard rows, EAR bit, issue-specific feedback).
    fn read_fe(&self, port: u16) -> u8;

    /// Write to ULA-owned I/O (port 0xFE: border colour, MIC/EAR bits).
    fn write_fe(&mut self, port: u16, val: u8, hc: u32);

    /// Return the frame timing constants for this ULA.
    fn frame_timing(&self) -> &FrameTiming;
}
```

ULA implementations:
- **Ferranti 6C001E** — 48K/Spectrum+, with Issue 1/2/3 board-level differences (EAR bit 6 feedback, capacitor, floating bus)
- **Sinclair 7K010E** — 128K/+2
- **Amstrad 40077** — +2A/+2B/+3/+3B (no I/O contention, different contention pattern [1,0,7,6,5,4,3,2], different contended bank rules)
- **Timex SCLD** — TC2048/TC2068/TS2068 (8 video modes, full I/O decoding, DOCK/EXROM paging via port 0xF4/0xFF)
- **Pentagon ULA** — no contention (always returns 0 delay), 224 T/line × 320 lines, 3.584 MHz
- **Scorpion ULA** — separate from Pentagon (different frame timing: 224 × 312, 3.5 MHz, 4 ROMs, secondary paging port)

Pentagon and Scorpion get **separate implementations** despite similarity — they'll diverge with Pentagon 512/1024 extensions and Scorpion's secondary paging port.

Rendering: `catch_up()` stays inside the Ula trait (rendering and contention are inseparable on the silicon — the ULA's screen fetches cause contention). Shared pixel-drawing helpers (e.g. `draw_standard_cell()`, `draw_hicolour_cell()`) live in `spectrum-common` to avoid duplicating standard display rendering across 6 ULA implementations.

### 3. Memory is a separate trait from the ULA

The ULA handles *timing/video/contention pattern*. Memory handles *address mapping/bank switching/contention status*.

- Memory answers: "is this address in a contended bank?" (`is_contended(addr) -> bool`)
- ULA answers: "what's the delay at this half-cycle?" (`contention_delay(hc) -> u32`)
- The Bus combines both: if memory says contended AND ULA says delay > 0, advance the clock

This separation means:
- Pentagon = "standard video output, contention always returns 0" — no need to duplicate banking logic in the ULA
- Timex MMU (port 0xF4 HOME/DOCK/EXROM paging) is a memory concern, not a video concern
- 128K bank switching and +3 special paging mode are memory implementations, not ULA implementations
- Contention on 0xC000-0xFFFF (128K with odd bank paged, +2A/+3 with bank 4-7 paged) is a memory.is_contended() concern

### 4. Bus trait — Z80-specific, lives in the CPU crate

The Bus trait maps to Z80 bus signals (MREQ, IORQ, M1, RFSH). It lives in `zilog-z80` and is reusable across any Z80-based machine (Spectrum, MSX, CPC, SMS).

**Every bus method that represents a T-state advances the HC counter by `cpu_divisor` half-cycles**, plus any contention delay. The CPU never advances the clock directly.

**Contention is applied BEFORE the T-state's base cost** (verified against FUSE source: `tstates += ula_contention[tstates]; tstates += time;`).

```rust
/// Z80 bus interface. Each machine provides its own implementation.
/// Time is tracked in half-cycles of the machine's master oscillator.
///
/// INVARIANT: Every method representing a T-state advances HC by at least cpu_divisor().
/// Contention methods add extra delay BEFORE the base advance.
/// Default contention methods advance the clock but skip contention — machines without
/// contention (Pentagon, MSX, SMS) use the defaults and get correct base timing.
///
/// CRITICAL: During cpu.tick(&mut bus), the bus's cpu field holds Default state
/// (via std::mem::take). Bus methods must NEVER read self.cpu.
pub trait Bus {
    /// Half-cycles per CPU T-state (4 for 48K, 5 for 128K, etc.)
    fn cpu_divisor(&self) -> u32;

    /// Current half-cycle position within the frame.
    fn current_hc(&self) -> u32;

    /// Advance the master clock by `hc` half-cycles.
    fn advance_hc(&mut self, hc: u32);

    /// T1 of a memory M-cycle: apply contention + advance 1 T-state.
    /// Default: advance without contention.
    #[inline]
    fn contend(&mut self, _addr: u16) { self.advance_hc(self.cpu_divisor()); }

    /// Internal op T-state: contend_no_mreq + advance 1 T-state.
    /// Default: advance without contention.
    #[inline]
    fn contend_no_mreq(&mut self, _addr: u16) { self.advance_hc(self.cpu_divisor()); }

    /// Per-T-state I/O contention + advance 1 T-state.
    /// Default: advance without contention.
    #[inline]
    fn contend_io(&mut self, _port: u16, _t: u8) { self.advance_hc(self.cpu_divisor()); }

    /// T2 of M1: read opcode byte + advance 1 T-state.
    fn m1_read(&mut self, addr: u16) -> u8;

    /// T3 of M1: refresh cycle + advance 1 T-state.
    fn refresh(&mut self, addr: u16);

    /// T2 of a memory read M-cycle + advance 1 T-state.
    fn read(&mut self, addr: u16) -> u8;

    /// T2 of a memory write M-cycle + advance 1 T-state.
    fn write(&mut self, addr: u16, val: u8);

    /// T3 of an I/O read M-cycle + advance 1 T-state.
    fn io_read(&mut self, port: u16) -> u8;

    /// T3 of an I/O write M-cycle + advance 1 T-state.
    fn io_write(&mut self, port: u16, val: u8);

    /// Data byte during interrupt acknowledge M-cycle.
    fn interrupt_data(&mut self) -> u8;

    /// Check if a maskable interrupt is pending.
    fn irq_pending(&self) -> bool;

    /// Check if a non-maskable interrupt is pending.
    fn nmi_pending(&self) -> bool;
}
```

**Updated 2026-04-03:** Added `cpu_divisor()`, `advance_hc()`, default contention implementations that advance the clock, `#[inline]` annotations, and the `self.cpu` invariant. See plan for rationale.

### 5. Machine struct IS the Bus

The machine struct (e.g. `Spectrum48K`) implements the Bus trait directly. It owns the CPU, ULA, Memory, and peripherals as fields. This avoids Rust borrow-checker issues — between `cpu.tick()` calls, the machine accesses its own fields freely.

The CPU is temporarily taken out of the machine for each tick (via `std::mem::take`) — it's a pure state machine (registers + tick walker state), so this is a cheap memcpy.

```rust
struct Spectrum48K {
    cpu: Z80,
    ula: FerrantiUla,
    memory: Memory48K,
    hc: u32,
    // peripherals, audio state, etc.
}

impl Bus for Spectrum48K {
    fn contend(&mut self, addr: u16) {
        if self.memory.is_contended(addr) {
            let delay = self.ula.contention_delay(self.hc);
            self.hc += delay;
        }
        self.hc += CPU_DIVISOR;  // 4 HC for 48K
    }

    fn read(&mut self, addr: u16) -> u8 {
        let val = self.memory.read(addr);
        self.hc += CPU_DIVISOR;
        val
    }

    // ... etc
}

fn run_frame(&mut self) {
    while self.hc < FRAME_HC {
        let mut cpu = std::mem::take(&mut self.cpu);
        cpu.tick(self);  // self is &mut Bus
        self.cpu = cpu;
        self.ula.catch_up(self.hc, &self.memory, &mut self.framebuffer);
    }
    self.hc -= FRAME_HC;  // carry remainder into next frame
}
```

### 6. Full board-level modelling of Issue 1/2/3 differences

Each board issue is a configuration of the Ferranti ULA:
- **Issue 1:** Different EAR bit 6 feedback (no diode on EAR circuit), different MIC output behaviour
- **Issue 2:** Added capacitor on EAR input, changed MIC/EAR interaction
- **Issue 3:** "Standard" behaviour, most common

These affect tape loading edge cases and some software detection routines (games that detect the board issue).

### 7. Full per-T-state floating bus tracking

The ULA fetches screen data in a known 8-T-state pattern during active display. Each ULA implementation tracks which byte it's currently fetching. `floating_bus(hc)` returns:
- During active display: the screen or attribute byte being fetched at that exact T-state
- During border/blanking: 0xFF (data bus pulled high)

Games use this for timing loops and some copy protection relies on specific floating bus values.

### 8. Audio: beeper in ULA, AY as peripheral, blip buffer resampling

The beeper is part of the ULA (bit 4 of port 0xFE write). The AY-3-8912 is an I/O peripheral:

| Variant | Audio sources |
|---|---|
| 48K / TC2048 | Beeper only |
| 128K / +2 / +2A / +3 | Beeper + AY-3-8912 |
| TC2068 / TS2068 | Beeper + AY-3-8912 (different I/O ports: 0xF5/0xF6 vs 0xFFFD/0xBFFD) |
| Pentagon / Scorpion | Beeper + AY-3-8912 (+ optional Covox DAC) |

The AY is its own crate (`general-instruments-ay-3-8912`), reusable across Spectrum, MSX, Atari ST, Amstrad CPC.

**Audio resampling:** blip buffer (band-limited synthesis). Each state change is recorded as a delta at a half-cycle timestamp. At frame end, the blip buffer resamples to 44100/48000 Hz with anti-aliasing. This produces the warm, correct sound of real hardware through a TV speaker — no aliasing artefacts from the square waves.

### 9. Bus dispatches I/O by priority

On I/O read/write, the Bus implementation walks its device list in priority order. Each device claims ports via address matching. First claim wins. Unclaimed reads return the floating bus value.

This mirrors real hardware partial decoding:
- **48K:** Bit 0 = 0 → ULA (any even port)
- **128K:** 0x7FFD → paging latch, 0xFFFD/0xBFFD → AY
- **Timex:** Full decode — SCLD responds only to exact ports 0xF4, 0xF5, 0xF6, 0xFE, 0xFF
- **Pentagon:** Port 0x1F → Beta disk (priority) / Kempston joystick

### 10. ROMs are part of the Memory implementation

Each Memory implementation knows which ROMs it needs and how they're switched:

| Variant | ROMs | Switching |
|---|---|---|
| 48K | 1 × 16K | Fixed |
| 128K / +2 | 2 × 16K | Bit 4 of port 0x7FFD |
| +2A / +3 | 4 × 16K | Bits from 0x7FFD + 0x1FFD |
| Pentagon | 3 × 16K | Bit 4 of 0x7FFD + Beta disk ROM |
| Scorpion | 4 × 16K | Bit 4 of 0x7FFD + 0x1FFD bit 1 override + shadow monitor (NMI) |
| Timex | 16K HOME + 8K EXROM chunks | Port 0xF4 + bit 7 of 0xFF |

No central ROM manager (YAGNI for now).

### 11. Contention model reference

#### 48K (Ferranti 6C001E)
- Pattern: `[6, 5, 4, 3, 2, 1, 0, 0]` repeating every 8 T-states, phase 0
- Contended range: 0x4000-0x7FFF
- I/O contention: 4 cases (high byte contended × even/odd port), per-T-state
- Internal ops (IR on bus): contended if IR in 0x4000-0x7FFF

#### 128K (Sinclair 7K010E)
- Pattern: `[6, 5, 4, 3, 2, 1, 0, 0]`, phase 1
- Contended: 0x4000-0x7FFF always, 0xC000-0xFFFF when odd bank (1,3,5,7) paged
- I/O contention: same 4 cases as 48K
- Internal ops: contended if address in contended range

#### +2A/+3 (Amstrad 40077)
- Pattern: `[1, 0, 7, 6, 5, 4, 3, 2]` (DIFFERENT)
- Contended: 0x4000-0x7FFF always, 0xC000-0xFFFF when bank 4,5,6,7 paged (NOT odd banks)
- **No I/O contention** (MREQ-only gate array)
- **No internal T-state contention** (MREQ not active during internal ops)

#### Pentagon / Scorpion
- **No contention at all.** `contention_delay()` always returns 0.

#### Timex (SCLD)
- Same contention pattern as 48K Ferranti: `[6, 5, 4, 3, 2, 1, 0, 0]`
- Applied to 0x4000-0x7FFF regardless of which bank (HOME/DOCK/EXROM) is mapped there

#### I/O contention per-T-state table (48K/128K only)

| High byte contended? | A0 | Per-T-state pattern |
|---|---|---|
| No | 0 (even) | N:1, C:3 |
| No | 1 (odd) | N:4 |
| Yes | 0 (even) | C:1, C:3 |
| Yes | 1 (odd) | C:1, C:1, C:1, C:1 |

(N = no contention applied, just advance. C = apply contention delay.)

### 12. Crate naming convention

Following the established convention with full product names:

| Type | Pattern | Examples |
|------|---------|----------|
| CPU | `{manufacturer}-{chip}` | `zilog-z80` |
| ULA/chips | `{manufacturer}-{chip}` | `ferranti-ula-6c001e`, `general-instruments-ay-3-8912` |
| Common traits | `spectrum-common` | Shared Bus trait (re-export from `zilog-z80`), ULA trait, Memory trait, FrameTiming, pixel helpers |
| Machines | `machine-{mfr}-{system}` | `machine-sinclair-zx-spectrum-48k`, `machine-sinclair-zx-spectrum-128k` |
| Machines (Amstrad) | `machine-{mfr}-{system}` | `machine-amstrad-zx-spectrum-plus2a`, `machine-amstrad-zx-spectrum-plus3` |
| Machines (clones) | `machine-{mfr}-{system}` | `machine-timex-tc2048`, `machine-timex-ts2068`, `machine-pentagon-128`, `machine-scorpion-zs256` |

### 13. Z80 tick walker design (ported, not copied)

The proven M-step sequence walker from the old codebase:
- 14 MStep types: FetchByte, FetchByteHi, FetchDisp, ReadAddr, ReadAddrHi, WriteAddr, WriteAddrHi, PushHi, PushLo, PopLo, PopHi, IoRead, IoWrite, Internal(n), IntAck, Execute
- ~50 static sequence arrays covering all Z80 instructions including DD/FD/ED/CB/DDCB/FDCB prefixes
- Staged execution: MSteps populate cs.data_lo/data_hi/addr, Execute consumes them
- Execute is 0 T-states (processed immediately by try_complete_step)
- Conditional branches use truncation (cs.done) for not-taken path
- Block repeat ops switch to non-repeat sequence when done

**What changes for the fresh start:**
- Each bus call advances HC by cpu_divisor (not a fixed "1 T-state")
- No "fallback countdown" for atomic prefix ops — all instructions fully sequenced
- No "contention catch-up" — contention is handled inside bus methods
- The CPU holds no clock state — the Bus owns the HC counter

### 14. Test strategy — cargo test + fixtures from day one

- **Tom Harte:** 1.6M Z80 tests as `#[ignore]` cargo tests. Target: 100% (current: 99.88%, 1996 failures in block I/O repeat flags)
- **FUSE:** Per-case unit tests against bus/ULA contention timing. Parse FUSE test format, run each case, compare T-state count.
- **ZEXALL/ZEXDOC:** Integration tests that boot a snapshot and run to completion. ZEXDOC first (documented behaviour), then ZEXALL (undocumented flags).
- **Signal Part 3:** The acid test — TZX loads, music plays, VU meters pulse. Manual verification initially.

### 15. Core only — no shell yet

Build the emulation core first. Test via cargo tests. A minimal SDL/winit harness for visual verification only (window + framebuffer blit + keyboard + audio callback). No debugger, no CRT filters, no save states, no fancy UI.

## Architecture Summary

```
Master Crystal (14 MHz / 17.7 MHz / 14.336 MHz / ...)
    │
    ├── Machine struct (implements Bus trait)
    │   ├── HC counter (half-cycles, frame-relative)
    │   ├── Contention arbitration: Memory.is_contended() + ULA.contention_delay()
    │   ├── I/O dispatch: priority-ordered device list
    │   └── Interrupt delivery: ULA.interrupt_active() → CPU
    │
    ├── Z80 (tick walker, 1 T-state per tick)
    │   ├── M-step sequence arrays (~50 static sequences)
    │   ├── Staged execution (data_lo / data_hi / addr)
    │   └── Prefix handling (CB/DD/ED/FD/DDCB/FDCB)
    │
    ├── ULA (variant-specific implementation)
    │   ├── Contention pattern + delay lookup
    │   ├── Video rendering (catch_up to current HC)
    │   ├── Floating bus tracking (per-T-state fetch state)
    │   ├── Interrupt generation (at fixed HC position)
    │   ├── Beeper state (port 0xFE bit 4)
    │   └── Shared pixel helpers from spectrum-common
    │
    ├── Memory (variant-specific implementation)
    │   ├── ROM banks (loaded at construction)
    │   ├── RAM banks (variant-specific count and size)
    │   ├── Bank switching (via I/O writes routed by bus)
    │   ├── is_contended(addr) → bool
    │   └── Timex DOCK/EXROM paging (TC2048/TC2068/TS2068)
    │
    ├── Audio
    │   ├── Blip buffer (band-limited resampling, HC timestamps → 44.1/48 kHz)
    │   ├── Beeper deltas (from ULA port 0xFE writes)
    │   └── AY deltas (from AY peripheral tick)
    │
    └── Peripherals (optional, bus-connected via I/O dispatch)
        ├── AY-3-8912 (128K+, Timex TC2068/TS2068)
        ├── FDC (+3, uPD765A)
        ├── Beta 128 disk interface (Pentagon, Scorpion)
        ├── Kempston joystick
        └── Covox DAC (Pentagon/Scorpion, optional)
```

## Timex SCLD Video Modes (reference)

| Mode | Bits 0-2 | Resolution | Pixel source | Colour source |
|---|---|---|---|---|
| STANDARD | 0x00 | 256×192 | Screen 0 (0x4000) | Attributes (0x5800), 8×8 blocks |
| ALTDFILE | 0x01 | 256×192 | Screen 1 (0x6000) | Attributes (0x7800), 8×8 blocks |
| EXTCOLOUR | 0x02 | 256×192 | Screen 0 (0x4000) | Screen 1 data area (0x6000), 8×1 blocks |
| EXTCOLALTD | 0x03 | 256×192 | Screen 1 (0x6000) | Screen 0 data area (0x4000), 8×1 blocks |
| HIRESATTR | 0x04 | 512×192 | Interleaved S0+S1 | Standard attributes |
| HIRESATTRALTD | 0x05 | 512×192 | Interleaved S0+S1 | Alternate attributes |
| HIRES | 0x06 | 512×192 | Interleaved S0+S1 | Fixed palette (bits 3-5 of port 0xFF) |
| HIRESDOUBLECOL | 0x07 | 512×192 | Interleaved S0+S1 | Doubled columns |

## Reference Emulators (~/Projects/Emu198x-Unclean/)

| Emulator | Language | Use as reference for |
|---|---|---|
| **SpecIde** | C++ (SFML) | Cycle-accurate ULA/Z80 interleaving on alternating half-cycles. Closest to our architecture. Supports 48K Issue 2/3, 128K, +2, +2A, +3, Pentagon. |
| **FUSE** | C | Contention lookup tables, test suite, I/O timing data. Gold standard timing reference. Event-driven architecture (not our approach). |
| **zxsp** | C++ (Qt) | Broadest variant coverage (48K, 128K, +2, +2A, +3, TC2048/2068, TS2068, Jupiter Ace). Per-model ULA subclasses. |
| **z80cpp** | C++ | Clean standalone Z80 core with MEMPTR and undocumented flags. Used by ESPectrum. |
| **zxian** | C (SDL) | Small readable codebase with explicit contention callbacks. Good for cross-checking. |
| **ESPectrum** | C++ (ESP32) | Claims 100% cycle accuracy on embedded hardware. Uses z80cpp core. |
| **specemu** | Binary only | Most accurate Spectrum emulator by reputation. No source available — run for comparison only. |

### 16. Beta disk interface (Pentagon / Scorpion)

**Hardware:** WD1793 FDC (Soviet clone: KR1818VG93). Five I/O ports, only accessible when TR-DOS ROM is paged in:

| Port | Read | Write |
|---|---|---|
| 0x1F | Status register | Command register |
| 0x3F | Track register | Track register |
| 0x5F | Sector register | Sector register |
| 0x7F | Data register | Data register |
| 0xFF | System status (DRQ bit 6, INTRQ bit 7) | System control (drive select, side, density, reset) |

**ROM auto-paging (critical):** The Beta disk pages its ROM in/out based on M1 fetch address:
- **Page in:** Opcode fetch from 0x3D00-0x3DFF (when 48K BASIC ROM is visible)
- **Page out:** Opcode fetch from 0x4000+ (any RAM address)

This check happens in `m1_read()` on every opcode fetch. On machines without Beta disk it's a no-op. On Pentagon/Scorpion it's a comparison + possible ROM swap.

**Interrupts:** The FDC does NOT generate Z80 interrupts. DRQ and INTRQ are polled via port 0xFF bits 6-7. No interrupt wiring needed.

**Disk formats:** TRD (raw sector dump: 16 sectors/track, 256 bytes/sector, double-sided, 80 tracks = 655,360 bytes) and SCL (compact container, built into TRD image at load time).

**Pentagon vs Scorpion:** The disk interface itself is identical. Only the ROM bank numbering differs (Pentagon: 3 ROMs; Scorpion: 4 ROMs with different slot for TR-DOS).

### 17. Tape system

**Architecture:** Split into format crates (parsers) and tape engine (playback/recording).

- **Format crates:** `format-tap`, `format-tzx`, `format-pzx` — parse tape container files into a common block representation
- **Tape engine:** In `spectrum-common` — edge-based state machine, motor control, EAR/MIC signal

**Edge state machine:** Each tape block type has a `next_edge() -> Option<u32>` method returning T-states until the next signal transition. The master clock decrements the countdown; when it hits zero, toggle EAR level and fetch next edge. No separate tape timer — the master oscillator drives everything.

**Standard ROM timing constants (for TAP and TZX block 0x10):**

| Parameter | T-states |
|---|---|
| Pilot pulse | 2168 |
| Sync pulse 1 | 667 |
| Sync pulse 2 | 735 |
| Zero bit pulse | 855 |
| One bit pulse | 1710 |
| Header pilot count | 8063 |
| Data pilot count | 3223 |

**TZX block priority:** 0x10 (standard speed), 0x11 (turbo), 0x12 (pure tone), 0x13 (pulse sequence), 0x14 (pure data), 0x15 (direct recording), 0x20 (pause/stop). These cover 95%+ of TZX files.

**Loading mode:** Accurate signal generation only at first. No ROM traps (instant loading) — traps risk masking timing bugs. Add traps later as a toggle-able quality-of-life feature.

**Saving:** Via ROM trap initially (intercept at 0x04C2, write TAP block from registers/memory). Signal capture for custom savers can come later.

**EAR/MIC loopback:** When no tape is connected, port 0xFE bits 3-4 (output) feed back to bit 6 (input) through ULA pin 28. The exact behaviour is issue-dependent (already captured in board-level modelling). When tape is connected, tape signal overrides the feedback.

### 18. ZX Spectrum Next — architecture fit assessment

The Next is a superset that configures itself to behave like any classic model. Our architecture accommodates it with four additions:

**What fits cleanly:**
- **Z80N instructions:** ~30 new ED-prefix ops (MUL, barrel shifts, NEXTREG, pixel helpers). Pure CPU additions, no bus changes except NEXTREG (a special register write).
- **Memory:** 8K MMU with 160 banks is a more capable memory mapper behind the same trait. Legacy 128K paging becomes a compatibility shim writing to the same underlying MMU. Port 0x7FFD and NextReg MMU (0x50-0x57) are synchronised — most recent write wins.
- **Clock speeds:** 28MHz master crystal. Speeds 3.5/7/14/28MHz are divisors (8/4/2/1). At 3.5MHz, full ULA contention applies. At 7MHz+, contention is disabled. The master-oscillator model handles this — contention_delay() returns 0 when speed > 3.5MHz.
- **Compatibility modes:** Register 0x03 selects machine type (48K/128K/+3/Pentagon) and display timing independently. Our existing per-model ULA timing tables are reused directly.
- **DMA (Z8410 subset):** Bus-level arbitration — in continuous mode, DMA steals bus cycles from CPU (master loop ticks DMA instead of CPU). DMA reads/writes through the same Bus trait. Same pattern as Amiga blitter.

**What needs new components:**
1. **Layer compositor:** ULA becomes one of four display layers (ULA, Layer 2, Tilemap, Sprites). Each renders independently; a compositor combines per-pixel according to priority register 0x15 (6 orderings + blending modes).
2. **Copper co-processor:** 1024 instructions, WAIT (raster position) + MOVE (write NextReg). Fires at pixel clock speed, independent of CPU. Writes registers mid-scanline.
3. **NextReg bank:** ~100+ registers (0x00-0xFF) controlling all Next-specific hardware. Central configuration hub. Two writers: CPU (via ports 0x243B/0x253B or NEXTREG instruction) and copper.

### 19. Video output: palette-indexed pixels

The ULA outputs **palette index (u8)** per pixel, not RGBA:
- Classic Spectrum: indices 0-15 (8 colours × bright/normal)
- Next enhanced ULA: indices 0-255 (programmable 9-bit RGB palette)
- Transparency: `index == transparent_colour` (configurable on Next, not applicable on classic)

A separate palette lookup stage converts indices to RGBA for display. This naturally becomes one layer in the Next's compositor. All ULA implementations share this output format.

### 20. Event-aware catch_up() rendering

`catch_up(target_hc)` renders video incrementally, checking for mid-scanline events between its current position and the target HC:

```rust
fn catch_up(&mut self, target_hc: u32, ...) {
    while self.render_hc < target_hc {
        let next_event = self.next_event_before(target_hc);
        let render_to = next_event.map(|e| e.hc).unwrap_or(target_hc);
        self.render_segment(self.render_hc, render_to, ...);
        if let Some(event) = next_event {
            self.apply_event(event);
        }
        self.render_hc = render_to;
    }
}
```

**Why this matters even for classic Spectrums:** Port 0xFE writes change border colour mid-scanline. Games and demos (Aquaplane, many multicolour effects) depend on the border change taking effect at the exact T-state of the write. The event-aware design handles this naturally.

For the 48K milestone, `next_event_before()` returns border-change events from port 0xFE writes. For the Next, it additionally returns copper MOVE events targeting video registers. Zero overhead when no events are pending (single render pass).

## Architecture Summary

```
Master Crystal (14 MHz / 17.7 MHz / 14.336 MHz / 28 MHz)
    │
    ├── Machine struct (implements Bus trait)
    │   ├── HC counter (half-cycles, frame-relative)
    │   ├── Contention arbitration: Memory.is_contended() + ULA.contention_delay()
    │   ├── I/O dispatch: priority-ordered device list
    │   ├── Interrupt delivery: ULA.interrupt_active() → CPU
    │   ├── Beta disk M1 paging hook (Pentagon/Scorpion)
    │   └── DMA bus arbitration (Next: DMA steals CPU cycles)
    │
    ├── Z80 / Z80N (tick walker, 1 T-state per tick)
    │   ├── M-step sequence arrays (~50 static sequences)
    │   ├── Staged execution (data_lo / data_hi / addr)
    │   ├── Prefix handling (CB/DD/ED/FD/DDCB/FDCB)
    │   └── Z80N extensions (~30 ED-prefix ops, Next only)
    │
    ├── ULA (variant-specific implementation)
    │   ├── Contention pattern + delay lookup
    │   ├── Event-aware catch_up() rendering (palette-indexed u8 output)
    │   ├── Floating bus tracking (per-T-state fetch state)
    │   ├── Interrupt generation (at fixed HC position)
    │   ├── Beeper state (port 0xFE bit 4)
    │   ├── Shared pixel helpers from spectrum-common
    │   └── Event queue (border changes; copper writes on Next)
    │
    ├── Memory (variant-specific implementation)
    │   ├── ROM banks (loaded at construction, auto-paged by Beta disk hook)
    │   ├── RAM banks (variant-specific count and size)
    │   ├── Bank switching (via I/O writes routed by bus)
    │   ├── is_contended(addr) → bool
    │   ├── Timex DOCK/EXROM paging (TC2048/TC2068/TS2068)
    │   └── 8K MMU with 160 banks (Next, superset of legacy paging)
    │
    ├── Tape
    │   ├── Edge state machine: next_edge() → Option<T-states>
    │   ├── Master-clock-driven countdown (no separate timer)
    │   ├── Format crates: TAP, TZX, PZX parsers
    │   └── EAR/MIC loopback (issue-dependent, suppressed when tape connected)
    │
    ├── Audio
    │   ├── Blip buffer (band-limited resampling, HC timestamps → 44.1/48 kHz)
    │   ├── Beeper deltas (from ULA port 0xFE writes)
    │   └── AY deltas (from AY peripheral tick)
    │
    ├── Peripherals (optional, bus-connected via I/O dispatch)
    │   ├── AY-3-8912 (128K+, Timex TC2068/TS2068)
    │   ├── FDC — uPD765A (+3) or WD1793/Beta 128 (Pentagon/Scorpion)
    │   ├── Kempston joystick
    │   ├── Covox DAC (Pentagon/Scorpion, optional)
    │   └── ZXN DMA (Next only, Z8410 subset)
    │
    └── Next-specific (optional)
        ├── NextReg bank (~100+ registers, written by CPU + copper)
        ├── Layer compositor (ULA + Layer 2 + Tilemap + Sprites → output)
        └── Copper co-processor (WAIT + MOVE, fires at raster positions)
```

## Resolved Questions (formerly open)

### WD1793 FDC Timing — Resolved

| Parameter | Value | T-states @ 3.584 MHz |
|---|---|---|
| DRQ interval (MFM byte time) | 32 µs | ~115 |
| Step rates (r1:r0 = 00/01/10/11) | 6 / 12 / 20 / 30 ms | 21,504 / 43,008 / 71,680 / 107,520 |
| Revolution period | 200 ms | 716,800 |
| ID search timeout | 5 revolutions | 3,584,000 |
| Head load delay (E-bit) | 15 ms | 53,760 |
| Head unload timeout | 15 revolutions (3 sec) | 10,752,000 |
| Bytes per track (MFM) | 6,250 | — |

**Lost data:** Set if CPU doesn't service DRQ within 32µs. FDC does NOT abort — continues transfer (lost bytes get stale data). 5-revolution timeout aborts if entire transfer stalls. FUSE doesn't model per-byte lost data; we should.

**Read Sector timeline:** Issue command → check READY → head load (15ms if E-bit) → search for ID field (up to 5 revolutions) → read data field (DRQ every 32µs) → CRC check → INTRQ.

**Implementation:** Model the FDC as a state machine ticked from the master clock. Track disk rotation position (0..bytes_per_track). DRQ fires at each byte boundary. Lost Data flag set if data register not accessed between DRQ assertions.

### TZX Edge Cases — Resolved

| Question | Answer |
|---|---|
| Block 0x15 sample bits | Each bit = literal EAR level for `t_per_sample` T-states. Convert to edges by counting runs of same level. |
| "Used bits in last byte" | **MSB-aligned.** Value 3 = `xxx00000`. |
| Loop blocks (0x24/0x25) | Flat, no nesting. Single `(block_index, count)` pair. Spec says "don't nest." |
| Group blocks (0x21/0x22) | **Cosmetic only.** No playback effect. Store name for UI. |
| Pause = 0 (block 0x20) | **Stop the tape.** Wait for user action to resume. |
| Pause > 0 | 1ms at current level (finishing last edge), then LOW for `(pause - 1)` ms. |
| Signal level between blocks | Maintain `current_level: bool`, start **LOW**, toggle per pulse. Support block 0x2B for explicit level set. |
| Block 0x2A (Stop if 48K) | Check machine model. Stop if 48K; skip if 128K+. |
| CSW (0x18) / Generalized (0x19) | **Defer.** Cover <1% of real TZX files. |

**Implementation priority:** 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x20, 0x2A, 0x2B first. Loops (0x24/0x25) and groups (0x21/0x22) next. CSW/Generalized last.

### Next Copper Scheduling — Resolved

The copper is a **tiny sequential processor ticked at 28MHz**. It does NOT pre-scan its program or build an event list.

| Instruction | Encoding | 28MHz ticks |
|---|---|---|
| NOOP | `0x0000` (MOVE with R=0) | 1 |
| MOVE | `0[RRRRRRR][DDDDDDDD]` (R = NextReg 0-127, D = data) | 2 |
| WAIT | `1[HHHHHH][VVVVVVVVV]` (H×8+12 = pixel position, V = line) | 1 per re-evaluation until met |
| HALT | `0xFFFF` (WAIT for unreachable line 511) | stalls forever |

- **WAIT comparison:** `vc == target_V AND hc >= target_H * 8 + 12` (the +12 compensates for ULA pixel prefetch)
- **Horizontal resolution:** 8 standard pixels (6-bit H field)
- **Register contention:** Copper wins. CPU write delayed by 1 tick (not stalled at bus level).
- **Program memory:** 1024 instructions in 2KB dedicated dual-port RAM. NOT Z80-addressable. Write-only from CPU via NextRegs 0x60-0x63.
- **Modes (NextReg 0x62 bits 7-6):** 00 = stop, 01 = start from 0 + loop, 10 = start from current PC + loop, 11 = start from 0 + auto-reset PC at frame start.

**Architectural impact:** The copper is ticked alongside the ULA in the master loop. When it executes a MOVE, that's a register change event that catch_up() handles. No pre-computed event queue needed — the copper *is* the event source, evaluated in real time.

### Next Sprite System — Resolved

**128 sprites, 16×16 base, 5-byte extended attributes:**

| Byte | Content |
|---|---|
| 0 | X position [7:0] |
| 1 | Y position [7:0] |
| 2 | Palette offset [7:4], X mirror [3], Y mirror [2], Rotate [1], X MSB [0] |
| 3 | Visible [7], Extended [6], Pattern [5:0] |
| 4 | H scale [7:6], V scale [6:5], Type [3], N5 [2], 4-bit [1], N6/Y8 [0] |

**Scaling:** 1x/2x/4x/8x per axis independently (byte 4 bits 7:4).

**Pattern RAM:** 16KB. 64 patterns in 8-bit mode (256 bytes each), 128 in 4-bit (128 bytes each). Linear storage, not planar.

**Per-scanline limit:** 12 sprites. Excess highest-index sprites dropped.

**Collision:** Boolean flag only (NextReg 0x303B bit 0, read-and-clear). No per-sprite info.

**Composite sprites:** Anchor (type=0) followed by consecutive relative sprites (type=1). Relatives use signed X/Y offsets from anchor. Optional pattern-relative and palette-relative inheritance. Anchor invisible → all relatives invisible.

**Layer priority:** NextReg 0x15 bits 4:2 select one of 6 orderings: SLU, LSU, SUL, LUS, USL, ULS (Sprites/Layer2/ULA). Sprite 0 has highest priority (painted on top).

**Rendering pipeline (per scanline):**
1. Iterate sprites 127→0 (so 0 paints last, on top)
2. Check visibility, anchor visibility (if relative), scanline intersection
3. Enforce 12-per-line limit
4. Apply transforms (mirror, rotate, scale), fetch pattern data
5. Add palette offset, check transparency (index 0xE3 default, configurable via NextReg 0x4B)
6. Write to sprite line buffer, set collision flag if pixel already occupied
7. Composite with ULA/Layer2/Tilemap per priority register

**Upload:** Port 0x57 for sequential attribute writes (auto-increments through 4-5 bytes, then next slot). Port 0x5B for pattern data. Port 0x303B to select slot.

## What's Next

All questions resolved. Run `/workflows:plan` to create an implementation plan for the first milestone: **48K (Issue 3) boots with correct contention timing, FUSE tests passing, Tom Harte Z80 tests at 100%.**

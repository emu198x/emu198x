# Architecture

> Archived document. Do not treat status claims here as current. Current state lives in `../../status/` and binding rules/decisions.


14,000 lines of Rust across 21 crates. ULA-drives model: the chipset owns the clock, the CPU is a passive signal-level state machine.

## Run Loop

The master oscillator ticks at 14 MHz (48K) or 17.7 MHz (128K). The ULA ticks every half-cycle. The CPU ticks only when the ULA asserts `cpu_clock_active()`. Contention is implicit — when the ULA withholds the clock, the CPU freezes.

```
while hc < frame_hc {
    if hc % 2 == 0 {
        ula.tick(memory, z80.addr, z80.mreq, z80.iorq, framebuffer);
        if ula.cpu_clock_active() {
            z80.tick();
            handle_bus();  // inspect Z80 signals, perform transactions
        }
        z80.irq = ula.interrupt_active();
    }
    hc += 1;
}
```

No Bus trait. The machine inspects Z80 signals (`addr`, `mreq`, `rd`, `wr`, `iorq`, `m1`) directly and performs bus transactions. Each machine provides its own driver loop.

## Crate Map

```
zilog-z80              Z80 half-cycle state machine (MStep walker, 14 phases)
common-sinclair-zx-spectrum        Ula trait, MemoryBus trait, UlaEngine, audio, tape, timing, palette

ferranti-ula-6c001e    48K ULA (448 px/line, 312 lines, I/O contention)
sinclair-ula-7k010e    128K/+2 ULA (456 px/line, 311 lines, phase 1 contention)
amstrad-ula-40077      +2A/+3 gate array (MREQ-only contention, no floating bus)
pentagon-ula           Pentagon ULA (no contention, 320 lines, 14.336 MHz)
scorpion-ula           Scorpion ULA (no contention, 312 lines, 14 MHz)
timex-scld             Timex SCLD (8 video modes, full I/O decode, same contention as 48K)

gi-ay-3-8912           AY-3-8912 PSG (3 tone, noise, envelope, /8 prescaler)
beta-disk-interface    Beta 128 disk interface (magic ROM paging, WD1793 stub)

machine-sinclair-zx-spectrum-48k     48K (16K ROM + 48K RAM)
machine-sinclair-zx-spectrum-128k    128K/+2 (2 ROMs + 128K paged RAM, $7FFD)
machine-sinclair-zx-spectrum-plus    +2A/+3 (4 ROMs + 128K paged RAM, $7FFD + $1FFD; Amstrad 40077 chip)
machine-pentagon-128                 Pentagon 128 (no contention, 128K paged RAM, Beta disk)
machine-scorpion-zs256               Scorpion ZS-256 (no contention, 256K/16 banks, Beta disk)
machine-timex-tc2048                 TC2048 (SCLD, full I/O decode, no AY)
machine-timex-ts2068                 TC2068/TS2068 (SCLD + DOCK/EXROM + AY on $F5/$F6)

format-sinclair-zx-spectrum-tap      TAP tape parser
format-sinclair-zx-spectrum-tzx      TZX tape parser → pulse sequences
format-sinclair-zx-spectrum-z80      .z80 and .SNA snapshot parsers

emu-sinclair-zx-spectrum             SDL2/OpenGL runner with --model selection
```

## Z80

Half-cycle signal-level state machine. Each `tick()` advances one phase (e.g., `M1_T1_Rise` → `M1_T1_Fall`). Output signals: `addr`, `data`, `mreq`, `iorq`, `rd`, `wr`, `m1`, `rfsh`, `halt`. Input signals: `data_in`, `wait`, `irq`, `nmi`.

Instructions decompose into MStep sequences (~50 static arrays). Steps: `FetchByte`, `ReadAddr`, `WriteAddr`, `PushHi`, `PushLo`, `PopLo`, `PopHi`, `IoRead`, `IoWrite`, `Internal(n)`, `IntAck`, `Execute`. Execute is zero half-cycles — processed immediately by the walker.

Test results: Tom Harte 1,604,000/1,604,000 (100%), ZEXDOC 268/268, ZEXALL 268/268, FUSE 1,351/1,356.

## ULA Engine

Shared rendering logic in `common-sinclair-zx-spectrum/ula_engine.rs`. Each ULA crate wraps `UlaEngine` with variant-specific timing and contention:

- **Ferranti 6C001E** (48K): memory + I/O + refresh contention, floating bus
- **Sinclair 7K010E** (128K/+2): same contention model, different timing (456 px/line, 311 lines)
- **Amstrad 40077** (+2A/+3): MREQ-only contention, no I/O contention, no floating bus
- **Pentagon ULA**: no contention, 320 lines, 14.336 MHz
- **Scorpion ULA**: no contention, 312 lines, 14 MHz
- **Timex SCLD**: same contention as 48K, 8 video modes via port $FF

## Memory

| Variant | ROM | RAM | Paging | Contended |
|---------|-----|-----|--------|-----------|
| 48K | 1×16K | 48K flat | None | $4000-$7FFF |
| 128K/+2 | 2×16K | 8×16K banked | $7FFD | $4000-$7FFF + odd banks at $C000 |
| +2A/+3 | 4×16K | 8×16K banked | $7FFD + $1FFD | $4000-$7FFF + banks 4-7 at $C000 |
| Pentagon | 2×16K | 8×16K banked | $7FFD | None |
| Scorpion | 4×16K | 16×16K banked | $7FFD + $1FFD | None |
| TC2048 | 1×16K | 48K flat | None | $4000-$7FFF |
| TC2068/TS2068 | 1×16K + 8K EXROM | 48K + DOCK | Port $F4 | $4000-$7FFF |

Screen bank switching: 128K+ machines select bank 5 or 7 for ULA display via $7FFD bit 3. The `MemoryBus::read_screen()` method handles this.

## Audio

Beeper (port $FE bit 4) + tape EAR mixed by area-averaging accumulator in `BeeperAudio`. AY-3-8912 generates samples via Bresenham downsampling at 44.1 kHz. Mixed 30/70 beeper/AY through a single-pole RC low-pass filter (~10 kHz cutoff).

## Tape and Snapshots

Tape: pulse-sequence player. Both TAP and TZX reduce to `Vec<u32>` of pulse durations. The player toggles the EAR level after each pulse.

Snapshots: .z80 (v1/v2/v3 with ED ED compression) and .SNA (fixed layout, no compression). Both produce the same `Z80Snapshot` struct for machine state restoration.

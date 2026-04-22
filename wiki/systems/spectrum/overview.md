# ZX Spectrum

First system implemented in the fresh start.

**Current status:** 6 machine crates with working `run_frame` exist:
48K, 128K, +2A/+2B/+3 (one machine, three model IDs), Pentagon 128,
Scorpion ZS-256, Timex TC2048, Timex TS2068 (one machine, two model
IDs). 11 model IDs total in the runtime catalogue.

**ROM-backed boot proven:** 48K (BASIC prompt), 128K (menu screen),
+3 (menu screen), Pentagon (menu screen). Scorpion / TC2048 / TS2068
are unit-tested only — no ROMs in the local set.

**Runtime wrappers:** only `Spectrum48kRuntime` exists today. The
other 10 model IDs are in the `Model` enum and `profiles()` catalogue
but reach the machine layer through their machine crates directly,
not through a `MachineCore` impl yet.

## Architecture

The [ULA-drives model](../../decisions/ula-drives-model.md): the master oscillator ticks at crystal frequency. The ULA ticks every half-cycle. The CPU ticks only when the ULA asserts `cpu_clock_active()`. Contention is implicit — when the ULA withholds the clock, the CPU freezes.

```
while hc < frame_hc {
    if hc % 2 == 0 {
        ula.tick(memory, z80.addr, z80.mreq, z80.iorq, framebuffer);
        if ula.cpu_clock_active() {
            z80.tick();
            handle_bus();
        }
        z80.irq = ula.interrupt_active();
    }
    hc += 1;
}
```

No Bus trait. The machine inspects [Z80](../../chips/zilog-z80.md) signals (`addr`, `mreq`, `rd`, `wr`, `iorq`, `m1`) and performs bus transactions directly.

## Crate map

| Crate | Purpose |
|-------|---------|
| `zilog-z80` | [Z80](../../chips/zilog-z80.md) half-cycle state machine |
| `common-sinclair-zx-spectrum` | Ula trait, MemoryBus trait, UlaEngine, audio, tape, timing, palette |
| `ferranti-ula-6c001e` | [48K ULA](../../chips/ferranti-6c001e.md) |
| `sinclair-ula-7k010e` | [128K/+2 ULA](../../chips/sinclair-7k010e.md) |
| `amstrad-ula-40077` | [+2A/+3 gate array](../../chips/amstrad-40077.md) |
| `pentagon-ula` | Pentagon ULA (no contention) |
| `scorpion-ula` | Scorpion ULA |
| `timex-scld` | Timex SCLD (TC2048/TS2068, 8 video modes) |
| `gi-ay-3-8912` | [AY PSG](../../chips/gi-ay-3-8912.md) |
| `beta-disk-interface` | TR-DOS ROM paging + WD1793 floppy controller |
| `nec-upd765a` | [NEC µPD765A](../../chips/nec-upd765a.md) floppy disk controller (+3) |
| `machine-*-48k` | 48K machine (16K ROM + 48K RAM) |
| `machine-*-128k` | 128K/+2 machine (2 ROMs + 128K paged RAM) |
| `machine-*-plus` | +2A/+3 machine (4 ROMs + 128K paged RAM) |
| `machine-pentagon-128` | Pentagon 128 (no contention, beta disk) |
| `machine-scorpion-zs256` | Scorpion ZS 256 (extended paging, beta disk) |
| `machine-timex-tc2048` | Timex TC2048 (48K + SCLD) |
| `machine-timex-ts2068` | Timex TC2068/TS2068 (SCLD + DOCK/EXROM paging) |
| `format-sinclair-zx-spectrum-tap` | TAP tape parser |
| `format-sinclair-zx-spectrum-tzx` | TZX tape parser (15+ block types) |
| `format-sinclair-zx-spectrum-z80` | .Z80 (v1/v2/v3) and .SNA (48K/128K) snapshot loader |
| `emu-sinclair-zx-spectrum` | SDL2/OpenGL runner with `--model` selection (11 models) |

## Clock tree

See [SPECTRUM-VARIANTS.md](../../docs/SPECTRUM-VARIANTS.md) for the full table. Summary:

| Variant | Crystal | CPU Hz | T/frame |
|---------|---------|--------|---------|
| 48K | 14.000 MHz | 3,500,000 | 69,888 |
| 128K/+2 | 17.734 MHz | 3,546,895 | 70,908 |
| +2A/+3 | 17.734 MHz | 3,546,895 | 70,908 |

## Key subsystems

- **Contention**: [detailed model](contention.md) — three different ULA implementations
- **Variants**: [differences table](variants.md) — memory maps, I/O ports, paging
- **Audio**: beeper (port $FE bit 4) + tape EAR mixed by area-averaging accumulator. AY via Bresenham downsampling. Mixed 30/70 beeper/AY through RC low-pass (~10 kHz)
- **Tape**: timing-span player. TAP (standard timing) and TZX (arbitrary timing)
  both reduce to a shared stream of pulse spans, held-level spans, and stop
  markers in CPU T-state units. Playback advances on the real machine cadence
  and drives the EAR line seen at `$FE`.
- **Interrupt**: INT asserted at vertical blank start for 32 T-states. IM 2 vector: `(I << 8) | $FF` on 48K (bus floats high, no device responds).

## Test results

See [test suites](../../tests/spectrum.md).

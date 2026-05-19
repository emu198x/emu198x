# Decision: NES clock topology

**Date:** 2026-04-09

## The decision

**The NES master loop ticks the PPU every master clock division; the CPU ticks every 3 PPU dots; the mapper observes CPU pin state directly; PPU IRQ and NMI are public pin fields on the PPU struct, routed by the machine layer to the CPU's `irq` and `nmi` inputs.**

This is the NES-specific application of [RULES.md item 1](../../RULES.md#clock) — *the master oscillator drives the loop. Not the CPU. Not the PPU. The crystal.* — and [cpu-bus-interface.md](cpu-bus-interface.md) — *every CPU exposes its bus state as public pin fields*.

## Why this is non-negotiable

The previous (pre-rewrite) NES port **could not pass the standard test ROMs** — not `blargg_ppu_vbl_nmi`, not `sprite_hit_timing`, not `vbl_nmi_timing`, not `nestest` cycle verification. The root cause was always the same: the CPU drove the loop, the PPU was stepped in batches (N cycles per CPU instruction), and the dot-cycle relationship between CPU reads and PPU events drifted away from what real silicon does.

The NES test ROMs care about **dot-level precision**. Examples:

- **NMI fires at PPU `(scanline=241, dot=1)`.** If you step the PPU in batches, NMI may fire a few CPU cycles early or late — enough to make code that polls `$2002` miss the frame.
- **Sprite-0 hit is set at the exact dot** where a non-transparent background pixel and a non-transparent sprite 0 pixel are rendered at the same screen position. Games use this to trigger mid-frame effects (status bar / split screen); a one-dot error makes the split appear on the wrong scanline.
- **The odd-frame dot skip.** On odd frames with rendering enabled, the PPU skips one idle dot at `(0, 0)` — a 340-dot frame instead of 341. Games that time music to PPU cycles care.
- **MMC3's IRQ counter** decrements on specific A12 transitions during background tile fetches. The mapper has to see the exact dot the A12 line rises on the PPU bus. Batching CPU-then-PPU loses this entirely.
- **OAMDMA stalls the CPU for 513 or 514 cycles** (the extra cycle happens on odd CPU cycles). The PPU advances during the stall, and DMC DMA can steal additional cycles. All of these interleave at the master-clock level.

Every one of these is solvable when the **master clock is the loop driver**. None of them are solvable when the CPU drives the loop and the PPU catches up in batches.

## The clock tree (NTSC)

| Clock | Rate | Divider from master |
|---|---|---|
| Master (2C02 XTAL) | 21.477 272 MHz | 1 |
| PPU pixel (dot) | 5.369 318 MHz | 4 |
| CPU (2A03 φ2) | 1.789 773 MHz | 12 |

**The ratio that matters: 1 CPU cycle = 3 PPU dots.**

On PAL it's 26.601 712 MHz master, PPU / 5 = 5.320 342 MHz, CPU / 16 = 1.662 607 MHz — but the CPU : PPU ratio stays at 1:3.2 on PAL, which is why PAL and NTSC use different PPU tick budgets.

## Tick topology

The machine's master loop, in pseudocode:

```rust
fn tick(&mut self) {
    // 1. PPU advances one dot. Updates its internal state
    //    (scanline, dot, BG pipeline, sprite pipeline),
    //    may raise nmi = true at (241, 1), may set irq
    //    pending via the mapper's A12 notifier, renders
    //    one pixel into the framebuffer.
    self.ppu.tick(&mut self.mapper);

    // 2. CPU ticks on every 3rd PPU dot.
    self.cpu_divider = (self.cpu_divider + 1) % 3;
    if self.cpu_divider == 0 {
        // Route PPU pins into CPU pin inputs.
        self.cpu.nmi = self.ppu.nmi;
        self.cpu.irq = self.mapper.irq_pending() || self.apu.irq_pending();

        // Perform the CPU's scheduled bus op (from the
        // previous cycle's pin state) and tick.
        if self.cpu.rw {
            self.cpu.data_in = self.cpu_read(self.cpu.addr);
        } else {
            self.cpu_write(self.cpu.addr, self.cpu.data);
        }
        self.cpu.tick();

        // APU ticks on every CPU cycle.
        self.apu.tick();
    }
}
```

The machine layer is responsible for:

- Ticking PPU every master clock division.
- Dividing master clock by 3 to get CPU clock ticks.
- Routing PPU `nmi` pin → CPU `nmi` input between ticks.
- Routing mapper + APU IRQ sources → CPU `irq` input (OR'd).
- Performing CPU memory reads/writes from pin state (same pattern as the C64 machine).
- Calling `mapper.notify_a12_rendering()` from **inside the PPU tick** when the PPU's address bus transitions A12 during background or sprite fetches (MMC3 IRQ counter hook).
- Implementing OAMDMA and DMC DMA stalls as CPU-side clock gating (similar to how the C64's machine layer uses `cpu.rdy = !ba_low || !cpu.rw` to stall for badlines).

## Pin contracts

### PPU 2C02

**Output pins:**
- `nmi: bool` — level-triggered, set at `(241, 1)` if `PPUCTRL` bit 7 is set, cleared at the next `$2002` read or at `(261, 1)`.
- `framebuffer: &[u8]` — indexed into the fixed 64-entry palette. Accessed by the runtime layer for host display.

**Input methods (not pins — peripheral register bus, same rationale as CIA/SID/VIC):**
- `read(reg)` — `$2000-$2007` register bus, `&mut self` for `$2002` side effects (clears VBL flag + write latch).
- `write(reg, value)` — `$2000-$2007` register writes.
- `tick(mapper: &mut dyn Mapper)` — advances one dot. Takes the mapper because PPU memory reads (pattern table, nametable, attribute table) route through the mapper.

**No CPU-bus pins exposed** — the PPU doesn't observe the CPU's bus directly on real hardware; communication is via the register interface at `$2000-$2007` which the machine layer routes. The mapper is the one chip that observes both the CPU's bus (via `cpu_read`/`cpu_write`) and the PPU's bus (via `chr_read`/`chr_write`).

### CPU 2A03 (mos-6502 with decimal disabled)

Same pin contract as the stock [mos-6502](../chips/mos-6502.md): `addr` / `data` / `rw` / `sync` outputs, `data_in` / `irq` / `nmi` / `rdy` inputs. The only behavioural difference is that `ADC` and `SBC` ignore the D flag — the 2A03 has the BCD decoder disabled on-die. A `decimal_disabled` field on the `M6502` struct gates the BCD path.

### Mapper

Per the archive's existing trait, with a few additions demanded by the pin-level world:

```rust
pub trait Mapper: Send {
    // CPU-side bus — the machine calls these from inside its
    // cpu_read / cpu_write routers for addresses >= $6000.
    fn cpu_read(&self, addr: u16) -> u8;
    fn cpu_write(&mut self, addr: u16, value: u8);

    // PPU-side bus — the PPU calls these from inside its tick
    // for addresses $0000-$1FFF (pattern tables).
    fn chr_read(&mut self, addr: u16) -> u8;
    fn chr_write(&mut self, addr: u16, value: u8);

    // Nametable mirroring — determined by the mapper, queried by
    // the PPU on every nametable access.
    fn mirroring(&self) -> Mirroring;

    // IRQ output pin (effectively). Level-triggered, OR'd into
    // the CPU's irq input by the machine layer. Default: never
    // asserted (most mappers don't do IRQ).
    fn irq_pending(&self) -> bool { false }

    // A12 notifiers for MMC3 IRQ counter. Called from inside
    // the PPU tick when A12 changes during rendering.
    fn notify_a12_rendering(&mut self, _a12_high: bool) {}

    // ... (audio, PRG RAM, save state, etc. — see the
    // format-nintendo-nes-ines crate for the full trait)
}
```

**Why the mapper isn't fully pin-level** (a `cpu_read` method rather than CPU bus pins as fields): the mapper is a cartridge chip, not a bus master. It responds to the CPU's bus state when the machine's address decoder routes a read or write to `$6000-$FFFF`. Other chips don't observe the mapper's internal state; they only see its effects on PPU-side pattern table reads. The trait-with-methods shape is the right abstraction here — same rationale as why CIA/SID/VIC register buses are methods.

## Odd-frame dot skip

On odd frames (frame count is odd) **and** rendering is enabled (PPUMASK bit 3 or 4 is set), the PPU skips dot `(0, 0)` and jumps straight to `(0, 1)`. This gives a 340-dot frame instead of 341. The test ROM `ppu_odd_even_frame_timing` verifies this behaviour.

Implementation: track `frame_is_odd: bool` in the PPU, toggle it every frame end, and when starting a new frame with rendering enabled and `frame_is_odd == true`, begin at dot 1 instead of dot 0.

## OAMDMA and DMC DMA stalls

OAMDMA is a CPU-side operation that writes `$4014` and then copies 256 bytes from CPU memory to PPU OAM, stalling the CPU for 513 or 514 cycles (514 if the current CPU cycle is an odd-numbered "get" cycle, 513 otherwise). The machine layer handles this by:

1. Detecting writes to `$4014`.
2. Setting an internal `dma_stall_cycles` counter.
3. On each CPU cycle, checking if `dma_stall_cycles > 0`; if so, perform the DMA byte copy (or the dummy read-wait cycle) instead of ticking the CPU.

DMC DMA is similar but fires from inside the APU when the DMC channel needs a new sample byte — steals 1-4 CPU cycles depending on the current CPU phase.

Both are implemented in the machine layer, not the CPU crate, same shape as the C64's VIC-II bad-line BA assertion but more complex (multi-cycle DMA instead of single-cycle stall).

## Porting provenance

The existing `ricoh-ppu-2c02` crate in `Emu198x-archive` is **not directly portable** per [archives-as-source.md](archives-as-source.md#what-does-not-port) — it was built for the old architecture where the CPU drove the loop. The rendering pipeline, palette, and register decode logic will lift mostly intact, but the `tick()` shape and its relationship to the CPU clock will need a full rewrite.

The existing `format-nintendo-nes-ines` crate is **lift-and-shift** — pure parsers and mapper state machines with no clock model. Its `Mapper` trait is the right shape and lifts almost unchanged.

## Drift triggers

**Phrases that signal you should re-read this entry:**

- *"Let's step the PPU N dots per CPU instruction, it's simpler"* — no. That's what the old port did, and every test ROM failed because of it.
- *"The CPU can just call into the PPU when it accesses `$2000-$2007`"* — that works for the register bus, but the PPU still needs to tick independently at the master clock rate so NMI fires at the right dot.
- *"Mapper IRQ is a method call, we can poll it once per CPU cycle"* — MMC3's IRQ counter decrements on A12 transitions during PPU rendering, which happen inside the PPU's tick. If the mapper doesn't see those transitions, the IRQ fires at the wrong scanline.
- *"OAMDMA is just a 256-byte memcpy, we can inline it"* — no, it takes exactly 513 or 514 CPU cycles with specific read/write interleave, and the PPU advances during those cycles.

## Related

- [RULES.md](../../RULES.md) item 1 — master oscillator drives the loop.
- [cpu-bus-interface.md](cpu-bus-interface.md) — pin-level CPU bus, applies to 2A03 as to any 6502.
- [ula-drives-model.md](ula-drives-model.md) — the Spectrum precedent (ULA owns the clock, same shape as PPU owns the clock here).
- [system-specific-run-loops.md](system-specific-run-loops.md) — each system's run loop matches its hardware; NES run loop is dot-driven.
- [mos-6502](../chips/mos-6502.md) — the CPU core. The 2A03 is a mos-6502 with `decimal_disabled: true`.
- [archives-as-source.md](archives-as-source.md) — which archive crates port cleanly (format-nintendo-nes-ines) and which need rewriting (ricoh-ppu-2c02, ricoh-apu-2a03, machine-nintendo-nes).

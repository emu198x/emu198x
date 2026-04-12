# Decision: Amiga port plan

**Date:** 2026-04-10

## The plan

Port the Amiga from the archive in **9 phases**, OCS (A500) first, ECS/AGA variants after the A500 boots Kickstart. Each phase is one or two sessions with a clean context. The archive has ~40,000 lines of working Amiga code across 24 crates with 260+ tests — the rendering logic lifts intact; the machine wiring needs rewriting for pin-level bus (same pattern as the NES port).

## Why OCS-first

The A500 with Kickstart 1.3 is the simplest Amiga that exercises every custom chip. ECS adds programmable beam timing and 6-bit palette; AGA adds 8-bit colour and Lisa. None of that matters until the A500 boots. Every subsequent variant is a delta on top of a working OCS machine.

## The clock tree

| Clock | Rate (PAL) | Divider from master |
|---|---|---|
| Master (crystal) | 28.375 160 MHz | 1 |
| Colour clock (CCK) | 3.546 895 MHz | 8 |
| CPU (68000 φ) | 7.093 790 MHz | 4 |
| E-clock (CIA, Paula) | 709.379 kHz | 40 |

**The ratio that matters: 1 CCK = 2 CPU cycles = 8 master clocks.**

Agnus advances the beam by one CCK every 8 master clocks. The CPU gets one bus cycle (4 master clocks minimum, may stretch if Agnus denies the bus) within each CCK. DMA slots are allocated per-CCK.

## Phase breakdown

### Phase 1 — Motorola 68000 CPU (pin-level conversion)

**Scope:** Convert `motorola-68000` (15,859 lines) from `M68kBus` trait to pin fields, same transformation as the 6502 port.

**What changes:**
- `M68kBus` trait → public pin fields: `addr: u32`, `data: u16`, `rw: bool`, `as_: bool`, `lds: bool`, `uds: bool`, `fc: u8` (function code), `ipl: u8` (interrupt priority level) as **outputs**; `dtack: bool`, `data_in: u16`, `berr: bool`, `vpa: bool` (6800 peripheral), `reset_in: bool`, `halt_in: bool` as **inputs**.
- `tick(bus)` → `tick()` that returns `bool` (instruction complete) or similar. The machine reads/writes pins between ticks.
- The micro-op queue, prefetch pipeline (IR/IRC), state machine (Idle/Internal/BusCycle), and all decode/execute logic lifts unchanged — only the bus interface shape changes.
- Drop the `BusStatus` enum — `dtack` is a pin. The machine asserts `dtack` when the addressed chip responds. If Agnus denies the bus (DMA slot not granted to CPU), `dtack` stays deasserted and the CPU holds in BusCycle state.

**Validation:** The archive has 140 tests + Musashi comparison suite. Port the tests to the pin-level interface. If they pass, the CPU is correct.

**Risk:** This is the largest single file (5,233 lines in decode.rs alone). The micro-op queue makes the pin conversion more mechanical than the 6502 (which had hand-written per-cycle state machines) — each micro-op that was `bus.poll_cycle(addr, fc, true, true, None)` becomes "set addr/fc/rw pins, wait for dtack".

**Deliverable:** `motorola-68000` crate with pin-level interface, 140+ tests passing.

### Phase 2 — CIA 8520 + Gary address decoder

**Scope:** 
- `mos-cia-8520` (634 lines) — minor variant of our existing `mos-cia-6526`. The 8520 replaces the 6526's BCD TOD with a binary 24-bit counter. Everything else (timers, I/O ports, serial shift register, interrupt controller) is identical. We can either fork the 6526 or add a `binary_tod` flag.
- `commodore-gary` (687 lines) — central address decoder. Maps the 68000's 24-bit address to chip selects. Pure combinational logic, no clock dependency. Lifts as-is.

**Validation:** Archive tests for both. Gary's address mapping is well-defined by the Amiga hardware reference.

**Deliverable:** Two crates, tests passing.

### Phase 3 — Agnus OCS (beam + DMA + copper)

**Scope:** Port `commodore-agnus-ocs` (1,706 lines). Beam counter, DMA slot allocation per CCK, copper coprocessor.

**What changes:** The tick interface needs to match the master-clock-driven model. `tick_cck()` advances beam and returns a `CckBusPlan` describing which DMA client owns each slot — this is already the right shape. The copper needs memory access for instruction fetches; in the archive it takes a closure, in the port it should take `&dyn` something (or return a fetch request the machine fulfils).

**Why this is critical:** Agnus owns the clock. Every other chip derives its timing from Agnus's beam position. Getting the DMA slot allocation right is the Amiga equivalent of getting the NES's 1:3 CPU:PPU ratio right — if slots are wrong, the CPU sees bus contention at the wrong times and timing-sensitive code breaks.

**Deliverable:** `commodore-agnus-ocs` crate, beam tracking + DMA + copper, tests passing.

### Phase 4 — Denise OCS (pixel pipeline)

**Scope:** Port `commodore-denise-ocs` (2,319 lines). Bitplane shift registers, playfield composition, sprite engine, palette lookup.

**What changes:** Minimal — Denise consumes bitplane data and beam position from Agnus, produces pixels. The interface is data-in, pixel-out. The archive's superhires-precision rendering lifts intact.

**Deliverable:** `commodore-denise-ocs` crate, pixel output, tests passing.

### Phase 5 — Paula (audio + interrupts)

**Scope:** Port `commodore-paula-8364` (1,394 lines). Four DMA audio channels with return-latency modelling, interrupt priority controller (14 sources → 6 IPL levels).

**What changes:** Paula's interrupt output needs to drive the 68000's `ipl` input pins. The audio DMA return-latency model (Agnus grants a slot, Paula gets the data ~14 CCK later) is already in the archive. The DAC non-linearity S-curve lifts as-is.

**Deliverable:** `commodore-paula-8364` crate, audio + interrupts, tests passing.

### Phase 6 — Machine wiring (A500 OCS)

**Scope:** `machine-commodore-amiga` — the moment of truth. Wire 68000 + Agnus + Denise + Paula + 2×CIA + Gary together with the master-clock tick loop.

**The tick loop (pseudocode):**

```rust
fn tick(&mut self) {
    self.master_clock += 1;

    // Agnus ticks every 8 master clocks (1 CCK).
    if self.master_clock % 8 == 0 {
        let plan = self.agnus.tick_cck();
        // Execute DMA for the slot owner.
        self.execute_dma_slot(&plan);
        // Feed bitplane data to Denise.
        self.denise.shift_pixel(/* beam, bitplane data */);
    }

    // CPU ticks every 4 master clocks.
    if self.master_clock % 4 == 0 {
        // CPU bus grant depends on Agnus DMA slot allocation.
        // If Agnus gave the slot to a DMA client, CPU gets
        // dtack = false and holds.
        self.cpu.dtack = self.cpu_bus_granted;
        if self.cpu.as_ {
            // CPU is requesting a bus cycle.
            if self.cpu.rw {
                self.cpu.data_in = self.bus_read(self.cpu.addr);
            } else {
                self.bus_write(self.cpu.addr, self.cpu.data);
            }
        }
        self.cpu.tick();

        // Route Paula's interrupt priority → CPU IPL pins.
        self.cpu.ipl = self.paula.ipl_level();
    }

    // E-clock ticks every 40 master clocks.
    if self.master_clock % 40 == 0 {
        self.cia_a.tick();
        self.cia_b.tick();
        self.paula.tick_audio();
    }
}
```

**Validation target:** Boot Kickstart 1.3 to the hand/insert-disk screen. This requires:
- CPU executing KERNAL code correctly
- Agnus beam advancing (VERTB interrupt fires)
- Denise rendering the boot animation
- Paula routing VERTB interrupt to CPU
- CIA-A keyboard handshake completing (keyboard sends $FD/$FE init sequence)
- Gary routing addresses to ROM and custom registers

**Deliverable:** `machine-commodore-amiga` crate, A500 model, boots Kickstart 1.3.

### Phase 7 — Floppy + keyboard + ADF format

**Scope:** Port the three peripheral crates:
- `format-commodore-amiga-adf` (139 lines) — sector layout, lifts as-is.
- `peripheral-commodore-amiga-floppy` (972 lines) — drive emulation, MFM encode/decode.
- `peripheral-commodore-amiga-keyboard` (357 lines) — init sequence, keycode transmission.

**Why this phase exists:** Kickstart 1.3's boot screen says "insert disk". To go further (boot Workbench, run a game), the floppy must work. The keyboard must work for the init handshake (Kickstart won't proceed without it).

**Deliverable:** Three crates, floppy loads ADF, keyboard init completes.

### Phase 8 — Runtime + CLI + screenshot

**Scope:** Same pattern as C64/NES:
- `runtime-commodore-amiga` — RGBA framebuffer conversion, System trait impl.
- `emu198x-script-amiga` — headless CLI: load Kickstart ROM + ADF, run N frames, screenshot.

**Validation target:** PNG of the Kickstart 1.3 hand/insert-disk screen.

**Deliverable:** Two crates, visible output.

### Phase 9 — ECS/AGA variants

**Scope:** Port the ECS and AGA wrappers:
- `commodore-agnus-ecs` (988 lines), `commodore-agnus-aga` (278 lines)
- `commodore-denise-ecs` (383 lines), `commodore-denise-aga` (372 lines)
- Machine model variants (A500+, A600, A1200, A2000, A3000, A4000)
- Gayle (A600/A1200), Fat Gary + Ramsey (A3000/A4000), Buster/Super Buster

**Why last:** ECS/AGA are deltas on OCS. The A500 is the validation baseline. Kickstart 2.x/3.x boot on ECS/AGA but the core rendering path is the same — Agnus feeds DMA data to Denise which outputs pixels.

**Deliverable:** Multiple Amiga models booting their respective Kickstart versions.

## What does NOT need rewriting

From the archive survey, these lift intact (same finding as NES PPU):

- Agnus DMA slot allocation logic
- Agnus beam counter and sync generation
- Copper instruction fetch/execute state machine
- Denise bitplane shift registers and pixel composition
- Denise sprite engine
- Paula audio DMA with return-latency modelling
- Paula interrupt priority encoder
- Paula DAC non-linearity curve
- CIA 8520 timers, I/O ports, serial shift register
- Gary/Gayle/Fat Gary/Ramsey address decode
- ADF sector layout parser
- Floppy MFM encode/decode
- Keyboard init sequence and keycode transmission
- All ECS/AGA wrapper logic (it's deltas on OCS)

## What DOES need rewriting

- **68000 bus interface** — `M68kBus` trait → pin fields (same as 6502 port)
- **Machine tick loop** — master clock drives Agnus, CPU is subordinate (same as NES port)
- **DMA bus arbitration** — the machine routes DMA reads/writes based on Agnus's slot plan, not the CPU's initiative
- **Interrupt routing** — Paula's IPL output → CPU's `ipl` input pins (same pattern as PPU NMI → CPU NMI)

## Archive crate sizes for reference

| Crate | Lines | Lifts? |
|---|---|---|
| motorola-68000 | 15,859 | Interface rewrite, logic lifts |
| machine-commodore-amiga | 8,344 | Full rewrite (clock topology) |
| commodore-denise-ocs | 2,319 | Lifts intact |
| commodore-agnus-ocs | 1,706 | Mostly lifts (copper interface tweak) |
| commodore-paula-8364 | 1,394 | Lifts intact |
| commodore-agnus-ecs | 988 | Lifts (wrapper over OCS) |
| peripheral-commodore-amiga-floppy | 972 | Lifts intact |
| commodore-gary | 687 | Lifts intact |
| mos-cia-8520 | 634 | Fork of existing 6526 |
| format-commodore-amiga-ipf | 550 | Lifts intact |
| commodore-fat-gary | 422 | Lifts intact |
| commodore-denise-ecs | 383 | Lifts (wrapper over OCS) |
| peripheral-commodore-amiga-keyboard | 357 | Lifts intact |
| commodore-denise-aga | 372 | Lifts (wrapper over ECS) |
| commodore-agnus-aga | 278 | Lifts (wrapper over ECS) |
| commodore-ramsey | 256 | Lifts intact |
| format-commodore-amiga-adf | 139 | Lifts intact |
| **Total** | **~35,660** | |

## Estimated effort

- **Phase 1 (68000):** Largest single piece. The decode.rs is 5,233 lines but the conversion is mechanical (replace `bus.poll_cycle()` with pin assignment). Two sessions.
- **Phases 2-5 (chips):** One session each. Most are lift-and-shift.
- **Phase 6 (machine wiring):** One or two sessions. This is where the architecture proves itself.
- **Phases 7-8 (peripherals + runtime):** One session.
- **Phase 9 (ECS/AGA):** One session per variant pair.

**Total: 8-12 sessions to a booting A500 with screenshot. 2-3 more for ECS/AGA variants.**

## Drift triggers

- *"Let's start with the A1200 because AGA is more interesting"* — no. OCS first. A1200 is a delta on A500.
- *"The 68000 can use instruction-level stepping for now"* — no. RULES.md item 5. The archive already has per-4-clock micro-ops. Port that.
- *"Let's skip Agnus and just run the CPU against ROM"* — the CPU can't execute anything useful without Agnus routing VERTB interrupts. The Kickstart ROM busy-waits for VERTB in its earliest init code.
- *"The blitter can be synchronous"* — for Phase 6 boot validation, yes. For accuracy, the blitter needs per-CCK DMA slot arbitration. Defer the per-slot model to a later phase but don't design the interface in a way that prevents it.

## Related

- [RULES.md](../../RULES.md) item 1 — master oscillator drives the loop.
- [RULES.md](../../RULES.md) item 5 — 68000 at native granularity (cycle-accurate).
- [cpu-bus-interface.md](cpu-bus-interface.md) — pin-level for every CPU.
- [nes-clock-topology.md](nes-clock-topology.md) — the NES precedent (PPU drives, CPU subordinate).
- [archives-as-source.md](archives-as-source.md) — port from archive, same lifecycle.

# Decision: CPU bus interface — pin-level for every CPU

**Date:** 2026-04-09

## The decision

**Every CPU we emulate exposes its bus state as public pin fields, and the machine inspects those pins between ticks to perform bus transactions. There is no `Bus trait`, no callback, no method-call-style memory access — ever, for any CPU.**

This applies universally: Z80, 6502, 6510, 2A03, 65C02, 65C816, 6809, 68000, 68010, 68020, ARM, and every other CPU we ever add. Past CPUs, present CPUs, future CPUs. The rule is non-negotiable.

The mechanical shape:

```rust
pub struct CpuName {
    pub regs: Registers,

    // Output signals (CPU → machine) — driven by tick()
    pub addr: u16,        // address bus
    pub data: u8,         // data bus, driven during writes
    pub rw: bool,         // (or mreq/iorq/rd/wr split per CPU's pinout)
    pub sync: bool,       // opcode-fetch indicator (or m1, etc.)
    // ... per-CPU control pins as needed

    // Input signals (machine → CPU) — read by tick()
    pub data_in: u8,      // data bus during reads, populated by machine
    pub irq: bool,
    pub nmi: bool,
    pub rdy: bool,        // or wait, depending on CPU pinout
    // ...
}

impl CpuName {
    /// Advance one cycle (or half-cycle, depending on CPU). Reads
    /// `self.data_in` if the previous tick scheduled a read; sets
    /// new pin state for the next bus operation.
    pub fn tick(&mut self) { /* ... */ }
}
```

The machine loop:

```rust
loop {
    chipset.tick();                  // every chip on the master clock
    if chipset.cpu_clock_active() {
        cpu.tick();
        // Inspect pins, perform bus op, populate data_in for next tick
        if cpu.rw {
            cpu.data_in = bus_read(cpu.addr);
        } else {
            bus_write(cpu.addr, cpu.data);
        }
    }
    cpu.irq = chipset.irq_active();
    cpu.nmi = chipset.nmi_pending();
}
```

## Why pin-level is the only acceptable answer

### The wrong reason this was originally written

When `RULES.md` item 6 was first added, it said *"No Bus trait. The machine inspects Z80 signals and performs bus transactions directly."* The implication was that the rule was Z80-specific — driven by the Spectrum's ULA contention model, the Z80's half-cycle wait state insertion, and the M1/refresh behaviour.

That framing was wrong. Pin-level isn't a Z80 quirk. It's the only way to get multi-chip bus interactions right on any system where more than one chip shares the bus.

### The right reason

Every system in our roadmap has at least one chip that **observes the CPU's bus activity in real time** to do its job:

- **Spectrum**: the ULA gates the CPU clock based on whether the address is in contended memory. During contention, the ULA observes the CPU's address pins.
- **C64**: the VIC-II steals bus cycles from the 6502 for sprite DMA, character fetch (bad lines), and color RAM reads. The VIC-II sees what the CPU is doing on the bus and asserts BA/RDY accordingly.
- **NES**: the PPU runs at 3× the CPU rate and has its own DMA controller that pauses the CPU when reading sprite data via OAMDMA. The PPU's bus is separate but the CPU's bus is observed by the PPU bus chip (mapper) for cartridge-side IRQ generation and bank switching.
- **Amiga**: Agnus is the bus master, arbitrating between CPU, blitter, copper, audio DMA, video DMA, sprite DMA, and disk DMA. Every cycle is allocated to a bus master. The CPU is just one client. Agnus needs to see the CPU's bus state continuously to know when to grant cycles.
- **BBC Micro**: 6502 + Video ULA shares memory with CPU at 2 MHz / 1 MHz alternation. Same shape as C64 but with the 6845.
- **Atari 800**: 6502 + ANTIC + GTIA. ANTIC is the DMA controller for video, ANTIC's display list interpreter, and player/missile graphics. Same shape.

In every one of these systems, **a Bus trait callback is the wrong abstraction**. The callback says "CPU asks bus for a byte at address X, bus returns the byte." But the other chips don't see that interaction unless they happen to be ticked at the right moment. Pin-level says "CPU has put X on the address bus with R/W high, and any chip that ticks now can see this."

### The Bus trait approach can technically work, but it puts the bus arbitration logic in the wrong layer

You *can* implement a C64 with a Bus trait — the machine layer mediates between CPU and VIC-II, making sure the right chip "owns" the bus on each cycle. But the bus arbitration logic ends up in the machine layer, where it has to reach into both the CPU's internal state and the VIC-II's internal state to figure out who gets the cycle. It works, but every modification touches three layers.

Pin-level puts the arbitration where it belongs: **on the bus itself, observed by every chip directly.** The CPU sets its pins; the VIC-II reads them; the VIC-II asserts BA when it wants the bus; the CPU's RDY input goes low; the CPU stalls. Each chip is responsible for its own pin state. The machine layer just routes the data bus reads/writes.

### Cycle accuracy is the same in both models

For the *6502 in isolation*, both approaches produce identical sequences of bus operations at identical cycles. The difference is invisible if you only look at the CPU.

The difference becomes load-bearing the moment you have more than one chip on the bus. Which is every system we care about.

## Why the rule is permanent

We could write this rule as "pin-level is required for any CPU on a multi-chip bus" and let single-chip embedded CPUs use a Bus trait. But:

1. **Every retro system has multi-chip buses.** There is no single-chip system in scope.
2. **The architectural divergence cost is real.** If half our CPUs use pin-level and half use callbacks, every cross-CPU port has to translate between them. Pattern code (chip implementations, machine wrappers, MCP introspection) would have to handle both. Two architectures is two bugs of every kind.
3. **Pin-level is also better for debugging.** A pin-level CPU can be inspected in any tick — you can dump `addr`/`data`/`rw` and know exactly what the chip is doing right now. A callback CPU can only be inspected by stepping through the call.
4. **Save states are cleaner.** All CPU bus state is in serialisable struct fields. No closures or trait objects to skip-serialize.
5. **Future-proof for novel chip combinations.** When we add a system with two CPUs sharing a bus (Sega System 16: 68000 + Z80; SNES: 65C816 + SPC700; Amiga 1000 with the Z80 bridge board) the pin model just works. The Bus trait would require dual-master arbitration logic everywhere.

The permanence is the point. *"Pin-level for the Z80 only"* is not coherent. Either we believe pin-level is the right hardware model — in which case it applies everywhere — or we don't, in which case the Z80 should also use a Bus trait. We chose the former. The rule binds us to consistency.

## Conversion pattern from a synchronous Bus trait CPU

When porting a CPU implementation that uses the synchronous Bus trait pattern (e.g. the April 2026 archive's `cpu-6502`), each `bus.read(addr)` and `bus.write(addr, val)` call site converts as follows.

**Synchronous (forbidden):**

```rust
fn tick(&mut self, bus: &mut dyn Bus) -> bool {
    match self.cs.cycle {
        1 => {
            self.cs.addr = bus.read(self.regs.pc) as u16;  // synchronous read
            self.regs.pc = self.regs.pc.wrapping_add(1);
            self.cs.cycle = 2;
            false
        }
        2 => {
            bus.write(self.cs.addr, self.cs.value);  // synchronous write
            true
        }
        _ => unreachable!(),
    }
}
```

**Pipelined (correct):**

```rust
fn tick(&mut self) -> bool {
    match self.cs.cycle {
        1 => {
            // Consume the read scheduled by the PREVIOUS tick (which set
            // self.addr = pc, self.rw = true). The machine populated
            // self.data_in between ticks.
            self.cs.addr = self.data_in as u16;
            self.regs.pc = self.regs.pc.wrapping_add(1);
            self.cs.cycle = 2;

            // Schedule the bus op for cycle 2: a write to cs.addr.
            self.addr = self.cs.addr;
            self.data = self.cs.value;
            self.rw = false;
            false
        }
        2 => {
            // Previous tick scheduled the write. The machine performed
            // it between ticks. Nothing to consume.
            self.cs.cycle = 0;  // back to opcode fetch

            // Schedule cycle 0's bus op: opcode fetch at PC.
            self.addr = self.regs.pc;
            self.rw = true;
            self.sync = true;
            true
        }
        _ => unreachable!(),
    }
}
```

The two transformations are:

1. **Reads become "consume from data_in"** — the value is whatever the machine put there based on the previous tick's `addr`.
2. **Each cycle ends by setting up the next cycle's bus op** — the pins are written *before* the tick returns, so the machine can act on them immediately after.

The bootstrap is `reset()`: the reset routine sets the pins for the first read (the reset vector low byte at `$FFFC` for 6502, the M1 fetch at `$0000` for Z80) so the very first tick after reset has valid `data_in` to consume.

## Worked examples

### Spectrum Z80 (`crates/zilog-z80/`)

Already uses this model. Output pins: `addr`, `data`, `mreq`, `iorq`, `rd`, `wr`, `m1`, `rfsh`, `halt`. Input pins: `data_in`, `wait`, `irq`, `nmi`. The machine loop in `crates/runtime-sinclair-zx-spectrum/` and the per-machine wrappers inspect these pins between half-cycle ticks.

### C64 6502 (`crates/mos-6502/`, in progress)

Being ported from `Emu198x-archive-april2026/crates/cpu-6502/` (which uses a Bus trait). Conversion follows the pipelined pattern above. Output pins: `addr`, `data`, `rw`, `sync`. Input pins: `data_in`, `irq`, `nmi`, `rdy`. The C64 machine wrapper will tick the 6502 once per CPU clock cycle and inspect pins between ticks; the VIC-II will tick on every dot clock and observe the same pins to drive its BA/RDY logic.

### NES 6502 (future)

Will reuse `mos-6502` directly. The NES variant has no decimal mode (handled by a feature flag or build-time config). Same pin interface; different memory map.

### Amiga 68000 (future)

The 68000 has a more complex bus protocol than the 6502 — ASn (address strobe), UDS/LDS (upper/lower data strobes), DTACK (data transfer acknowledge from peripheral), BG/BR/BGACK (bus grant chain for DMA arbitration). All of these become pin fields. Agnus, the bus master, observes these pins continuously and asserts BG/BGACK to take ownership of the bus during DMA cycles.

## Drift triggers

**Phrases that signal you should re-read this entry:**

- "Let's just add a Bus trait for this CPU, it's simpler" — no.
- "Callbacks are equivalent to pins, they're just different syntax" — they're not, see "the right reason" above.
- "The 6502 is simple enough that pin-level is overkill" — the 6502 in isolation is, but the C64 isn't.
- "We can refactor to pin-level later when we add the second chip" — by then the bus arbitration logic is wedged into the machine layer and the refactor is harder than doing it right the first time.
- "The Bus trait is just an internal implementation detail of the CPU crate, the external API is still pin-based" — no, the external interface determines what the CPU's contract is. If the Bus trait is reachable from `tick()`, it's part of the public model.

**Code patterns to reject:**

- `pub trait Bus6502 { fn read(&mut self, addr: u16) -> u8; fn write(...); }`
- `fn tick(&mut self, bus: &mut dyn Bus6502)` (any tick that takes a bus parameter)
- `cpu.read_mem(addr)` (any method on the CPU that returns memory data)
- `cpu.set_memory_callback(...)` (any function that registers a closure for memory access)
- A new CPU crate without `addr`, `data`, `data_in`, `rw` (or equivalent control pins) as `pub` fields

**What to do when triggered:** rewrite the CPU to use pin fields and a synchronous tick. The conversion is mechanical (one cycle = one bus op = one tick = one pin update), and the resulting code is easier to debug, easier to inspect, easier to save-state, and accurate to real hardware. Future-you will thank present-you.

## Related

- [`RULES.md`](../../RULES.md) item 6 — the binding form of this rule.
- [`crates/zilog-z80/`](../../crates/zilog-z80/) — the worked Z80 example.
- [`crates/mos-6502/`](../../crates/mos-6502/) — the worked 6502 example (in progress).
- [Half-cycle signals](half-cycle-signals.md) — the related decision about timing granularity.
- [No Bus trait](no-bus-trait.md) — the original (Z80-specific) framing of this rule, now superseded by this universal version.

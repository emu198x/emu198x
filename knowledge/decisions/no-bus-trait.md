# Decision: No Bus Trait

**Date**: April 2026 (architecture revision)

## The decision

The Z80 does not call methods on a Bus trait. Instead, it exposes signal-level outputs (`addr`, `data`, `mreq`, `iorq`, `rd`, `wr`, `m1`, `rfsh`, `halt`) and accepts inputs (`data_in`, `wait`, `irq`, `nmi`). The machine loop inspects these signals and performs bus transactions directly.

## Why

A Bus trait creates an abstraction boundary at the wrong place. The machine needs to see the CPU's signals to decide what happens — contention depends on address lines, I/O depends on port decoding, and different systems wire the same CPU differently. A trait either becomes so wide it's not an abstraction, or so narrow it prevents accurate emulation.

Each machine provides its own driver loop. This is more code per machine but simpler per machine — no shared abstractions to work around.

## What this means in practice

- No `bus.read(addr)` calls from the CPU
- The CPU sets `addr` and asserts `mreq` + `rd`; the machine sees these signals and puts data on `data_in`
- Each system's driver loop is a few dozen lines that exactly match the hardware's signal flow
- Adding a new system means writing a new driver loop, not implementing a trait

## Drift triggers

This decision fights typical Rust instincts, which makes it the most drift-prone entry in this directory. If I'm about to write any of these, stop — I am about to repeat a pattern that was explicitly ruled out.

**Code patterns to reject:**

- `trait Bus { fn read(&self, addr: u16) -> u8; fn write(&mut self, addr: u16, val: u8); }` — the decision's literal negation
- `cpu.run(&mut bus)` or any CPU method that takes a bus-like argument
- `cpu.step(&mut memory)` where memory is passed down into the CPU
- `impl Bus for ...` on anything
- `Box<dyn Bus>` or `&mut dyn Bus` anywhere near the CPU
- `bus.read(addr)` / `bus.write(addr, val)` as calls from CPU code
- Any wrapper struct whose stated purpose is "bus abstraction"

**Phrases that signal drift:**

- "Let's add a Bus trait to clean this up"
- "The CPU can just call `bus.read`"
- "We need to abstract memory access for testing"
- "A trait would be cleaner than machine-specific driver loops"
- "Dependency injection for the memory system"
- "The bus abstraction would let us share code across systems"
- "This duplication across machine crates is a smell, let's trait it up"

**Architectural framings to reject:**

- Treating "the bus" as an object the CPU interacts with (it isn't — it's a set of signals the machine reads from the CPU)
- Unifying memory access and I/O access behind a trait (they're different signals)
- Adding a layer between the CPU and the machine's signal inspection
- Any refactor whose justification is "reduces duplication across machine crates"

**What to do when triggered:** the duplication across machine driver loops is intentional, not an accident. Each machine's signal flow matches its real hardware. If I find myself wanting to unify, the correct move is to document the pattern in prose and keep the code separate — not to introduce a trait. If the duplication genuinely hurts, raise it explicitly; do not silently refactor.

## Related

- [ULA-drives model](ula-drives-model.md) — the machine loop that inspects CPU signals
- [Z80](../chips/zilog-z80.md) — signal interface details

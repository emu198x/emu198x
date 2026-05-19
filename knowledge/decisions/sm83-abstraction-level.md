# Decision: SM83 abstraction level — m-cycle, not T-cycle

**Date:** 2026-04-22

## The decision

The `sharp-lr35902` (SM83) CPU crate ticks at **m-cycle granularity**: one
tick advances one 4-T-state machine cycle. Pin state (`addr`, `data`,
`rd`, `wr`, `mreq`, `data_in`, `irq`, `halt`, `ime`) is valid for the
full m-cycle and the machine inspects it once per m-cycle between ticks.

The crate does **not** expose intra-m-cycle T-state phases. The shape is
still pin-level — the [`CPU bus interface`](cpu-bus-interface.md) rule
applies — but the granularity is coarser than the Z80 / 6502 / 68000
CPUs already in the tree.

## Why not T-cycle

[Half-cycle signals](half-cycle-signals.md) argues for the finest
granularity any observer requires. The observers on a Game Boy are:

- **Bus / memory / MBC** — sees one read or one write per m-cycle. The
  SM83 has no separate address-valid-vs-read-strobe edges exposed to
  the outside world.
- **PPU** — runs on the same 4 MHz base clock but is driven by dot
  counters, not by the CPU's T-state phase. The PPU and CPU share only
  the VRAM bus, and the PPU blocks the CPU at m-cycle granularity via
  the OAM/VRAM access modes (mode 2 / mode 3).
- **APU** — frame sequencer ticks at 512 Hz, driven by the timer's
  DIV counter. Channels advance on m-cycle edges.
- **OAM DMA** — copies one byte per m-cycle for 160 m-cycles.
- **Timer (DIV / TIMA / TMA / TAC)** — the internal 16-bit divider
  increments every T-cycle, but every documented edge case (TIMA
  overflow reload, the "obscure" TMA write-while-reloading behaviour)
  is defined at m-cycle precision and verified by mooneye-gb at
  m-cycle precision.
- **Interrupts** — latched on m-cycle boundaries; dispatch takes 5
  m-cycles.

No observer on the bus sees T-cycle edges. The T-cycle exists only
inside the CPU's own control unit, and the hardware test suites we
care about (Blargg `cpu_instrs`, `mem_timing`, `instr_timing`, and the
mooneye-gb timing tests) grade at m-cycle resolution.

T-cycle granularity would give unverifiable precision: four times the
state transitions, four times the surface area to maintain, and no
test in the Game Boy canon that distinguishes the T-cycle implementation
from the m-cycle one. That's overengineering disguised as rigour.

## The rule this generalises

**Match the finest-grained observation any component makes of the CPU.**

- Z80 on the Spectrum: the ULA observes signals at half-cycle resolution
  and gates the CPU clock — [half-cycle signals](half-cycle-signals.md).
- 6502 on the C64: the VIC-II asserts `BA` based on cycle-level CPU
  state — cycle-accurate is sufficient, half-cycle would be dead code.
- 68000 on the Amiga: Agnus allocates bus cycles at the CCK grain
  (2 CPU clocks each), and the 68000 bus protocol has address-strobe
  and data-strobe edges visible to Agnus — half-cycle is justified.
- SM83 on the Game Boy: no observer on the bus sees below m-cycle —
  m-cycle is sufficient.

The rule is not "always go as low as possible". The rule is "go as low
as anything on the bus actually looks". Anything finer is unverifiable
and costs maintenance every time the CPU touches a state transition.

## Why pin-level still applies

Coarser granularity does not relax [the pin-level rule](cpu-bus-interface.md).
The SM83 still exposes `addr` / `data` / `data_in` / `rd` / `wr` /
`mreq` / `irq` as public fields. The machine still performs the read or
write between ticks. The only difference versus the Z80 is the tick
cadence — one per m-cycle, not one per half-cycle.

This matters because the DMG has more than one bus client: OAM DMA
steals CPU bus cycles, and the PPU blocks CPU access to VRAM and OAM
during specific PPU modes. Both effects are m-cycle-resolution
observations of the CPU's pin state, and both want the pin-level model
for the same reasons the C64 / Amiga / Spectrum do.

## What this locks in

- `sharp-lr35902` ticks once per m-cycle.
- Pin fields are public and valid across the m-cycle.
- The machine inspects pins between ticks exactly like every other CPU
  in the tree.
- The SM83 state machine has an `m_cycle: u3` counter (like the Zig
  source we're porting from); we do **not** add a `t_phase: u2` below it.
- Blargg `cpu_instrs`, `mem_timing`, `instr_timing`, and the mooneye-gb
  timing tests are the acceptance bar. T-cycle precision is not a goal.

## Drift triggers

**Phrases that signal drift:**

- "Let's go to T-cycle for completeness."
- "M-cycle might be too coarse for the PPU."
- "The mooneye suite has some tests that might need T-cycle."
- "The Z80 is half-cycle, shouldn't the SM83 be too?"

**What to do when triggered:** list the specific observer that would
require sub-m-cycle precision. If the observer doesn't exist (it
doesn't, on the DMG bus), drop it. If it does, add that observer to
this page and reconsider. Don't add precision without an observer
demanding it.

## Related

- [Half-cycle signals](half-cycle-signals.md) — Z80-specific framing,
  generalised here.
- [CPU bus interface](cpu-bus-interface.md) — pin-level rule, which
  applies at whatever granularity the CPU ticks.
- [Nintendo Game Boy overview](../systems/nintendo-game-boy/overview.md)
  — the family this decision serves.
- [Sharp LR35902](../chips/sharp-lr35902.md) — the chip page.
- [Game Boy timing](../systems/nintendo-game-boy/timing.md) — where the
  m-cycle constants live.

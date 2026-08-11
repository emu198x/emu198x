# How the Spectrum emulator is meant to work

**Date:** 2026-08-11
**Status:** Describes the intended design, not everything the code does today.
Gaps between the two are listed at the end with pointers to the plan that
closes them.

This exists because a long contention session repeatedly mistook missing
conventions for design faults, and proposed rewriting an architecture that was
right. A description of the intent, written down once, is cheaper than
rediscovering it.

## The one idea

**The crystal drives everything, and the ULA decides whether the CPU gets a
clock tick.** Nothing schedules the CPU. Nothing catches up afterwards.
Contention is not a delay added to an instruction — it is a clock pulse the
Z80 never receives.

Every other rule below follows from that.

## The clock

One counter, `hc`, advancing at the master rate. There is no second notion of
time: no per-instruction budget, no T-state accumulator, no catch-up loop. A
frame ends when `hc` says so.

The ULA is stepped once per **half T-state** — 7 MHz on a 48K, two ULA steps
per Z80 T-state. That resolution is not decorative. Contention, the floating
bus, border effects and snow all live at it, and a T-state-resolution model
cannot express them.

The Z80 is stepped only when the ULA permits it. `driver.rs`:

```rust
tick_ula();
if !contended() || cpu_clock_active() {
    tick_cpu_and_bus();
}
```

A stalled half-cycle is a half-cycle the CPU simply does not experience. Its
pins hold, its internal state does not advance, and the ULA carries on.

## The bus

**Chips expose pins. The machine reads them. There is no bus callback.**

The Z80 has public `addr`, `data`, `mreq`, `iorq`, `rd`, `wr`, `m1`, `rfsh`.
The ULA is handed them each step and inspects them exactly as it would inspect
wires. A `Bus` trait or a `read(addr)` callback cannot express what this needs
to: several chips observing the same wires simultaneously, one of them
deciding whether the CPU advances at all.

**Pins persist between ticks.** A pin set during one CPU step stays driven
until the CPU steps again. So when the ULA reads pins that the CPU set on its
previous step, it is reading what the Z80 is *currently driving* — the same
thing the hardware sees. The ULA-before-CPU order in `driver.rs` is correct
for this reason, and is not a delay to be compensated for.

**Convention, load-bearing and easy to get wrong:** a Z80 phase handler sets
the pins that the ULA observes during the **following** half-cycle. `T1Rise`
setting `/MREQ` means the ULA sees `/MREQ` low from `T1`'s second half. Two
test harnesses have been written against the opposite assumption and produced
confident, wrong answers. `zilog-z80/tests/bus_pin_waveform.rs` measures this
and is the reference; treat any hand-derived pin timing as suspect until it
agrees.

## Contention

The ULA withholds `CPUClk` when three things coincide: the raster is inside
the contended window, the address bus is showing something the ULA wants, and
the access is at the point in its cycle where the ULA has priority.

The intended shape, from `SpecIde`'s signal-level ULA — the model this engine
targets, and the only other implementation at this abstraction level:

```
memContention    = contended_address && cpu_clock_phase
memContentionOff = /MREQ active                       // the live pin
ioContention     = ula_port && /IORQ active && cpu_clock_phase
ioContentionOff  = ula_port && /IORQ delayed          // Smith's IOREQTW3

contention = (memContention && !ioContention && !memContentionOff)
          || (ioContention && !ioContentionOff)
cpuClock   = !(contention && window[pixel])
```

Three properties matter and are easy to lose:

1. Memory contention keys off the **live `/MREQ`**, not a latched `MREQT23`.
2. I/O contention is **cancelled by a delayed `/IORQ`** — without it a ULA
   port contends for its whole cycle instead of once.
3. The two are **mutually exclusive**. A contended *port* address is charged
   once as I/O, not twice.

The window itself is `C2 | C3` of the pixel counter: high for 6 of every 8
T-states. It should be derived from counter bits rather than written out as a
table, so its phase is stated once.

**Which authority governs:** FUSE, for the window position. See
[`fuse-governs-the-contended-window.md`](fuse-governs-the-contended-window.md)
— the gate-level source opens three T-states later, and we deliberately do not
follow it.

**What judges the result:** real test programs, not model differentials. The
ZXSpectrum4.net timing survey is the primary gate; the frame-wide FUSE
differential is a diagnostic. See
[`spectrum-contention-the-way-out.md`](spectrum-contention-the-way-out.md).

## Video

`UlaEngine` owns rendering shared across every Spectrum variant: the fetch
sequence, the two-stage shifter, border latching, flash, and interrupt
generation. Variants configure it with timing constants; they do not
reimplement it.

The engine renders to a **palette-indexed `u8` framebuffer**, converted to
RGBA in a later stage. This is a ULA-family choice, not a fleet rule — most
other video chips here render straight to ARGB32.

The shifter is deliberately two-stage: a Data Latch and a Shift Register,
clocked by separate ULA signals. Collapsing them renders a byte one C0 cycle
early, which is visible in multicolour effects.

## Variants

One ULA implementation per chip family — `ferranti-ula-6c001e` (48K),
`sinclair-ula-7k010e` (128K/+2), `amstrad-ula-40077` (+2A/+3), `timex-scld`,
`pentagon-ula`, `scorpion-ula`. No parameterisation across families.

**What differs between variants is the decode, not the topology.** Which ports
the ULA answers, which pages are contended, where the window sits, whether
there is a floating bus at all. The contention *expression* is the same
silicon idea everywhere it exists. Today it is written out three times and the
copies have drifted — the 128K contends from `/Border`, the 48K from the
video-fetch window — which is a bug, not a variant difference.

## What the design buys

The engine can be compared to a gate-level model **half-cycle for
half-cycle**, and has been: zero divergences across all four port classes on
the real machine, with latch state cross-checked. An emulator whose contention
is a per-M-cycle table lookup cannot be compared at that resolution at all,
because it does not have it.

That capability is the whole point. It is also the standard to hold new work
to: if a change cannot be checked at half-cycle resolution against something
independent, the check is missing, not unnecessary.

## Where the code differs from this today

| Gap | Where | Fix |
|---|---|---|
| Five Z80 pin timings wrong against Zilog | `zilog-z80` `tick_m1`, `tick_mem_read`, `tick_mem_write`, `tick_io_read` | way-out plan, Phase 1 |
| No I/O cancellation, no mem/IO exclusion | `ferranti-ula-6c001e` gate | Phase 3 |
| `IORQ` delay line freezes during a stall | `UlaEngine::track_z80_clock` | Phase 3 |
| Window is a hand-written table, one pixel out from the counter-bit form | `DELAY_TABLE_48K` | Phase 1 (C) |
| Contention expression written out three times, already diverged | ferranti / 7k010e / scld | Phase 4 |
| Pin-presentation convention undocumented | — | this document, plus the golden waveform test |

Nothing in that list is a design fault. They are defects and undocumented
conventions in an architecture that is otherwise sound.

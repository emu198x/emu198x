# Each machine has its own machine crate

**Status:** Adopted 2026-10-26 (Steve: the CoCo 1 and 2 belong in their own
machine crate "irrespective of how similar they are" to the Dragon, and a
machine is distinct "whenever it has different hardware from others in its
family, even if those differences are subtle"). Binding through
[`RULES.md`](../../RULES.md) rule 34.

## The decision

Every machine gets its own `machine-*` crate and its own `machine_id` in
[`docs/status/systems.toml`](../../docs/status/systems.toml). Two machines
never share a machine crate. Hardware they have in common lives in a crate
both depend on, a `common-<family>` crate or a chip crate. It never lives in
one machine's crate for the other to borrow, and no machine crate depends on
another.

Machines are still grouped into families. A family can share one runtime
and ship as one emulator that runs all of its machines. The rule applies to
the machine layer, not to what we ship.

## What counts as a machine

A machine is distinct when its hardware differs from the others in its
family in any way, however small:

- a different chip, or a different revision of one (a 6581 or 8580 SID, a
  315-5124 or 315-5246 VDP);
- a PAL or NTSC board, whether the difference is a chip, a crystal, a strap
  or a diode (the ZX80's D11, the ZX81's ULA pin 22);
- added or removed hardware (the Dragon 64's ACIA, the 130XE's banked RAM,
  the A2000's Zorro slots);
- the same chips on a different board layout (where each MSX maker put its
  RAM in the slot map; how the 520ST and 1040ST wire their RAM banks).

These are **not** separate machines; they are profiles of one:

- the RAM fitted on the same board (the MTX500 and MTX512);
- a ROM revision;
- the case or keyboard (the Spectrum+ is a 48K in a new case);
- an add-on: a RAM pack, a disc interface such as the Electron's Plus 3, a
  trapdoor expansion such as the A501, an accelerator such as the GVP A530.
  An add-on with hardware of its own is a peripheral crate, not a machine.

When the code and the reference library disagree about what a board
contains, the reference library decides. If it is silent, ask.

## Why

- **Simpler wiring.** A machine crate wires one board. It hard-wires its
  chips and timing instead of branching on a model or region, and the
  differences between boards become differences between crates.
- **Evidence.** A machine crate's tests are evidence about that machine only
  when nothing else lives in it. `current-system-usability.md` marks shared
  crates with ⚠️ because a passing Dragon test says nothing about the CoCo.
- **Names.** A machine inside another's crate hides behind the other's
  names. The Game Boy Color sat under the Game Boy's `machine_id`, and the
  Master 128 under the BBC Micro's, so neither appeared in the registry.
- **Precedent.** The C128 has its own crate beside the C64's. The Spectrum
  models each have one, with family code in `common-sinclair-zx-spectrum`.

## Cost

There will be many more machine crates, roughly one per regional board, and
each one that is published counts against crates.io's limit on new crates.
Each machine crate should stay thin, wiring a board over its family's
`common-*` crate, so the extra crates add little code.

## Where it applies today

Every machine crate that holds more than one machine is listed in #2144.
Some have their own issues already: #2138 (CoCo),
#2139 (MSX), #2140 (Game Gear), #2141 (Game Boy Color) and
#2142 (BBC Master 128). The SG-1000 crates name an SC-3000 that nothing
emulates (#2143). When it is emulated, it gets its own crate.

## Drift triggers

- Adding a model, region or board variant to a machine crate. STOP: if it
  differs in hardware, it is a new machine crate.
- A `match` on model or region inside a machine crate. That is two machines
  sharing one crate.
- Adding a dependency from one `machine-*` crate on another. The shared part
  belongs in a crate both depend on.
- A crate description or changelog naming a machine that no crate builds.

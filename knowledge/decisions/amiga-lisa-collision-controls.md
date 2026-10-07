# Decision: Match Lisa collision controls at each source pixel

**Date:** 2026-10-04

**Status:** BINDING

## Evidence

The [AGA register synthesis](../../../../reference/by-system/commodore-amiga/amiga-aga-and-chip-revisions.md#39-collision-detection-on-lisa-clxcon2)
records CLXCON2 at `$DFF10E`: bits 6/7 enable BP7/BP8 comparison and bits
0/1 supply their requested values. WinUAE `drawing.cpp::expand_colmask`
and Minimig `rtl/denise_collision.v` implement this same layout. Their base
CLXCON write clears the extension. Reserved bits do not enter comparison.

The [display synthesis](../../../../reference/by-system/commodore-amiga/amiga-graphics-display.md#37-collisions)
records source-family agreement on simultaneous collision matching. WinUAE's
`denise_collide_sprites` and Minimig's `oddmatch`/`evenmatch` distinguish
single and dual playfields: in single-playfield mode, odd-plane sprite collision
also requires the even planes to match; in dual mode they match independently.

Source-pixel granularity follows WinUAE's renderer. Minimig independently agrees
on the register layout and match logic, but its `clk7_en` capture cadence does
not establish the full hires/superhires sampling phase. The source revisions are
WinUAE `c32694e338fa5f34977f522eb4898adb069d2e73` and Minimig
`3ab91cd9220d4d047886d215b515227cbe568bdd`. These are audited implementation
observations, not physical captures.

## Decision

Only Lisa decodes CLXCON2. The shared rendering core holds its serialized value
because that core consumes raw bitplane samples and latches CLXDAT. OCS/ECS
ignore writes to the extension and exclude it from their comparisons. Every
CLXCON write clears CLXCON2 while leaving already latched collision bits intact.
The read-only chip diagnostic snapshot reports both controls.

The board register surface adds CLXDAT's fixed bit 15 to destructive CPU reads
and non-destructive debug inspection. The raw chip latch remains a 15-bit
collision state; the fixed bit is never saved as a detected collision. The
primary display synthesis records independent WinUAE/vAmiga agreement on the
read value.

BP7 joins the odd group; BP8 joins the even group. Disabled planes impose no
condition; enabled planes must equal their requested zero or one. The existing
sprite collision group enables and sprite-to-sprite matrix remain separate from
these bitplane comparisons. Visible sprite priority does not select collision
membership.

Each actual source pixel is compared separately. Collision events are OR-ed
into CLXDAT after comparison. Adjacent hires/superhires bitplane masks must
never be OR-ed before comparison: that would invent simultaneous set bits
or discard requested zero-bit matches. The aggregate plane-bit diagnostic
remains a summary only. Existing display eligibility and sprite timing gates
are preserved; this decision does not calibrate blanking or register propagation.

Snapshot version 39 persists CLXCON2. Version 38 is rejected before payload
decode because it cannot retain the enable/match state.

## Verification

- A 64-case truth table checks BP7/BP8 values, enables and requested matches
  against emitted plane bits, all associated sprite/playfield collision bits,
  independent sprite-to-sprite collisions, and CLXDAT read-and-clear.
- Base-register writes clear the extension without clearing the latch; reserved
  and disabled extension bits cannot override the first six planes.
- Single/dual fixtures distinguish the odd-group gating rule.
- Alternating plane data checks that hires and superhires samples never create
  a simultaneous collision, including requested zero bits.
- An MC68020 guest program writes the real custom-register address through
  CPU bus dispatch. Runtime snapshots retain the control, reproduce forward
  execution, and preserve the base-write reset behavior after restore.
- OCS/ECS ignore the AGA-only write. Existing video and scroll reference gates
  remain independent of these truth tables.

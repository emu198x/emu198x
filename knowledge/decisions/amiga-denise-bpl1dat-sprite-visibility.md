# Decision: Require BPL1DAT before normal Denise sprite contribution

**Date:** July 2026

## The question

What must happen on a raster line before an armed Denise sprite may
contribute pixels and collision codes to normal display output?

## Evidence

The third-edition *Amiga Hardware Reference Manual*, printed pages 123–126
(PDF pages 138–141), documents the sprite data registers, horizontal
comparator and parallel-to-serial converters. It states that writing
`SPRxDATA` arms the comparator, writing `SPRxCTL` disarms it, and an armed
converter repeats its current data at the selected horizontal position on
successive lines.

The same manual identifies `BPL1DAT` as bitplane-one data and the trigger for
bitplane parallel-to-serial conversion. It does not say that a `BPL1DAT`
arrival also controls sprite visibility, nor does its sprite block diagram
show a per-line `BPL1DAT` prerequisite. The behaviour in this decision is
therefore not presented as a manufacturer-documented programming rule.

The inspected WinUAE revision
`c32694e338fa5f34977f522eb4898adb069d2e73` models the behaviour explicitly
in `drawing.cpp`:

- `hstart_new` resets the `bpl1dat_trigger` state and records separate
  horizontal-display-window and BPL1DAT reasons for hiding sprites;
- `do_hstrt_ecs` removes the horizontal-display-window reason independently;
- `bpl1dat_enable_sprites` removes the BPL1DAT reason;
- `expand_drga_early` invokes `bpl1dat_enable_sprites` for register `$110`
  before the later bitplane-visible and parallel-copy stages;
- `denise_render_sprites` continues to advance sprite serial state while
  hidden; and
- only unhidden sprite data reaches `denise_render_sprites2`, where normal
  sprite composition and collision accumulation occur.

This separates three operations that must not be conflated: arming a sprite,
advancing its horizontal shifter, and allowing its current code to reach the
priority and collision logic.

The registered vAmiga Amiga Test Kit v1.21 reference provides an independent
end-to-end observation rather than an implementation description. After the
separate sprite horizontal-output phase was corrected, the EBU-bars case
retained 114 differing pixels confined to the menu-pointer region. The
surrounding playfield and the pointer in the other settled patterns isolated a
line-dependent visibility difference rather than a general sprite-position or
palette error.

## The decision

Denise retains a line-local BPL1DAT sprite-visibility latch for normal display
output.

At the start of each raster line represented by the current display pipeline,
the latch is clear. An arrival at `BPL1DAT` sets it, regardless of the word's
value. Both direct CPU or Copper register writes and an Agnus bitplane-one DMA
fetch use the same transition. A write to another bitplane data register does
not set it.

The transition belongs to the `BPL1DAT` register-arrival stage. It is not
deferred until BPLCON1's bitplane-copy phase, and it does not depend on a
non-zero bitplane count. A direct zero-valued `BPL1DAT` write can therefore
enable normal sprite contribution even when bitplane DMA is not producing a
playfield.

An armed sprite's horizontal comparator and serial shifter continue to run
while the latch is clear. If `BPL1DAT` arrives after HSTART, the next visible
sprite code is the serial position already reached; the sprite does not
restart from its first bit.

Before the latch is set, ordinary sprite codes do not enter display priority,
palette selection, sprite-to-sprite collision or sprite-to-playfield
collision. After it is set, they remain subject to the normal display-window,
blanking and priority rules.

The latch is live mid-line state. Save states preserve it so restoring between
the line reset and a later `BPL1DAT` arrival cannot expose a sprite early or
hide one that was already enabled.

The one-low-resolution-pixel delay between HSTART and the first shifted sprite
bit remains a separate operation. It is defined by
[Amiga sprite horizontal output phase](amiga-sprite-horizontal-output-phase.md).

## Model boundary

This decision establishes the normal OCS display behaviour exercised by the
registered A500+A501 PAL lane. It does not claim complete border and blanking
semantics across every Denise and Lisa revision.

The pinned WinUAE implementation contains the following additional
distinctions:

- on A1000 and other OCS Denise configurations,
  `bpl1dat_enable_sprites` does not enable sprites while the internal colour
  `BURST` signal is active; ECS Denise is exempt from that restriction;
- AGA `BPLCON3.BORDSPR` removes both the horizontal-display-window and
  BPL1DAT hidden reasons, with separate border-blank and physical-blank
  interactions;
- `hstart_new` resets the state at horizontal blank only while Denise is
  outside vertical blank, permitting vblank and programmed-blank carry cases
  that are not equivalent to clearing an abstract flag on every raw line; and
- `do_hstrt_ecs` contains an OCS/ECS border-transition collision case in which
  the preceding sprite code can collide with a zero playfield.

Those cases require revision-aware BURST, BORDSPR, blanking and border
transition inputs. They are deferred rather than approximated by this
line-local normal-output latch.

The existing board renderer also combines horizontal and vertical display
eligibility before it calls the shared compositor. Whether manually armed
sprites can contribute outside the vertical display window is a separate
display-window question and is not decided here.

This decision likewise does not define the bitplane border latch or the exact
OCS, ECS and AGA sub-pixel distance between sprite enable and the first
bitplane-visible pixel. It fixes only the ordering needed here: the sprite
visibility transition occurs on BPL1DAT arrival, before the deferred
bitplane-copy operation.

## Verification

Hermetic Denise tests establish that:

- an otherwise armed sprite remains hidden on a line without `BPL1DAT`;
- a zero-valued `BPL1DAT` write enables normal sprite output even with BPU
  zero;
- a later line is hidden again until its own enabling arrival;
- the sprite shifter advances while hidden, so a late enable exposes the
  current serial position rather than restarting the sprite; and
- ordinary hidden sprite codes do not latch collisions, while enabled codes
  do.

Snapshot verification preserves the latch across a mid-line round trip and
rejects the preceding runtime schema rather than silently defaulting the new
state.

The system-level rerun removed all 114 EBU-bars menu-pointer-region
differences left by the earlier phase-only run. Gradients, the static
checkerboard, both alternating-checkerboard phases, EBU bars and dots now
match their registered references exactly.

The strict lane remains red only for the independently attributed crosshatch
residual: 56 pixels in canonical columns 712–715 across 14 horizontal lines.
Its count and location were unchanged by this decision.

The boot golden matrix identified one expected change in its Emu198x-produced
baselines. The A500 Workbench 1.3 desktop lost a 10-by-2-pixel rectangle at
golden coordinates `(74,34)..(83,35)`: the old image exposed the first
line-doubled row of the mouse pointer before that line's BPL1DAT prerequisite.
The change was confined to those 20 pointer pixels, reviewed against this
decision and the independent Test Kit result, then accepted as the new
regression baseline. The updated row passes strictly. Every other runnable
matrix row remained exact; the A1000 Workbench 1.2 row was skipped because its
Kickstart disk was unavailable.

## Drift triggers

Reject these patterns:

- treating `SPRxDATA` arming as sufficient to bypass the line's BPL1DAT
  prerequisite;
- enabling sprites only when the BPL1DAT value or BPU count is non-zero;
- waiting for the bitplane parallel copy before enabling sprite contribution;
- pausing or reloading sprite shifters while their output is hidden;
- letting hidden ordinary sprite codes enter the collision matrix;
- reconstructing the latch from current registers during snapshot restore; or
- claiming that this normal OCS path implements BURST, BORDSPR, vblank carry
  or border-transition collision behaviour.

## Related documents

- [Amiga sprite horizontal output phase](amiga-sprite-horizontal-output-phase.md)
- [Amiga sprite DMA lifecycle](amiga-sprite-dma-lifecycle.md)
- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga Test Kit v1.21 video conformance](../processes/amiga-test-kit-video-conformance.md)

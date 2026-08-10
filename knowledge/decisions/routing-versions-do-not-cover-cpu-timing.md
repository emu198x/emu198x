# Decision: routing versions do not cover CPU timing

**Date:** 2026-08-10
**Status:** ACTIVE
**Applies to:** every system with catalogue routing-version constants

## The question

`FRAME_ROUTING_VERSION` and `AUDIO_ROUTING_VERSION` exist so a behaviour change
cannot silently relabel captured output as expected. What behaviour changes do
they actually cover, and what falls through?

## The gap

They cover **rendering and audio-routing** changes, by convention: an author
changing the ULA's output path bumps the constant, and Seam 4 then refuses
every stale hash until the affected entries are re-captured. That works.

They do not cover **CPU timing**. A Z80 change alters instruction and interrupt
costs, which changes what a machine has computed by any given frame, which
changes captured frame and audio hashes — and bumps nothing. The manifest goes
on declaring a version that no longer describes the machine, and the gate stays
green because the version still matches.

## Evidence

The 2026-08-09 Spectrum re-capture used the 48K family as a control: frame
v3→v4 touched only `CONFIG_128K` and `CONFIG_PLUS2A`, so every 16K / 48K /
Spectrum+ frame hash had to return byte-identical. 46 of 47 did.

The exception was `sabre-wulf-kempston-start`, and its cause was CPU timing:
`4284a55d` (Z80 interrupt response bus cycles) and `faecec34` (interrupt
acknowledge WAIT states), both 2026-07-26, both all-variant, both bumping no
routing version. That entry is the only 48K-family entry capturing live
gameplay ~900 frames in rather than a static screen, so it was the only one
sensitive enough to notice.

The damage was not the hash. The entry's documented semantic anchor —
"without-FIRE 1UP 000545 versus with-FIRE 001070 from a sabre-swing kill" —
had quietly become false: both runs now score 001050, and the discriminator is
the lives counter. Verified by capturing the entry with and without its FIRE
step; the routing is live, the anchor was not.

Had the re-capture been mechanical, that comment would have survived as
documentation of a mechanism that no longer happens.

## The decision

**Treat a CPU-timing change as a catalogue-affecting change**, even though no
constant forces you to.

1. **Run the affected system's catalogue after a CPU timing change.** Not the
   whole fleet — see `EMU198X_CATALOGUE_SYSTEMS`.
2. **Expect deep-state entries to move and static-screen entries not to.** An
   entry capturing live gameplay hundreds of frames in is timing-sensitive by
   construction; a title screen is not. A CPU change that moves a *title
   screen* hash deserves investigation, not re-capture.
3. **Re-derive semantic anchors on re-capture, never just the hash.** If an
   entry's comment claims a score, a state or a visible effect, verify the
   claim still holds and rewrite it with what was actually observed.
4. **Prefer anchors that cannot coincide.** A score is fragile: two runs can
   converge on one by unrelated routes, which is exactly what happened here.
   Prefer asserting that two runs *differ* — that tests the mechanism directly
   rather than a downstream consequence of it.

## Why not just add a CPU routing version

Considered and rejected for now. A `CPU_TIMING_VERSION` would fire on every
Z80, 6502 or 68000 change across every system sharing that core, forcing
fleet-wide re-captures for changes that move nothing in most entries. That
trades a silent gap for loud noise, and noisy gates get bypassed — which is the
failure mode in
[a gate nobody runs is a silent gate](a-gate-nobody-runs-is-a-silent-gate.md).

Revisit if CPU-timing changes start moving catalogue hashes often enough that
the discipline above is not being followed.

## Drift triggers

Stop and re-read this decision if you find yourself:

- Changing instruction or interrupt timing and reasoning that no routing
  version needs bumping, therefore nothing needs re-running.
- Re-capturing a hash without reading the entry's comment.
- Explaining away a moved hash on a *static-screen* entry as "timing drift".
- Writing a new catalogue entry whose discriminator is a score, a counter or
  any value two different runs could arrive at independently.

## Related Documents

- [Spectrum accuracy closure campaign](spectrum-accuracy-closure-campaign.md)
- [A gate nobody runs is a silent gate](a-gate-nobody-runs-is-a-silent-gate.md)
- [Spectrum architecture review](spectrum-architecture-review.md) — Seam 4
- [Spectrum catalogue manifest](../../crates/emu198x-catalogue/manifest/spectrum.toml) — `sabre-wulf-kempston-start`

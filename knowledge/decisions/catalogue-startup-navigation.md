# Decision: Catalogue startup navigation

**Date:** 2026-08-08
**Status:** BINDING

## The question

How does a catalogue entry pass a release screen, cracktro, trainer, selector
or guest prompt before reaching the software waypoint under test?

## Decision

The catalogue drives the guest through ordinary emulated input. It does not
patch media, poke guest memory, install a trap, replace the boot path or inject
a post-prompt machine state.

New entries describe a sequential `entry.startup` list. Its strictly tagged
actions are limited to:

- advancing an exact number of native machine frames;
- pressing and releasing one symbolic keyboard key;
- clicking one button on the primary emulated mouse; and
- pressing and releasing one named controller button on a labelled port.

Actions run in manifest order and waits are relative to the preceding action.
Input actions expand to the shared shell's normal press event, exact frame
hold, release event and one release-observation frame. That final frame makes
the released state guest-visible before an adjacent action can press the same
control again. The default hold is three frames. The entry's ordinary
`boot.wait_frames` begins only after startup navigation finishes.

Every action is bounded. A single wait may consume at most 60,000 frames, an
input hold at most 120 frames, and the complete startup list—including release
observation—at most 120,000 frames. Zero-length actions, unknown fields and
empty input names are rejected. An entry cannot combine the sequential form
with the legacy absolute-frame `entry.script` form.

These bounds make a checked-in navigation path deterministic and suitable for
unattended catalogue execution. They do not assert that a particular prompt
appeared; the unchanged frame and audio waypoints, followed by snapshot and
replay checks, remain the assertions.

## Evidence

The Ackerlight release of *Arkanoid: Revenge of Doh* is the first closed case.
The entry waits 6,500 PAL frames, dismisses the intro with the ordinary left
mouse-button path, waits another 6,000 frames and reaches the established
Imagine title waypoint without changing its golden:

- frame: `xxh64:5155963b4ae77b9d`;
- audio: `xxh64:f6389c53ba66240d`;
- catalogue result: exact `PASS` and `SNAP-PASS`.

`SNAP-PASS` covers byte-identical snapshot re-encoding plus matching frame and
audio replay from a fresh runtime restored at the waypoint. It is evidence for
this entry only.

The complete final-core run covered ten Amiga entries across OCS PAL, OCS
NTSC, ECS and AGA. All ten passed the snapshot/replay gate. Six retained their
exact frame and audio hashes in the full sweep. The four deterministic phase
shifts were reviewed as coherent guest output, requalified and rerun
individually; all four then passed their exact frame/audio and snapshot/replay
checks. Banshee's final waypoint is the midpoint of a settled POWERUPS page
whose 100-frame samples matched across an 800-frame span. This closes
catalogue navigation for the current Amiga set; it does not claim
compatibility beyond those entries.

## Why

Release screens and trainers are guest software. Passing them through the same
mouse, keyboard or controller path used interactively exercises the actual
machine and retains the original media as evidence. A media-specific patch or
memory shortcut could make the catalogue green while bypassing the chipset,
input, loader or protection behaviour that the entry is meant to cover.

A deliberately small action language is sufficient for deterministic known
prompts and keeps catalogue manifests auditable. More general scripting,
unbounded polling and guest-state mutation do not belong in this layer.

## Related Documents

- [October catalogue](october-catalogue.md)
- [No ROM trap-load](no-rom-trap-load.md)
- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)
- [Amiga disk rotation and DMA arbitration](amiga-disk-dma-fifo-arbitration.md)
- [Save-state: serde the live machine](savestate-live-machine-serde.md)

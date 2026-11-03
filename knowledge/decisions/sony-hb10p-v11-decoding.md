# Sony HB-10P corrected decoding uses v11 saves

Adopted 2026-11-03 for #2200. Steve explicitly approved writing v11 and
rejecting this board's v10 saves before payload decoding or mutation. The
other eight named MSX boards retain v10. Original fixtures remain immutable;
no migration relabels the incorrect topology or silently changes its execution.
This supersedes the HB-10P v10 continuation guarantee in the named-board split.

Sony's PU-37 schematic and Yamaha S3527 pin table establish that slot-0 page 3
selects the same physical RAM as slot 3, and A2 is absent from the engine's
I/O address inputs. The fixed owner chooses this wiring; other owners retain
their decoder. The immutable decoder choice is reattached after construction
and checked restore, and carries no live serialized state. The additional RAM
region aliases the existing store; it allocates no second RAM.

Primary audit: umbrella `reference/by-system/msx/sony-hb10p-pu37-decoding.md`,
with original manufacturer manual acquisition identity and page references.
The fitted video part is T6950 and sound is the S3527's integrated SSG;
this decoding correction does not claim a complete T6950/SSG personality,
printer peripherals or analogue calibration.

Tests must preserve all other named-board v10 bytes/output, reject Sony v10
atomically, check actual v11 foreign topology rather than only version
mismatch, and compare native video/audio/clocks/full state across restore.

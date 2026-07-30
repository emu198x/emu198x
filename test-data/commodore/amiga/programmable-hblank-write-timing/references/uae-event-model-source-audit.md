# UAE Mid-Line HBLANK Event Model

This document answers how the registered UAE-family source handles mid-line
writes to programmable-horizontal-blanking registers.

The audited source is FS-UAE revision
`f362278ccd4c60991caac3b4d240d4a3f751bea2`, whose chipset core identifies
itself as derived from WinUAE 6.0.1.

## Comparator writes

The ECS `HBSTRT` and `HBSTOP` write path replaces the active comparator value
and its event-table entry. It does not compare the new value with the current
beam position at write time (`custom.cpp`, `write_hbstrt` and
`write_hbstop`, near line 7574).

Later equality events set or clear the persistent Agnus programmable-blank
state (`custom.cpp`, `check_vidsyncs`, near line 11920). A comparator moved
behind the beam therefore misses that event for the current line. A
comparator moved to a position still ahead can match later on the same line.

On AGA, the write is also delivered through the Denise/Lisa register pipeline
(`custom.cpp`, near line 4389; `drawing.cpp`, near lines 3975 and 4766).
Pixel-phase equality events set or clear persistent Lisa programmable-blank
state (`drawing.cpp`, `do_phbstrt_aga` and `do_phbstop_aga`, near line 4545).
The write itself does not synthesize an edge.

## Selector writes

`BPLCON0.ECSENA` and `BPLCON3.EXTBLKEN` update the enhanced-blank selector
when their Denise-side writes arrive (`drawing.cpp`, `update_ecs_features`,
near line 3025). They do not manufacture comparator events.

The ECS output path continuously applies the selected CSYNC-derived blank
state. Enabling either selector can therefore expose an already-latched ECS
state after the register pipeline delay (`drawing.cpp`,
`do_exthblankon_ecs`, near line 3570).

The AGA output path applies its programmable state from the start and stop
handlers. Enabling its selector after the start event does not reapply the
already-latched raw state (`drawing.cpp`, `do_exthblankon_aga`, near line
3595).

`BEAMCON0.BLANKEN` participates in the ECS route but is not part of the AGA
programmable-comparator selector. The registered steady-state package records
the same AGA distinction.

## Evidence boundary

This audit predicts discriminating outputs and explains the registered
producer. It does not prove the physical chipset circuit, register pipeline
delay, or precedence for a write coincident with an edge.

## Related documents

- [Comparator capabilities](comparator-capabilities.md)
- [Corpus overview](../README.md)
- [Steady-state FS-UAE package](../../programmable-hblank/references/fs-uae-5.0.7-f362278c/README.md)

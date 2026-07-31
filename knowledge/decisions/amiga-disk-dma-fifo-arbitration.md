# Decision: Amiga disk rotation and DMA arbitration

**Date:** 2026-07-31
**Status:** BINDING

## The question

How do rotating floppy data, Paula's disk controller and Agnus's chip-memory
slots interact in the running Amiga machine?

## Decision

Disk rotation and disk memory traffic are separate events.

The drive advances its encoded track stream at the selected rotational rate.
On reads, each paced word reaches Paula whether or not disk DMA is enabled.
On writes, each paced opportunity removes one buffered word from Paula and
delivers it to the drive. This stream updates the disk data and byte latches;
it does not access chip RAM.

Agnus alone moves disk words across the chip bus. Its fixed disk cells at
horizontal positions `$07`, `$09` and `$0B` service Paula's FIFO when both
`DMAEN` and `DSKEN` are set. A read grant moves one queued drive word to
`DSKPT`; a write grant moves one word from `DSKPT` into the FIFO. `DSKPT`
advances only after an accepted memory transfer.

The machine services an Agnus disk grant before a rotational word event at the
same CCK. This is the event order used by the registered vAmiga comparator and
prevents a newly arrived word from being consumed by an earlier bus cell.

## Paula FIFO

Paula retains a bounded three-word FIFO distinct from CPU-written `DSKDAT`.
The FIFO has an explicit read or write direction and is part of serialized
machine state.

For read DMA:

- rotational arrivals update `DSKDATR` and `DSKBYTR` even when DMA is idle;
- an armed transfer queues at most three complete words;
- a full FIFO preserves its existing words and drops the new arrival;
- an Agnus grant consumes the oldest word and decrements `DSKLEN`;
- `WORDSYNC` prevents memory service while searching, then discards the
  accumulated alignment words and the matching sync word before opening the
  gate.

For write DMA:

- an Agnus grant is requested only while the FIFO has room;
- the transfer count decrements only when the fetched word is accepted;
- `DSKBLK` is raised when the final memory word enters the FIFO;
- the completed transfer remains stream-active until every buffered word has
  rotated out to the drive.

The second `DSKLEN` arm starts a new transfer, clears stale FIFO contents and
selects the new direction.

## Rotational pace

The sector-derived DD track contains 12,668 encoded bytes and represents one
300 RPM revolution. `ADKCON.FAST` set selects the normal 2 µs MFM bit-cell
clock. At that rate the shared driver advances a PAL stream every 112 CCKs per
word; the NTSC approximation uses 113 CCKs. Clearing `FAST` selects the 4 µs
GCR-compatible clock and doubles those intervals.

Paula's byte latch advances after 56 PAL CCKs with `FAST` set and after 112
CCKs with it clear. An earlier revision incorrectly treated `FAST` as a
multiplier applied to an already-normal MFM rate. It therefore delivered one
whole word every 56 CCKs, overflowed the three-word FIFO faster than Agnus's
three disk cells per line could drain it, and corrupted ordinary Kickstart
track reads. The portable Paula-audio probe exposed the regression by failing
to boot through the current disk path.

## Persistence and inspection

The FIFO contents, direction and transfer state affect the next granted cell
and must survive restore. Amiga runtime snapshots therefore advance from
version 29 to version 30.

The `disk.*` query group exposes FIFO contents, direction, occupancy,
empty/full state, memory-transfer activity and write-stream activity. The
latter two states differ after the final write grant while buffered words
still await the drive.

## Evidence boundary

The three-word FIFO, three fixed disk cells and separation between rotational
events and memory service agree with the registered vAmiga and WinUAE source
trees under `emulators/amiga/`. The implementation is tested at the Paula
component boundary, at the machine's Agnus-grant boundary and through full
ADF write-back.

This is not yet a bit-cell or flux model. The NTSC word interval is an integer
approximation, and the active media path remains sector-derived ADF. Weak
bits, custom track lengths, IPF and flux timing remain outside this decision.

## Related documents

- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)
- [Save-state: serde the live machine](savestate-live-machine-serde.md)

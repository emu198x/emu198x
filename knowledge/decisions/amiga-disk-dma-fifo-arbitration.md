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

Agnus alone moves disk words across the chip bus. Its fixed D0, D1 and D2
cells at horizontal positions `$07`, `$09` and `$0B` service Paula's FIFO
when both `DMAEN` and `DSKEN` are set and Paula requests that particular
cell. A read grant moves one queued drive word to `DSKPT`; a write grant moves
one word from `DSKPT` into the FIFO. `DSKPT` advances only after an accepted
memory transfer. An enabled but idle disk channel does not reserve all three
cells from the CPU.

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
- one queued word requests D2, two request D1 and D2, and three request all
  of D0, D1 and D2;
- an Agnus grant consumes the oldest word and decrements `DSKLEN`;
- `WORDSYNC` prevents memory service while searching, then discards the
  accumulated alignment words and the matching sync word before opening the
  gate.

For write DMA:

- all three fixed cells are eligible while the transfer has words remaining
  and the FIFO has room;
- the transfer count decrements only when the fetched word is accepted;
- `DSKBLK` is raised when the final memory word enters the FIFO;
- the completed transfer remains stream-active until every buffered word has
  rotated out to the drive.

The second `DSKLEN` arm starts a new transfer, clears stale FIFO contents and
selects the new direction.

The FIFO and transfer state from which the request mask is derived, plus the
record of whether disk DMA actually used the current CCK, are serialized
machine state. The actual-use latch remains valid across both CPU phases of
the CCK. This matters on the final transfer: Paula may clear its live request
while servicing the word, but the CPU must not reuse the cell that disk DMA
already consumed.

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

Changing cylinder or head replaces the encoded-track cache without resetting
the rotating word phase. The cursor is retained modulo the replacement
track's word count. Mounting replacement media or ejecting DF0 invalidates the
cache so the next rotational event cannot consume bytes encoded from the
previous disk; these media operations do not otherwise manufacture a new
spindle phase.

When a completed writable MFM capture persists at least one decoded sector,
the machine also invalidates that track's cached encoding without resetting
the rotational word cursor. The next read therefore re-encodes the live ADF
at the retained phase instead of replaying pre-write MFM bytes. A failed or
write-protected capture changes neither the image nor its cache.

## Persistence and inspection

The FIFO contents, direction and transfer state, actual same-CCK use, track
cache and rotational cursor affect the next event and must survive restore.
The original FIFO change advanced Amiga runtime snapshots from version 29 to
version 30.

The current version-34 envelope also stores whether DF0 is writable. The disk
image object itself is omitted from the machine postcard, so restore first
installs the serialized drive and track state and then reattaches the persisted
ADF object without raising `/DSKCHANGE`, resetting mechanical state or
invalidating the restored encoded-track cache. Encoding reads the live ADF
bytes, including completed guest writes. A restored read-only archive remains
write-protected. This ordering restores snapshot byte fixed-point behaviour
for a machine captured during real disk activity.

The `disk.*` query group exposes FIFO contents, direction, occupancy,
empty/full state, overrun count, D0/D1/D2 request mask, memory-transfer
activity and write-stream activity. The latter two states differ after the
final write grant while buffered words still await the drive. Agnus
arbitration diagnostics expose both the planned disk owner and recorded
same-CCK use.

The shared memory-write watch identifies chip-RAM writes from the CPU, the
blitter D channel and disk read DMA. Each record carries its source, CCK,
address, value, width and the concurrent CPU PC. For DMA records that PC is
context, not an instruction attributed as the writer. Source and inclusive
CCK filters allow a long capture to isolate disk traffic without changing the
legacy CPU-only watch stream.

## Evidence boundary

The three-word FIFO, three fixed disk cells and separation between rotational
events and memory service agree with the registered vAmiga and WinUAE source
trees under `emulators/amiga/`. The read request-stage mask follows WinUAE's
`disk_dmal` mapping. vAmiga currently takes the earliest available cell
instead, so direct hardware confirmation of the exact read stage remains
desirable. Write DMA still requests any fixed cell while the FIFO has room;
separate D0/D1/D2 write-stage timing is not claimed.

The implementation is tested at the Paula component boundary, at the
machine's Agnus-grant and same-CCK CPU-arbitration boundaries, through full
ADF write-back and immediate re-read on OCS, ECS and AGA, and through
version-34 read-only and writable snapshot fixed-point restore.

This is not yet a bit-cell or flux model. The NTSC word interval is an integer
approximation, and the active media path remains sector-derived ADF. Weak
bits, custom track lengths, IPF and flux timing remain outside this decision.

## Related documents

- [One Agnus DMA-slot authority per CCK](amiga-single-slot-authority.md)
- [Amiga accuracy closure campaign](amiga-accuracy-closure-campaign.md)
- [Save-state: serde the live machine](savestate-live-machine-serde.md)
- [Catalogue startup navigation](catalogue-startup-navigation.md)

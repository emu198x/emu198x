# Decision: DMA chips read memory through a narrow trait

**Date:** 2026-09-01

## The decision

**A chip that addresses memory on its own behalf — ANTIC, the VIC-II, and whatever comes next — reads it through a narrow, read-only trait supplied by the machine at the moment of the fetch. Not a snapshot handed over in advance, and not a slice.**

```rust
pub trait AnticMemory {
    fn read(&self, addr: u16) -> u8;
}
```

The machine implements it on a small view struct that borrows its live fields, and passes it into the chip's per-line or per-cycle entry point. Because the view borrows fields rather than `&self`, the chip can be borrowed mutably alongside it.

## Hardware behaviour, independent of interface shape

The former CPU interface-shape ban was retired on 2026-10-28; see
[cpu-bus-interface.md](cpu-bus-interface.md). It no longer forbids traits or
callbacks for either CPUs or DMA chips. This decision's narrow memory view
remains useful because it reads the live mapping at the fetch point and
avoids a duplicated memory image. Its correctness depends on those semantics,
not on being an exception to a ban.

## Why not a snapshot

`machine-atari-800xl` handed ANTIC a 64 KB copy of memory rebuilt once per frame. Everything the CPU wrote during the frame was invisible to the display until the next one, so a DLI that repoints the character set or the display list saw stale memory, and a program that built a display list and enabled DMA in the same frame walked a list of zeros, never reached its own JVB, and lost its display-list pointer permanently. That was issue #1384.

`machine-atari-5200` kept a shadow too, but wrote through it: `mem_write` mirrored every RAM write, and a second path re-baked a cart window when a bank register was touched. It was correct, and it shows the cost of the shape — 64 KB of duplicated state, a bespoke serde adapter to get the boxed array through postcard, and two places that have to remember to refresh. The second one was added only after the first proved insufficient for banked carts.

The trait has no copy to keep in step, so neither failure is reachable. Both shadows are gone.

## What it costs

Nothing measurable. ANTIC issues roughly 110 fetches per scan line, so about 1.7 million reads a second — against a 64 KB memcpy per frame that the trait removes. 6000 headless frames of the 800XL timed the same before and after, inside run-to-run noise.

The chip's fetch path is generic over `M: AnticMemory + ?Sized` rather than taking `&dyn`, so a machine's view is monomorphised and a plain `&[u8]` still works for tests.

## Where the boundary sits

The view answers for the addresses the chip fetches from, at the banking in force **at that moment** — the 800XL samples PORTB per scan line now, where the snapshot sampled it once a frame.

It does not reach into the I/O page. Real ANTIC would drive an address in `$D000-$D7FF` onto the bus like any other, but no display fetches data from there, and giving a DMA chip a path into chips it does not otherwise touch would be a bus model, not a memory view. Those addresses read as RAM on the 800XL and open bus on the 5200, which is what the snapshots did.

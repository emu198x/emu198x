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

## This is not the interface RULES.md rule 6 forbids

Rule 6 says: *no Bus trait, no bus callback, no method-call-style memory access, ever, for any CPU.* That reads as a fleet-wide ban on any memory trait, and this decision exists because that reading is wrong.

The rule is about how a **CPU** presents itself. Its rationale — set out in [cpu-bus-interface.md](cpu-bus-interface.md) — is that other chips must be able to watch the CPU's address, data and control pins *continuously*, on the same master clock, to do their jobs: ULA contention, VIC-II bad lines, Agnus arbitration. A callback hides that state between calls, so the pins have to be public fields.

A DMA chip is the other side of that relationship. It is not the thing being watched; it is driving the address bus for its own fetch, and nothing needs to observe the read half-way through. `mos-vic-ii` has read its memory through `VicMemory` since it was written, for exactly this reason.

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

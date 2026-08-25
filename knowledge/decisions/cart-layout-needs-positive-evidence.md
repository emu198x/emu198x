# Decision: an ambiguous cartridge layout needs positive evidence

**Date:** 2026-08-25
**Status:** Active. Governs how a machine picks a cartridge layout or
mapper when the image itself does not say. Written from the Atari 5200,
which is the first case; the SMS (#204) and SG-1000 (#223) have the same
shape and should follow it.

## The problem

A headerless cartridge dump is just bytes. Several systems shipped carts
of the same size in incompatible layouts, and the image carries nothing
that distinguishes them:

- **Atari 5200, 16 KB.** Linear (`$8000-$BFFF`, mirrored down) or two-chip
  (two 8 KB chips decoded by A15). 23 titles linear, 39 two-chip.
- **Master System.** Sega-standard, Codemasters, and several Korean
  mappers, all in plain `.sms` images.
- **SG-1000.** Plain and bank-switched carts, some with on-cart RAM.

Picking by size means picking a default, and a default is wrong for
whichever group it excludes.

## What we did wrong first

The 5200 shipped a linear-only map, someone found that Pac-Man executed
padding, and the fix (2026-06-04) made *every* 16 KB cart two-chip. That
traded 39 broken titles for 23 broken ones and looked like a fix because
the title that prompted it now worked.

The failure was invisible in a way that matters. The two layouts agree at
the cart start vector in `$BFFE`, so a mismapped cart still loads, still
runs its 3,000 frames, and still exits 0 — it just executes the wrong 8 KB.
Robotron's vector points at `$8000`, where linear serves `LDA #$00 /
STA $D40E` (disable NMIs, the canonical first act of a 5200 cart) and
two-chip serves `JSR $9D3D` into uninitialised code. The result is a black
frame that reports success.

## The decision

**Treat the unusual layout as a claim that needs evidence, and default to
the plain one.** Concretely:

1. The plain layout is what an unidentified image gets. For the 5200 that
   is linear, which is also what MAME assumes for a headerless dump.
2. Anything else requires positive identification — a header field where
   the format has one, or a table of known images keyed by hash.
3. Where a header exists, it outranks the table. The 5200's `.a52` /
   `.car` cart-type byte (#419) will do this directly.

The asymmetry is deliberate. A wrong plain-layout guess on an unknown
homebrew cart is one broken title nobody has tested; a wrong exotic-layout
guess applies to every image we cannot identify.

## Where the table comes from

MAME's software lists record the layout per title. `hash/a5200.xml` is
**CC0-1.0** — a public-domain dedication, so distilling it carries no
attribution or share-alike obligation, unlike the emulator source beside
it. `tools/a5200-cart-layouts.py` reads the vendored copy in the umbrella
tree and emits `crates/machine-atari-5200/src/cart_layouts.rs`: the CRC32
of every two-chip cart, sorted for binary search.

Three constraints on doing this again:

- **Check the licence of the specific file, not the project.** MAME is
  BSD-3-Clause; its hash files are CC0. Other emulators' equivalents may be
  neither. See `198x/decisions/licensed-third-party-sources.md`.
- **Generate, commit, and record how to regenerate.** The vendored hash
  file lives in the umbrella tree, so CI cannot see it; the generated table
  is committed source and the script is the audit trail.
- **This is metadata, not payload.** `test-rom-policy.md` governs bundling
  ROM images and is untouched by this: a table of checksums redistributes
  no copyrighted content.

## What it does not solve

A dump we cannot identify still gets a guess, and a bad dump (`[b]`,
`[a]`) will miss the table even when the good dump would match. That is
acceptable because the guess is now the safe one and the failure is
one title rather than a class — but it means the table is a floor, not a
guarantee, and a header parse is strictly better where one exists.

Nor does it address the deeper problem the 5200 exposed: a mismapped cart
is indistinguishable from a working one at the exit code. Detecting "ran
to completion and never painted" belongs in the harness, above any one
machine.

## Consequences

- `machine-atari-5200` gained `CartLayout`, a detection step, and a
  generated table; the snapshot version bumped to 4 because the cartridge
  struct grew a field and postcard is not self-describing.
- Robotron: 2084 renders its title screen. Verified by screenshot, and by
  sweeping the clean TOSEC set to confirm the 37 locally-present two-chip
  titles still render.
- Missile Command's blank frame was filed as the same bug and is not: its
  8 KB map is byte-for-byte MAME's. Split out rather than fixed here.

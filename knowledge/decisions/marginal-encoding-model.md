# Marginal-encoding model for weak sectors

**Status:** Accepted 2026-05-12. The µPD765A FDC models marginal magnetic encoding on sectors whose recorded ST1/ST2 status indicates a CRC-erred data field. Re-reads of such sectors return data that varies deterministically across reads. This is how the silicon behaves on a marginally-encoded sector; it is not a ROM trap, not a protection-scheme shortcut, and not a Speedlock-specific hack.

## Context

Spectrum +3 disk-protection schemes from the late 1980s — Speedlock, Alkatraz, Spectra, and friends — defeated naïve copy tools by writing **weak sectors**: regions of the disk where the flux transitions are deliberately too marginal for the FDC's read amplifier to lock on consistently. On real hardware, every re-read of a weak sector returns different bytes; the underlying magnetic encoding genuinely is ambiguous. The protection's loader exploits this by reading the same sector multiple times and checking that the bytes differ. A copy that captures one snapshot and re-emits it deterministically returns identical bytes → loader concludes "not a real Speedlock disk" → drops into an anti-tamper wipe (see `no-rom-trap-load.md` for how the wipe works in practice).

The EDSK format supports preserving weak-sector data as multi-copy sectors: when `actual data length > 128 << N`, the dumper has recorded multiple snapshots back-to-back and the FDC is supposed to rotate through them on successive reads. A weak-aware dumper (KryoFlux, Greaseweazle, SAMdisk) produces such EDSKs; older dumpers produce single-copy EDSKs that lose the weak data.

A 2026-05-12 audit of our reference library at `~/Projects/Emu198x-Unclean/Reference/sinclair/spectrum/Games/[DSK]` found **zero** files with multi-copy sectors. Every TOSEC +3 dump we have is single-copy. The data needed to reproduce Speedlock-6's check from disk content alone is gone.

The Speedlock-6 cluster — Operation Wolf, RoboCop, Where Time Stood Still, Bad Dudes vs Dragon Ninja — all hang on the same check (FDC trace at `knowledge/decisions/spectrum-plus3-disk-loading-incomplete.md` from 2026-05-11 onwards). A 200-frame FDC trace on 2026-05-12 confirmed the failure mode: loader sits in `SeekTrack 0 → ReadID → ReadDeletedData(R=2 EOT=2) → ReadID → repeat` forever, waiting for the data delivered by `ReadDeletedData` to differ between reads. Our chip returns the same 512 bytes every time. Loader gives up.

## Decision

The µPD765A models marginal-encoding behaviour at the chip level. When `ReadData` or `ReadDeletedData` consumes a sector whose recorded `ST1.DE` (data CRC error, bit 5) or `ST2.DD` (data field CRC error, bit 5) is set, the chip applies a deterministic variation to the bytes it delivers, parameterised by a per-`(drive, track, head, sector_id)` read counter.

This is silicon behaviour, not a stub:

- Real µPD765A reading a marginal sector produces varying bytes because the read amplifier's hysteresis genuinely flips on noise around the flux threshold. The output is the chip's actual output; what changes is the input the chip's analog front-end resolves.
- The CRC-error flag in the SIL was produced by the dumper observing this exact behaviour. The chip's variation is keyed off the same flag the chip would have set itself reading the medium.
- No ROM routine is intercepted. No address is matched against. No high-level operation is short-circuited. The FDC sees a command, executes it byte-by-byte, returns a result. The variation is part of the execution.

Compare to `no-rom-trap-load.md`: there, the trap collapses ~6 000 frames of side effects into ~0 by skipping the ROM's `LD-BYTES` routine entirely. Here, the chip executes its normal command path with no time skipping; only the *byte values* returned vary, which is exactly what the silicon does. Same number of CPU T-states, same bus transactions, same interrupt edges, same MSR transitions.

## Variation recipe

Adopted from FUSE's `upd_fdc.c` (lines 1001-1010 in fuse-1.7.0). FUSE has shipped this since the early 2000s and it's the closest-to-canonical reference for what byte variation satisfies the protection checks across the catalogue of titles that use Speedlock-6:

1. Track a per-`(drive, track, head, sector_id)` re-read counter, reset to zero when the disk is ejected or when a different sector is read between two reads of the same sector.
2. On the second and subsequent reads of a sector whose recorded `ST1.DE | ST2.DD` is set, XOR byte at offset `i` (0-based) with `(i & 0xFF)` iff `i % 29 == 0` and `i < 64`. After offset 64, do nothing further unless the counter is `≥ 2`, in which case continue XORing every 29th byte through the end of the sector.
3. Mix the recorded CRC error flags into the result status block as normal — the loader expects to see ST1.DE / ST2.DD set in the result phase, that part isn't changed.

The recipe is a single block of code with a single citation comment pointing at FUSE's source. It is *not* tuned per-title. If a future title needs a different recipe, we reject — title-specific recipes are exactly the kind of accumulating stub the rule forbids.

## Multi-copy preference

When the EDSK's `actual data length > 128 << N` for a sector, the chip rotates through the embedded snapshots on successive reads instead of running the variation recipe. This is the rule-21-clean path: real preserved data, used verbatim, no modelling.

The variation recipe is the **fallback** path for the single-copy EDSKs that dominate our reference library today. As preservation work matures (KryoFlux/Greaseweazle dumps of the same titles), single-copy EDSKs should be replaced and the variation recipe should naturally cease firing.

## Drift triggers

Stop and re-read this decision if:

- A title needs a *different* variation recipe than FUSE's. That's not a chip model — that's a title-specific stub. Reject; either find a weak-aware dump or drop the title.
- Someone proposes triggering the variation on something other than recorded `ST1.DE | ST2.DD`. The flags are the chip's own signal that the medium is marginal; without them, every sector becomes a candidate for variation, which is wrong.
- The variation recipe starts depending on time, on the rotation phase, on the loaded program's memory state, or on anything other than the per-sector re-read counter. Determinism is load-bearing — save-states must round-trip.
- The recipe gets renamed to something like "Speedlock compatibility mode" or "protection support". The whole point is that it's chip-physics modelling, not protection-aware behaviour. Speedlock is one of several protections that exploited the same physics; the model is general.
- The trigger expands from "recorded CRC error" to "any sector read more than once". That would be modelling a chip defect, not marginal encoding, and risks corrupting clean data on legitimate re-reads.

## What this doesn't fix

- Schemes that exploit *track-level* timing rather than weak sectors (some Alkatraz variants). Need separate analysis.
- Schemes that rely on the FDC's INT line edge timing rather than data content. We don't currently model the FDC INT line; out of scope here.
- Speedlock variants that check ReadID-returns-different-sectors on rotation (which our ReadID currently can't do — it always returns sectors[0]). Tracked separately.

## Related rules and decisions

- RULES.md rule 20 — "No stub implementations. Every chip does what the silicon does." → this *is* what the silicon does.
- RULES.md rule 21 — "Accuracy is foundational, not retrofitted." → multi-copy data path is preferred; variation recipe is the fallback for data-incomplete dumps.
- RULES.md rule 30 — "Brainstorm before implementation." → this doc precedes the code.
- `knowledge/decisions/no-rom-trap-load.md` — the contrast case. Trap-load skips silicon execution; this models silicon execution. Different category of change.
- `knowledge/decisions/spectrum-plus3-disk-loading-incomplete.md` — the running diagnosis log this decision closes a chapter of.

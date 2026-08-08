# Verifying Amiga programmable HBLANK write timing

This process answers how changes to programmable horizontal blanking are
tested when a register write occurs after a relevant comparator position on
the current line.

It complements the steady-state programmable-HBLANK process. The steady-state
corpus establishes which programmed interval is visible after registers have
settled. The write-timing corpus distinguishes event-latched behaviour from
output that is recomputed geometrically from the current register values at
every sample.

## Scope

The first write-timing slice covers PAL ECS and AGA profiles. It asks what
happens when the Copper:

- moves `HBSTRT` behind the current beam;
- moves `HBSTOP` ahead after the original stop event;
- enables `ECSENA` after the programmed start position;
- enables `EXTBLKEN` after the programmed start position;
- enables `BLANKEN` after the programmed start position.

It does not cover writes coincident with a comparator edge, gate-disable
writes, AGA half-CCK propagation, programmable vertical blanking or sync,
variable totals, analogue output, or other chipset revisions.

## Portable corpus boundary

The portable corpus is
[`test-data/commodore/amiga/programmable-hblank-write-timing/`](../../test-data/commodore/amiga/programmable-hblank-write-timing/).
It contains project-authored CC0-1.0 probe sources, declarative cases,
deterministic ADF build tools, interchange schemas, and registered evidence.

Each case restores a known state on beam line 127. On beam line 128, the
Copper changes `COLOR00` to a visible marker immediately before the tested
write. The following line demonstrates that the new register value reached
the chipset even when no current-line transition was produced.

Commercial firmware and producer binaries remain external. Records identify
them by revision and SHA-256.

## Evidence contract

An admissible capture must:

- identify the exact suite, ADF, payload, producer, firmware, and machine
  configuration;
- preserve the completed raw chipset framebuffer before frontend crop,
  scaling, filtering, shaders, or overlays;
- capture the line before the marker, the marked mutation line, and the
  following control line;
- state how those rows map to the producer framebuffer;
- retain three adjacent stable fields;
- identify storage padding and exclude it from semantic blank runs;
- classify the observed transition without image alignment or tolerance
  search.

The visible marker proves the ordering of the preceding Copper operation and
bounds the tested write. It does not by itself expose the exact custom-chip
bus sample of that write. Records must retain that uncertainty rather than
claiming a more precise position.

## Registered evidence

The registered package uses FS-UAE 5.0.7 revision
`f362278ccd4c60991caac3b4d240d4a3f751bea2`, whose chipset implementation is
derived from WinUAE 6.0.1. All ten ECS and AGA runs produced three
byte-identical adjacent fields.

The source audit and captures support the same event-latched interpretation:

- changing a start comparator after its equality event does not synthesize a
  new start event on the current line;
- changing a stop comparator after the original stop event does not
  retroactively reassert blanking;
- the ECS selector can expose an already-latched raw state when a gate is
  enabled;
- the AGA selector does not synthesize a missed start when `ECSENA` or
  `EXTBLKEN` is enabled;
- `BLANKEN` controls the ECS route measured here but does not gate the AGA
  programmable route.

These are stable observations from one audited implementation family. They
are not physical-hardware evidence or cross-family consensus.

Copperline 0.13.0 applies the final register state as a whole-frame
post-process and cannot answer a mid-line write question. vAmiga 4.4b12 does
not dispatch `HBSTRT` or `HBSTOP` and uses a fixed horizontal-blank interval.
Both are therefore recorded as unsupported rather than as behavioural votes.

## Emu198x use

The registered Emu198x consumer is
`crates/runtime-commodore-amiga/tests/amiga_programmable_hblank_write_timing.rs`.
It is invoked through
[`scripts/verify-amiga-programmable-hblank-write-timing.sh`](../../scripts/verify-amiga-programmable-hblank-write-timing.sh).
The wrapper builds the corpus without Python bytecode output, verifies both
firmware images, and records the full Git revision and dirty-worktree state.

The consumer verifies the corpus sources and artifacts, the complete
registered FS-UAE package identity, and every package-referenced capture,
configuration, manifest, log, and observation record before booting a case.
It boots all five cases on both the ECS and AGA profiles. Each run validates
the ready record, captures three byte-identical adjacent fields, and derives
the tested write from the machine's Copper MOVE log. CPU custom-register
writes are retained as diagnostics but are not used as evidence of a Copper
write. Each captured field must retain the marker MOVE at horizontal position
138 and the tested MOVE at position 142; a lower-bound check against the
programmed `WAIT` is not sufficient for this timing lane.

The baseline, mutation, and following lines are measured separately at fixed
beam-to-framebuffer rows. Reference raw samples map to native Emu198x output
pixels through the declared version-1 transform
`emu_output_pixel = fs_uae_raw_sample + 8`. This preserves the AGA marker's
half-lores edge. The comparison performs no alignment search, rounding,
tolerance, or automatic phase adjustment.

Regression tests may encode the selected event-latched implementation model.
Until an independent audited family or physical-hardware capture agrees, a
passing Emu198x result means that it matches the registered UAE-family
observation. It must not be described as hardware conformance.

Every run writes a structured result, including successful runs, below
`target/accuracy/amiga-programmable-hblank-write-timing/1.0.0/<full-revision>/`.
The revision summary retains all ten outcomes. Failures additionally retain
the captured RGBA fields, measured intervals, relevant CPU custom-register
writes, and Copper MOVE log. The test completes the matrix before failing, so
one disagreement does not hide the remaining observations. There is no
golden-update mode.

## Promotion

The evidence classification may be raised only when a second independent
audited implementation family or a registered physical-hardware capture
answers the same cases through an admissible mid-line capture path.
Disagreement remains visible and does not become a tolerance.

New timing questions require new focused cases. They must not silently change
the identities or interpretation of version 1.0.0.

## Related documents

- [Steady-state programmable-HBLANK process](amiga-programmable-hblank-conformance.md)
- [Write-timing corpus](../../test-data/commodore/amiga/programmable-hblank-write-timing/README.md)
- [Registered FS-UAE package](../../test-data/commodore/amiga/programmable-hblank-write-timing/references/fs-uae-5.0.7-f362278c/README.md)
- [Comparator capability audit](../../test-data/commodore/amiga/programmable-hblank-write-timing/references/comparator-capabilities.md)
- [Accuracy corpora](../../test-data/accuracy-corpora.md)

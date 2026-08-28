# Per-system status docs (`systems/<manufacturer>/<system>.md` in `emu198x/docs`)

**Status: ACCEPTED (2026-06-06).**

## Decision

Every emulated system gets one committed status page at
[`systems/<manufacturer>/<system>.md`](https://github.com/emu198x/docs/tree/main/systems) in the
[`emu198x/docs`](https://github.com/emu198x/docs) repository — the path derived mechanically from its
`machine-<manufacturer>-<system>` crate name — or a
`systems/<manufacturer>/<system>/` folder with `index.md` when it needs several
pages. The page records the **current state of the emulation**: what works
(with validation), what's not implemented, where the accuracy gaps are, and a
**Known unknowns / disproven hypotheses** section. Template and conventions in
[`systems/README.md`](https://github.com/emu198x/docs/blob/main/systems/README.md).

These pages are prose, so they live in the documentation repository rather than
beside the code; see
[`documentation-splits-on-what-the-build-reads.md`](documentation-splits-on-what-the-build-reads.md).
They were written here first, at `docs/systems/`, and moved with the rest of
the prose. The rule below is unchanged — only the repository is.

## Context

A loose `docs/systems/*.md` convention already existed (flat, ~10 pages,
inconsistent section names, no page for most extended systems). In parallel,
`knowledge/systems/<…>` holds a deeper distillation — but it is **gitignored**
(local-only) and schema'd as "correct, not comprehensive: remove stale facts."
Neither layer durably captures the *negative space*: known gaps, unverified
assumptions, and hypotheses already disproven.

The cost of that gap is concrete. The Sord M5 Dig Dug freeze took four sessions
partly because the key finding — "interrupt delivery looks fine" — was **wrong**
(it compared the aggregate IM2 ack rate, not the per-channel rate) and lived only
in a probe-test doc-comment and a decaying memory file. A durable "interrupts
only aggregate-validated — suspect per channel" note would have saved three of
those sessions. The Aquarius cartridge work hit the same shape: facts scattered
across commit messages, code comments, and memory.

## What the page must add over a feature list

- **Not implemented / accuracy gaps** — missing features and known inaccuracies,
  with their real-software impact.
- **Known unknowns / disproven hypotheses** — open questions, unverified
  assumptions, and explicitly the dead ends we've already walked, so they are not
  re-walked. This is the highest-value, most easily-lost content.
- **Validated against** — provenance: the MAME file, datasheet, or test ROM
  behind each non-obvious claim. The same discipline that cracked the Aquarius
  scrambler and the M5 interrupt polarity.
- **Timing & cycle-accuracy** — where the implementation sits relative to the
  master clock. This is mandatory, because master-clock cycle-accuracy is the
  project's central commitment (RULES.md §51-64, §91), and the donor/extended
  systems mostly boot via *relaxed* timing (scanline-batched VDPs, flat clock
  ratios, fixed DMA stalls). A page must state the master clock + divider tree,
  which timing model is realised (`hc`-driven / per-dot / scanline-batched /
  per-frame), and the concrete distance to full cycle-accuracy. It must **not
  conflate a green CPU oracle with system cycle-accuracy**: RULES §62 separates
  instruction-accuracy (what Tom Harte / ZEXALL / SM83 prove) from cycle-accuracy
  (bus timing against the chipset on the shared master clock). "CPU oracle green"
  says nothing about the latter.
- **Tooling & drivability** — the `--script` / `--mcp` surface, chip `query()`
  paths and debug tools (run_until_pc, memory_read, io_trace, disasm), native
  window vs headless, and what's pending (notably the shared disassembler, a live
  Asm198x dependency). Drivability — an agent being able to drive and inspect
  every core — is the project's other through-line and the reason these tools
  exist; the per-system page is where its state is recorded.
- **Peripherals & connectivity** — the realistic peripheral surface (disk,
  printer, mouse, light gun, expansions) and, deliberately, **whether the system
  could ever reach the internet** — natively (rare), via a period modem / network
  device, or via a modern third-party device we could emulate (FujiNet, WiModem,
  Spectranet, RR-Net, Mobile Adapter GB, DriveWire, the BBC's native Econet, …).
  Tracked as a forward-looking capability axis with a `Yes / Marginal / No`
  verdict so it can be scanned across the fleet; the path matters as much as the
  verdict, because "what device would we emulate" is the actual work item.

## Relationship to other layers

- **Ships** (committed) — public, citable status ledger. Distinct from the
  local `knowledge/systems/` distillation, which it links into for depth rather
  than duplicating.
- Slots into the umbrella shared-hardware-reference layering
  (`198x/decisions/shared-hardware-reference-canon.md`) as a codebase-tied,
  shipping *status* artifact — adjacent to, not a replacement for, the local
  distillation (layer 3) and curriculum extracts (layer 4).
- Cites the primary library (`reference/`) and `syntheses/` for hardware
  facts; cites `knowledge/decisions/` for architecture.

## Consequences

- New systems: add the page as part of bringing the machine up (extend the
  [notes on adding a system](https://github.com/emu198x/docs/blob/main/adding-a-system.md) to require it).
- Existing flat `docs/systems/*.md` pages migrate into the manufacturer-subdir
  layout and gain the two new sections. No inbound links reference them, so the
  move is mechanical.
- These pages are updated when a bug is fixed or a gap is found — the fix's
  commit and the page edit go together, the way the M5 and Aquarius fixes now do.

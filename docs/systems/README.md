# Per-system status docs

One committed page per system, recording **everything currently known about
its emulation**: what works, what's missing, where the accuracy gaps are, and —
most valuably — what we *don't* know and what we've already chased down a dead
end. These pages are the durable, shippable answer to "where does this system
stand?", so a future session (or a new contributor, or the Code198x curriculum)
starts from current truth instead of re-deriving it from code, commits, and
scattered memory.

## Where a page lives

Derive the path mechanically from the machine crate name:

```
machine-<manufacturer>-<system>   →   docs/systems/<manufacturer>/<system>.md
machine-sord-m5                    →   docs/systems/sord/m5.md
machine-mattel-aquarius            →   docs/systems/mattel/aquarius.md
machine-commodore-vic-20           →   docs/systems/commodore/vic-20.md
```

When a system needs more than one page (deep dives, audits, per-variant notes),
use a folder with an `index.md`:

```
docs/systems/commodore/amiga/index.md
docs/systems/commodore/amiga/aga-fetch.md
docs/systems/sinclair/zx-spectrum/index.md   (the variants share one tree)
```

## How this relates to the other knowledge layers

- **These pages ship** (committed). They are the public, citable status ledger.
- **`knowledge/systems/<…>`** is the *local* deep distillation (gitignored, schema
  in `knowledge/SCHEMA.md`): how the hardware works, in detail, pressure-tested by
  the code. It is "correct, not comprehensive" — it *removes* stale facts. These
  status pages link down into it for depth; they do not duplicate it.
- **`knowledge/decisions/`** holds binding architectural choices. Link to the
  relevant ones; don't restate them.
- The umbrella reference library (`../../reference/`, `../../syntheses/`) is the
  source of hardware truth. Cite it for facts.

The division of labour: `knowledge/systems/` answers *"how does this chip behave?"*;
these pages answer *"what is the state of our emulation of it, and what's
uncertain?"* The second is the thing that kept getting lost.

## Page template

Copy this skeleton. Keep every claim grounded — cite MAME, a datasheet, or a
test ROM. Prefer a short honest page over a long vague one.

```markdown
# <Manufacturer> <System>

## Status: <one line — what a developer can actually do today>

<A paragraph: support tier, what boots/plays, the headline limitation.>

## What works

- **<feature>** — <how it behaves> (validated: <test ROM / MAME cross-check / hardware note>)

## Not implemented / accuracy gaps

- **<missing feature>** — <impact: does it block real software, or cosmetic?>
- **<known inaccuracy>** — <what's wrong, how far off, why it hasn't been fixed>

## Known unknowns / disproven hypotheses

- **<open question>** — <what we don't know and what would settle it>
- **DISPROVEN: <hypothesis>** — <what we believed, why it was wrong, the real cause>
  (<session/commit>). Recorded so it isn't chased again.

## Validated against

- MAME `<path/file.cpp>` — <what we cross-checked>
- <datasheet / test ROM / reference emulator> — <what it confirmed>

## Timing & cycle-accuracy

- **Master clock & dividers** — <crystal; how each chip derives (CPU /n, dot /m)>
- **Timing model realised** — <hc-driven (the goal) / per-dot / scanline-batched /
  per-frame>. <which chips are at which level>
- **CPU timing** — cycle-accurate per RULES §62? <oracle that proves the
  *instruction set* — note it does NOT prove bus-cycle timing>
- **Distance to full cycle-accuracy** — <the concrete sub-cycle gaps and what
  closing them needs>

## Tooling & drivability

- **Script / MCP** — <`--script` + `--mcp` present? which chip `query()` paths /
  debug tools: run_until_pc, memory_read, poke, io_trace, disasm>
- **Native window** — <yes (primary tier) / headless only>
- **Disassembler** — <available, or pending the Asm198x shared spec-driven crate>

## Crates

| Crate | Role |
|-------|------|

## ROMs

<BIOS / test ROMs required, and where they live.>

## Launch

<Headless run + screenshot commands.>
```

Two pairs of sections carry the weight:

- **Accuracy gaps** + **known unknowns / disproven hypotheses** — the
  institutional memory that stops a fixed-but-undocumented dead end (the M5
  "interrupts are fine" mis-conclusion that cost three sessions) from being
  re-walked. Record disproven hypotheses with the same care as working features.
- **Timing & cycle-accuracy** + **tooling & drivability** — the project's two
  through-lines. Master-clock cycle-accuracy is the central architectural
  commitment (RULES.md §51-64: *"the master oscillator drives the loop… one
  clock, everything derives"*; §91: foundational, not retrofitted), so each page
  must say where the implementation sits relative to it — and must **not conflate
  a green CPU oracle (instruction-accurate, RULES §62) with system cycle-accuracy
  against the master clock (bus timing)**. Drivability is the other: every core
  exposes `--script` + `--mcp` so an agent can drive and debug it, and the
  shared disassembler is a live Asm198x dependency — so each page records its
  debug surface and what's still pending.

See `../../knowledge/decisions/per-system-status-docs.md` for the binding
decision behind this layer.

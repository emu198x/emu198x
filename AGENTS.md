# Emu198x

> Read [`PRINCIPLES.md`](PRINCIPLES.md) first. [`MANIFESTO.md`](MANIFESTO.md) is why the project exists.

Multi-platform retro emulator (Rust workspace). Flagship repo of the local `Emu198x/` org container and one of seven sibling projects under the `198x/` umbrella; see [`../../AGENTS.md`](../../AGENTS.md) for umbrella context and cross-project rules.

## Binding constraints

Read **[RULES.md](RULES.md)** before writing any code. It holds the binding constraints — clock model, CPU model, no-Bus-trait rule, session-start anchors, drift triggers. `RULES.md` is the entry point for this project; this file is a pointer.

## Hardware reference layering

Per [`../../decisions/shared-hardware-reference-canon.md`](../../decisions/shared-hardware-reference-canon.md), hardware reference is layered. The primary library at [`../../reference/`](../../reference/) is the source of truth — datasheets, manuals, magazines with sidecar metadata. This project's [`knowledge/`](knowledge/) is a codebase-tied distillation (chips in [`knowledge/chips/`](knowledge/chips/), systems in [`knowledge/systems/`](knowledge/systems/), architectural decisions in [`knowledge/decisions/`](knowledge/decisions/)) — schema-bound and pressure-tested by working code. The distillation cites the library, not the other way round.

## Where things live

- [`RULES.md`](RULES.md) — binding constraints
- [`knowledge/`](knowledge/) — LLM-curated knowledge base; see [`knowledge/SCHEMA.md`](knowledge/SCHEMA.md) for the schema
- [`docs/`](docs/) — the status pages CI renders and checks
- [`emu198x/docs`](https://github.com/emu198x/docs) — project documentation: testing policy, architecture,
  plans, handoffs, and the archive. A separate repository.
- [`crates/`](crates/) — Rust workspace

For cross-project knowledge spanning Emu198x and Code198x, see [`../../AGENTS.md`](../../AGENTS.md) and [`../../decisions/`](../../decisions/). For personal cross-cutting knowledge, see `~/knowledge/`.

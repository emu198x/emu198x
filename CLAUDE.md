# Emu198x

Multi-platform retro emulator (Rust workspace). One of two sibling projects under the `198x/` umbrella; see [`../CLAUDE.md`](../CLAUDE.md) for umbrella context and cross-project rules.

## Binding constraints

Read **[RULES.md](RULES.md)** before writing any code. It holds the binding constraints — clock model, CPU model, no-Bus-trait rule, session-start anchors, drift triggers. `RULES.md` is the entry point for this project; this file is a pointer.

## Hardware reference is canon here

Per [`../decisions/shared-hardware-reference-canon.md`](../decisions/shared-hardware-reference-canon.md), this project's `knowledge/` is the authoritative hardware reference for both Emu198x and Code198x. Chip docs in [`knowledge/chips/`](knowledge/chips/), system docs in [`knowledge/systems/`](knowledge/systems/), architectural decisions in [`knowledge/decisions/`](knowledge/decisions/). When hardware facts change, they change here first.

## Where things live

- [`RULES.md`](RULES.md) — binding constraints
- [`knowledge/`](knowledge/) — LLM-curated knowledge base; see [`knowledge/SCHEMA.md`](knowledge/SCHEMA.md) for the schema
- [`docs/`](docs/) — operational docs (architecture, features, plans, handoffs, status)
- [`crates/`](crates/) — Rust workspace

For cross-project knowledge spanning Emu198x and Code198x, see [`../CLAUDE.md`](../CLAUDE.md) and [`../decisions/`](../decisions/). For personal cross-cutting knowledge, see `~/knowledge/`.

# Contributing to Emu198x

Thanks for the interest. Emu198x is a fresh-start Rust workspace for
cycle-accurate vintage computer and console emulators. The priority order is
accuracy first, breadth second, polish third.

## Before you start

Read [`RULES.md`](RULES.md). It holds the binding constraints — clock model,
CPU bus interface, no-Bus-trait rule, ULA pattern — that any new chip or
machine work has to honour.

Skim [`knowledge/decisions/`](knowledge/decisions/). The 44 records explain
why specific architectural choices were made. Most "shouldn't we just…"
questions are answered there. If you find yourself wanting to relitigate one,
open an issue rather than a PR.

The umbrella context lives at [`../CLAUDE.md`](../CLAUDE.md) for contributors
who clone the wider 198x umbrella. Read it if you're touching anything that
interacts with the shared hardware reference layer.

## Dev setup

See the [Building](README.md#building) and [Getting ROMs](README.md#getting-roms)
sections of the README. ROMs are not shipped; you provide them yourself.

The workspace tracks the latest stable Rust toolchain via `rust-toolchain.toml`.

## Code style

- `cargo fmt --all --check` — formatting must pass
- `cargo clippy --workspace --all-targets -- -D warnings` — clippy must pass
  clean (the workspace denies `unwrap_used`, `dbg_macro`, `todo`)
- `unsafe_code` is forbidden workspace-wide

## Tests

Read [`docs/testing-policy.md`](docs/testing-policy.md) — the verification
standard for the project. Briefly:

- Chip- and format-level unit tests first
- Machine wiring tests second
- ROM/software regressions above that
- External reference suites (Tom Harte, ZEXALL, Blargg, mooneye) where
  appropriate

`cargo test --workspace` should pass on a clean tree. Tests that require local
ROMs you don't have will skip cleanly — env vars that gate them are documented
in each test file's prologue (`EMU198X_SPECTRUM_MANIC_MINER_TZX`,
`EMU198X_GB_BLARGG_ROOT`, etc.).

## Adding a system

Tier-1 systems (Spectrum, C64, Amiga, NES, Game Boy, Dragon 32) are the current
focus. New systems get added via the
[`docs/adding-a-system.md`](docs/adding-a-system.md) playbook. Open an issue
before starting work — system additions touch enough of the workspace that
early alignment saves rework.

## Commit and PR style

- Title describes the effect, not the implementation
- One commit's worth of work per commit — atomic, self-contained, passes tests
- Explain the why in the body, not just the what
- PRs that touch architectural seams should reference the relevant decision
  record in `knowledge/decisions/`

Commit subjects use [Conventional Commits](https://www.conventionalcommits.org)
prefixes so [release-plz](https://release-plz.dev) can compute version bumps
and append CHANGELOG entries automatically:

- `feat:` — new user-facing capability (minor bump; major with `!` after 1.0)
- `fix:` — bug fix (patch bump)
- `chore:`, `docs:`, `refactor:`, `style:`, `test:`, `ci:`, `build:`, `perf:` —
  no bump, appears in CHANGELOG under "Other"
- Scope notation (`feat(spectrum):`, `fix(amiga):`) is optional — use it when
  the change is clearly system-specific

The convention is by convention, not CI-enforced. Missing prefixes degrade
gracefully — release-plz lands them under "Other" and skips the bump. Don't
pick a prefix to satisfy a lint; the effect-described-in-the-title and
why-in-the-body rules above are the higher bar.

## Reporting bugs

Use the bug report template. The most useful bug reports include the system
being emulated, the media kind (TZX tape / D64 disk / iNES cartridge / …),
what you expected, what you saw, and a `--script` JSON or short
`cargo run -p emu198x-<system> --no-default-features -- --script …` command
that reproduces it.

Security issues go through [SECURITY.md](SECURITY.md), not the public issue
tracker.

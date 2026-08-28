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

This repository sits inside a wider 198x tree that carries the shared
hardware reference layer. That tree is not part of this repository, and its
context file — `CLAUDE.md`, two levels above this one — is only present if you
have it checked out. If you do, read it before touching anything that cites the
shared reference layer.

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

Read the [testing policy][policy], in the [`emu198x/docs`][docs] repository —
the verification standard for the project. Briefly:

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
focus. Open an issue before starting work — system additions touch enough of the
workspace that early alignment saves rework, and the shape of the job differs
enough per machine that the issue is where the plan gets made. The
[`emu198x/docs`][docs] repository carries the per-system status pages and the
[notes on adding a system][adding].

## Commit and PR style

- Title describes the effect, not the implementation
- One commit's worth of work per commit — atomic, self-contained, passes tests
- Explain the why in the body, not just the what
- PRs that touch architectural seams should reference the relevant decision
  record in `knowledge/decisions/`

Commit subjects use [Conventional Commits](https://www.conventionalcommits.org)
prefixes, because [git-cliff](https://git-cliff.org) computes the version bump
and writes the CHANGELOG from them. The rules live in `cliff.toml`:

- `feat:` — new user-facing capability. Moves the minor, filed under "Added".
- `fix:` — bug fix. Moves the patch, filed under "Fixed".
- `perf:` / `revert:` — filed under "Performance" / "Reverted".
- `docs:`, `test:`, `chore:`, `ci:`, `build:`, `refactor:`, `style:` — no bump,
  and **absent from the CHANGELOG entirely**. They are most of the commits
  here and would bury the twenty that matter.
- `feat(ci):`, `fix(ci):`, `feat(release):`, `fix(release):` — also absent. The
  machinery that ships a release is not a change *in* the release.
- Scope notation (`feat(spectrum):`, `fix(amiga):`) is optional — use it when
  the change is clearly system-specific.

A `!` after the prefix, or a `BREAKING CHANGE:` footer, marks a breaking
change. Before 1.0 that moves the **minor**, not the major — 0.x semver's
breaking release — and the entry is marked **Breaking** in the CHANGELOG and
carries its footer, which is the part telling a reader what to do about it.
Declaring 1.0 is a deliberate act rather than something one `fix!:` decides.

The convention is by convention, not CI-enforced. A missing prefix degrades
gracefully: the commit is skipped rather than misfiled, and contributes no
bump. Don't pick a prefix to satisfy a lint; the effect-described-in-the-title
and why-in-the-body rules above are the higher bar.

## Reporting bugs

Use the bug report template. The most useful bug reports include the system
being emulated, the media kind (TZX tape / D64 disk / iNES cartridge / …),
what you expected, what you saw, and a `--script` JSON or short
`cargo run -p emu198x-<system> --no-default-features -- --script …` command
that reproduces it.

Security issues go through [SECURITY.md](SECURITY.md), not the public issue
tracker.

[docs]: https://github.com/emu198x/docs
[policy]: https://github.com/emu198x/docs/blob/main/testing-policy.md
[adding]: https://github.com/emu198x/docs/blob/main/adding-a-system.md

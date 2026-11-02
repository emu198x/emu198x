# Families move now: platform crates, then the ColecoVision pilot

Agreed with Steve on 2 November 2026: drop the shell gate and move system
families to their own repositories now, ColecoVision first
(`knowledge/decisions/workspace-shape.md`, § Amendment 2026-11-02).

## What the dependency graph says

Counting normal and build dependencies (dev-dependencies are dropped when a
crate is published), almost every family builds on six platform crates that
stay here, and none of them is on crates.io:

| Crate | Version | Workspace dependencies |
|---|---|---|
| `emu198x-pointer` | suite (0.46.2) | none |
| `emu198x-win-menu` | suite | none |
| `emu198x-app-icon` | suite | none |
| `emu198x-shell` | own (0.44.2) | none |
| `emu198x-native-video` | suite | shell |
| `emu198x-ui` | suite | shell, native-video, pointer, win-menu |

`emu198x-native-video` and `emu198x-web` run four real machines in their tests.
Those are dev-dependencies and don't block publishing.

`emu198x-fleet-web` depends on every family's runtime, so each family
repository publishes its runtime and this repository takes it from crates.io.

## Tasks

### 1. Make the six platform crates publishable

- Give each its own version, starting from the version it has now (as the
  shell did on 2026-10-27), and set `publish = true`.
- Give their workspace dependency entries a `version` beside `path`.
- **Verified by:** `scripts/publishable-crates.py` lists them in the order
  above; `cargo package` succeeds for each, in that order; the workspace
  builds and its tests pass.

### 2. First publish (Steve)

crates.io only allows Trusted Publishing once a crate exists, so the first
version of each needs an API token: `cargo publish -p <crate>` in the order
above. Then, on crates.io, add this repository's `publish-crates.yml` as each
crate's trusted publisher, so later versions go through the workflow.

- **Verified by:** each crate's version on crates.io matches its manifest.

### 3. ColecoVision pilot

The family's five crates: `common-coleco-colecovision`,
`machine-coleco-colecovision-ntsc`, `machine-coleco-colecovision-pal`,
`runtime-coleco-colecovision` and `emu198x-colecovision`. They depend on
no other family, only on the platform crates, `emu198x-test-skip`, and chips
that are already published.

- Stand up `emu198x/coleco-colecovision`, following `scripts/extract-repo.py`'s
  pattern and keeping the history of the moved paths.
- Publish the family's library crates there; this repository's
  `emu198x-fleet-web` and any other host then uses the published runtime.
- Move the family's release: its binary goes out from its own repository.
  Releasing a binary from a family repository (cargo-dist, Homebrew) is new
  and is the part the pilot is for.
- **Verified by:** the family's tests and corpus results are unchanged before
  and after the move; `emu198x-fleet-web` passes against the published
  runtime; the family repository's release produces an installable binary.

### 4. The rest

After the pilot, write down what it taught us and order the remaining
families from the same graph: shell-only families first, then those needing
a format crate published, then the Spectrum (after the DSK adapter) and the
68k families last.

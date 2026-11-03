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
- **Done (2 November 2026):** `publishable-crates.py` lists all six, in that
  order. `cargo package --locked` packages and verify-builds each against
  the others. It drops `emu198x-native-video`'s three examples, which live
  in `tools/spectrum-composite/` and stay local. `Cargo.lock` is unchanged,
  and `emu198x-colecovision`, `emu198x-ui` and `emu198x-fleet-web` build.

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
- Publish the family's four library crates there, as
  `emu198x-common-coleco-colecovision`,
  `emu198x-machine-coleco-colecovision-ntsc`,
  `emu198x-machine-coleco-colecovision-pal` and
  `emu198x-runtime-coleco-colecovision` (`crate-naming.md`, § Amendment
  2026-11-02).
- `emu198x-colecovision`, the binary, stays here in the suite release and,
  like `emu198x-fleet-web`, takes the published runtime
  (`workspace-shape.md`, § Amendment 2026-11-02).
- **Verified by:** the family's tests and corpus results are unchanged before
  and after the move; `emu198x-colecovision` and `emu198x-fleet-web` pass
  against the published crates; the suite release still produces an
  installable ColecoVision binary.

- **Done (3 November 2026).** The four crates are in
  `emu198x/coleco-colecovision` with 115 commits of history, published as
  0.46.2, byte-identical to the source they left here. `emu198x-colecovision`
  and `emu198x-fleet-web` take the runtime from crates.io and pass as before
  (16 and 13 tests).

#### What the pilot taught

Each is fixed, so the next family doesn't meet it:

1. **The status scripts read a crate's category from its name.** A
   published `emu198x-machine-…` wasn't a machine to them (#2247).
2. **`extract-repo.py` assumed chips.** It copied path-based workspace
   dependencies such as `emu198x-shell`, dry-ran publishing one crate at a
   time (siblings can't resolve), pushed this repository's suite tags, and
   gave the CI no Linux packages for the shell (#2249, #2251, #2252).
3. **A moved board's evidence leaves with it.** `docs/status/systems.toml`
   records it with `machine_repository`; the registry check then requires the
   board to be a crates.io dependency its binary reaches, and the status page
   says where its tests run.
4. **Two copies of the shell.** A published runtime asks crates.io for
   `emu198x-shell`; the binary here builds it from source. They are
   different crates, so the runtime's traits didn't match the binary's.
   `[patch.crates-io]` in the root manifest makes them one. The patch only
   holds while the in-tree shell's version satisfies what the family asks
   for, so **a breaking shell release means republishing every moved family
   against it first.** That is the real cost of the split, and it grows with
   each family that moves.
5. **First publishes need Steve.** crates.io allows Trusted Publishing only
   for crates that exist, so each new crate's first version needs his token,
   and then a trusted publisher set per crate.

### 4. The rest

After the pilot, write down what it taught us and order the remaining
families from the same graph: shell-only families first, then those needing
a format crate published, then the Spectrum (after the DSK adapter) and the
68k families last.

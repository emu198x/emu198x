# Accuracy corpora — fixtures manifest

Single source of truth for the external CPU test corpora that the
[`nightly-accuracy`](../.github/workflows/nightly-accuracy.yml) workflow runs.
Each corpus is freely redistributable and lives upstream at the source below;
the nightly does **not** fetch from those public sources directly — it pulls
from a **mirror you control** (see § The mirror), so the corpora are hermetic
and upstream-independent, and so firmware-dependent suites (Lorenz needs the C64
KERNAL) can run without publishing a commercial ROM.

`scripts/check-fixtures.sh` reads this table to report which corpora are present
locally; the workflow uses the same env-var contract.

## Corpora

| Corpus | Crate · test file | Env var (points at the extracted dir) | Upstream source | Licence | Needs firmware? |
|--------|-------------------|----------------------------------------|-----------------|---------|-----------------|
| Tom Harte 6502 | `mos-6502` · `single_step_tests` | `EMU198X_6502_TOM_HARTE_DIR` | github.com/SingleStepTests/ProcessorTests (`6502/v1`) | MIT-like (see repo) | no |
| Tom Harte Z80 | `zilog-z80` · `single_step_tests` | `EMU198X_Z80_TOM_HARTE_DIR` | github.com/SingleStepTests/ProcessorTests (`z80/v1`) | MIT-like | no |
| Tom Harte 68000 | `motorola-68000` · `tom_harte` | `EMU198X_68000_TOM_HARTE_ROOT` | github.com/SingleStepTests/ProcessorTests (`680x0/68000/v1`) | MIT-like | no |
| SM83 (Tennant) | `sharp-lr35902` · `single_step_tests` | `EMU198X_SM83_TENNANT_DIR` | github.com/adtennant/sm83-test-data | see repo | no |
| Klaus Dormann 6502 | `mos-6502` · `dormann_tests` | `EMU198X_6502_DORMANN_DIR` | github.com/Klaus2m5/6502_65C02_functional_tests | GPL-3.0 | no |
| FUSE Z80 | `zilog-z80` · `z80_fuse` | `EMU198X_FUSE_Z80_TESTS_DIR` | FUSE emulator (`fuse-emulator-fuse/z80/tests`) | GPL-2.0-or-later | no |
| Wolfgang Lorenz 6502 | `mos-6502` · `lorenz_tests` | `EMU198X_6502_LORENZ_DIR` | Wolfgang Lorenz C64 test suite (public mirrors) | freeware | **yes — C64 KERNAL** |

ZEXDOC/ZEXALL is **not** here: its `.com` corpus is checked into `test-data/zex/`
and runs from its own hermetic [`zex.yml`](../.github/workflows/zex.yml). z80test
is likewise a local-only survey today.

Directory layout each env var points at: the extracted corpus directory. The
SingleStepTests and SM83 corpora are per-opcode JSON files (`ab.json` →
opcode 0xAB, plus `cb.json` for the SM83 CB table). Dormann is a single
`.bin`; FUSE is its `tests.in` / `tests.expected` pair; Lorenz is the suite's
`*.prg` cases plus the KERNAL.

## The mirror

The nightly pulls each corpus from a **private GitHub repo's release assets** —
the "dedicated assets store" — via the `gh` CLI. This keeps the corpora
hermetic (upstream going away doesn't break the nightly) and lets the store hold
the C64 KERNAL for Lorenz without any public redistribution.

**Store contract** (what the workflow expects):

- A repo named by the `ACCURACY_CORPORA_REPO` Actions **variable** (e.g.
  `emu198x/accuracy-corpora`), with a release tagged by `ACCURACY_CORPORA_TAG`
  (default `v1`).
- One `zstd` tarball asset per corpus, named `<artifact>.tar.zst`:
  `harte-6502`, `harte-z80`, `harte-68000`, `sm83`, `dormann-6502`,
  `fuse-z80`, `lorenz-6502` (the Lorenz tarball includes the KERNAL).
- A `SHA256SUMS` asset listing each tarball's checksum — the workflow verifies
  against it, so checksums live in the store, not hard-coded here.
- An Actions **secret** `ACCURACY_CORPORA_TOKEN`: a fine-grained PAT with
  read-only `contents` access to the store repo.

Each tarball extracts to a directory whose path becomes the corpus's env var.

## First-time setup (one-off, yours to do)

1. Create the private store repo; add the `ACCURACY_CORPORA_TOKEN` secret and the
   `ACCURACY_CORPORA_REPO` / `ACCURACY_CORPORA_TAG` variables to *this* repo
   (Settings → Secrets and variables → Actions).
2. Assemble each corpus from its upstream (table above) into
   `<artifact>/` and pack it: `tar --zstd -cf <artifact>.tar.zst <artifact>/`.
   For Lorenz, include your `kernal.rom` in the tarball.
3. `sha256sum *.tar.zst > SHA256SUMS`.
4. Create the release and upload the tarballs + `SHA256SUMS` as assets:
   `gh release create v1 -R <store-repo> *.tar.zst SHA256SUMS`.

Until the store exists and the secret is set, the nightly's `preflight` job
reports "corpora store not configured" and the corpus jobs are skipped (not
failed) — so a fork or a PR without the secret stays green.

Re-run on demand from the Actions tab (`workflow_dispatch`) once the store is
live.

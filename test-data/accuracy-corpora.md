# Accuracy corpora — fixtures manifest

Single source of truth for the external CPU test corpora that the
[`nightly-accuracy`](../.github/workflows/nightly-accuracy.yml) workflow runs.
Each corpus has item-specific access and redistribution terms. The nightly does
**not** fetch from the upstream locations directly — it pulls from a **mirror
you control** (see § The mirror), so configured runs are hermetic and
upstream-independent. A private mirror controls access; it does not establish
permission to copy an upstream corpus.

`scripts/check-fixtures.sh` reads this table to report which corpora are present
locally; the workflow uses the same env-var contract.

## Corpora

| Corpus | Crate · test file | Env var (points at the extracted dir) | Upstream source | Licence | Needs firmware? |
|--------|-------------------|----------------------------------------|-----------------|---------|-----------------|
| Tom Harte 6502 | `mos-6502` · `single_step_tests` | `EMU198X_6502_TOM_HARTE_DIR` | github.com/SingleStepTests/ProcessorTests (`6502/v1`) | MIT-like (see repo) | no |
| Tom Harte Z80 | `zilog-z80` · `single_step_tests` | `EMU198X_Z80_TOM_HARTE_DIR` | github.com/SingleStepTests/ProcessorTests (`z80/v1`) | MIT-like | no |
| SingleStepTests 68000 | `motorola-68000` · `tom_harte` | `EMU198X_68000_TOM_HARTE_ROOT` | github.com/SingleStepTests/680x0 (`68000/v1`) | unknown; no tracked licence at `e0d5ece` | no |
| SM83 (Tennant) | `sharp-lr35902` · `single_step_tests` | `EMU198X_SM83_TENNANT_DIR` | github.com/adtennant/sm83-test-data | see repo | no |
| Klaus Dormann 6502 | `mos-6502` · `dormann_tests` | `EMU198X_6502_DORMANN_DIR` | github.com/Klaus2m5/6502_65C02_functional_tests | GPL-3.0 | no |
| FUSE Z80 | `zilog-z80` · `z80_fuse` | `EMU198X_FUSE_Z80_TESTS_DIR` | FUSE emulator (`fuse-emulator-fuse/z80/tests`) | GPL-2.0-or-later | no |
| Wolfgang Lorenz 6502 | `mos-6502` · `lorenz_tests` | `EMU198X_6502_LORENZ_DIR` | Wolfgang Lorenz C64 test suite (via VICE `bin/`) | freeware | no — uses a synthetic free KERNAL |
| ZEXDOC + ZEXALL | `zilog-z80` · `zex_tests` | `EMU198X_ZEX_DIR` | Frank Cringle Z80 exerciser (`*.com`) | freeware | no |
| z80test | `machine-sinclair-zx-spectrum-48k` · `z80test` | `EMU198X_Z80TEST_DIR` (+ `EMU198X_SPECTRUM_48K_ROM`) | raxoft/z80test (`*.tap`) | MIT | 48K Spectrum ROM — free (Amstrad), shipped in the tarball |

The SingleStepTests 68000 fixture bytes are pinned by
[`singlesteptests-680x0-e0d5ece.sha256`](singlesteptests-680x0-e0d5ece.sha256).
The manifest contains one SHA-256 for each of the 124 compressed fixtures in
registered revision `e0d5ece9670205cc84a0101081837deb446f86a3`. The nightly
checks this manifest after extraction in addition to checking the mirror's
tarball checksum. It covers the fixture inputs consumed by the harness, not
the repository README files or opcode map.

**ZEX and z80test moved here 2026-07-04** for consistency — every external
corpus now runs from this one nightly. ZEX previously ran from checked-in
binaries via a dedicated `zex.yml` workflow, which was retired when the binaries
were removed. z80test runs Patrik Rak's exerciser on a full 48K Spectrum, so its
tarball also carries the free Amstrad-permissioned 48K ROM.

## Project-authored system-level corpus

| Corpus | Consumer | Corpus path | Strict wrapper | Licence | Required firmware |
|---|---|---|---|---|---|
| Amiga programmable HBLANK | `runtime-commodore-amiga` · `amiga_programmable_hblank` | [`commodore/amiga/programmable-hblank/`](commodore/amiga/programmable-hblank/) | [`scripts/verify-amiga-programmable-hblank.sh`](../scripts/verify-amiga-programmable-hblank.sh) | CC0-1.0 | Kickstart images for the selected ECS and AGA profiles, supplied externally |
| Amiga programmable HBLANK write timing | Reference evidence registered; Emu198x consumer pending | [`commodore/amiga/programmable-hblank-write-timing/`](commodore/amiga/programmable-hblank-write-timing/) | none | CC0-1.0 | Kickstart images for the selected ECS and AGA profiles, supplied externally |
| Amiga Paula audio | `runtime-commodore-amiga` · `amiga_paula_audio` | [`commodore/amiga/paula-audio/`](commodore/amiga/paula-audio/) | [`scripts/verify-amiga-paula-audio.sh`](../scripts/verify-amiga-paula-audio.sh) | CC0-1.0 | Kickstart 1.3 r34.005, supplied externally |

The programmable-HBLANK corpus is project-authored and emulator-neutral.
Sources, case definitions, schemas, and deterministic build tools are retained
in the corpus directory. Generated ADFs, payloads, and the suite manifest below
`dist/` are ignored and rebuilt by the strict wrapper. Commercial Kickstart
ROMs are not included.

The source cases currently leave expected observations unresolved. The
Emu198x lane therefore verifies identities, boots the probes, and reports
stable measurements; it does not claim semantic conformance until independent
evidence promotes an expected observation. The first gate covers CCK-aligned
cases on ECS and AGA profiles. The AGA fine-position cases are excluded until
the capture grid can represent their 70 ns and 35 ns placement.

The write-timing corpus is a separate five-case suite because it asks about
state changes within a line rather than settled output geometry. Its
registered FS-UAE package contains ten stable ECS and AGA observations from
the UAE implementation family. Copperline 0.13.0 and vAmiga 4.4b12 cannot
answer the question through an admissible path. The observations therefore
remain single-family evidence, and no Emu198x conformance consumer is
registered yet.

The Paula-audio corpus is a three-case steady-waveform suite. Its Emu198x
consumer verifies corpus identity, boots each case, and asserts internal
cadence, stereo-routing, equal-channel, and paired-volume invariants. Those
measurements are Emu198x-produced observations rather than independent
expected results. Semantic conformance remains unresolved until an audited
reference producer supplies a registered capture.

## System-level external gates

| Fixture | Consumer | Env var | Upstream source | Licence | Required firmware |
|---|---|---|---|---|---|
| Amiga Test Kit v1.12 | `runtime-commodore-amiga` · `amiga_test_kit` | `EMU198X_AMIGA_TEST_KIT_ADF` | keirf/amiga-stuff tag `testkit-v1.12` | Public domain / Unlicense | Kickstart 1.3 r34.005 through `EMU198X_AMIGA_KICKSTART_13_ROM` |
| Amiga Test Kit v1.21 video | `runtime-commodore-amiga` · `amiga_test_kit_video` | `EMU198X_AMIGA_TEST_KIT_V121_ADF` | keirf/amiga-stuff tag `testkit-v1.21` | Public domain / Unlicense | Kickstart 1.3 r34.005 through `EMU198X_AMIGA_KICKSTART_13_ROM` |

The Test Kit ADFs and their required Kickstart image are pinned by
[`amiga-test-kit-v1.12.sha256`](amiga-test-kit-v1.12.sha256) and
[`amiga-test-kit-v1.21.sha256`](amiga-test-kit-v1.21.sha256). An ADF may be
delivered raw or in a ZIP; each manifest applies to the normalised ADF bytes.
The public-domain ADFs remain externally supplied, and the proprietary
Kickstart ROM must not be added to the corpus store.

The v1.12 gate is invoked through
[`scripts/verify-amiga-test-kit.sh`](../scripts/verify-amiga-test-kit.sh). The
v1.21 video gate is invoked through
[`scripts/verify-amiga-test-kit-video.sh`](../scripts/verify-amiga-test-kit-video.sh).
Neither is part of the CPU-corpus matrix or the current private-mirror contract
below. Their assertion boundaries are documented in
[`Amiga Test Kit verification`](../knowledge/processes/amiga-test-kit-verification.md)
and
[`Amiga Test Kit v1.21 video conformance`](../knowledge/processes/amiga-test-kit-video-conformance.md).

Directory layout each env var points at: the extracted corpus directory. The
6502 and Z80 SingleStepTests corpora and the SM83 corpus use per-opcode JSON
files (`ab.json` → opcode 0xAB, plus `cb.json` for the SM83 CB table). The
68000 corpus uses compressed instruction-group files such as
`ADD.b.json.gz`. Dormann is a single `.bin`; FUSE is its `tests.in` /
`tests.expected` pair; Lorenz is the suite's case files plus a `kernal.rom`.

**Lorenz uses a synthetic, fully-free KERNAL — no commercial ROM.** The Lorenz
harness traps CHROUT and installs its own reset/IRQ vectors, so the only KERNAL
code the suite executes is the interrupt handlers at `$EA31`/`$FE66`/NMI. A
hand-authored 8 KB KERNAL supplying compatible minimal handlers there (filler
elsewhere) reproduces the real-KERNAL result on all 265 cases — verified
2026-07-04. It is generated by
[`commodore/c64/synthetic-kernal/`](commodore/c64/synthetic-kernal/); the
`lorenz-6502` tarball carries it as `kernal.rom`. This keeps the Cloanto C64
KERNAL entirely off CI.

## The mirror

The nightly pulls each configured corpus from a **private GitHub repo's release
assets** — the "dedicated assets store" — via the `gh` CLI. This keeps the
corpora hermetic when the store has been populated.

The store's privacy is an access control, not a licence. Each asset requires a
recorded basis for the intended mirroring. In particular, the registered
SingleStepTests `680x0` revision contains no tracked licence, so its
redistribution remains unknown. Its existing private delivery asset must not be
published or made more widely accessible without a rights review.

**Store contract** (what the workflow expects):

- A repo named by the `ACCURACY_CORPORA_REPO` Actions **variable** (e.g.
  `emu198x/accuracy-corpora`), with a release tagged by `ACCURACY_CORPORA_TAG`
  (default `v1`).
- One `zstd` tarball asset per corpus, named `<artifact>.tar.zst`:
  `harte-6502`, `harte-z80`, `harte-68000`, `sm83`, `dormann-6502`,
  `fuse-z80`, `lorenz-6502` (the Lorenz tarball includes the KERNAL).
- A `SHA256SUMS` asset listing each tarball's checksum — the workflow verifies
  against it, so checksums live in the store, not hard-coded here.
- The `harte-68000` asset must contain files matching the in-repository
  `singlesteptests-680x0-e0d5ece.sha256` manifest.
- An Actions **secret** `ACCURACY_CORPORA_TOKEN`: a fine-grained PAT with
  read-only `contents` access to the store repo.

Each tarball extracts to a directory whose path becomes the corpus's env var.

## First-time setup (one-off, yours to do)

1. Create the private store repo; add the `ACCURACY_CORPORA_TOKEN` secret and the
   `ACCURACY_CORPORA_REPO` / `ACCURACY_CORPORA_TAG` variables to *this* repo
   (Settings → Secrets and variables → Actions).
2. For each corpus whose intended mirroring has been reviewed, assemble it from
   its upstream (table above) into `<artifact>/` and pack it:
   `tar --zstd -cf <artifact>.tar.zst <artifact>/`.
   For Lorenz, drop the synthetic `kernal.rom` (from
   `commodore/c64/synthetic-kernal/`) into the tarball alongside the cases.
3. `sha256sum *.tar.zst > SHA256SUMS`.
4. Create the release and upload the tarballs + `SHA256SUMS` as assets:
   `gh release create v1 -R <store-repo> *.tar.zst SHA256SUMS`.

Until the store exists and the secret is set, the canonical
`emu198x/emu198x` workflow fails in `preflight`. A fork without the store
configuration reports the missing assets and skips the corpus jobs.

Re-run on demand from the Actions tab (`workflow_dispatch`) once the store is
live.

## Related documents

- [Amiga Test Kit v1.12 fixture identity](amiga-test-kit-v1.12.md)
- [Amiga Test Kit v1.21 fixture identity](amiga-test-kit-v1.21.md)
- [Amiga Test Kit verification](../knowledge/processes/amiga-test-kit-verification.md)
- [Amiga Test Kit v1.21 video conformance](../knowledge/processes/amiga-test-kit-video-conformance.md)
- [Portable programmable-HBLANK corpus](commodore/amiga/programmable-hblank/README.md)
- [Amiga programmable-HBLANK conformance](../knowledge/processes/amiga-programmable-hblank-conformance.md)
- [Portable programmable-HBLANK write-timing corpus](commodore/amiga/programmable-hblank-write-timing/README.md)
- [Amiga programmable-HBLANK write timing](../knowledge/processes/amiga-programmable-hblank-write-timing.md)
- [Portable Paula-audio corpus](commodore/amiga/paula-audio/README.md)
- [Amiga Paula-audio conformance](../knowledge/processes/amiga-paula-audio-conformance.md)
- [Test ROM bundling policy](../knowledge/decisions/test-rom-policy.md)

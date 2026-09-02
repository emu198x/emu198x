# Decision: Test ROM bundling policy

**Date:** 2026-05-23
**Status:** Locked. Governs what test ROMs are checked into the
repo (`test-data/`) vs referenced from user-provided external
locations (env-var path patterns). Audit of current state +
policy for future additions.

## What this is

The project verifies CPU and chip behaviour against external test
ROM corpora (Blargg, Tom Harte, mooneye, mealybug, ZEX, …). Some
of these ROMs have explicit permissive licenses; some don't but
have been universally redistributed for decades. This record
audits the current state and sets the policy for what we may
bundle in-repo vs reference externally.

The decision matters because:

- Bundled ROMs travel with every clone, mirror, fork, and binary
  distribution. We become a redistributor with whatever obligations
  that carries.
- Referenced ROMs (via env-var path patterns) stay with the user;
  we just point at where they live. No redistribution by us.

## Current state (audit)

### Bundled

No external test ROM corpus is currently bundled in `test-data/`.
The generated M68k fixtures retained there are project-owned test
outputs rather than redistributed ROMs.

### Not bundled; user-provided via env var

| Test corpus | Env var | License | Bundled-safe? |
|---|---|---|---|
| Blargg Game Boy test ROMs (`cpu_instrs`, `instr_timing`, etc.) | `EMU198X_GB_BLARGG_ROOT` | Implicit by universal redistribution; no explicit grant | No — don't bundle |
| mooneye-gb test ROMs | `EMU198X_GB_MOONEYE_ROOT` | MIT (test ROMs separately MIT-licensed from the GPLv3 main emulator) | Yes if we want; currently not bundled |
| `dmg-acid2.gb` (mattcurrie) | `EMU198X_GB_DMG_ACID2_ROM` | MIT | Yes if we want |
| Blargg NES test ROMs | (no env var yet; planned per NES Seam 1) | Implicit by universal redistribution; no explicit grant | No — don't bundle |
| `nestest.nes` (Kevin Horton / kevtris) | (referenced in README example) | Implicit by 20+ years of universal redistribution; no explicit grant | No — don't bundle |
| Super Mario Bros. ROM (NES regression) | `EMU198X_NES_SMB_ROM` | Commercial, copyrighted (Nintendo) | No — never bundle |
| Manic Miner / Jet Set Willy TZX (Spectrum regression) | `EMU198X_SPECTRUM_MANIC_MINER_TZX` / `…_JET_SET_WILLY_TZX` | Commercial (Bug-Byte / Software Projects); some titles have permissive distribution permission, varies | No — never bundle |
| Mealybug Tearoom (mattcurrie, future use) | (env var TBD when first referenced) | MIT | Yes if we want |
| Sinclair 48K ROM (firmware, npm package only) | `EMU198X_SPECTRUM_48K_ROM` | Commercial (Amstrad), permitted for redistribution with emulators | Not in this repo; embedded in the published `@emu198x/zx-spectrum` wasm only — see § Firmware in a published browser build |
| ZEXDOC / ZEXALL | `EMU198X_ZEX_DIR` | No explicit grant; long-standing redistribution | No — referenced externally since 2026-07-04 |
| Amiga Test Kit v1.12 | `EMU198X_AMIGA_TEST_KIT_ADF` | Public domain / Unlicense | Yes for the ADF; required Kickstart remains proprietary |
| Amiga Test Kit v1.21 | `EMU198X_AMIGA_TEST_KIT_V121_ADF` | Public domain / Unlicense | Yes for the ADF; required Kickstart remains proprietary |

### Hand-rolled in-repo (not external test ROMs, just shaped like)

- **`crates/motorola-68040/tests/tom_harte.rs`** — project-authored
  single-step harness for the Musashi-generated MC68040 corpus. No
  upstream SingleStepTests MC68040 corpus exists; the filename is a
  legacy identifier. The harness is project work
  (GPL-2.0-or-later like the rest of Emu198x).

### SingleStepTests processor repositories

- [github.com/TomHarte/ProcessorTests](https://github.com/TomHarte/ProcessorTests)
  redirects to the archived `SingleStepTests/ProcessorTests`
  repository, whose suites have since been split into separate
  repositories.
- The exact `SingleStepTests/680x0` revision consumed by the M68k
  harnesses, `e0d5ece9670205cc84a0101081837deb446f86a3`, has no
  tracked `LICENSE`, `COPYING`, `NOTICE`, copyright statement, or
  license declaration.
- **Rights status for the retained 680x0 revision: undetermined.**
  A license in a related or containing repository must not be
  applied to a split suite without evidence that it covers the
  exact retained files.
- The checkout is referenced by `motorola-68000` and by the
  inherited-subset harnesses in `motorola-68010` and
  `motorola-68020`. It is not currently bundled in this repository;
  its size supports retaining the referenced-fixture pattern.
- **SingleStepTests/680x0 redistribution: unknown.** Keep it
  referenced and do not bundle it pending an exact rights review.

The separate `SingleStepTests/m68000` repository at revision
`64b253116a3de04aaac4346c43680960dc9b67e5` carries an MIT licence
covering its compact binary fixtures. The 68000 comparison harness
references its 127 files through `EMU198X_68000_MAME_ROOT`; the corpus
is not bundled because its size and independent revision history fit
the external-fixture pattern. Its README identifies MAME's microcoded
MC68000 core as the generator and explicitly excludes TAS and TRAPV
from its verified set.

## The policy

### Three tiers based on license clarity

**Tier 1: explicit permissive license (MIT / Apache-2.0 / BSD /
CC0 / public domain).** Bundling is safe. Whether to bundle is a
practical decision (size, churn, convenience). Examples: Tom
Harte vectors whose exact source carries an applicable license,
mealybug, mooneye-gb test ROMs (the test ROMs specifically, not
the main emulator code), dmg-acid2.

**Tier 2: no explicit license but universally redistributed for
decades.** Effectively public domain by practice. Examples: ZEX
(historically bundled, now external), Blargg test ROMs, nestest.
**Default: don't bundle; reference via env-var path.**

**Tier 3: commercial / copyrighted.** Never bundle. Examples: any
commercial game ROM (Manic Miner, Super Mario Bros., …),
Kickstart ROMs, C64 KERNAL. Always referenced via env var or
documented per-system path convention (`~/.emu198x/roms/<system>/`
or `~/.emu198x/media/<system>/`).

### When bundling, document provenance

Every bundled test ROM (or test ROM directory) ships with a
README documenting:

- Author and year
- Original distribution channel
- License or redistribution status (with reasoning if Tier 2)
- Byte size and exact file checksums (so anyone can verify they
  have the same bytes everyone else does)
- Which tests / regressions consume it

Use the provenance fields above as the required template.

### When referencing, document via env var

Every test ROM consumed via env var path follows the pattern:

- `EMU198X_<SYSTEM>_<CORPUS>_<DETAIL>` env var (e.g.,
  `EMU198X_GB_BLARGG_ROOT`, `EMU198X_NES_SMB_ROM`)
- Ordinary local tests use skip-if-missing semantics: they log
  `skipping: set EMU198X_X to <description>` and pass
- Documented in the consumer test file's prologue comments and (if
  user-facing) in the README's Getting ROMs section

### Explicit accuracy-gate exception

An ignored test that is deliberately invoked as an accuracy gate may
require its external asset. In that context:

- an explicitly supplied path is authoritative, and a missing, unreadable
  or malformed asset fails the test;
- the canonical scheduled workflow fails preflight when required corpus
  storage is not configured, while forks may record a skip;
- an in-repository checksum manifest pins the identity of every consumed
  fixture independently of the delivery archive;
- a private object store is a delivery cache, not the canonical source
  and not evidence of redistribution rights.

Each exact asset still requires an item-specific rights assessment. Do
not upload an asset with undetermined rights solely because a workflow
can consume it. Existing private availability does not establish a right
to redistribute or expand access.

## Why this policy

### Why Tier 2 stays unbundled

ZEX was bundled before this policy existed. It moved to the external
corpus store on 2026-07-04 and is now supplied through
`EMU198X_ZEX_DIR`, so the regression gate no longer requires repository
redistribution. Its long-standing availability remains evidence of
provenance, not an explicit licence grant.

Bundling new or previously removed Tier 2 ROMs would expand our
redistribution exposure without expanding the project's value
proportionally.

The cost of referencing Blargg / nestest (env var + skip-if-
missing) is small for the user (download once into the path) and
zero for us. The benefit of bundling them (no per-user download
step) doesn't justify becoming a redistributor for content we
don't have explicit rights to redistribute.

### Why permissively licensed test ROMs stay unbundled too

Even with explicit MIT, Unlicense, or public-domain permission, we default to
referenced-not-bundled because:

- The MIT-licensed test ROMs we'd want (mealybug, mooneye-gb
  tests, dmg-acid2) total a few hundred KiB to a few MiB —
  bundling is feasible but adds noise to the repo
- Users who want these tests already have the source repos at
  hand; the env-var pattern matches how they currently work
- Future Tom Harte expansion (more CPUs, more vectors) makes the
  cost of bundling unbounded
- The env-var pattern is uniform across all external test corpora;
  bundling some-but-not-others is a special case we'd have to
  document

**Exception:** if a small, stable, Tier 1 test ROM becomes
critical to a CI regression gate, we
revisit bundling for that specific case with the standard
provenance README.

Amiga Test Kit follows the default. Each registered 901,120-byte ADF is safe
to redistribute, but the complete gates also need a proprietary Kickstart
image. Keeping both kinds of input external gives each lane one delivery
contract without placing firmware in the repository. The exact normalised
inputs remain pinned by `test-data/amiga-test-kit-v1.12.sha256` and
`test-data/amiga-test-kit-v1.21.sha256`, with the A1200 profile pinned
separately by `test-data/amiga-test-kit-v1.21-a1200-aga-pal.sha256`.

### Why we never bundle commercial ROMs

Obvious. Just naming it for completeness.

### Firmware in a published browser build (added 2026-09-02)

The `@emu198x/zx-spectrum` npm package embeds the Sinclair 48K ROM in its
wasm artifact. This is the documented special case the "bundle a commercial
ROM with permission from the rights-holder" drift trigger asks for, and it is
narrow.

**Scope.** The Spectrum 48K ROM, in the published browser package only. No
other firmware, no other system, and not in this repository — see § How it
reaches the artifact below. C64 KERNAL and Amiga Kickstart are unchanged:
never bundled, no permission of this kind on record.

**Basis.** Cliff Lawson of Amstrad plc, answering Andrew Owen on
comp.sys.sinclair, 31 August 1999. Archived by World of Spectrum:
<https://web.archive.org/web/20180828125931/http://www.worldofspectrum.org/permits/amstrad-roms.txt>

> Amstrad are happy for emulator writers to include images of our copyrighted
> code as long as the (c)opyright messages are not altered and we appreciate
> it if the program/manual includes a note to the effect that "Amstrad have
> kindly given their permission for the redistribution of their copyrighted
> material but retain that copyright".

It is a public statement by the rights-holder rather than a signed licence,
and the whole retro-computing field has relied on it for a quarter of a
century. Treat it as what it is: durable, specific, and informal.

**What it covers, and what it does not.** The statement is explicit about its
own edges, and they matter here because this repository emulates machines on
both sides of the line.

| ROM | Covered? | Source |
|---|---|---|
| Spectrum 48K, 128K | Yes | "Amstrad only bought the rights to Spectrum 48/128 from Sinclair" |
| Spectrum +2, +2A, +3 | Yes | "then produced the + machines ourselves" |
| ZX80, ZX81, Interface 1/2 | **No** | "I do not believe the (c) for ZXs or IF1/2 has anything to do with Amstrad" |
| Timex TC2048 / TS2068 | **No** | "Ask Timex. We only hold the copyright for code written by Sinclair or Amstrad" |
| Pentagon, Scorpion | **No** | Clones, and answered with an opinion about piracy rather than a grant |
| Spectrum game software | **No** | "remains the copyrighted property of its authors (Ocean etc.)" |

This repository has runtimes for Pentagon, Scorpion, TC2048 and TS2068. **None
of their ROMs are covered by this permission**, and none may be bundled under
this section. It authorises the Sinclair/Amstrad Spectrum firmware and nothing
else.

**Conditions, all three of which bind us.**

1. *Copyright messages must not be altered.* We embed the image verbatim and
   do not patch it, so this holds by construction. A future change that
   patches firmware — a fast-loader hook, say — would step outside the
   permission as written, even though Lawson separately allows modification.
2. *An acknowledgement is requested* in the program or manual, in specific
   words. Carried in the package README, which wasm-pack ships inside the
   published artifact, so it travels with the ROM rather than sitting only in
   this repository.
3. *No charging for the ROM code.* "No one should be charging for the ROM
   code." The package is free; if any part of this family is ever sold, the
   firmware cannot be part of what is charged for.


**Why bundling rather than serving the file.** A `.rom` served beside the
page is functionally a ROM download: separable, linkable, and indistinguishable
from a ROM site. Embedded in the wasm it is inseparable from the emulator,
which is the condition the permission is granted under. The bundling is
therefore the more conservative option here, not the more permissive one — the
opposite of how it first looks.

**What it costs.** The project becomes a redistributor through npm, which
mirrors and caches more widely than a site does. That exposure is accepted
deliberately for one 16 KiB firmware image on one system, and is the reason
this section is narrow rather than a general firmware exemption.

**How it reaches the artifact.** Not through this repository. The crate
compiles without the ROM by default; `scripts/build-npm.sh` enables the
`bundled-rom` feature and reads the image from `EMU198X_SPECTRUM_48K_ROM`,
the same environment variable the tests already use. So a clone, a fork or a
mirror of this repository carries no firmware, and only the published package
does. `*.rom` stays in the crate's `.gitignore` to keep that true by
construction.

## What we are NOT doing

- **Bundling Blargg test ROMs** even though they're universally
  redistributed. The cost of becoming a redistributor without
  explicit grant is not paid back by avoiding a one-time user
  download.
- **Bundling nestest.nes** same reasoning.
- **Bundling large external JSON corpora.** Size + churn argue for
  referenced-not-bundled, and each exact source still requires a
  license review.
- **Treating a private CI delivery cache as a canonical or blessed
  mirror.** Canonical identity comes from registered source provenance
  and committed checksums; storage configuration does not replace rights
  review.
- **Re-bundling ZEX for convenience.** The environment-variable path
  already supports the regression without repository redistribution.
- **Bundling Amiga Test Kit because it is public domain.** The ADFs are
  permissively redistributable, but the external fixture path is already
  required for their Kickstart dependency and the committed checksum manifests
  supply reproducible identities without adding the images.

## Future additions

When a new test ROM corpus is added:

1. Determine its tier (1 / 2 / 3) by license check
2. Default: reference via env var (with `EMU198X_<SYSTEM>_<CORPUS>`
  pattern + skip-if-missing semantics for ordinary local tests)
3. If an ignored test becomes an explicit accuracy gate, document its
   required-asset failure semantics and pin every consumed fixture
4. If bundling is genuinely needed (e.g., CI-gated regression with
   no reasonable user-download path), and the corpus is Tier 1 and
   small (< 100 KiB), bundle with a per-folder provenance README
5. Update the audit table at the top of this record

## Drift triggers

- **"Bundle Blargg/nestest so users don't have to download
  separately"** — re-read § Why Tier 2 stays unbundled. The
  redistribution exposure isn't justified by the convenience.
- **"Bundle SingleStepTests vectors because a related repository
  is MIT"** — review the exact retained source. The `680x0`
  revision currently has no tracked license, and related-repository
  metadata is not sufficient.
- **"Bundle this small MIT test ROM for convenience"** — only if
  it's CI-gated AND small AND a per-user download path is
  awkward. Otherwise stay with env-var pattern for uniformity.
- **"Bundle a commercial ROM with permission from the rights-
  holder"** — out of scope of this policy. If permission is real,
  document it as a special case and revisit. Done once, narrowly:
  § Firmware in a published browser build. Reaching for that section
  to justify a second firmware image is the drift — it covers one ROM,
  on one system, in one artifact.

## Log

### 2026-08-01 — A1200 Test Kit video profile registered

The A1200 AGA PAL profile reuses the external Test Kit v1.21 ADF and requires
an external proprietary Kickstart 3.1 A1200 image. Its profile-specific
checksum manifest, FS-UAE reference manifest, and PNG checksums are independent
of the earlier A500 delivery record. No firmware or ADF payload was added to
the repository.

### 2026-07-28 — Amiga Test Kit v1.21 video gate registered

The pixel-reference gate now references Test Kit v1.21 through
`EMU198X_AMIGA_TEST_KIT_V121_ADF`. The ADF and proprietary Kickstart remain
external. Their normalised identities are pinned independently of the
committed vAmiga reference manifest and PNG checksums.

### 2026-07-28 — Amiga Test Kit v1.12 registered

The system-level Amiga gate now references Test Kit v1.12 through
`EMU198X_AMIGA_TEST_KIT_ADF`. Its public-domain ADF and the proprietary
Kickstart 1.3 image are both external. The committed checksum manifest pins the
normalised bytes consumed by the explicit ignored gate; it does not grant a
right to redistribute Kickstart.

### 2026-07-04 — ZEX moved to external corpus storage

The checked-in ZEXDOC and ZEXALL binaries were removed when the Z80
exerciser joined the shared external-corpus workflow. Tests now use
`EMU198X_ZEX_DIR`. This removed the only grandfathered Tier 2 bundle;
no external test ROM corpus is currently stored in `test-data/`.

### 2026-07-21 — SingleStepTests 680x0 rights correction

The exact retained `SingleStepTests/680x0` revision contains no
tracked license or rights notice. The earlier blanket MIT and
bundled-safe wording was therefore narrowed to item-specific
review. The suite remains referenced rather than bundled, so no
repository content needed removal.

The explicit MC68000 full-sweep invocation is also recorded as an
exception to ordinary skip-if-missing behaviour. Its canonical scheduled
job requires configured storage and verifies an in-repository per-file
checksum manifest. The delivery cache does not change the suite's
undetermined redistribution status.

### 2026-05-23 — Policy locked

At the time of this audit, ZEX was the only bundled test ROM and
had documented rationale; everything else was referenced via env
var with skip-if-missing semantics. ZEX moved out of the repository
on 2026-07-04.

Policy locked: three tiers (explicit permissive / universal
redistribution / commercial), default to referenced-not-bundled
even for Tier 1, never bundle Tier 3, bundle Tier 1 only when
CI-gated and small with a provenance README.

The README's references to Blargg + nestest as `~/.emu198x/media/`
paths align with this policy. No action needed in the README.

## Related documents

- [Accuracy corpora](../../test-data/accuracy-corpora.md)
- [Amiga Test Kit v1.12 fixture identity](../../test-data/amiga-test-kit-v1.12.md)
- [Amiga Test Kit v1.21 fixture identity](../../test-data/amiga-test-kit-v1.21.md)
- [Amiga Test Kit verification](../processes/amiga-test-kit-verification.md)
- [Amiga Test Kit v1.21 video conformance](../processes/amiga-test-kit-video-conformance.md)

### 2026-09-02 — Firmware bundling, narrowly, for the browser package

The browser programme (#1414) reached the point where a lesson embed needs a
ROM to boot. Serving one beside the page and embedding one in the wasm are
both redistribution; the second is inseparable from the emulator and so sits
better with the permission the project already relies on.

Recorded as the special case the drift trigger asks for, scoped to the
Spectrum 48K ROM in the published npm package. The repository still carries no
firmware: the crate builds without it, and the publish step supplies it from
`EMU198X_SPECTRUM_48K_ROM`.

Citation added the same day: Cliff Lawson, Amstrad plc, comp.sys.sinclair,
31 August 1999, archived at World of Spectrum. Reading the source changed the
record rather than merely footnoting it — the permission is conditional and
bounded, it excludes the ZX80/ZX81, Interface 1/2, Timex and the Russian
clones this repository also emulates, and it asks for an acknowledgement we
were not yet carrying. That acknowledgement now ships in the package README.

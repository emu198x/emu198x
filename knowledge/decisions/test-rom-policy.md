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

- **`test-data/zex/zexdoc.com`** + **`test-data/zex/zexall.com`**
  (~17 KiB total). Frank Cringle's Z80 exerciser, 1996.
  Originally distributed under "yaze" (Yet Another Z80 Emulator).
  Redistributed for ~30 years across every major Z80 emulator
  project (Fuse, MAME, RetroArch, ZEsarUX, …) as standard
  regression fixtures. Provenance + redistribution rationale
  documented at [`test-data/zex/README.md`](../../test-data/zex/README.md).
  **Effectively public domain by long-standing universal
  redistribution.** Status: keep bundled.

### Not bundled; user-provided via env var (skip-if-missing)

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

### Hand-rolled in-repo (not external test ROMs, just shaped like)

- **`crates/motorola-68040/tests/tom_harte.rs`** — Tom Harte-style
  single-step harness for the 68040. No upstream Tom Harte 68040
  corpus exists yet; this is our own baseline. No licensing issue
  (project's own work, GPL-2.0-or-later like everything else).

### Tom Harte processor tests (upstream, MIT)

- [github.com/TomHarte/ProcessorTests](https://github.com/TomHarte/ProcessorTests)
- Covers 6502, Z80, 65816, 68000 (and growing). Millions of JSON
  test vectors.
- **License: MIT.** Explicit and clean.
- Currently consumed by each CPU crate's test fixtures (downloaded
  / referenced as needed). Not currently bundled in our repo;
  vectors are large enough that referencing is the right pattern.
- **Bundled-safe: yes.** But size + churn argue for referenced-not-
  bundled at the gigabytes-of-JSON scale.

## The policy

### Three tiers based on license clarity

**Tier 1: explicit permissive license (MIT / Apache-2.0 / BSD /
CC0 / public domain).** Bundling is safe. Whether to bundle is a
practical decision (size, churn, convenience). Examples: Tom
Harte vectors, mealybug, mooneye-gb test ROMs (the test ROMs
specifically, not the main emulator code), dmg-acid2.

**Tier 2: no explicit license but universally redistributed for
decades.** Effectively public domain by practice. Examples: ZEX
(already bundled with documented rationale), Blargg test ROMs,
nestest. **Default: don't bundle; reference via env-var path.**
The ZEX case is grandfathered (already bundled with documented
rationale predating this policy); future Tier 2 additions are not
bundled.

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

`test-data/zex/README.md` is the template.

### When referencing, document via env var

Every test ROM consumed via env var path follows the pattern:

- `EMU198X_<SYSTEM>_<CORPUS>_<DETAIL>` env var (e.g.,
  `EMU198X_GB_BLARGG_ROOT`, `EMU198X_NES_SMB_ROM`)
- Skip-if-missing semantics: tests log `skipping: set EMU198X_X
  to <description>` and pass
- Documented in the consumer test file's prologue comments and (if
  user-facing) in the README's Getting ROMs section

## Why this policy

### Why Tier 2 stays unbundled by default (grandfathering ZEX)

ZEX is bundled because it was bundled before this policy existed
and removing it would break the existing Z80 regression CI gate.
The rationale (universal 30-year redistribution) is documented in
its README. **That's the bar for grandfathering Tier 2, not a
template for adding more.** Bundling new Tier 2 ROMs would expand
our redistribution exposure without expanding the project's value
proportionally.

The cost of referencing Blargg / nestest (env var + skip-if-
missing) is small for the user (download once into the path) and
zero for us. The benefit of bundling them (no per-user download
step) doesn't justify becoming a redistributor for content we
don't have explicit rights to redistribute.

### Why MIT test ROMs (mealybug, dmg-acid2) stay unbundled too

Even with explicit MIT permission, we default to referenced-not-
bundled because:

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
critical to a CI regression gate (the way ZEX did for Z80), we
revisit bundling for that specific case with the standard
provenance README.

### Why we never bundle commercial ROMs

Obvious. Just naming it for completeness.

## What we are NOT doing

- **Bundling Blargg test ROMs** even though they're universally
  redistributed. The cost of becoming a redistributor without
  explicit grant is not paid back by avoiding a one-time user
  download.
- **Bundling nestest.nes** same reasoning.
- **Bundling Tom Harte JSON vectors** even though MIT-licensed.
  Size + churn argue for referenced-not-bundled.
- **Maintaining a "blessed mirror" of external test corpora** in
  a separate repo or release artifact. Defer until someone asks
  for it; the env-var pattern works.
- **Removing ZEX from `test-data/`.** Grandfathered with documented
  rationale.

## Future additions

When a new test ROM corpus is added:

1. Determine its tier (1 / 2 / 3) by license check
2. Default: reference via env var (with `EMU198X_<SYSTEM>_<CORPUS>`
  pattern + skip-if-missing semantics)
3. If bundling is genuinely needed (e.g., CI-gated regression with
  no reasonable user-download path), and the corpus is Tier 1 and
  small (< 100 KiB), bundle with a per-folder provenance README
4. Update the audit table at the top of this record

## Drift triggers

- **"Bundle Blargg/nestest so users don't have to download
  separately"** — re-read § Why Tier 2 stays unbundled. The
  redistribution exposure isn't justified by the convenience.
- **"Bundle Tom Harte vectors because they're MIT"** — size and
  churn make this expensive; referenced-not-bundled is the right
  pattern even for Tier 1 at that scale.
- **"Bundle this small MIT test ROM for convenience"** — only if
  it's CI-gated AND small AND a per-user download path is
  awkward. Otherwise stay with env-var pattern for uniformity.
- **"Bundle a commercial ROM with permission from the rights-
  holder"** — out of scope of this policy. If permission is real,
  document it as a special case and revisit.

## Log

### 2026-05-23 — Policy locked

Audit confirms the current state is clean: ZEX is the only
bundled test ROM and has documented rationale; everything else is
referenced via env var with skip-if-missing semantics. No
licensing concerns surfaced in the audit.

Policy locked: three tiers (explicit permissive / universal
redistribution / commercial), default to referenced-not-bundled
even for Tier 1, grandfather ZEX, never bundle Tier 3, bundle Tier
1 only when CI-gated and small with a provenance README.

The README's references to Blargg + nestest as `~/.emu198x/media/`
paths align with this policy. No action needed in the README.

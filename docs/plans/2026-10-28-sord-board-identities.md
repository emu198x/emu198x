# Sord M5 board identities and catalogue accuracy

Lane: engineering-frontier. The per-board boundary is already adopted by
RULES.md 34 and knowledge/decisions/each-machine-has-its-own-machine-crate.md.
This plan proposes a bounded first split rather than starting every family.

## Independent data corrections

- Check the Sord monitor/BASIC-cartridge distinction against primary artefacts
  and manufacturer material. Correct runtime-sord-m5/src/profiles.rs if its
  8 KiB required image is incorrectly described as including BASIC-I.
- Check TMS9928A/9929A output descriptions against TI's original November 1982
  data manual. Correct the primary-reference syntheses before downstream data.
  Do not change video timing or electrical presentation in a wording fix.
- Verify existing profile/firmware tests and inspect built catalogue output.
  Avoid tests that merely pin the replacement prose.

## Proposed split — approval required before API/dependency changes

1. Retire the shared machine-sord-m5 implementation. Introduce private workspace
   common-sord-m5, machine-sord-m5-ntsc and machine-sord-m5-pal. Neither concrete
   machine depends on the other. Common code owns shared wiring; concrete board
   types fix video/raster configuration at construction rather than a runtime
   region switch inside a machine crate. Reuse the family's existing shared-core
   and runtime-dispatch conventions, not an unrelated new execution architecture.
2. Move the current implementation/tests into those owners. Inventory every
   serialized field before moving code. Keep the existing clock loops, recent
   exact-frame and restore/reset fixes, and pending PAL-state limits from #252.
3. Update Cargo.toml/lock and runtime-sord-m5/src/{runtime,input,queries,snapshot}.rs
   for the two concrete types behind family-owned dispatch. This replaces the
   old shared Rust construction API; all in-workspace consumers must migrate.
   Keep emu198x-sord-m5 and its CLI/model/profile IDs. No registry publication
   belongs to this PR.
4. Preserve the version-5 snapshot byte contract, using an explicit compatibility
   adapter if dispatch wrapping would otherwise add a tag. Validate selected
   board identity on restore. Capture baseline state bytes and future audio/
   CPU/video/input observations before moving code; compare old/new execution.
   A real schema change requires separate approval, not silent version churn.
5. Give the boards distinct machine IDs in profiles and docs/status/systems.toml.
   Audit machine-ID consumers first: catalogue/fleet probes, native state paths,
   browser routes, status scripts and existing family labels/milestones. Preserve
   user entry points with explicit family aliases where needed; do not invent
   board-specific issue attribution. Update the #2144 tracker after verification.
6. Run focused machine/runtime/app and real identified firmware tests, then the
   affected catalogue/registry/fleet/UI checks, strict Clippy and formatting.
   Push a reviewable PR; merge only after current exact-head CI is complete.
   Verify remote retention and remove worktrees, caches and task processes.

## Sequencing to settle

#2144 still says splits wait for the current family release; #2170 (v0.46.0)
is open. The current request explicitly prioritises splits. Obtain agreement
that this first private-workspace migration starts now, superseding that earlier
sequencing note. Its internal API replacements/new workspace dependencies also
fall under AGENTS.md's Ask First rules. Data corrections above are independent.

This is not approval to split every other family, publish new packages, rewrite
chip emulation, infer new board identities from ROM names, or change snapshots.

User approved this Sord split on 2026-10-28, including its internal API and
workspace dependencies, starting before release #2170. Registry publishing
remains outside this PR. Baseline fixtures are captured from ee937d475.

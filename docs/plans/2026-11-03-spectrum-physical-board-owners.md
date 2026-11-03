# Spectrum physical PCB owners

Engineering-frontier follow-up to merged #2273. Steve approved separate machine
crates and explicitly requested all documented later boards on 2026-11-03.

## Scope and evidence

Create owners for Issues 1, 2, 3, 3B, 4A, 4B, 5 and 6A. Names describe the PCB,
not RAM population or case: machine-sinclair-zx-spectrum-issue{1,2,3,3b,4a,4b,5,6a}.
RAM fitted on one PCB and a Spectrum+ case remain profiles of that PCB under
RULES.md 34. Do not infer a PCB from a generic 16K/48K/Plus label.

Primary sources: held Sinclair Issue 2/3/3B/4AB/5/6A schematic GIFs in
/Volumes/Data/WOS-Archive/technical-docs/ and Smith chapter 24 printed 249–255
(held photographed PDF). Navigation and confidence limits:
reference/by-system/sinclair-zx-spectrum/zx-spectrum-board-revisions.md and
zx-spectrum-ula-chapter-24-ula-versions.md. Inspect the originals before adding
or changing a hardware fact. Cross-check vendored Fuse before changing timing.
Do not mistake PCB issue numbers for ULA issue numbers, or assume a universal
ULA fitting from a secondary account. AMI SAGA-1A and unverified fittings must
remain explicitly unqualified unless independently established.

## Approved construction and compatibility

Hard-wire each owner's board/ULA/memory-controller configuration; do not expose
a constructor that can put an arbitrary ULA in a named board. Retire generic
16K/48K/Plus physical identities and select explicit PCB/RAM profiles. Keep
current Issue 1/2 CLI IDs where their hardware definitions remain valid.
Steve approved v18 for the affected 48K-class envelopes, rejecting their v17 saves
before decoding: the old payload lacks an independent PCB identity and generic
profiles cannot establish one. Timex stays v18; unaffected 128K/Amstrad profiles
stay v17. Preserve every old specimen and native-output oracle. No legacy reader is being added. The old generic IDs retire in favour of explicit board profiles.

## Tasks

1. Freeze the pre-split 15-profile identity/configuration and native continuation
   evidence from 77440f443. Audit existing Issue 1 spider/cockroach assumptions
   against Smith's page images, later ULA CAS changes and ZX8401 drawings.
   Retain old behaviour as evidence, not automatically as hardware truth.
2. Record a primary-source board/fitting matrix, concrete supported RAM/case
   profiles, digital differences and limits. Resolve mismatches before coding.
3. Move shared host-input adapters from machine-48k into the existing runtime
   input module. Shared hardware stays common-sinclair-zx-spectrum-48k-class;
   no machine owner depends on another machine owner.
4. Add the eight thin owner crates, per-owner tests and immutable construction.
   Model discrete versus ZX8401 wiring at the core's evidence-backed sampling
   granularity; qualify timing differences with independent pin/guest oracles.
   Changes to a published ULA/chip crate require its own verified release and
   the published dependency, never a committed local registry override.
5. Update runtime Model/profiles, closed dispatch, input and snapshot validation;
   update native launch, browser factory and catalogue consumers. Update root
   Cargo.toml/Cargo.lock and docs/status/systems.toml; regenerate status pages.
6. Test actual odd/even-port contention and MIC/EAR reads, board/ULA/decoder
   identity, RAM fittings, atomic foreign-board/legacy rejection, exact native
   continuation and identified original-ROM boot/typed results for each owner.
   Keep unrelated profiles' saved bytes and native output unchanged.
7. Run affected suites, meaningful negative controls, Clippy/fmt, registry,
   dependency/evidence ownership and browser checks. Review and merge tested
   commits through normal CI. Update current docs and #2144 with exact limits.
8. Preserve original fixtures and review heads durably; remove task worktrees,
   caches and owned processes before completion. Preserve unrelated work.

## Task-owned temporary resources

- Source: /private/tmp/198x-spectrum-board-owners
- Build cache: /private/tmp/198x-spectrum-board-target
- Diagnostic evidence/rendered primary sources: /private/tmp/198x-spectrum-board-evidence

## Frozen baseline

The pre-split capture test passed on 77440f443: all 192 original profile/mode/phase
cases produced initial state, two complete native fields/audio/timestamps and
final state. All 577 original files are individually SHA-256 pinned in the
task evidence directory. They remain immutable; later comparisons do not
refresh them. The temporary capture test is diagnostic-only and will be
removed after its source and evidence are durably retained.

## Source corrections to the initial outline

The Issue 5 schematic explicitly labels IC1 6C-001E-6, while the service
supplement's parts table gives 6C001E-7. Retain both documented fittings as
distinct immutable owners if both are exposed; never silently choose one
and claim the whole PCB population. The supplement says no Issue 4 or
Issue 6 was issued: actual names are 4A/4B and 6A. Its Issue 5 description
explicitly identifies Mullard ZX84/01 replacing six 74LS packages with no
change in circuit functions; Issue 6A permits Ferranti or SAGA. The supplied
6A drawing is the Ferranti circuit. SAGA behaviour remains unqualified until
primary device/wiring evidence can establish it.

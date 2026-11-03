# Spectrum physical PCB owners

Engineering-frontier follow-up to merged #2273. Steve approved separate machine
crates and explicitly requested all documented later boards on 2026-11-03.

## Scope and evidence

Create owners for Issues 1, 2, 3, 3B, 4A, 4B, 5 and 6A. Names describe the PCB,
not RAM population or case. Nine crates cover eight PCB issues: Issue 5 has
separate `issue5-6c001e6` and `issue5-6c001e7` fittings.
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
4. Add the nine thin owner crates, per-owner tests and immutable construction.
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

## ZX8401 prerequisite completed

Steve approved the independent dependency/repository on 2026-11-03.
`emu198x/mullard-zx8401` now publishes `emu198x-mullard-zx8401` 0.1.0.
Commit/tag: ad2347a02efb46922efe28d82b45d7d084638a65 /
`emu198x-mullard-zx8401-v0.1.0`. Downloaded registry package SHA-256:
`09baff7955e5bc4da46cf6fd5dde205a9cbd775bb34c34a7bcc811fedc9a5fdb`.
The downloaded source, tests and README match the reviewed checkout.

The dependency-free no_std API takes electrical levels and evaluates settled
DMA0–6, MA0–7 and CASL. Every one of 8,388,608 address/control combinations
matches an independent discrete-gate oracle. Reversed DMA selection, wrong
TS strap and wrong read/write gating mutations each fail through that oracle.
Local fmt, Clippy, tests, packaging and 100% line/region coverage passed;
GitHub Linux CI 41667694301 passed. Required checks protect the new main branch.
Propagation delays, DRAM and display-bus arbitration remain outside its claim.

Primary-source navigation and PDF identities are in umbrella PR #303. The
Sinclair scan lacks referenced timing diagrams; Cygnus's original replacement
schematic supplies inspectable settled gate connections. No source image is
redistributed in the chip package. Canonical chip checkout:
`/Users/stevehill/Projects/198x/Emu198x/mullard-zx8401`.

The common board decoder consumes the published 0.1.0 package. CPU lower RAM
uses the ZX8401 DMA row/column outputs; upper RAM uses MA and CAS. ULA fetch
ownership and the existing native oscillator loop remain unchanged. Healthy
RAM uses logical storage; this does not qualify bad DRAM halves or type straps.

## Retained regression gate

`crates/runtime-sinclair-zx-spectrum/tests/board_owner_baseline.rs` now checks
all 192 captured cases against immutable SHA-256 fingerprints in
`tests/fixtures/physical_boards/pre_split_77440f443.json`. The first run passed
with complete initial/final save equality and first-resumed native outputs.
A deliberately wrong complete-output fingerprint failed the assertion; the
original manifest was restored. This does not qualify analog differences or
assign an old generic profile a PCB identity.

Original capture files, generator and chip evidence were individually verified
inside `.git/recovery/2026-11-03-spectrum-board-prerequisites.tar.gz` in the main
source repository. SHA-256:
`978ce4a0a5e80ef103e30ccdc474f516b97bca6b2bcddf948174dc8b9d37c879`.
The original private capture generator was removed from the worktree after
archive verification; the public test now consumes the frozen fingerprints.
The completed reference worktree and standalone chip build cache were removed.
The source worktree, shared build cache and current evidence directory remain
for tasks 4–7; remove them after the board migration is verified and retained.

## Independent board identity reproduced

Before changing the schema, `physical_board_identity.rs` relabelled exactly the
Issue 1 envelope profile as Issue 2. Restore incorrectly succeeded: its check
compares only the common 5C ULA family, while the serialized phantom marker is
absent. The native wiring then comes from the destination Rust type. The test
failed with `same ULA family does not establish the physical board: ()`.

Use an independently serialized board tag plus fixed construction, and validate
that tag, ULA family and actual ROM/RAM lengths before mutating the runtime.
The approved v18 boundary rejects v17 before parsing its absent identity.

The owner inventory includes both documented Issue 5 ULA fittings as separate
owners (6C001E-6 in the schematic, 6C001E-7 in the service parts list), giving
nine qualified board/fitting owners over eight PCB issues. The other fixed
fittings are Issue 1/5C102E, Issue 2/5C112E, Issue 3 and 3B/6C001E-6,
Issue 4A and 4B/6C001E-7, and Ferranti Issue 6A/6C001E-7. This bounds the
implemented circuits; it does not assert every factory/repair fitting was the
same. Alternative early Issue 2 modifications and the AMI SAGA fitting remain
explicitly unqualified. Physical propagation margins remain outside the current
ULA model, rather than acquiring invented delays during extraction.

## Implemented contract and verification

Nine fixed owners expose eighteen 16/48 KiB profiles. The family now has 28
profiles in total. The old explicit Issue 1/2 48 KiB profile and CLI IDs remain;
`spectrum_16k`, `spectrum_48k` and `spectrum_plus` retire. Native/browser launch
and fresh catalogue qualification use explicit Issue 3 48 KiB by default.
The historical catalogue entry IDs, inputs and all 103 expected hashes remain
unchanged; their variant selectors now identify the actual qualification target.
The ten pre-existing +3 catalogue failures remain tracked in #2265.

V18 independently saves PCB/fitting identity. The rejection matrix exercises
all 306 foreign board/RAM combinations atomically, plus 90 rejections using
five complete original v17 specimens. V17 is rejected before payload decoding.
The seven unaffected non-Timex profiles retain v17; all three Timex profiles
retain v18. Product metadata's 1982 year denotes the original Spectrum launch,
not the introduction date of each later PCB.

The original 192-case baseline now makes 352 comparisons, covering 296 unique
current profile/mode/phase inputs. Only the approved envelope and new board tag
are normalized for old save comparison. Remaining serialized bytes, complete
native video/audio and resumed outputs must agree. Newly exposed 5C/16 KiB
profiles have no old same-population state oracle; they still check full native
output, exact current save continuation, CPU RAM sweeps and original-ROM BASIC.

All fourteen former integration targets and their three PNG goldens are retained
under the runtime. Migrated unit and constructor tests are retained there too.
The continuation matrix covers 296 cases directly. New native CPU tests write
both address-byte patterns through every RAM address on every profile. They
reproduced and caught a lower-RAM ZX8401 integration error (0x4000 reached
0xC000); explicit DMA row/column wiring fixes it without adding any clock tick.

The identified original 16,384-byte Sinclair ROM passed boot and native BASIC
RUN/POKE/PEEK checks on all eighteen profiles, distinguishing absent upper RAM
(255) from fitted RAM (85). Full SHA-256 is verified before execution. Eighteen
native PNGs and per-profile observations are retained privately. The Linux
firmware CI job runs this strict test against its existing identified corpus.
Both browser crates pass wasm32-unknown-unknown checks. Registry/status checks
cover 116 machines and 34 binaries; machine dependency and evidence ownership
checks retain the separate physical owners.

No analog video/power, E6/E7 sub-cycle margin, propagation delay, AMI SAGA or
unverified repair fitting is qualified by these digital tests. The source audit
bounds the supported fittings explicitly rather than aliasing the alternatives.

## Final local checks

The combined affected-package run passed, including all nine owner doctests,
common/core tests, the complete family runtime and all four consuming packages.
All fourteen moved integration targets list nonzero tests. Clippy found one
`vec_init_then_push` warning in the expanded typed-version matrix; constructing
the vector directly resolves it without changing the assertions. The version
matrix is rerun after that edit. Format, workflow lint, fixture/ignore/doc-link
hygiene, help coverage, affected-package planning and registry/status checks pass.

Negative controls prove that an exposed revision-selecting constructor fails
the compile-fail gate, omitting independent board validation fails the original
Issue 1/2 relabel regression, and changing the final ROM byte fails full-input
identity before boot. The coverage scope now names all nine owners; removing
Issue 6A data from a controlled report fails the existing gate. These controls
were restored before positive checks. The original fingerprint manifest and
all 103 catalogue expected values remain unchanged.

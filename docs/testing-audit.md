# Testing Audit

First-pass audit of the current workspace against
[testing-policy.md](testing-policy.md).

This is a structural audit based on the in-repo test surface and
representative code review. It is not a release sign-off and it does not imply
that every suite has been executed in this pass.

For current project priorities, see [roadmap.md](roadmap.md). For the current
crate inventory, see [inventory.md](inventory.md).

---

## Audit Labels

- `Strong`: broad direct coverage appears to exist for most of the policy
  surface
- `Good`: meaningful direct coverage exists, but some categories or
  traceability work are still likely missing
- `Thin`: tests exist, but the current surface is narrow, inherited, or missing
  whole categories
- `Missing`: no meaningful direct test surface exists in the crate today

## Highest-Priority Gaps

1. `commodore-agnus-aga` and `commodore-denise-aga` have AGA-specific logic but
   no direct isolated tests.
2. `emu-core` defines shared contracts and helpers but currently has no direct
   contract tests.
3. `motorola-68010` and `motorola-68020` only prove wrapper selection and
   exposed capability flags, not model-specific behavior.
4. Amiga support-chip work is only partially reflected in isolated tests, so
   machine-level success still carries too much verification weight.
5. Several `Complete` inventory labels still need explicit source traceability
   and category-by-category confirmation to satisfy the new policy.

## Infrastructure And Tooling

- `emu-core`: `Missing`. Shared traits, helper types, and feature-gated MCP and
  video helpers have no direct contract tests. Next: add low-cost tests for
  `SimpleBus`, `MasterClock`, `Ticks`, path parsing, and feature-gated helper
  behavior.
- `m68k-test-gen`: `Missing`. This is a generator crate, but it still lacks
  golden-output or fixture-generation tests. Next: add deterministic generation
  checks if it remains part of the authoritative CPU-fixture pipeline.

## CPU Crates

- `mos-6502`: `Strong`. Dedicated `tests/` suites and fixture data align well
  with the policy. Next: keep non-obvious edge cases mapped back to their
  source fixtures and notes.
- `zilog-z80`: `Strong`. Large `tests/` suites and fixture binaries provide a
  solid verification base. Next: keep undocumented behavior and timing
  references explicit.
- `motorola-68000`: `Strong`. Extensive single-step and differential coverage
  make this the current 68k verification anchor. Next: preserve explicit
  source mapping as more model-specific behavior is added.
- `motorola-68010`: `Thin`. Current tests only prove wrapper construction and
  capability flags. Next: add 68010-specific exception restart, loop mode, and
  control-register tests.
- `motorola-68020`: `Thin`. Current tests only prove wrapper construction and
  capability flags. Next: add 68020-specific exceptions, addressing behavior,
  and control-register tests.

## Amiga Custom Chips

- `commodore-agnus-ocs`: `Strong`. Large direct test surface plus machine-level
  timing checks suggest broad coverage. Next: make key DMA and timing claims
  explicitly traceable to source material.
- `commodore-agnus-ecs`: `Good`. Direct tests exist and much behavior is
  inherited from OCS. Next: isolate ECS-only deltas and test those explicitly.
- `commodore-agnus-aga`: `Missing`. No direct tests exist for FMODE fetch-width
  decoding or AGA-only bitplane-slot logic. Next: add isolated contract and
  timing tests for FMODE width decoding, 7/8-plane lowres fetch assignment, and
  ECS-delegation boundaries.
- `commodore-denise-ocs`: `Strong`. Large direct test surface suggests good
  isolated coverage. Next: keep raster and palette behavior tied to reference
  sources where practical.
- `commodore-denise-ecs`: `Thin`. Current tests mainly prove wrapper behavior
  and state preservation. Next: either keep it explicitly as a behavior-
  identical wrapper or add ECS-only display-mode tests before keeping a
  `Complete` label.
- `commodore-denise-aga`: `Missing`. No direct tests exist for palette banking,
  LOCT handling, HAM8, BPLCON4 XOR, or wide sprite logic. Next: add isolated
  tests for each AGA-only feature.
- `commodore-paula-8364`: `Good`. Direct tests exist and machine-level audio
  tests provide extra confirmation. Next: ensure interrupt, DMA-return, and
  mixer edge cases are partitioned clearly.
- `mos-cia-8520`: `Good`. Meaningful unit coverage exists for the Amiga CIA
  path. Next: audit timer, serial, and handshake completeness against 8520
  references.

## Amiga Support Chips And Peripherals

- `commodore-gayle`: `Good`. Address decode and register behavior are already
  tested. Next: expand around interrupt behavior, drive-present states, and
  PCMCIA edge cases.
- `commodore-dmac-390537`: `Thin`. The stub is appropriately tested as a stub,
  but that is still a narrow boot-unblock contract. Next: keep the stub scope
  explicit until real DMA and SCSI behavior land with deeper tests.
- `drive-amiga-floppy`: `Good`. The direct test surface is sizeable for a
  peripheral crate. Next: verify that write-path, timing, and media-edge cases
  are all covered explicitly.
- `peripheral-amiga-keyboard`: `Thin`. Functional state-machine tests exist,
  but the handshake and timeout behavior is still lightly covered for a timing-
  sensitive peripheral. Next: add timeout, resend, and queueing edge-case
  tests tied to reference behavior.
- `machine-amiga`: `Strong`. Heavy integration and boot-path coverage make it a
  solid machine-level confirmation layer. Next: keep moving behavior into chip
  crates so machine tests confirm wiring rather than substitute for missing
  component tests.

## Shared Chips

- `gi-ay-3-8910`: `Good`. Direct unit tests exist. Next: make tone, envelope,
  mixer, and timing expectations more explicitly source-backed.
- `mos-sid-6581`: `Good`. Direct tests exist, but the policy bar for per-model
  behavior is higher than the current surface. Next: separate digital register
  contract tests from analog-model calibration work.
- `mos-vic-ii`: `Strong`. Mature direct test coverage likely spans the core
  video and timing behavior. Next: keep badline, sprite, and raster-IRQ claims
  tied back to references.
- `mos-cia-6526`: `Good`. Direct tests exist for core timer and port behavior.
  Next: confirm TOD, serial, and interrupt-clear semantics against primary
  references.
- `mos-via-6522`: `Good`. Direct tests exist. Next: verify timer, shift-
  register, and handshake completeness with clearer source mapping.
- `nec-upd765`: `Good`. Direct tests exist for a complex controller. Next:
  expand command-sequencing and error-status edge cases if they are still thin.
- `ricoh-ppu-2c02`: `Good`. Direct tests exist, but the policy wants more
  explicit separation of register-side effects, rendering, and dot-level timing
  guarantees. Next: partition those expectations more clearly.
- `ricoh-apu-2a03`: `Good`. Direct tests exist for a complex audio path. Next:
  make frame-sequencer timing, channel behavior, and mixer expectations more
  explicit.
- `sinclair-ula`: `Strong`. Large direct test coverage and a mature machine path
  indicate a solid verification base. Next: keep contention and timing claims
  explicitly tied to source material.

## Format And Cartridge Crates

- `format-adf`: `Good`. Direct parser tests exist. Next: make corrupt and
  truncated-image coverage explicit and keep round-trip expectations clear
  where writes are supported.
- `format-c64-tap`: `Good`. Direct tests exist. Next: confirm pulse edge cases
  and malformed-stream handling.
- `format-d64`: `Good`. Direct tests exist. Next: ensure directory, BAM, and
  invalid-sector edge cases are covered explicitly.
- `format-gcr`: `Good`. Direct tests exist. Next: add malformed nibble-stream
  and checksum edge cases if missing.
- `format-ipf`: `Good`. Direct tests exist. Next: keep structural validation
  and corruption rejection explicit.
- `format-prg`: `Thin`. The crate is small, but the current test surface is
  still narrow relative to loader semantics. Next: expand malformed-header and
  BASIC-relink edge cases.
- `format-sna`: `Thin`. Basic tests exist, but snapshot validation still needs
  broader corrupt-input and model-compatibility cases. Next: add more negative
  and field-level assertions.
- `format-spectrum-tap`: `Good`. Direct tests exist. Next: add malformed-block
  and edge-duration checks as needed.
- `format-tzx`: `Good`. Direct tests exist for a timing-sensitive format.
  Next: make unsupported-block handling and corrupt-block rejection explicit.
- `format-z80`: `Thin`. The current surface looks light relative to the format
  complexity. Next: expand compression, model-flag, and invalid-combination
  coverage.
- `nes-cartridge`: `Strong`. Large direct test coverage for parsing and mapper
  behavior makes this one of the better non-CPU verification surfaces. Next:
  keep mapper-specific source references explicit.

## Runnable Packages

- `emu-spectrum`: `Strong`. Direct tests and overall system maturity give it a
  good runnable-surface baseline. Next: keep CLI, script, and MCP entry points
  explicitly covered as those features expand.
- `emu-c64`: `Strong`. Large test surface supports both the system and the host-
  facing runner behavior. Next: keep media-loading and batch-mode paths
  explicit.
- `emu-nes`: `Good`. Direct tests exist, but the runnable-surface coverage is
  lighter than the more mature packages. Next: expand CLI, scripting, and
  media-loading edge cases.
- `amiga-runner`: `Thin`. Some direct tests exist, but this is still a large
  host-facing package with many CLI, audio, screenshot, scripting, and MCP
  paths. Next: add focused contract tests for argument parsing, headless
  workflows, and mode parity.

## Inventory Entries Not Yet Represented As Crates

These entries from [inventory.md](inventory.md) are outside this audit because
they are not currently present as workspace crates or are still inline inside
other crates:

- `commodore-fat-gary`
- `commodore-ramsey`
- `commodore-gary`
- `commodore-buster`
- `commodore-super-buster`
- `commodore-akiko`
- `drive-amiga-hd`

## Recommended Remediation Order

1. Add direct isolated tests for `commodore-agnus-aga` and
   `commodore-denise-aga`.
2. Add contract tests for `emu-core`.
3. Expand direct tests for `motorola-68010` and `motorola-68020`.
4. Tighten Amiga support-chip and peripheral coverage, especially Gayle,
   keyboard, and DMAC scope boundaries.
5. Audit `Complete` crates for explicit source traceability and missing
   category coverage rather than relying on marker counts alone.

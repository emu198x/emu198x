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

1. `m68k-test-gen` now has helper, CLI, and serialization coverage, but still
   lacks deterministic end-to-end fixture-generation checks for its core
   output.
2. Amiga support-chip work is only partially reflected in isolated tests, so
   machine-level success still carries too much verification weight.
3. Several `Complete` inventory labels still need explicit source traceability
   and category-by-category confirmation to satisfy the new policy.
4. `commodore-denise-ecs` now has real ECS-specific colour-path coverage, but
   larger display-mode deltas such as SuperHires or productivity-mode output
   still remain outside the direct test surface.

## Infrastructure And Tooling

- `emu-core`: `Good`. Shared traits and core helpers now have direct contract
  tests for `SimpleBus`, `MasterClock`, `Ticks`, address parsing, and
  observable-value formatting. Next: extend that to the feature-gated MCP/video
  helpers and any new shared contracts as they land.
- `m68k-test-gen`: `Thin`. Direct coverage now exists for CLI parsing,
  MessagePack round-trips, output-path mapping, instruction catalogue
  filtering, memory tracking, address alignment, indexed-EA generation rules,
  and deterministic initial-state summaries. Next: add deterministic
  golden-output or full fixture-generation checks if it remains part of the
  authoritative CPU-fixture pipeline.

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
- `motorola-68010`: `Good`. Direct wrapper tests now prove real 68010 behavior
  such as `MOVEC VBR` handling and `CACR` rejection. Next: add exception
  restart, loop-mode, and more control-register coverage.
- `motorola-68020`: `Good`. Direct wrapper tests now prove 68020-only behavior
  such as `MOVEC CACR` and `EXTB.L`. Next: add more exception, addressing, and
  control-register coverage.

## Amiga Custom Chips

- `commodore-agnus-ocs`: `Strong`. Large direct test surface plus machine-level
  timing checks suggest broad coverage. Next: make key DMA and timing claims
  explicitly traceable to source material.
- `commodore-agnus-ecs`: `Good`. Direct tests exist and much behavior is
  inherited from OCS. Next: isolate ECS-only deltas and test those explicitly.
- `commodore-agnus-aga`: `Good`. Direct isolated tests now cover FMODE fetch
  widths, sprite width decoding, lowres 7/8-plane slot assignment, and
  ECS-delegation boundaries. Next: keep timing-sensitive AGA fetch behavior
  explicit as the wrapper grows.
- `commodore-denise-ocs`: `Strong`. Large direct test surface suggests good
  isolated coverage. Next: keep raster and palette behavior tied to reference
  sources where practical.
- `commodore-denise-ecs`: `Good`. Direct tests now cover ECS-owned `BPLCON3`
  state, `DENISEID`, `KILLEHB` half-brite suppression, and the no-op boundary
  cases where `KILLEHB` must not perturb HAM or dual-playfield decode. Next:
  add larger ECS display-mode deltas such as SuperHires, productivity, or
  other ECS-only Denise behavior as those paths land.
- `commodore-denise-aga`: `Good`. Direct isolated tests now cover palette
  banking, LOCT merge behavior, `BPLCON4` XOR lookup, HAM8 channel chaining,
  sprite width decoding, and wide sprite packing. Next: keep the AGA-only delta
  explicit as more modes land.
- `commodore-paula-8364`: `Good`. Direct tests exist and machine-level audio
  tests provide extra confirmation. Next: ensure interrupt, DMA-return, and
  mixer edge cases are partitioned clearly.
- `mos-cia-8520`: `Good`. Meaningful unit coverage exists for the Amiga CIA
  path. Next: audit timer, serial, and handshake completeness against 8520
  references.

## Amiga Support Chips And Peripherals

- `commodore-gayle`: `Good`. Address decode and register behavior are already
  tested. Next: expand around drive-present states and PCMCIA edge cases as the
  implementation broadens.
- `commodore-dmac-390537`: `Good`. The current stub contract is now directly
  tested for reset defaults, masks, port mirroring, byte access, and IRQ
  semantics. Next: keep the stub scope explicit until real DMA and SCSI
  behavior land with deeper tests.
- `drive-amiga-floppy`: `Good`. The direct test surface is sizeable for a
  peripheral crate. Next: verify that write-path, timing, and media-edge cases
  are all covered explicitly.
- `peripheral-amiga-keyboard`: `Good`. Functional state-machine tests now cover
  timeout and resend behavior in addition to the base handshake flow. Next: add
  deeper queueing and edge-timing cases as the peripheral interface grows.
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
- `format-prg`: `Good`. Direct tests now cover non-BASIC metadata preservation,
  wraparound behavior, multi-line BASIC relinking, and end-marker-only BASIC
  loads. Next: add malformed-header coverage if new loader paths appear.
- `format-sna`: `Good`. Direct tests now cover header parsing, 128K border
  masking, bank restore behavior, and invalid duplicated-bank layouts. Next:
  add more corrupt-input and model-boundary cases if new snapshot variants are
  supported.
- `format-spectrum-tap`: `Good`. Direct tests exist. Next: add malformed-block
  and edge-duration checks as needed.
- `format-tzx`: `Good`. Direct tests exist for a timing-sensitive format.
  Next: make unsupported-block handling and corrupt-block rejection explicit.
- `format-z80`: `Good`. Direct tests now cover version detection, base-header
  flags, 48K/128K compatibility handling, AY restore, and truncated v2/v3
  inputs. Next: expand compression and malformed model-flag combinations if new
  variants appear.
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
- `emu-nes`: `Good`. Direct tests now cover CLI validation, MCP helpers,
  media-loading failure paths, minimal valid iNES loading, and headless capture
  mode promotion. Next: if the runner grows further, add more output-error and
  event-loop lifecycle coverage.
- `amiga-runner`: `Good`. Direct tests now cover argument parsing, model-derived
  chipset selection, help/error paths, and headless capture mode promotion.
  Next: expand workflow coverage further if the runner grows new host-facing
  features.

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

1. Add deterministic end-to-end fixture-generation checks for `m68k-test-gen`.
2. Tighten the remaining Amiga support-chip and peripheral coverage as new
   implementation work lands.
3. Continue expanding `commodore-denise-ecs` into larger ECS-only display-mode
   behavior, not just colour-path deltas.
4. Audit `Complete` crates for explicit source traceability and missing
   category coverage rather than relying on marker counts alone.

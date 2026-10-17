# Complete the missing µPD765A command paths

Lane: Spectrum launch-hardening. Scope: the existing sector-image controller,
not raw flux or full timing conformance. Work in this isolated worktree while
other sessions use canonical checkouts. User approved Write Deleted Data and
asked to cover the remaining commands in one campaign. The user explicitly approved all five within the sector-model boundary.

## Design

- Write Deleted Data (09) uses the existing WriteData command state, opcode and
  paced engine. A helper derives DAM/DDAM from the retained command byte. Both
  incomplete CRC state and completed fields preserve the selected mark. Normal
  writes clear it. No field or existing enum tag changes, no dependencies.
- Proposed Scan Equal/Low-or-Equal/High-or-Equal: share one paced, non-mutating
  comparison engine with sector stepping, explicit SH/SN, deleted marks, errors,
  overrun and MT. Append internal command tags only; retain existing state fields.
- Proposed Read Track: read represented payloads in physical sector order, count
  EOT sectors rather than logical IDs, preserve recorded errors and explicitly
  document missing raw gaps/index fidelity. Do not synthesize archival flux.

## Tasks and verification

1. `crates/nec-upd765a/tests/write_deleted.rs`: public-register positive and
   negative tests. Show missing opcode fails before changing code. Reuse write
   engine, verify all read mark/SK combinations, normal overwrite, partial write,
   protection, overrun, EOT/MT and reset. Run controller tests and Clippy.
2. `crates/machine-sinclair-zx-spectrum-plus3/tests/`: exercise native state
   mid-transfer and completed, export/reparse and a new machine's read-back.
   Reuse existing machine export/state APIs. Confirm source files unchanged and
   active export refusal. Do not add snapshot schema fields.
3. Execute an independent bounded MAME public-port comparison against synthetic
   redistributable media, retaining hashes, logs and expected/observed results.
   This is controller evidence, not ROM-loading qualification. Compare twice.
4. Within the agreed representation boundary, implement Read Track and all three
   Scan commands as separate tested commits, with readable primary/reference
   citations and the same independent comparisons. Investigate disagreements
   before changing tests. No generic framework or architectural rewrite.
5. Run affected controller/machine/runtime checks and normal CI. Push reviewable
   owning-repo PRs and merge only after required checks. Cut normal releases when
   ready. Update umbrella evidence/visual checklist without granting whole-FDC
   or family equivalence. Keep timing/drive/format gaps explicit.
6. Preserve source commits and evidence before removing task worktree/build
   caches/private reference firmware and processes. Never touch other worktrees.

Primary: NEC December 1978 µPD765 datasheet pp13–15,17–18, held in umbrella
`reference/by-topic/fdc-765/`. Precedent: vendored Fuse 1.7.0 upd_fdc.c and MAME
upd765.cpp. The three historic home-directory archives are absent; the umbrella
retired trees yielded no reusable FDC file. Reuse the active write engine.

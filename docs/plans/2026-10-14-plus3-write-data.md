# Preserve bounded +3 ROM disk writes

Lane: Spectrum launch-hardening. Scope is the standard-sector Write Data path needed by a ROM SAVE → erase RAM → LOAD → compare diagnostic, plus writable mounting and save-state persistence. Format Track, Write Deleted Data, foreign-format export and full #45 acceptance remain separate.

## Reproduction and sources

At `5d4de5e4d3f8040c2af89bf36c0e1a5f2041a9d2`, a nine-byte Write Data command against an existing 512-byte sector returns MSR `$80` (idle), instead of requesting CPU-to-controller execution bytes (`$B0`). The decoder recognises the command but `execute_command` has no arm. The execution-buffer write arm has no disk mutation.

Primary reference: umbrella `reference/by-topic/fdc-765/upd765-data-sheet-dec78.md`, NEC December 1978 datasheet, Write Data p.13 and command/status tables. Vendored implementation precedent: `emulators/zx-spectrum/fuse-1.7.0/peripherals/disk/upd_fdc.c`, `start_write_data`, `upd_fdc_write_data`, and timeout handling; `machines/specplus3.c` selects a 4 MHz controller and aliases drive selects 2/3 to 0/1. The older archive's controller has the same missing execution arm, so there is no completed implementation to port.

## Approved contract

Use the existing `MediaImage.writable` and script/MCP `load_media.writable` flag. Existing mounts remain protected. Mutate the mounted RAM image only; never write the source path or emit sidecars. Serialize mounted disks, protection and in-flight write state as live controller state. The existing raw image cache must not overwrite this state on restore.

This changes the positional Spectrum save-state payload. The existing policy rejects old versions before decoding; propose version 4 → 5, with an explicit regression test for the rejection. No CPC version change: its current machine has no FDC, despite depending on the crate for a test. Implementing a legacy migration would need a legacy controller/machine decoder and is a separate architectural choice. The user explicitly approved Spectrum version 4 → 5 and rejection of old version-4 states; CPC remains unchanged.

## Implementation and verification

1. `crates/nec-upd765a/src/lib.rs`: retain the red MSR reproduction; add protected-by-default insertion and an additive explicit writable insertion API. Carry per-drive protection. ST3.WP and Write Data rejection must agree. Preserve drive readiness independently.
2. Add bounded Write Data command/execution/result handling. Match selected physical track/head and sector CHRN, preserve unrelated sectors, enforce host-to-controller direction and byte readiness, and make every accepted byte observable in the mounted image. Distinguish missing media/sector, protection, wrong CHRN/size, EOT and overrun. Use the master-loop tick for deadlines; no private catch-up loops. Test single- and multi-sector writes, existing deleted/bad-data metadata replacement, partial writes, wrong-direction accesses, timeout, reset and protection. Do not claim unsupported format/write-deleted/flux behaviour.
3. `crates/runtime-sinclair-zx-spectrum/src/{runtime,variants}.rs`: thread the existing writable mount property into the +3 FDC through additive trait methods. `crates/common-sinclair-zx-spectrum-amstrad-class/src/core.rs`: reset controller transfer state while retaining mounted disk contents/protection.
4. On schema approval, preserve authoritative controller media and transfer state in serde and prevent raw-media reattachment from replacing it. Update Spectrum snapshot envelope version and its documentation. Test completed and mid-transfer write → snapshot → restore → continue against uninterrupted execution; both reads and protection must agree, including a runtime round trip through the existing source-byte cache.
5. Run affected controller/parser, Amstrad-class, +3 and Spectrum-runtime tests, format and Clippy checks. Run the authored diagnostic through ROM SAVE and LOAD with explicit writable mounting, plus protected-media negative control. Compare the original host image hash before/after. Keep independent MAME evidence separate.
6. Commit, push and open a narrow owning-repository PR linked to #45 and the umbrella qualification. Merge only after review. Retain logs and exact source/tool identities, then remove the task worktree/build cache.

## Verification delivered

- The original command-entry test failed on main (`MSR=0x80`); the implemented command enters execution with DIO clear and requests bytes after the pre-data interval.
- The first ROM probe exposed a separate phase error: requesting the first byte immediately after the last command byte caused ST1.OR before the ROM reached its transfer loop. The sector model now includes the minimum ID-to-data interval (38 MFM byte periods from Fuse's gap/sync/mark sequence). A boundary test proves no early RQM or accepted write; the 32 µs byte deadline is unchanged. Rotational ID-search timing remains outside this bounded sector model.
- 196 affected library tests pass. 110 runtime snapshot/variant/boot/native-format regressions pass, with eight existing firmware-dependent tests ignored. Formatting and affected all-target Clippy pass.
- The authored ROM diagnostic reaches generated, RAM-cleared and verified checkpoints. All 1,024 reloaded bytes match the generated bytes after 1,024 zeroes were observed; the completion signature changes from `[0,0]` to `[90,91]`. The trace contains four Write Data commands covering file sectors and the directory update.
- A protected mount reaches only the generated checkpoint, with no completion signature. A loader omitting LOAD reaches generation and clearing but cannot verify or signal completion. Both host source image hashes remain unchanged. Independent MAME evidence belongs to the umbrella campaign.

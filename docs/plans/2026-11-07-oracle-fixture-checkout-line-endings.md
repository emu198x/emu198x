# Keep byte-exact text oracles unchanged across checkouts

Windows main CI 42417133790 fails the S-PPU ares oracle: all 70 cases differ despite matching line diagnostics. The unchanged test passes against Git's LF blobs, then fails against actual Git core.autocrlf=true fixture checkouts. ORACLE_DUMP proves all 70 differences are exclusively inserted CR bytes. For brightness_blank, byte 24 is LF in output and CR in checkout; output/blob is 5758 bytes, checkout 5992. The test's exact comparator is correct; its lines-based diagnostic hides the representation difference.

The sibling audit finds the same exact generated-text contract in YM2610, YM2413, YM2612 and shell ISA-disassembly snapshots. Sony S-DSP compares parsed lines; NES/Atari/Amiga fixtures are parsed records, JSON or binary snapshots. Those remain outside this correction. No chip accuracy, reference output, golden content, comparator, source-line semantic assertion or release version changes are intended.

1. Verify nonempty tracked fixture selections and existing attributes; retain blob identities for each file. Reproduce every affected unchanged target against LF and actual Git CRLF checkouts in serial bounded child processes (jobs=2).
2. Add only these exact fixture rules to `.gitattributes`, each `text eol=lf`:
   - `/crates/emu198x-nintendo-s-ppu/tests/oracle/golden/*.txt` (70)
   - `/crates/emu198x-yamaha-ym2610/tests/oracle/golden/*.txt` (9)
   - `/crates/emu198x-yamaha-ym2413/tests/oracle/golden/*.txt` (28)
   - `/crates/emu198x-yamaha-ym2612/tests/oracle/golden/*.txt` (24)
   - `/crates/emu198x-shell/tests/snapshots/isa-disasm-*.txt` (2)
3. Delete only owned copies, then re-checkout all selected fixtures with core.autocrlf=true. Require all 133 checkout files to match their original Git blobs, effective attributes text+LF, unchanged comparator sources and original fixture blob IDs. Re-run all five existing targets.
4. For each target, alter one reference value in an owned fixture copy and require the unchanged comparator to fail. Restore original bytes immediately; repeat the passing target. This checks that checkout pins preserve the accuracy gate. Never regenerate reference goldens or enable snapshot-update variables.
5. Review exact diff (attributes and this plan only), run whitespace checks, commit and push. Parent reviews exact head before any merge. Require fresh applicable PR checks and final current-base gate, then read the full actual Windows/macOS main workspace outcomes and all-features step status accurately. No native release is part of this task.

Raw logs, per-file original blob/checkout hashes, controls and audit live durably under `Emu198x/.recovery/2026-11-07-s-ppu-windows-oracle/`. Task workspace `/private/tmp/198x-s-ppu-oracle-audit`, target `/private/tmp/198x-s-ppu-oracle-build`, dump `/private/tmp/198x-s-ppu-oracle-dump`; remove only after remote commit/evidence retention and tracked/untracked/ignored/process inspection. Canonical checkout and inherited workspaces stay untouched.

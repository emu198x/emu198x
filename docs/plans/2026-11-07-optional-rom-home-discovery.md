# Preserve optional-ROM test skips without a Unix HOME

Windows main CI 42395921009 passed workspace compilation and linking after the example-name correction, then failed `ks31_executes_beyond_reset_during_the_first_pal_frame` with `HOME is set: NotPresent`. Its optional licensed-ROM loader unconditionally required HOME before checking whether the ROM existed.

A child-process reproduction on main `502f3e442` removed HOME and USERPROFILE while retaining explicit Cargo and Rust toolchain roots. It reproduced 38 failures in 19 test targets: one AGA boot failure, 34 OCS failures across 17 targets, and three Amiga MCP failures. The 25 synthetic tests passed; two existing diagnostics remained ignored.

The approved scope is these 19 test source files plus the existing OCS test-support module. Other HOME assumptions remain inventoried. No runtime API, dependency, fixture, assertion or ignore-policy changes are needed.

1. Add an optional local-fixture path helper to the existing OCS test support. Resolve HOME, then USERPROFILE, and record a missing home through emu198x_test_skip. Update only the 17 reproduced OCS loaders, preserving relative paths and read failures.
2. Update the existing AGA and MCP local loaders in the same way. Preserve explicit per-ROM override precedence and fix MCP's second A500 lookup. Existing unreadable or invalid ROMs must still fail. Strict-fixture mode must still fail on recorded absence.
3. Run the original 19 targets serially with both home variables absent. Assert all 38 former failures record skips, with no failures, while the 25 synthetic tests still pass. Use separate child environments to verify USERPROFILE fallback, HOME precedence, explicit ROM override precedence, strict missing-fixture failure, and unreadable or invalid existing-ROM failures. Run bounded real-ROM smoke targets under the normal local home. Do not mutate process-global environment in Rust tests.
4. Run fmt, affected Clippy and fixture-guard checks with at most two Cargo jobs. Review the final diff, push a small PR and obtain parent review before merge. Require actual postmerge Windows confirmation. No binary release is needed solely for this test fix.
5. Retain red/green and raw CI evidence. Inspect tracked, untracked and ignored task files and verify remote retention before removing the owned workspace and build cache.

Affected test files:

- `crates/common-commodore-amiga-aga/tests/ks31_boot.rs`
- `crates/common-commodore-amiga-ocs/tests/boot_reaches_exec_idle.rs`
- `crates/common-commodore-amiga-ocs/tests/boot_with_adf_inserted.rs`
- `crates/common-commodore-amiga-ocs/tests/cia_phase2_machine.rs`
- `crates/common-commodore-amiga-ocs/tests/m0_reset_vector.rs`
- `crates/common-commodore-amiga-ocs/tests/m10_copper.rs`
- `crates/common-commodore-amiga-ocs/tests/m11_1_bitplanes.rs`
- `crates/common-commodore-amiga-ocs/tests/m11_2_slow_ram.rs`
- `crates/common-commodore-amiga-ocs/tests/m11_denise.rs`
- `crates/common-commodore-amiga-ocs/tests/m1_first_chip_ram_write.rs`
- `crates/common-commodore-amiga-ocs/tests/m2_custom_register_storage.rs`
- `crates/common-commodore-amiga-ocs/tests/m3_ovl_clear.rs`
- `crates/common-commodore-amiga-ocs/tests/m4_chip_ram_aliasing.rs`
- `crates/common-commodore-amiga-ocs/tests/m5_bootstrap_execbase.rs`
- `crates/common-commodore-amiga-ocs/tests/m6_beam_and_vbl.rs`
- `crates/common-commodore-amiga-ocs/tests/m7_chipset_reads.rs`
- `crates/common-commodore-amiga-ocs/tests/m8_cia_timers.rs`
- `crates/common-commodore-amiga-ocs/tests/m9_cia_b.rs`
- `crates/emu198x-amiga/tests/mcp_smoke.rs`

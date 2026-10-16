# Fix Windows Spectrum startup stack overflow (#1792)

Lane: Spectrum launch-hardening. The user requested a fix after the native
Windows audit reproduced a startup-only stack overflow in released binaries.

1. Inspect `crates/emu198x-spectrum/src/script/runner.rs`, the shared launcher
   and `crates/runtime-sinclair-zx-spectrum/src/family_runtime.rs`. Compare
   generated optimised startup frames and runtime type sizes. Reproduce with a
   bounded small-stack child process, retaining its exit and diagnostics.
2. Agree the smallest repair after locating the stack pressure, as RULES.md
   section 31 requires. Record the chosen approach before implementation.
   Preserve GUI main-thread requirements, script/MCP exit semantics and
   machine accuracy. No new dependencies or unrelated cleanup.
3. Implement the agreed repair in its owning file(s), adding a regression
   which fails on the original startup path. Run relevant Rust tests,
   formatting and Clippy, with bounded task-owned build storage.
4. Build the candidate on native Windows using the release distribution
   profile. Verify its actual PE metadata and run the unchanged query_cpu
   reproducer, then the existing positive game and missing-CODE checks from
   PR #1791 with the same samples, firmware and Asm/Build inputs. Retain safe
   reports and explicit failures; never count an unexecuted runtime control.
5. Push the fix and evidence for review. Verify remote retention before
   removing the temporary worktree, build cache, scratch and helper processes.

Workspace: `/private/tmp/emu198x-windows-startup`.
Owned build cache: `target/` in that worktree. Local disk has about 16 GiB
free; build only the headless Spectrum crate locally and use hosted Windows
for the native candidate. Do not touch other worktrees or caches.

## Agreed repair

The user chose heap-allocation of concrete variants, retaining static dispatch
and the existing Windows stack reserve. Change the payloads and constructor
sites of `SpectrumRuntimeKind` in `family_runtime.rs`, its query forwarding,
and the affected fixture constructors in runtime and binary tests. Add a
bounded process regression in `crates/emu198x-spectrum/tests/script_startup.rs`.
Run it automatically in the Windows build job in `.github/workflows/ci.yml`.
Use `.github/workflows/test-linux-packages.yml` for the opt-in before/after
comparison and unchanged full journey, with a small PE/startup probe in
`scripts/check-windows-spectrum-startup.py`.
Keep the runtime's machine state and snapshot formats unchanged.

Before the fix, the local dist build measures the family enum at 74,568 bytes,
the concrete 48K runtime at 840 bytes and Timex at 74,560 bytes. This makes
48K startup carry the largest machine through each family-valued return and
session allocation. Existing variant tests already use 8 MiB worker stacks.
The published Windows binary's unwind records show large startup frames;
macOS passes the query at a reduced 1 MiB limit, so that local result is not
a native Windows regression proof. Native Windows must go red before the
repair and green afterward with its 1 MiB PE reserve unchanged.

## Verified outcome

Native run 38925425661 passes the before/after comparison, script and MCP
regressions, unchanged positive Meteor Storm journey and real missing-CODE
control. Both binaries reserve 1 MiB of stack. Firmware and scratch cleanup
pass. The adjacent evidence folder retains the reports, screenshot, command
output, run metadata and hashes. The repaired family enum measures 16 bytes
locally; all 373 scoped tests pass, with 63 existing fixture tests ignored.

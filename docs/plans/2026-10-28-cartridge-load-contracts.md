# Cartridge loading must reject inaccessible bytes before mutation

Lane: engineering-frontier. Focused bug fixes under #720; mapper breadth in
#223, #229 and #249 remains separate. Reuse ColecoVision's existing two-pass
media validation convention. No dependencies, hardware timing, machine-board
branches, public signatures or serialized layouts change.

1. Verify the current address decoders in machine-sord-m5, machine-sega-sg-1000
   and machine-coleco-colecovision. They expose linear cartridge windows of
   0x5000, 0xC000 and 0x8000 bytes respectively. These are current implementation
   limits, not a claim that larger physical cartridges never existed.
2. Add tests/cartridge_loading.rs in each affected runtime. First reproduce
   acceptance of an inaccessible extra byte and, on M5/SG-1000, mutation after
   a later invalid image. Check whole saved state and time, not only an error.
3. In each runtime's src/runtime.rs preflight every media item before insertion.
   Reject empty M5/SG images and images beyond the current window with explicit
   InvalidMedia. Preserve Coleco signature/truncation validation. Existing
   snapshots and low-level direct construction remain readable/unchanged.
4. Positive controls use original synthetic CPU programs: load the last mapped
   cartridge byte into RAM and write a completion sentinel, then execute through
   MachineCore. Cover every existing runtime profile, exact-size boundaries,
   unknown slots/kinds, multi-image rejection, reset and snapshot continuation.
5. Verify native CLI startup rejects oversized images through the same runtime
   loader. Run affected suites, strict all-target Clippy, formatting and diff
   checks. Keep the failing baseline output. Commit, push, check final CI and
   merge; retain the parent issue's wider scope and mapper issues.
6. Verify remote retention before removing task worktree, caches and processes.

The archive paths listed in RULES.md rule 25 are absent. This change corrects
current media admission and reuses the current loader convention; no archive
code or new hardware facts are introduced. The runtime-shape and live-media
export decisions have been reconsulted; source images remain immutable.

## Adjacent restore/reset regression

The positive CPU guests read the last mapped bytes and complete before the
baseline fails after restore then hard reset: all three restored machines
become absent. `set_machine` installs live state but leaves reset-image caches
empty/stale. First commit fixes that separately, following runtime-msx's current
saved-image rehydration convention. Add read-only machine image accessors and
derive caches from restored images; None clears stale caches. Snapshot fields,
versions, live execution and media admission are unchanged in that commit.

Restore verification: 101 passed, 0 failed, 9 existing fixture-ignored across
all six affected packages. All nine new restore/reset tests pass. CPU guests
verify the final mapped byte, completion, exact save/replay and reset; stale
cartridge and blank-state restoration are separately covered. The old positive
guests fail at reset with no live machine in all three runtimes. Strict
all-target Clippy, formatting and diff checks pass. No snapshot layout changed.

Admission verification: all affected runtime tests pass, strict all-target
Clippy passes, and all three headless apps build without default UI features.
Nine CLI cases verify valid guest output [167,90], oversized-image rejection,
empty-image rejection and input-file SHA-256 immutability. The retained probe
fails on any unexpected exit, guest byte or modified source; each child has a
30-second timeout. Legacy unchecked oversized snapshots still restore with
identical bytes. Existing restore/reset tests pass unchanged after the guards.

These fixes do not complete #720's fleet-wide media conformance work or the
mapper support in #223/#229/#249. They address measured false success and
rejection-state loss in the three current linear cartridge loaders.

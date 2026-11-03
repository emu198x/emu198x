# Runtime/state correctness and remaining board qualification

Engineering-frontier batch approved after the Spectrum PCB split. Work in order:
BBC #2259, Aquarius #2205, staged M5/ColecoVision cartridges #2186, Spectrum +3
catalogue #2265, then the CPC Gate Array/CRTC/manufacturer-link source audit.
Leave Asm198x and unrelated active worktrees untouched.

1. Reproduce the exact BBC teletext/no-font CLI case on current main with a
   bounded parent process. Read `common-acorn-bbc-micro/src/lib.rs`'s native
   monitor/framebuffer loop, `runtime-acorn-bbc-micro/src/runtime.rs` and the
   existing firmware tests. Compare held hardware references and vendored b-em
   before changing timing. Preserve actual monitor boundaries; an exhausted
   safety budget must report NoFrame rather than invent a frame.
2. Find and reuse an existing bounded subprocess test helper if possible. Make
   the BBC firmware/MCP helpers fail with the named case and kill/reap children
   on timeout. Test real success, stderr/stdout handling and a deliberately
   hung child. Extend the same pattern only to directly affected helpers;
   any new shared dependency or API design needs agreement first.
3. Reproduce Aquarius Up+Fire and opposing-direction continuation on both
   controllers/regions and over an already-used runtime. Retain original v4
   specimens. Agree the new version/legacy policy before changing snapshots;
   the electrical byte cannot reconstruct held opposing directions.
4. Reproduce staged cartridge loss before BIOS on M5/ColecoVision in both
   regions. Preserve two distinct cartridges and empty/live states. Agree
   explicit versions/readers, represent staged bytes only when no live machine
   owns them, retain existing admission rules and original legacy specimens.
5. Diagnose #2265 using the retained 103-case recapture, unchanged input/settings
   and expected hashes. Compare the ten +3 failures against independent native
   reference execution, fix established faults, and rerun all 103 cases. Never
   refresh expectations merely to hide a regression.
6. Audit CPC primary schematics/service documentation for Gate Array, CRTC and
   manufacturer-link differences. Record evidence in the umbrella reference
   library first. Deliver a bounded board/fitting inventory and concrete next
   split proposal; do not invent runnable boards from names alone.
7. Verify each self-contained change, commit/push through normal CI and update
   the relevant issues. Preserve review heads, specimens and evidence in
   verified recovery archives. Inspect/remove task worktrees, caches and
   processes before completion. Preserve unrelated sessions.

Task resources:
- /private/tmp/198x-runtime-state-correctness (source worktree)
- /private/tmp/198x-runtime-state-target (build cache)
- /private/tmp/198x-runtime-state-evidence (bounded diagnostics/evidence)

Start: b52deeb8ea7059d861d788dcb88c20985a507aa2. The primary source checkout
has unrelated untracked tools/__pycache__; it remains untouched.

## Aquarius reproduction and proposed compatibility policy

The current-main 32-case matrix reproduces #2205 over both regions, ports,
Up+Fire release paths, opposing directions and clean/dirty restore targets.
Twelve complete original v4 specimens are retained privately with SHA-256
identities before any production change. Opposing directions cancel electrically;
that byte does not contain the held-direction history.

Proposed design: derive serde for the existing private ControllerCache and its
per-port JoystickCache. Add that actual logical state to the runtime envelope,
write v5 and reject v4 before payload decode. Restore validates the entire
snapshot before installing the machine and its matching cache. Preserve v4
specimens as rejection controls; test next-event electrical output and complete
native continuation rather than merely immediate reserialization. This adds no
dependency or public Rust API. The user approved v5 with v4 rejection on 2026-11-03.

## BBC verification

The exact original guest finishes on current main in all four font/mode cases:
0.65–1.44 seconds, one completed frame and 41,238/41,239 actual ticks, with the
missing-font warning only for teletext without its font. The original report's
scratch-build revision is unknown, so no new hardware/timing fault is claimed.
Main already has the monitor/framebuffer boundary change in 227acecc2.

All BBC firmware CLI/MCP children now have a 15-second deadline and an owned
kill/reap guard for error/unwind paths. Reports assert one completed frame and
ReachedTarget for each named font/mode case. An intentional million-frame child
is killed/reaped at a short test deadline; changing its timeout result makes the
regression fail. Restored positive checks pass all six firmware tests and Clippy.
This does not promise cleanup after the test process is forcibly killed by the OS.

## Staged cartridge reproduction and proposed compatibility policy

Both M5 and the published ColecoVision source reproduce #2186 for NTSC/PAL and
two valid cartridges each. The eight original v5 saves are retained with full
SHA-256 identities; each region's two cartridge choices produce identical old
save bytes, proving their data never entered the envelope.

Proposed policy: write v6 with an optional staged cartridge only when the live
machine is absent. Keep explicit v5 readers: live saves retain their own media,
while old empty saves remain empty because missing cartridge bytes cannot be
recovered. Validate contradictory new payloads before mutation; do not reapply
new-load admission checks to original saved media. Qualify empty/staged/dirty
states, two BIOS-executed cartridge guests, source immutability and exact old
running-machine continuation. ColecoVision's fix must publish before the umbrella
updates its existing runtime dependency. Compatibility approval is pending.

## BBC CI doctest selection

The narrowly selected binary-only BBC package builds its test executables, but
CI then fails `cargo test --doc --package emu198x-acorn-bbc-micro` with “no library
targets found”. Select actual library doctest targets from successful Cargo
metadata, reject unknown/missing selection data, and run every selected library's
doctests. A binary-only selection reports that there is no doctest target, while
its ordinary binaries continue through the collector. Add selection/failure
controls and a real tiny-workspace test; never mask rustdoc failures or force
unrelated packages into the selection merely to make Cargo accept it.

## Aquarius verification

Approved v5 now serializes both private controller caches and restores them only
once the complete envelope and board have passed validation. All 32 next-release
cases match uninterrupted electrical input, complete native video/audio and final
state across both boards, both ports and clean/dirty targets. Twelve original v4
specimens plus a version-only control reject before decode without changing either
target. The original board-split v4 fixtures remain unchanged; their hardware
payloads qualify current saves through explicit test-only envelopes, never a
production legacy reader. Full runtime/CLI tests, formatting and all-target Clippy
pass. The local real-firmware test remains explicitly ignored when ROMs are absent.

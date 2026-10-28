# Optional monochrome boot firmware (#323)

Lane: engineering-frontier authenticity. The user approved optional 256-byte
DMG0/DMG/MGB firmware and snapshot version 4, including explicit version-3 reads.
Skipped boot remains default; existing CGB startup stays intact; SGB is #326.

1. Read the original Game Boy programming manual and vendored SameBoy startup/
   FF50 handling. Search frozen archives for precedent, not a port of old CPU
   wiring. Verify current ignored monochrome firmware using a synthetic boot.
2. Before changing serde layout, retain a real version-3 snapshot fixture made
   by the baseline runtime with an original cartridge. Add a legacy machine
   reader/conversion for precisely that old layout; test against the fixture.
3. Add machine-owned monochrome ROM/unmap state, power-on CPU/peripheral setup,
   the $0000-$00FF overlay and irreversible FF50 latch. Only DMG0/DMG/MGB use it.
   Firmware size validation and optional profile sources follow the CGB seam.
4. Version runtime snapshots to 4. Read v3 explicitly via the legacy layout;
   preserve boot image/latch in v4, including mid-boot and after-unmap states.
   Reject wrong profiles, malformed/future data atomically. Do not rely on
   serde(default) to read truncated postcard structs.
5. Test boot/cartridge overlay boundaries, zero/even/odd FF50 writes, CPU
   handoff, reset and save/restore, optional firmware routing, unchanged skipped
   boot, and real firmware logo/checksum gates/exit state where available.
   Run affected tests, existing phase2 gate, strict Clippy and formatting.
6. Publish a checked PR, merge only after complete successful current checks,
   verify #323 closure/remote retention, clean temporary worktree/builds.

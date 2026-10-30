# NES v3 saves retain CPU bus and first-field video output

**Status:** Adopted. Native NES v3 writes and v2 rejection approved before the regional board extraction (#2213, #2144).

Native NES saves write version 3 and reject every other version from the leading version field before decoding the payload. Version 2 omitted the CPU open-bus latch, which cannot be reconstructed exactly. This narrowly supersedes the legacy-reader expectation in `save-state-format.md` for NES v2 and the earlier completeness claim in `nes-architecture-review.md` Seam 3. Portable cartridges and FDS media are unchanged.

`NesSnapshot` retains `open_bus` through capture, reconstruction and in-place restore. A guest saved just before `LDA $4000; STA $0000` previously stored `$00` after reconstruction instead of the uninterrupted `$40`. Direct write-only, unmapped and controller reads exercise the same latch. Vendored Mesen2's `Core/NES/NesMemoryManager.cpp::Serialize` saves `_openBusHandler` too. This fixes state retention, not bus or clock algorithms.

The snapshot also retains the PPU colour/emphasis output mirror. The PPU's own serialization deliberately omits it; waiting a complete field to rebuild it changes composite output when resuming partway through visible lines. The containing machine captures it separately, validates its fixed dimensions and nine-bit code range during decode, and restores it through the additive `Ppu::restore_signal_codes` API. The PPU crate's serialized layout is unchanged. NTSC first-field tests compare the entire emitted packet, including codes, levels and phase, rather than only RGB pixels.

The original six v2 NTSC/PAL/peripheral captures remain rejection fixtures and an execution control. V3 packets must retain their pixels, dimensions, timing and exact audio bits. Corrected complete v3 saves and continuation are frozen before extracting fixed board owners. A serialized mid-instruction guest test verifies both regions, while invalid version prefixes and complete v2 files must fail before altering a live target.

The PPU also skips eight per-bit bus-refresh timestamps. They are mutable deadlines, not defaults: a freshly refreshed `$1f` bus saved after the reset deadline previously read as `$00` after serialized restore. V3 carries those timestamps alongside the mirror and restores them through additive accessors, retaining the PPU’s own serialization. The skip inventory locks both exceptions and their containing-machine restoration paths.

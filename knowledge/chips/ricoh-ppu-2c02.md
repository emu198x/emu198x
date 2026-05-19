# Ricoh 2C02 PPU

NES Picture Processing Unit. Dot-level rendering — one `tick()` call per PPU dot. Runs at 5,369,318 Hz (21,477,272 / 4 master). 341 dots × 262 scanlines per frame (NTSC). Generates a 256×240 ARGB32 framebuffer.

## Crate

`ricoh-ppu-2c02` — **ported from the archive, interface rewritten for pin-level machine layer.** The internal rendering logic lifts almost intact from the archive; the change is in how the PPU interacts with the rest of the machine.

### What changed from the archive

The archive PPU was correct at dot level but was designed to be called from the old CPU-driven architecture. The old machine layer stepped the CPU and then batched PPU dots to catch up — which is the root cause of every NES test ROM failure in the old port.

**Interface changes (not rendering changes):**

| Archive | Port | Why |
|---------|------|-----|
| `tick(chr_read: &mut dyn FnMut(u16) -> u8, mirroring: Mirroring)` | `tick(mapper: &mut dyn Mapper)` | PPU calls through the mapper directly — one place for CHR reads and mirroring queries |
| `cpu_read(reg, chr_read, mirroring)` / `cpu_write(reg, val, chr_write, mirroring)` | `cpu_read(reg, mapper)` / `cpu_write(reg, val, mapper)` | Same mapper consolidation |
| Active-low `/NMI` pin with `poll_nmi()` edge detection | `pub nmi: bool` field, active-high | The mos-6502 crate's `nmi` input is active-high; the CPU handles its own edge detection internally |
| `take_a12_change()` deferred polling | `mapper.notify_a12_rendering()` called from inside `tick()` | A12 transitions reach the mapper at the correct dot, not deferred until the machine polls |
| `set_v()` tracked A12 changes for deferred poll | `set_v()` simplified — A12 edge detection centralised in `check_a12()` | One place for A12 logic |

**What lifts unchanged:** background tile fetch pipeline, shift registers, sprite evaluation (including the hardware overflow bug), pixel composition, scroll increment/copy, palette mirroring, nametable mirroring, greyscale + emphasis, odd-frame skip, VBL timing (flag at dot 1, NMI at dot 3 — 2-dot pipeline delay), $2002 VBL suppression race condition.

### Test coverage

20 tests covering:
- `flip_byte` (sprite horizontal flip)
- Palette address mirroring ($3F10→$3F00 etc.)
- Nametable mirroring (horizontal + vertical modes)
- Sprite overflow hardware bug (both false negative and false positive paths)
- Greyscale palette masking
- RGB emphasis attenuation
- VBL flag set at (241, 1)
- NMI asserted at (241, 3) when enabled
- NMI not asserted when disabled
- $2002 read clears VBL flag
- PPUSCROLL two-write protocol (X + Y scroll latch)
- PPUADDR two-write protocol (v register load)
- PPUDATA write increments v by 1 or 32
- Pre-render line clears status flags at dot 1
- Odd-frame skip (dot 339 → 0 on odd frames with rendering enabled)
- Visible-line fetch timing (dots 337-340)
- `flush_nmi_line()` commits deferred $2000 NMI enable

## Pin contract

Per [nes-clock-topology.md](../decisions/nes-clock-topology.md):

**Output fields:**
- `nmi: bool` — active-high, set at (241, 3) if PPUCTRL bit 7 is set, cleared on $2002 read or at pre-render dot 3. Machine layer routes this to `cpu.nmi`.
- `framebuffer() -> &[u32]` — 256×240 ARGB32, indexed via the 64-entry palette.

**Input methods (register bus):**
- `cpu_read(reg, mapper)` / `cpu_write(reg, val, mapper)` — $2000-$2007.
- `tick(mapper)` — advance one dot. Takes the mapper because pattern table + nametable reads route through it.
- `write_oam(offset, value)` — OAMDMA byte injection from the machine layer.
- `flush_nmi_line()` — commit deferred $2000 NMI enable after all dots in a CPU cycle.

## Related

- [NES clock topology](../decisions/nes-clock-topology.md) — how this crate fits into the master-clock tick loop
- [MOS 6502](mos-6502.md) — the CPU (2A03 variant with BCD disabled)
- [format-nintendo-nes-ines](../decisions/archives-as-source.md) — the Mapper trait this crate calls through
- [MOS 6569 VIC-II](mos-vic-ii.md) — the C64's video chip, same architectural role (video + timing + DMA) in a different system

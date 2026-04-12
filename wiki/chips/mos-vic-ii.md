# MOS 6569 PAL / 6567 NTSC VIC-II

Video Interface Controller — the C64's video chip. Three text modes (standard, ECM, MCM), two bitmap modes (hires, MCM), 8 hardware sprites (single-colour and multi-colour), raster interrupts, badline bus stealing, sprite DMA, smooth scrolling (XSCROLL/YSCROLL), CSEL/RSEL border control, sprite-sprite and sprite-background collision detection, and a light-pen latch.

## Crate

`mos-vic-ii` — **ported.** Third C64 chip after the 6502, CIA, and SID, closing out the Phase 2 "chip waves." 23 unit tests cover raster advance, frame complete, raster IRQ, framebuffer size, register r/w, bank selection, sprite positioning, bitmap base selection, collision registers (clear-on-read vs peek), sprite-sprite and sprite-background collision, invalid BMM+ECM mode rendering black, ECM background selection, badline BA assertion over cycles 12-54, non-badline BA stays idle, sprite BA 3-cycle lead-in, light pen latching (incl. once-per-frame), floating bus on unmapped registers, XSCROLL zero unchanged, XSCROLL=4 pixel shift.

## Architecture

Single-file `lib.rs` (~1700 lines) plus `palette.rs` (VICE PAL colours as ARGB32). Functionally split:

- Raster state machine: `raster_line`, `raster_cycle`, badline evaluation, frame-complete flag.
- Memory access via the `VicMemory` trait (see below).
- Render pipeline (per-tick): evaluate sprite DMA → fetch sprite data → render 8 pixels (modes + XSCROLL carry) → overlay sprites → collision detection → advance beam → raster IRQ check → pin updates.
- Mode rendering: `render_standard_text`, `render_ecm_text`, `render_mcm_text`, `render_hires_bitmap`, `render_mcm_bitmap`, each returning a `CellPixels` of 8 ARGB values plus an 8-bit foreground mask for sprite priority/collision.
- Sprite pipeline: `fetch_sprite_data` (once per line), `overlay_sprites` (three passes — coverage, collision, render in reverse priority order).
- BA evaluation: `badline_ba_low` + `sprite_ba_low`, combined into the `ba_low` pin field.

## Pin contract

The first C64 chip with genuine cross-chip bus visibility.

**Output pins (VIC-II → machine):**

- `irq: bool` — true when `(irq_status & irq_enable & 0x0F) != 0`. Routed to the 6510's `irq` input. Updated after the raster-compare check at the end of each tick.
- `ba_low: bool` — true when BA is asserted low (the VIC-II is stealing the bus for a badline DMA slot or a sprite DMA window, with 3-cycle lead-in). The machine routes this to the 6510's `rdy` input; the CPU stalls on its next read (writes pass through on real hardware, matching NMOS 6502 RDY semantics). Updated **before** the beam advance inside each tick so it reflects the cycle just processed.

**Input method:**

- `set_bank(bank: u8)` — set from CIA2 port A bits 0-1 inverted.
- `trigger_light_pen()` — machine calls when the LP pin falls.

**Register bus:** `read(&mut self, reg)` / `peek(&self, reg)` / `write(&mut self, reg, value)`. Same method-based shape as CIA and SID — only the CPU touches the VIC-II's registers, so there's no cross-chip observation to preserve via pin fields.

**Framebuffer:** `framebuffer() -> &[u32]` returns the ARGB32 buffer. `take_frame_complete()` signals end-of-frame.

### Why VRAM access is a trait, not pin fields

The cpu-bus-interface rule requires pin fields for bus signals that **other chips observe in real time**. The VIC-II's VRAM reads do *not* meet that bar on a stock C64:

- When BA goes low and the VIC takes the bus, the CPU's bus is tri-stated and the VIC has exclusive access. Nothing else is reading the memory bus during those cycles.
- The 6510 observes BA (via the RDY input) and stalls accordingly, but it does **not** observe the VIC's address or data lines.
- The CIAs only respond to their own address ranges, which the VIC never reads from.

So the VIC's VRAM access is a one-party-per-cycle operation with no cross-chip observer. Passing a `&dyn VicMemory` trait to `tick()` is architecturally equivalent to pin fields for this case — same as the Spectrum ULA uses `&dyn MemoryBus` for its screen fetches. The rule is about *observability*, not about uniform method-vs-field style.

```rust
pub trait VicMemory {
    fn read_vram(&self, addr: u16) -> u8;
    fn read_colour(&self, offset: u16) -> u8;
}
```

The machine's memory router implements this — folds the VIC bank in, handles character ROM visibility at `$1000-$1FFF` in banks 0 and 2, and reads colour RAM out of the 1 KiB nibble array at `$D800`.

## Deviations from the archive

- **Closures → trait.** The archive's `tick(&dyn Fn(u16) -> u8, &dyn Fn(u16) -> u8)` becomes `tick(&dyn VicMemory)`. Easier to implement for a concrete machine memory struct than passing two closures; matches the Spectrum ULA precedent.
- **Public `irq` and `ba_low` pin fields.** The archive had `irq_active()` and `ba_low()` getter methods; these remain as convenience wrappers but the canonical state is now the pub field (updated during `tick()`).
- **`irq` updated after register writes too.** A write to `$D019` (acknowledge) or `$D01A` (mask) can change whether any enabled source is pending, so `write()` recomputes the `irq` pin immediately rather than waiting for the next tick.
- **Serde derives + `BigArray`** on `regs: [u8; 64]`, `screen_row: [u8; 40]`, `colour_row: [u8; 40]` via the workspace's `serde-big-array` dependency. The framebuffer is `#[serde(skip)]` with a `default_framebuffer()` helper — save states don't carry raw pixels.

## Known gaps (deliberate)

Documented as follow-up work because each one can be added when a test or a C64 game needs it:

- **Batched screen-row fetch.** The archive reads all 40 screen codes + 40 colour bytes in a tight loop at the start of cycle 15, rather than interleaving c-access, g-access, sprite-access, and refresh cycles across cycles 15-54. The fetched data lands at the right tick boundary, BA is asserted for the correct 42-cycle window, and the CPU is stalled for cycles 15-54 — all the *timing* is right — but any test that inspected VIC memory-bus *ordering* within the window would notice.
- **Timer A/B output on PB6/PB7** isn't a VIC-II concern, it's CIA — listed here only because the cross-chip audio/timer path through the CIA is still stubbed.
- **Sprite DMA memory access** is batched in `fetch_sprite_data` at cycle 0 of each visible line rather than spread across the cycle slots 55-10 (which wraps the line boundary). Same trade-off as the screen row fetch.
- **RC (row counter) edge cases**. The archive increments RC on the line wrap when `den_latch && line in $30..$FB`. A few edge cases around the border display window (DEN toggling mid-display, YSCROLL changes inside a badline) aren't fully modelled.
- **Open-bus reads** on `$2F-$3F` return `last_bus_data` (last VIC fetch), which matches the common case but not every edge case documented in the 6569R3 die analysis.

## Related

- [Archives as source](../decisions/archives-as-source.md) — per-subsystem port-source decisions. VIC-II primary source is the March archive; the backup was consulted but doesn't implement bad lines at all.
- [CPU bus interface](../decisions/cpu-bus-interface.md) — the pin-level contract and its scope (cross-chip observability, not uniform field style).
- [MOS 6526 CIA](mos-cia-6526.md), [MOS 6581 SID](mos-sid-6581.md) — sibling chips with the same "register bus is methods, pins are fields" pattern.
- [MOS 6502](../../crates/mos-6502/) — the CPU whose bus the VIC-II steals. The 6510's `rdy` input consumes this chip's `ba_low` pin.

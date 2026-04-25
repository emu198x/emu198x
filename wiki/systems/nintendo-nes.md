# Nintendo Entertainment System (NES)

> Status as of 2026-04-25: **fresh NTSC NES headless/runtime/native path.**
> The current system boots NROM, MMC1, and UxROM cartridges through the shared
> `MachineCore` boundary, passes the full `nestest` instruction-log
> proof, renders `Super Mario Bros.`, and emits RGBA frames plus mono
> audio through `emu198x-script-nes`. `emu198x-nes` now provides a
> minimal native verifier window for those cartridge mappers with controller
> input, reset, live audio, and host-side APU channel controls. Mapper
> support now covers the first wave-1 bank-switching targets;
> snapshots and DMC DMA cycle stealing are still pending.

## Implementation status

| Component | Crate | Tests | Status |
|-----------|-------|-------|--------|
| 2A03 CPU (6502, BCD disabled) | `mos-6502` | 7 smoke + 2×2.47M Tom Harte | Validated |
| 2C02 PPU | `ricoh-ppu-2c02` | 20 | Ported, interface rewritten |
| iNES parser + NROM/MMC1/UxROM mappers | `format-nintendo-nes-ines` | 31 | Validated |
| APU | `ricoh-apu-2a03` | 24 | Ported and wired into the machine |
| Machine wiring | `machine-nintendo-nes` | 13 + `nestest` | Tick loop + OAMDMA + controller I/O |
| Runtime | `runtime-nintendo-nes` | 7 | Fresh `MachineCore` runtime over the machine crate |
| Native shell | `emu198x-nes` | 3 | Minimal verifier window for mapper-supported cartridges, controller input, reset, live audio, APU channel controls |
| Headless runner | `emu198x-script-nes` | 3 | Cartridge boot, screenshots, audio capture, scripted input |

### What works

- Master-clock-driven tick loop: PPU every dot, CPU every 3rd dot (NTSC).
- NMI routed from PPU → CPU at (241, 3) with 2-dot pipeline delay.
- IRQ routed from mapper/APU → CPU.
- OAMDMA at `$4014` stalls CPU for 514 cycles.
- Controller 1 serial shift register at `$4016`.
- Full NES address space: 2 KiB RAM (mirrored), PPU registers, APU registers, mapper.
- `run_frame()` runs until the pre-render → scanline 0 transition.
- Headless cartridge insertion through `cartridge-1` in the fresh shell path.
- RGBA framebuffer output, mono audio capture/native playback, host-side APU channel toggles/gain, and shared scripted button input.

### Validated

- **nestest.nes** — 8,991 / 8,991 instructions match the golden log (PC, A, X, Y, P, SP at every instruction fetch). `$02` (official opcodes) = `0x00`, `$03` (unofficial opcodes) = `0x00` — all tests pass. This validates the tick loop, address space routing, PPU register bus, and CPU instruction correctness in the context of a real NES machine.
- **Headless runner smoke** — the fresh `emu198x-script-nes` path now runs local `nestest.nes` and `Super Mario Bros.` ROMs and emits PNG screenshots through the shared shell capture pipeline.
- **Mapper unit coverage** — NROM, MMC1, and UxROM parser/banking behaviour is covered in `format-nintendo-nes-ines`.

### What doesn't work yet

- **Mappers beyond NROM/MMC1/UxROM** — CNROM, MMC3, and the longer tail still live in the archive and will be lifted when the real-game matrix needs them.
- **Save states** — the current fresh NES runtime deliberately returns unsupported for snapshot import/export.
- **DMC DMA cycle stealing** — DMC sample bytes are fetched, but the APU does not yet steal CPU cycles for those fetches.

## Architecture

The machine layer (`machine-nintendo-nes`) owns:
- `M6502` (via `new_2a03()`)
- `Apu`
- `Ppu`
- `Box<dyn Mapper>` (constructed by `parse_ines()`)
- `[u8; 2048]` internal RAM
- OAMDMA state machine
- Controller shift registers

The fresh runtime layer (`runtime-nintendo-nes`) owns:
- family/profile metadata for `nintendo-nes-ntsc`
- `MachineCore` translation over cartridge media, frame/audio sinks, and shared input events
- a small query surface (`nes.cartridge.loaded`, `nes.machine.frame_count`, `nes.cpu.pc`, `nes.ppu.scanline`, `nes.ppu.dot`)

The tick loop follows `wiki/decisions/nes-clock-topology.md` exactly:

```rust
fn tick(&mut self) {
    self.ppu.tick(self.mapper.as_mut());
    self.cpu_divider = (self.cpu_divider + 1) % 3;
    if self.cpu_divider == 0 {
        self.cpu.nmi = self.ppu.nmi;
        self.cpu.irq = self.mapper.irq_pending() || self.apu.irq_pending();
        // bus op + cpu.tick() or DMA cycle
        self.apu.tick();
        self.ppu.flush_nmi_line();
    }
}
```

## Related

- [NES clock topology](../decisions/nes-clock-topology.md) — the decision doc this machine implements
- [MOS 6502](../chips/mos-6502.md) — the CPU (2A03 variant)
- [Ricoh 2C02 PPU](../chips/ricoh-ppu-2c02.md) — the PPU
- [Ricoh 2A03 APU](../chips/ricoh-apu-2a03.md) — the APU
- [Commodore 64](commodore-c64.md) — the C64 machine, same architectural pattern

# Nintendo Entertainment System (NES)

> Status as of 2026-04-25: **fresh NTSC NES headless/runtime/native path.**
> The current system boots NROM, MMC1, UxROM, CNROM, MMC3, AxROM,
> MMC5, Color Dreams, VRC2a, Action 53, BxROM/BNROM, NINA-001, Sunsoft-4, and
> Camerica/Codemasters cartridges through the shared
> `MachineCore` boundary, passes the full `nestest` instruction-log
> proof, renders `Super Mario Bros.`, and emits RGBA frames plus mono
> audio through `emu198x-script-nes`. `emu198x-nes` now provides a
> minimal native verifier window for those cartridge mappers with controller
> input, reset, live audio, and host-side APU channel controls.
> The headless runner can also produce a local ROM smoke matrix, and
> runtime snapshots now round-trip the active machine state.

## Implementation status

| Component | Crate | Tests | Status |
|-----------|-------|-------|--------|
| 2A03 CPU (6502, BCD disabled) | `mos-6502` | 7 smoke + 2×2.47M Tom Harte | Validated |
| 2C02 PPU | `ricoh-ppu-2c02` | 21 | Ported, interface rewritten |
| iNES parser + NROM/MMC1/UxROM/CNROM/MMC3/MMC5/AxROM/Color Dreams/VRC2a/Action 53/BxROM/NINA-001/Sunsoft-4/Camerica mappers | `format-nintendo-nes-ines` | 86 | Validated |
| APU | `ricoh-apu-2a03` | 24 | Ported and wired into the machine |
| Machine wiring | `machine-nintendo-nes` | 14 + `nestest` | Tick loop + OAMDMA + DMC DMA cycle stealing + controller I/O |
| Runtime | `runtime-nintendo-nes` | 8 | Fresh `MachineCore` runtime over the machine crate with snapshot import/export |
| Native shell | `emu198x-nes` | 3 | Minimal verifier window for mapper-supported cartridges, controller input, reset, live audio, APU channel controls |
| Headless runner | `emu198x-script-nes` | 4 | Cartridge boot, screenshots, audio capture, scripted input, local smoke matrix |

### What works

- Master-clock-driven tick loop: PPU every dot, CPU every 3rd dot (NTSC).
- NMI routed from PPU → CPU at (241, 3) with 2-dot pipeline delay.
- IRQ routed from mapper/APU → CPU.
- OAMDMA at `$4014` stalls CPU for 514 cycles.
- DMC sample fetch DMA steals a CPU cycle and feeds the DMC reader.
- Controller 1 serial shift register at `$4016`.
- Full NES address space: 2 KiB RAM (mirrored), PPU registers, APU registers, mapper.
- `run_frame()` runs until the pre-render → scanline 0 transition.
- Headless cartridge insertion through `cartridge-1` in the fresh shell path.
- RGBA framebuffer output, mono audio capture/native playback, host-side APU channel toggles/gain, and shared scripted button input.
- Runtime snapshots serialize CPU, PPU, APU, RAM, DMA/controller state, cartridge bytes, and concrete mapper state.

### Validated

- **nestest.nes** — 8,991 / 8,991 instructions match the golden log (PC, A, X, Y, P, SP at every instruction fetch). `$02` (official opcodes) = `0x00`, `$03` (unofficial opcodes) = `0x00` — all tests pass. This validates the tick loop, address space routing, PPU register bus, and CPU instruction correctness in the context of a real NES machine.
- **Headless runner smoke** — the fresh `emu198x-script-nes` path now runs local `nestest.nes`, `Super Mario Bros.`, and mapper-specific ROMs such as `After Burner`, and emits PNG screenshots through the shared shell capture pipeline.
- **Local smoke matrix** — `emu198x-script-nes --smoke-root ... --frames 300 --smoke-report ...` scanned 629 local `.nes` files: 627 valid ROMs ran for 300 frames. Mapper 5 is 8/8, mapper 22 is 2/2, and mapper 28 is 4/4; the only remaining errors are two invalid-header files.
- **Mapper unit coverage** — NROM, MMC1, UxROM, CNROM, MMC3, MMC5, AxROM, Color Dreams, VRC2a, Action 53, BxROM, NINA-001, Sunsoft-4, and Camerica parser/banking behaviour is covered in `format-nintendo-nes-ines`.

### What doesn't work yet

- **Mappers beyond NROM/MMC1/UxROM/CNROM/MMC3/MMC5/AxROM/Color Dreams/VRC2a/Action 53/BxROM/NINA-001/Sunsoft-4/Camerica** — the longer tail still lives in the archive and will be lifted when the real-game matrix needs it.
- **MMC5 accuracy** — MMC5 now covers PRG/CHR banking, ExRAM, nametable fill mode, multiplier registers, pulse/PCM expansion audio, and scanline IRQ detection via the PPU nametable-read pattern. Further hardware-test comparison would still be useful for edge-case timing.
- **Mapper 34 ambiguity** — mapper 34 currently selects BxROM/BNROM for CHR-RAM images and NINA-001 for CHR-ROM images. NES 2.0 submapper handling would be a cleaner long-term discriminator if we add ROMs that need it.
- **Snapshot format stability** — NES snapshots exist now, but they should be treated as version-1 internal snapshots until broader compatibility policy lands for mapper-specific state.

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

The tick loop follows `knowledge/decisions/nes-clock-topology.md` exactly:

```rust
fn tick(&mut self) {
    self.ppu.tick(self.mapper.as_mut());
    self.cpu_divider = (self.cpu_divider + 1) % 3;
    if self.cpu_divider == 0 {
        self.cpu.nmi = self.ppu.nmi;
        self.cpu.irq = self.mapper.irq_pending() || self.apu.irq_pending();
        // bus op + cpu.tick(), OAMDMA cycle, or DMC DMA stolen cycle
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

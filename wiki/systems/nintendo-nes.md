# Nintendo Entertainment System (NES)

## Implementation status

| Component | Crate | Tests | Status |
|-----------|-------|-------|--------|
| 2A03 CPU (6502, BCD disabled) | `mos-6502` | 7 smoke + 2×2.47M Tom Harte | Validated |
| 2C02 PPU | `ricoh-ppu-2c02` | 20 | Ported, interface rewritten |
| iNES parser + NROM mapper | `format-nintendo-nes-ines` | 17 | Validated |
| Machine wiring | `machine-nintendo-nes` | 12 | Tick loop + OAMDMA + controller I/O |
| APU | — | — | Not started |
| Runtime + CLI | — | — | Not started |

### What works

- Master-clock-driven tick loop: PPU every dot, CPU every 3rd dot (NTSC).
- NMI routed from PPU → CPU at (241, 3) with 2-dot pipeline delay.
- IRQ routed from mapper → CPU (ready for MMC3 when ported).
- OAMDMA at `$4014` stalls CPU for 514 cycles.
- Controller 1 serial shift register at `$4016`.
- Full NES address space: 2 KiB RAM (mirrored), PPU registers, APU stubs, mapper.
- `run_frame()` runs until the pre-render → scanline 0 transition.

### Validated

- **nestest.nes** — 8,991 / 8,991 instructions match the golden log (PC, A, X, Y, P, SP at every instruction fetch). `$02` (official opcodes) = `0x00`, `$03` (unofficial opcodes) = `0x00` — all tests pass. This validates the tick loop, address space routing, PPU register bus, and CPU instruction correctness in the context of a real NES machine.

### What doesn't work yet

- **APU** — audio registers are stubbed (reads return 0, writes are no-ops). No sound.
- **Mappers beyond NROM** — only mapper 0 is ported. MMC1, UxROM, CNROM, MMC3, etc. live in the archive and will be lifted when there's a game that needs them.
- **Runtime / System trait** — no headless CLI or screenshot pipeline yet.
- **Save states** — no serde derives.
- **DMC DMA** — the APU's delta modulation channel steals 1-4 CPU cycles for sample fetches. Requires the APU crate.

## Architecture

The machine layer (`machine-nintendo-nes`) owns:
- `M6502` (via `new_2a03()`)
- `Ppu`
- `Box<dyn Mapper>` (constructed by `parse_ines()`)
- `[u8; 2048]` internal RAM
- OAMDMA state machine
- Controller shift registers

The tick loop follows `wiki/decisions/nes-clock-topology.md` exactly:

```rust
fn tick(&mut self) {
    self.ppu.tick(self.mapper.as_mut());
    self.cpu_divider = (self.cpu_divider + 1) % 3;
    if self.cpu_divider == 0 {
        self.cpu.nmi = self.ppu.nmi;
        self.cpu.irq = self.mapper.irq_pending();
        // bus op + cpu.tick() or DMA cycle
        self.ppu.flush_nmi_line();
    }
}
```

## Related

- [NES clock topology](../decisions/nes-clock-topology.md) — the decision doc this machine implements
- [MOS 6502](../chips/mos-6502.md) — the CPU (2A03 variant)
- [Ricoh 2C02 PPU](../chips/ricoh-ppu-2c02.md) — the PPU
- [Commodore 64](commodore-c64.md) — the C64 machine, same architectural pattern

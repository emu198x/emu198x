# Memory Map — All Models Side by Side

Quick-reference showing what lives at each address range across all emulated
Amiga models. Every row answers the question: "on this machine, who responds
to this address?"

## 24-Bit Address Space ($000000–$FFFFFF)

This is the chip bus. All models share it. Differences are in which optional
peripherals are present.

| Range | A500 | A1000 | A2000 | A500+ | A600 | A1200 | A3000 | A4000 |
|-------|------|-------|-------|-------|------|-------|-------|-------|
| $000000–$07FFFF | Chip RAM 512K | Chip RAM 256K | Chip RAM 512K | Chip RAM 1M | Chip RAM 1M | Chip RAM 2M | Chip RAM 1M–2M | Chip RAM 2M |
| $080000–$0FFFFF | Mirror ¹ | Mirror | Mirror | Chip ² | Chip ² | Chip | Chip ² | Chip |
| $100000–$1FFFFF | Mirror ¹ | Mirror | Mirror | Mirror | Mirror | Chip | Mirror / Chip | Chip |
| $200000–$5FFFFF | Unmapped ³ | Unmapped | Zorro II ³ | Unmapped | Unmapped | Unmapped ³ | Zorro II/III | Zorro II/III |
| $600000–$9FFFFF | Unmapped | Unmapped | Unmapped | Unmapped | **PCMCIA common** | **PCMCIA common** | Unmapped | Unmapped |
| $A00000–$A5FFFF | Unmapped | Unmapped | Unmapped | Unmapped | **PCMCIA attr/IO** | **PCMCIA attr/IO** | Unmapped | Unmapped |
| $A60000–$BEFFFF | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped |
| $BFD000–$BFDF00 | CIA-B | CIA-B | CIA-B | CIA-B | CIA-B | CIA-B | CIA-B | CIA-B |
| $BFE001–$BFEF01 | CIA-A | CIA-A | CIA-A | CIA-A | CIA-A | CIA-A | CIA-A | CIA-A |
| $C00000–$C7FFFF | Unmapped ⁴ | Unmapped | Unmapped ⁴ | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped |
| $C80000–$D7FFFF | Unmapped ⁴ | Unmapped | Unmapped ⁴ | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped |
| $D80000–$D9FFFF | Unmapped | Unmapped | Unmapped | Unmapped | **Gayle IDE** | **Gayle IDE** | Unmapped | Unmapped |
| $DA0000–$DABFFF | Unmapped | Unmapped | Unmapped | Unmapped | **Gayle control** | **Gayle control** | Unmapped | Unmapped |
| $DC0000–$DC003F | Unmapped | Unmapped | **RTC** | **RTC** | RTC (via Gayle) | RTC (via Gayle) | **RTC** | **RTC** |
| $DD0000–$DDFFFF | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped | **DMAC** | Unmapped ⁵ |
| $DE0000–$DEFFFF | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped | **Resource regs** | **Resource regs** |
| $DFF000–$DFF1FF | Custom regs | Custom regs | Custom regs | Custom regs | Custom regs | Custom regs | Custom regs | Custom regs |
| $E80000–$EFFFFF | Autoconfig | Autoconfig | Autoconfig | Autoconfig | Autoconfig | Autoconfig | Autoconfig | Autoconfig |
| $F00000–$F7FFFF | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped | Unmapped | Diag ROM ⁶ | Unmapped |
| $F80000–$FFFFFF | ROM 256K | ROM 256K | ROM 256K | ROM 256K | ROM 512K | ROM 512K | ROM 512K | ROM 512K |

**Notes:**

¹ Chip RAM mirrors within $000000–$1FFFFF. 512K chip RAM at $000000 mirrors at
$080000, $100000, $180000. The mirror is a side effect of incomplete address
decoding (chip_ram_mask).

² 1 MB chip RAM fills $000000–$0FFFFF; $100000–$1FFFFF mirrors or is unused.
2 MB fills the entire $000000–$1FFFFF range.

³ Zorro II expansion boards are mapped here by autoconfig. Without expansion
boards, these addresses are unmapped (return 0).

⁴ Slow RAM (A501 trapdoor on A500, or ranger board on A2000) at $C00000–$D7FFFF
when installed. Without it, unmapped.

⁵ The A4000 has no SDMAC. SCSI is handled differently (or not at all — the
A4000 uses IDE via a Gayle-like mechanism on the motherboard).

⁶ A3000 diagnostic ROM at $F00000–$F7FFFF. Decoded by the motherboard. Returns
0 when no diagnostic ROM is installed.

## 32-Bit Address Space (A3000/A4000 only)

Addresses above $00FFFFFF are in the 32-bit domain. Fat Gary's bus gate does
not forward these to the chip bus — they bypass Agnus entirely.

| Range | A3000 | A4000 |
|-------|-------|-------|
| $01000000–$07DFFFFF | Zorro III expansion | Zorro III expansion |
| $07E00000–$07FFFFFF | Fast RAM (2 MB typical) | Fast RAM (configurable) |
| $08000000–$7FFFFFFF | Zorro III expansion | Zorro III expansion |
| $80000000–$FFFFFFFF | Unmapped | Unmapped |

Fast RAM base address depends on RAMSEY configuration and SIMM population. The
2 MB at $07E00000 is the standard A3000 configuration.

## Overlay Mode ($000000)

At reset, CIA-A PRA bit 0 (OVL) is set. This maps Kickstart ROM at $000000
so the CPU can read the reset vectors (SSP at $000000, PC at $000004).

| Address | OVL = 1 (reset) | OVL = 0 (normal) |
|---------|-----------------|-------------------|
| $000000–$07FFFF | ROM (mirrored) | Chip RAM |
| $F80000–$FFFFFF | ROM | ROM |

Kickstart clears OVL early in boot (writes 0 to CIA-A PRA) to expose chip RAM.
This is a one-time operation per boot.

## ROM Sizes

| Version | Size | Address Range |
|---------|------|---------------|
| KS 1.0–1.3 | 256 KB | $FC0000–$FFFFFF (mirrored at $F80000) |
| KS 2.0+ | 512 KB | $F80000–$FFFFFF |

256 KB ROMs mirror within the 512 KB space. Software reads the ROM header to
determine the actual size.

## Emulator Implications

- The memory dispatcher must check addresses in priority order: CIA → Custom →
  DMAC → Resource regs → Gayle → PCMCIA → RTC → Chip RAM → Slow RAM →
  Autoconfig → ROM → Unmapped. This matches Gary's decode chain.
- Chip RAM aliasing is a simple mask: `addr & chip_ram_mask`. The mask depends
  on installed size (512K = $7FFFF, 1M = $FFFFF, 2M = $1FFFFF).
- On A3000/A4000, addresses ≥ $01000000 skip the chip bus entirely — no DMA
  contention, no Agnus arbitration. Fast RAM runs at full CPU speed.
- PCMCIA ranges ($600000–$A5FFFF) only respond when a card is present AND
  pcmcia_present is set in Gary. Without this check, reads to $600000 on A500
  would falsely hit the PCMCIA handler.
- The diagnostic ROM range ($F00000–$F7FFFF) on A3000 is decoded separately
  from Kickstart ROM. It must return 0 (not bus error) when empty.

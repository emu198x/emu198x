# Amiga Implementation Gaps

Comprehensive audit of what's missing, stubbed, or incomplete across the Amiga
emulation — CPU, custom chips, and system wiring. Items ordered by priority
(critical → nice-to-have). Each item has pass/fail verification criteria.

Audited: 2026-03-11, commit 5b77151. Updated: 2026-03-11.

---

## Completed

The following items from the original audit have been implemented:

1. **FPU** (motorola-68000) — full 68881/68882/68040 FPU
2. **Gayle IDE** (commodore-gayle) — LBA, multi-sector, error handling
3. **DMAC SCSI DMA** (machine-amiga + commodore-dmac-390537)
4. **Mouse and Joystick Input** (machine-amiga)
5. **ECS/AGA FMODE Fetch** (commodore-agnus-ecs/aga)
6. **ECS SuperHires Rendering** (commodore-denise-ecs) — 1816-wide framebuffer, quad_color_idx
7. **AGA Sprite Colour Base** (commodore-denise-aga) — BPLCON4 OSPRM/ESPRM XOR
8. **MOVEC Control Registers** (motorola-68000) — all 17 registers, model-gated
9. **MOVE16** (motorola-68000) — 68040+ block move
10. **Battery-Backed Clock** (machine-amiga) — MSM6242B via Gary chip select
11. **DIVSL/DIVUL Overflow Flags** (motorola-68000) — matches WinUAE
12. **Serial Port Receive** (machine-amiga) — RBF interrupt, baud-rate countdown, queue API
13. **68040/060 Instruction Timing** (motorola-68000) — MULU/MULS/DIVU/DIVS/MULL/DIVL per timing class
14. **Data Cache Model** (motorola-68000) — 68030-style 256B direct-mapped, write-through, CACR ED/FD/CD/CED, CINV/CPUSH
15. **MMU Address Translation** (motorola-68000) — 68030 + 68040 page table walks with real bus reads, ATC (22-entry fully associative for 030, dual 64-entry 4-way set-associative for 040), TT register matching, PMOVE/PFLUSH/PTEST execution, write-protect faults → bus error exceptions, State::TableWalk cycle-accurate descriptor reads

---

## Remaining Gaps

### ~~1. MMU Address Translation (motorola-68000)~~ DONE

See completed item 15 above. All A3000 (68030) and A4000 (68040) boot tests pass
with real MMU translation active.

### 2. ~~PCMCIA (commodore-gayle)~~ DONE

**Status:** Implemented. Three card types supported: SRAM, CompactFlash, NE2000.

Gary decodes $600000-$9FFFFF (PcmciaCommon) and $A00000-$A5FFFF (PcmciaAttr)
when `pcmcia_present` is set. Gayle routes attribute memory (CIS tuples + config
registers), I/O space (CF ATA registers, NE2000 DP8390 registers), and common
memory (SRAM direct access). NE2000 includes a full DP8390 register machine with
48KB internal memory, ring buffer packet reception, and queue-based TX/RX for
runner integration. Gayle CS bits (CCDET, WR, BSY, DIS) control card presence
detection, write-protect, IRQ routing, and slot disable.


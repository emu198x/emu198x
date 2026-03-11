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

---

## Remaining Gaps

### 1. MMU Address Translation (motorola-68000)

**Status:** PMOVE/PFLUSH consume extension word as NOP. No address
translation tables, no TC/TT0/TT1/SRP/URP/CRP registers stored.

**What's needed (68030 style):**
- Translation control register (TC) with page size, IS, TIA/TIB/TIC/TID
- Supervisor/User root pointers (SRP, URP, CRP) — 64-bit descriptors
- Transparent translation registers (TT0, TT1)
- Page table walker: table descriptor → page descriptor → physical address
- ATC (Address Translation Cache) — 22-entry fully associative
- PFLUSH/PFLUSHA/PFLUSHR instructions
- PTEST instruction (walk table, report status without translation)
- Bus error on invalid/write-protected pages

**What's needed (68040 style):**
- Simplified 3-level table walk (7-7-6 bit split, 4 KB pages)
- ITT0/ITT1/DTT0/DTT1 (transparent translation via MOVEC)
- URP/SRP (single 32-bit root pointers, not 64-bit descriptors)
- MMUSR for PTEST results

**Verification:**
- AmigaOS 3.x memory protection works
- Enforcer/MuForce detect illegal memory accesses
- VMM (Virtual Memory Manager) can swap pages to disk

### 2. ~~PCMCIA (commodore-gayle)~~ DONE

**Status:** Implemented. Three card types supported: SRAM, CompactFlash, NE2000.

Gary decodes $600000-$9FFFFF (PcmciaCommon) and $A00000-$A5FFFF (PcmciaAttr)
when `pcmcia_present` is set. Gayle routes attribute memory (CIS tuples + config
registers), I/O space (CF ATA registers, NE2000 DP8390 registers), and common
memory (SRAM direct access). NE2000 includes a full DP8390 register machine with
48KB internal memory, ring buffer packet reception, and queue-based TX/RX for
runner integration. Gayle CS bits (CCDET, WR, BSY, DIS) control card presence
detection, write-protect, IRQ routing, and slot disable.


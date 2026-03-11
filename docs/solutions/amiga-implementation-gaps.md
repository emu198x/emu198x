# Amiga Implementation Gaps

Comprehensive audit of what's missing, stubbed, or incomplete across the Amiga
emulation — CPU, custom chips, and system wiring. Items ordered by priority
(critical → nice-to-have). Each item has pass/fail verification criteria.

Audited: 2026-03-11, commit 5b77151. Updated: 2026-03-11.

## Critical — Required for Real Software

### 1. FPU (motorola-68000) — DONE

Full 68881/68882/68040 FPU implementation. FP register file (FP0–FP7, FPCR,
FPSR, FPIAR), all cpGEN arithmetic (FADD through FSCALE), transcendentals
(FSIN, FCOS, etc.), FMOVE/FMOVEM/FMOVECR, FBcc/FScc/FDBcc/FTRAPcc,
FSAVE/FRESTORE, data type conversions (Byte/Word/Long/Single/Double/Extended/
Packed BCD). 68040 unimplemented opcodes trap to vector 11 for FPSP.
11 integration tests in cpu.rs.

### 2. Gayle IDE Completion (commodore-gayle) — DONE

LBA addressing, multi-sector READ/WRITE, READ MULTIPLE ($C4), WRITE MULTIPLE
($C5), SET MULTIPLE MODE ($C6), INITIALIZE DEVICE PARAMETERS ($91). Proper
error register handling.

### 3. DMAC SCSI DMA Service (machine-amiga + commodore-dmac-390537) — DONE

`service_dmac_dma()` in machine-amiga tick loop. Transfers bytes between DMAC
buffer and Amiga memory via ACR/WTC. Both read and write directions. Completion
signalled via DMAC interrupt.

### 4. Mouse and Joystick Input (machine-amiga) — DONE

`push_mouse_delta(dx, dy)` with quadrature counters for JOY0DAT, `set_joystick`
for JOY1DAT direction encoding. JOYTEST write resets counters. Wired through
to runner integration.

## Important — Needed for Correct Display

### 5. ECS/AGA FMODE Fetch Integration (commodore-agnus-ecs/aga) — DONE

`bpl_fetch_width()` and `spr_fetch_width()` decode FMODE bits and feed into
bitplane and sprite DMA slot allocation. 2x/4x fetch widths produce wider
word transfers per slot.

### 6. ECS SuperHires Rendering (commodore-denise-ecs) — DONE

BPLCON0 SHRES bit (0x0040) drives 4× lores pixel rate through the existing
shift register pipeline. Raster framebuffer widened to superhires resolution
(1816 = 227 CCKs × 8 pixels). Each output call produces 4 independently
composed colour indices via `quad_color_idx`. Machine-amiga render loop writes
8 sub-pixels per CCK. Standard viewport extracts at 1280 pixels wide (hires
content pixel-doubled). 1 integration test in denise-ecs.

### 7. Denise AGA Sprite Colour Base (commodore-denise-aga) — DONE

BPLCON4 OSPRM/ESPRM XOR applied in `sprite_pixel()` before palette lookup.

## Nice-to-Have — Correctness and Completeness

### 8. MMU Address Translation (motorola-68000)

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

### 9. Missing MOVEC Control Registers (motorola-68000) — DONE

All MOVEC registers implemented: SFC ($000), DFC ($001), CACR ($002),
TC ($003), ITT0/TT0 ($004), ITT1/TT1 ($005), DTT0 ($006), DTT1 ($007),
BUSCR ($008), USP ($800), VBR ($801), CAAR ($802), MSP ($803), ISP ($804),
MMUSR ($805), URP ($806), SRP ($807), PCR ($808). Model-gated by capability
(MMU registers need mmu=true, CACR needs cacr=true, etc.).

### 10. 68040/060 Instruction Timing (motorola-68000)

**Status:** Uses 68020 timings for all 68040/060 instructions.

**What's needed:**
- 68040: 1-clock effective bus cycle, deeper pipeline model
- 68060: Superscalar dual-issue, branch prediction model
- Separate timing tables per timing class

**Impact:** Low for correctness. Affects performance accuracy only.

### 11. MOVE16 Instruction (motorola-68000) — DONE

68040+ block move of 16 bytes (aligned). Both forms: (Ax)+,(Ay)+ and
(Ax)+,$xxxxxxxx. Two-stage microcode with 8-word read/write loops.

### 12. Serial Port (machine-amiga) — PARTIAL

SERPER stored, SERDAT triggers transmit with baud-rate countdown. TBE/TSRE
status in SERDATR, TBE interrupt fires via Paula when transmit completes.
RBF (receive) not implemented. No external serial I/O hook yet.

### 13. Battery-Backed Clock (machine-amiga) — DONE

MSM6242B at $DC0000-$DC003F via Gary chip select. 16 nybble-wide BCD registers
(time digits, day-of-week, control D/E/F). Fixed time 1993-01-01 12:00:00.
HOLD latch via control E bit 0. A500/A1000 have rtc_present=false (no RTC).

### 14. PCMCIA (commodore-gayle)

**Status:** Not implemented. Gayle PCMCIA address space not wired.

**What's needed:**
- PCMCIA attribute memory at $A00000
- PCMCIA I/O space at $A20000
- PCMCIA common memory at $600000
- Card detect, configuration registers
- SRAM card support (most common use case)

**Impact:** Very low. Few users relied on PCMCIA storage.

### 15. DIVSL/DIVUL Overflow Flags (motorola-68000) — DONE

Overflow flag logic now matches WinUAE's `divsl_overflow` and `divul_overflow`.
68020/030: N/Z flags derived from dividend (sign-dependent for DIVSL, low-32
for DIVUL). 68040+: just V set, C cleared. Musashi test runner still masks
these because Musashi's reference logic differs from WinUAE.

### 16. Data Cache Model (motorola-68000)

**Status:** Capability flag present, no implementation. Harmless for
correctness since DMA writes are immediately visible.

**What's needed (for accuracy):**
- 68030: 256-byte direct-mapped data cache (16 lines × 4 longwords)
- 68040: 4 KB 4-way set-associative data cache
- Write-through (030) or copyback (040) modes
- Cache coherency with DMA (CDIS pin, CACR DCI bit)

**Impact:** Very low. Only matters for cache-dependent timing or code
that explicitly tests cache behaviour.

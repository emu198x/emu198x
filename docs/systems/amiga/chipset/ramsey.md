# RAMSEY — DRAM Controller

RAMSEY is the DRAM controller on A3000 and A4000 systems. It manages the fast
RAM subsystem (up to 16 MB on the motherboard) and provides configuration and
revision registers through the motherboard resource register space at $DE0000.

## What RAMSEY Does

Three functions:

1. **DRAM control** — configures page mode, static column, refresh rate, burst
   mode, and DRAM type (256K×4 vs 1M×4). This is the hardware that drives the
   RAS/CAS/WE lines on the DRAM chips.

2. **Configuration register** — 8-bit register that software reads and writes
   to control DRAM timing and test features.

3. **Revision register** — read-only 8-bit register that identifies the RAMSEY
   variant. KS reads this during boot to adapt memory configuration.

## Register Access

RAMSEY's registers are accessed through the shared $DE0000 resource register
space (see [fat-gary.md](fat-gary.md) for the overall scheme). The sub-address
decode uses two address bits:

| addr64 | addr2 | Register | Access |
|--------|-------|----------|--------|
| 0 | 0 | (Fat Gary TIMEOUT) | — |
| 0 | 1 | (Fat Gary TOENB) | — |
| 1 | 0 | RAMSEY Config | R/W |
| 1 | 1 | RAMSEY Revision | Read |

### Config Register (addr64=1, addr2=0)

| Bit | Name | Meaning |
|-----|------|---------|
| 0 | PAGE | Page mode enable |
| 1 | BURST | Burst mode enable |
| 2 | WRAP | Address wrap enable |
| 3 | DRAM_TYPE | 0 = 256K×4 (1 MB SIMMs), 1 = 1M×4 (4 MB SIMMs) |
| 4 | REFRESH | Refresh rate (0 = normal, 1 = high) |
| 5 | STATIC_COL | Static column mode |
| 6 | TEST | Test mode (writes are visible on next read — diagnostic) |
| 7 | TIMEOUT | Timeout flag (mirrors Fat Gary TIMEOUT for convenience) |

Not all bits are meaningful on all RAMSEY revisions. Software typically reads
the config register, masks the bits it understands, and writes back a modified
value during boot.

### Revision Register (addr64=1, addr2=1)

Returns the RAMSEY chip revision:

| Value | Revision | Found in |
|-------|----------|----------|
| $0D | Rev 04 | A3000 rev 6.x motherboards |
| $0F | Rev 07 | A3000 rev 9.x, A4000 |

KS reads this to determine which DRAM configuration features are available.
Rev 07 supports wider burst transfers and additional timing options.

## DRAM Addressing

RAMSEY manages the motherboard fast RAM, which sits in the 32-bit address space
above $01000000. On the A3000, the standard configuration is 2 MB fast RAM at
$07E00000 (just below the 128 MB boundary). The A4000 can have up to 16 MB of
fast RAM at a configurable base address.

This memory is **not** on the chip bus — it bypasses Agnus entirely and has no
DMA contention. CPU accesses to fast RAM run at full 68030/040 speed with no
wait states from chip-bus arbitration.

## Boot Sequence Interaction

During boot, KS probes RAMSEY as follows:

1. Read revision register to identify the RAMSEY variant
2. Read config register to get current DRAM settings
3. Write config register to set page mode, burst mode, and DRAM type based on
   the installed SIMMs
4. Size the fast RAM by writing test patterns and reading them back

If the RAMSEY revision read fails (returns $FF), KS concludes this is not an
A3000/A4000 and skips DRAM configuration.

## Emulator Implications

- RAMSEY is two bytes of state — one config register, one revision constant.
  No timing behaviour to model.
- The revision value must match what KS expects for the emulated model. Using
  the wrong revision causes KS to skip fast RAM or configure it incorrectly.
- Fast RAM must be in the 32-bit address space (above $01000000). It is not on
  the chip bus and must not participate in DMA slot contention.
- The $DE0000 sub-address decode must correctly route to RAMSEY (addr64=1) vs
  Fat Gary (addr64=0). Getting this wrong causes KS to read Fat Gary's TOENB
  register instead of RAMSEY's config register, or vice versa.

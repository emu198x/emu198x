# Gary — Address Decoder

Gary is the Amiga's central address decoder. It takes a 24-bit address from the
CPU bus and asserts the correct chip-select line so the right peripheral or memory
responds. Every Amiga has Gary logic — on A500/A1000/A2000 it is a discrete PAL
or gate array; on later models the same decode moves into Gayle (A600/A1200) or
Fat Gary (A3000/A4000), but the fundamental address map is unchanged.

## What Gary Does

One job: given an address, decide who owns it. This is combinational logic — no
clocking, no state, no registers. Gary fires a chip-select output and the
selected device responds.

## Address Map

The 24-bit address space ($000000–$FFFFFF) is carved up as follows. Higher
entries in the table take priority when ranges overlap.

| Range | Chip Select | Notes |
|-------|-------------|-------|
| $BFE001, $BFE101, ... $BFEF01 | CIA-A | Odd bytes only (accent on D0–D7) |
| $BFD000, $BFD100, ... $BFDF00 | CIA-B | Even bytes only (accent on D8–D15) |
| $DFF000–$DFF1FF | Custom registers | Agnus, Denise, Paula |
| $DD0000–$DDFFFF | DMAC | A3000 only (SCSI controller) |
| $DE0000–$DEFFFF | Resource registers | A3000/A4000 (Fat Gary + RAMSEY) |
| $D80000–$DFFFFF | Gayle | A600/A1200 (IDE + PCMCIA control) |
| $600000–$9FFFFF | PCMCIA common | A600/A1200, when card present |
| $A00000–$A5FFFF | PCMCIA attribute/IO | A600/A1200, when card present |
| $DC0000–$DC003F | RTC | A2000/A3000/A4000/A500+ |
| $000000–$1FFFFF | Chip RAM | Up to 2 MB, DMA-accessible |
| $C00000–$D7FFFF | Slow RAM | Ranger/trapdoor (A500/A2000) |
| $E80000–$EFFFFF | Autoconfig | Zorro II/III expansion |
| $F80000–$FFFFFF | Kickstart ROM | 256 KB or 512 KB |
| Everything else | Unmapped | Returns 0 (floating bus) |

### Decode Priority

When multiple ranges could match the same address, Gary resolves using a fixed
priority chain. The order above reflects this — CIA decode wins over the
general $BF0000 range, custom registers win over the $D00000 region, and so on.

### CIA Address Decoding

The CIAs use a peculiar scheme inherited from the C64 era. CIA-A and CIA-B are
not at contiguous addresses — they sit at interleaved positions within $BF0000:

- **CIA-A** responds to odd-byte addresses at $BFE001, with register offsets
  spaced $100 apart ($BFE001, $BFE101, $BFE201, ... $BFEF01). The chip-select
  is triggered by address lines matching the pattern `$BFExxx` with A0=1.

- **CIA-B** responds to even-byte addresses at $BFD000, with register offsets
  at $BFD000, $BFD100, $BFD200, ... $BFDF00. The chip-select fires when the
  address matches `$BFDxxx` with A0=0.

This means reading CIA-A at $BFE001 and CIA-B at $BFD000 are both valid in the
same 16-bit bus cycle — they occupy different data lines (D0–D7 vs D8–D15).
However, most software accesses them separately.

## Model Configurations

Gary's decode output varies by model because different machines have different
peripherals wired to the bus.

| Model | Slow RAM | Gayle | PCMCIA | DMAC | Resource Regs | RTC |
|-------|----------|-------|--------|------|---------------|-----|
| A500 | Optional (A501) | No | No | No | No | No |
| A1000 | No | No | No | No | No | No |
| A2000 | Optional | No | No | No | No | Yes |
| A500+ | Optional | No | No | No | No | Yes |
| A600 | No | Yes | Yes | No | No | Yes (via Gayle) |
| A1200 | No | Yes | Yes | No | No | Yes (via Gayle) |
| A3000 | No | No | No | Yes | Yes | Yes |
| A4000 | No | No | No | No | Yes | Yes |

When a peripheral is not present, its address range falls through to the
Unmapped chip-select, which returns 0 on reads and sinks writes.

## Emulator Implications

- Gary is pure combinational logic — implement `decode(addr) -> ChipSelect` as
  a match chain, no state needed.
- The decode must run for every chip-bus access (CPU reads/writes, DMA), so keep
  it fast. No allocations, no branches that touch memory.
- CIA decoding must check the byte lane (A0), not just the address range. Getting
  this wrong causes CIA-A and CIA-B to overlap.
- Model configuration flags control which chip-selects are active. These are set
  once at machine construction and don't change at runtime.
- When no chip-select matches, reads return 0 (matching WinUAE's
  `NONEXISTINGDATA=0` convention). Some emulators return open-bus ($FFFF) for
  16-bit reads — the Amiga convention is 0.

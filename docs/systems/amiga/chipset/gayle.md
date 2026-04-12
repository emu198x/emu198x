# Gayle — IDE Interface, PCMCIA Slot, Address Decode

Gayle is the gate array on A600 and A1200 systems. It provides three functions:
an IDE (ATA) hard disk interface, a PCMCIA card slot, and four control/status
registers that manage interrupts for both subsystems.

## Address Space

Gayle sits between the CPU and the $D80000–$DFFFFF address range. Gary routes
the chip-select to Gayle when `gayle_present` is set. Within that range, Gayle
subdivides addresses into IDE task-file registers and its own control registers.

### IDE Registers ($DA0000–$DA3FFF)

Standard ATA task-file mapped into the Amiga address space. Each register is
word-wide (16-bit), with the task-file register index encoded in address bits
A5–A2:

| Address | Register | Access |
|---------|----------|--------|
| $DA0000 | DATA | R/W (16-bit) |
| $DA0004 | ERROR / FEATURES | R / W |
| $DA0008 | SECTOR COUNT | R/W |
| $DA000C | SECTOR NUMBER | R/W |
| $DA0010 | CYLINDER LOW | R/W |
| $DA0014 | CYLINDER HIGH | R/W |
| $DA0018 | DEV/HEAD | R/W |
| $DA001C | STATUS / COMMAND | R / W |
| $DA2018 | ALT STATUS / DEV CTRL | R / W |

When no drive is attached, STATUS reads $7F ("no drive") and other task-file
registers read $FF — matching WinUAE behaviour.

### Gayle Control Registers ($DA8000–$DABFFF)

Four registers, each mapped to a $1000-aligned block:

| Address | Register | Access | Purpose |
|---------|----------|--------|---------|
| $DA8000 | CS (Card Status) | R/W | Card detect, write protect, IDE IRQ |
| $DA9000 | IRQ (Interrupt Request) | R/W | Interrupt pending flags |
| $DAA000 | INT (Interrupt Enable) | R/W | Interrupt enable mask |
| $DAB000 | CFG (Configuration) | R/W | PCMCIA timing configuration |

### Card Status Register (CS)

| Bit | Name | Meaning |
|-----|------|---------|
| 7 | IDE | IDE interrupt pending |
| 6 | CCDET | Card detect — PCMCIA card is inserted |
| 5 | BVD1 | Battery voltage detect 1 |
| 4 | BVD2 | Battery voltage detect 2 |
| 3 | WR | Write protect — card is writable when set |
| 2 | BSY | PCMCIA card interrupt (active low from card) |
| 1 | DAEN | Data acknowledge enable |
| 0 | DIS | PCMCIA slot disabled |

### Interrupt Request Register (IRQ)

Same bit layout as CS. Bits 2–7 are **write-to-clear** — writing 0 to a set bit
clears it. Bits 0–1 (RESET/BERR) are written directly. This asymmetric
write behaviour is a common emulator mistake — treating it as a normal register
causes interrupts to never clear.

### Interrupt Enable Register (INT)

Same bit layout. Each bit enables the corresponding interrupt source. An
interrupt reaches the CPU only when both the IRQ bit and the INT bit are set.

### Configuration Register (CFG)

Only the low 4 bits are significant. Controls PCMCIA access timing (speed
select for attribute memory reads).

## IDE Interface

Gayle implements standard ATA protocol for a single drive (master). The
supported command set:

| Command | Code | Function |
|---------|------|----------|
| IDENTIFY DEVICE | $EC | Returns 512 bytes of drive parameters |
| READ SECTORS | $20 | PIO read, 1 sector per IRQ |
| WRITE SECTORS | $30 | PIO write, 1 sector per IRQ |
| READ MULTIPLE | $C4 | PIO read, N sectors per IRQ |
| WRITE MULTIPLE | $C5 | PIO write, N sectors per IRQ |
| SET MULTIPLE MODE | $C6 | Set N for READ/WRITE MULTIPLE |
| INIT DEVICE PARAMS | $91 | Set CHS geometry |
| READ VERIFY | $40 | Verify without data transfer |
| SEEK | $70 | Move head (no-op in emulation) |
| SET FEATURES | $EF | Accept but no-op |
| EXECUTE DIAGNOSTIC | $90 | Returns no-error code |
| DEVICE RESET | $08 | Reset task-file to defaults |

### Data Transfer Pipeline

Read and write commands use a sector-at-a-time state machine:

1. **Command** → software writes the COMMAND register
2. **Data phase** → for reads, Gayle loads sectors into an internal buffer and
   asserts DRQ. Software reads 256 words (512 bytes) from the DATA register.
   For writes, software writes 256 words, then Gayle commits to the disk image.
3. **IRQ** → after each block (1 sector for single commands, N sectors for
   multiple commands), Gayle asserts an IDE interrupt.
4. **Repeat** → if more sectors remain, the next block loads automatically.

### CHS and LBA Addressing

Both addressing modes are supported. LBA mode (DEV/HEAD bit 6 set) packs a
28-bit LBA across the sector number, cylinder, and head registers. CHS mode
converts using the logical geometry set by INIT DEVICE PARAMS.

### NIEN (No Interrupt Enable)

DEV CTRL bit 1 (NIEN) suppresses IDE interrupt assertion. When set, all
commands complete silently — software must poll STATUS instead of waiting for an
interrupt. KS uses NIEN during drive identification.

## PCMCIA Slot

Gayle manages a Type II PCMCIA slot with three memory spaces:

| Address Range | Space | Purpose |
|---------------|-------|---------|
| $600000–$9FFFFF | Common memory | Direct byte-addressable storage |
| $A00000–$A3FFFF | Attribute memory | Card Information Structure (CIS) |
| $A40000–$A5FFFF | I/O space | Device registers (CompactFlash, NE2000) |

Gary routes these ranges to Gayle when `pcmcia_present` is set.

### Supported Card Types

| Card | Common Memory | Attribute | I/O | Notes |
|------|--------------|-----------|-----|-------|
| SRAM | Direct R/W | CIS tuples | — | Byte-addressable, simple |
| CompactFlash | — | CIS + config | ATA task-file | IDE via PCMCIA I/O |
| NE2000 | — | CIS + config | DP8390 regs | Ethernet via PCMCIA I/O |

### CIS (Card Information Structure)

Every PCMCIA card provides CIS tuples in attribute memory. The OS reads these
during card insertion to identify the card type, manufacturer, and capabilities.
Key tuples:

- **CISTPL_DEVICE** ($01): Device type and speed
- **CISTPL_VERS_1** ($15): Product name strings
- **CISTPL_FUNCID** ($21): Function class (memory=1, fixed disk=4, network=6)
- **CISTPL_CONFIG** ($1A): Config register base address
- **CISTPL_CFTABLE_ENTRY** ($1B): I/O ranges and IRQ configuration
- **CISTPL_END** ($FF): End marker

### Card Configuration

Before a CompactFlash or NE2000 card can use I/O space, the OS must write a
configuration index to the card's config register (location given by
CISTPL_CONFIG). This switches the card from memory mode to I/O mode. The
`configured` field in the emulator tracks this state: -1 = unconfigured,
>=0 = configured with the given index.

### PCMCIA Interrupts

PCMCIA card interrupts feed through Gayle's CS register (bit 2, BSY). When
the card asserts its IRQ line and the corresponding INT bit is enabled, Gayle
asserts the interrupt to the CPU. For NE2000, the DP8390's ISR/IMR logic
determines whether the NIC has a pending interrupt; for CompactFlash, the IDE
drive's IRQ state propagates.

## Emulator Implications

- Gayle has significant state — IDE task-file, transfer buffers, PCMCIA card
  state, four control registers. All must be saved in save states.
- The IRQ register's write-to-clear semantics for bits 2–7 are critical. Writing
  $FF to clear all interrupts must clear bits 2–7 but write 1 to bits 0–1.
- Without an attached drive, IDE STATUS must read $7F. KS uses this to detect
  "no drive" and skip IDE initialisation.
- PCMCIA CIS tuples must be accurate — card.resource parses them to decide
  how to configure the card. Wrong tuples cause the card to not be recognised.
- CompactFlash cards reuse the IDE state machine (IdeDrive) through PCMCIA I/O
  space. The same ATA protocol applies, just routed differently.
- NE2000 is a full DP8390 state machine (48 KB internal memory, ring buffer,
  remote DMA). It needs a separate queue-based API for network I/O.

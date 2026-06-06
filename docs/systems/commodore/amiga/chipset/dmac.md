# DMAC 390537 — SCSI DMA Controller

The Commodore 390537 SDMAC (Super DMA Controller) provides a SCSI interface for
the A3000. It sits at $DD0000–$DDFFFF, with Fat Gary generating the chip-select.
Inside, it contains a WD33C93(A) SCSI bus interface controller plus DMA transfer
logic for moving data between the SCSI bus and system memory.

## Architecture

The DMAC is two chips in one package:

1. **WD33C93(A)** — Western Digital SCSI protocol controller. Handles
   selection, message phases, and command execution on the SCSI bus. Accessed
   indirectly through an address/data register pair.

2. **SDMAC** — DMA engine with control, status, and address registers. Manages
   bulk data transfers between the WD33C93's internal FIFO and system memory.

## Register Map

All registers are within $DD0000–$DD00FF. The 68030 accesses them as word or
longword cycles; the bus wrapper presents individual byte addresses.

### SDMAC Registers

| Offset | Name | Access | Purpose |
|--------|------|--------|---------|
| $02 | DAWR | Write | DACK width (2 bits) |
| $04 | WTC (high) | Write | Word transfer count, bits 23–16 |
| $06 | WTC (low) | Write | Word transfer count, bits 15–0 |
| $0A | CNTR | R/W | Control register |
| $0C | ACR (high) | Write | DMA address counter, bits 31–16 |
| $0E | ACR (low) | Write | DMA address counter, bits 15–0 |
| $12 | ST_DMA | Write | Start DMA (strobe) |
| $16 | FLUSH | Write | Flush FIFO (strobe) |
| $1A | CINT | Write | Clear interrupts (strobe) |
| $1E | ISTR | Read | Interrupt status |
| $3E | SP_DMA | Write | Stop DMA (strobe) |
| $40 | SASR | R/W | WD33C93 address select / ASR read |
| $42 | SCMD | R/W | WD33C93 indirect data |
| $48 | SASR (alt) | R/W | Alternate SASR port |
| $4A | SCMD (alt) | R/W | Alternate SCMD port |

### CNTR (Control Register)

| Bit | Name | Meaning |
|-----|------|---------|
| 4 | PREST | Peripheral reset — resets WD33C93 |
| 2 | INTEN | Interrupt enable |

### ISTR (Interrupt Status Register, read-only)

| Bit | Name | Meaning |
|-----|------|---------|
| 7 | INT_F | Any interrupt source active (follow bit) |
| 6 | INTS | SCSI peripheral interrupt (WD33C93 INT pin) |
| 4 | INT_P | Interrupt pending (only when CNTR.INTEN = 1) |
| 0 | FE_FLG | FIFO empty |

Reading ISTR does not clear flags. The CINT strobe clears latched interrupt
state.

## WD33C93 SCSI Controller

The WD33C93 is accessed indirectly: write a register address to SASR ($40),
then read or write the register data through SCMD ($42).

### Key WD33C93 Registers

| Address | Name | Purpose |
|---------|------|---------|
| $00 | OWN_ID | Initiator SCSI ID (usually 7) + feature flags |
| $01 | CONTROL | DMA mode, interrupt enables |
| $02 | TIMEOUT_PERIOD | Selection timeout (units of 8ms) |
| $0F | TARGET_LUN | Target LUN for commands |
| $10 | COMMAND_PHASE | SCSI command phase tracking |
| $15 | DESTINATION_ID | Target SCSI ID for selection |
| $17 | SCSI_STATUS | Command Status Register (CSR) — read clears INT |
| $18 | COMMAND | Command execution (write triggers action) |
| $1F | AUXILIARY_STATUS | ASR — directly readable without SASR select |

### Auxiliary Status Register (ASR)

| Bit | Name | Meaning |
|-----|------|---------|
| 7 | INT | Interrupt pending |
| 5 | BSY | Level II command executing |
| 4 | CIP | Command in progress |

Reading the ASR does not require SASR selection — it is read directly from the
SASR port. Reading the SCSI_STATUS register through SCMD clears the INT bit
in the ASR.

### Auto-Increment

After reading or writing through SCMD, the selected register address
auto-increments (wrapping at $1F). This allows burst reads/writes of consecutive
registers without repeatedly writing SASR. The ASR, DATA, and COMMAND registers
do not auto-increment.

### WD33C93 Commands

| Command | Code | Function |
|---------|------|----------|
| RESET | $00 | Software reset. Sets CSR to $00 or $01 depending on EAF flag |
| ABORT | $01 | Abort current operation. Sets CSR to $22 |
| SELECT | $07 | Select target (no ATN). Succeeds or times out |
| SELECT+ATN | $06 | Select with attention. Same as SELECT with ATN asserted |
| SELECT+XFER | $09 | Select and transfer — atomic command execution |
| SELECT+ATN+XFER | $08 | Select with ATN and transfer |

**SELECT+XFER** is the primary command used by AmigaOS. It performs target
selection, sends the SCSI CDB, transfers data, and reports completion in a
single operation.

### Command Status Register (CSR) Values

| Code | Name | Meaning |
|------|------|---------|
| $00 | RESET | Reset completed (no advanced features) |
| $01 | RESET_AF | Reset completed (advanced features enabled) |
| $42 | TIMEOUT | Selection timed out — no target responded |
| $16 | XFER_DONE | Transfer completed successfully |
| $11 | SEL_COMPLETE | Selection completed (for non-transfer commands) |

## SCSI Bus

The WD33C93 supports up to 7 target devices (IDs 0–6; ID 7 is the initiator).
Each target is a SCSI hard disk with a raw disk image (LBA-ordered, 512 bytes
per sector).

### Supported SCSI Commands

| Command | Code | Data Direction |
|---------|------|---------------|
| TEST UNIT READY | $00 | None |
| REQUEST SENSE | $03 | Target → Initiator |
| READ(6) | $08 | Target → Initiator |
| WRITE(6) | $0A | Initiator → Target |
| INQUIRY | $12 | Target → Initiator |
| MODE SENSE(6) | $1A | Target → Initiator |
| READ CAPACITY(10) | $25 | Target → Initiator |
| READ(10) | $28 | Target → Initiator |
| WRITE(10) | $2A | Initiator → Target |

Unknown commands set CHECK CONDITION status with an ILLEGAL REQUEST sense key.

### Target Selection

When the WD33C93 executes a SELECT or SELECT+XFER command:

1. It reads DESTINATION_ID to get the target SCSI ID
2. If a target exists at that ID, selection succeeds
3. If no target exists, selection times out (CSR = $42)

Selection timeout is instant in emulation — there is no real SCSI bus
arbitration delay.

## DMA Transfer Flow

Data transfers between the SCSI bus and system memory use the ACR/WTC registers:

1. Software writes ACR with the system memory address
2. Software writes WTC with the word count
3. Software writes ST_DMA to start the transfer
4. After the SCSI command completes, data is in the DMA buffer
5. The machine-level bus wrapper copies between the buffer and system memory
   using ACR as the base address

In the emulator, DMA is instantaneous — the entire transfer happens when the
SCSI command executes. The buffer is then available for the bus wrapper to copy.

## Interrupt Flow

1. WD33C93 completes a command → sets ASR.INT
2. SDMAC latches ISTR.INTS from the WD33C93 INT pin
3. If CNTR.INTEN is set, ISTR.INT_P is also set
4. ISTR.INT_F is the OR of all active interrupt sources
5. The SDMAC output feeds CIA-B (level 6 EXTER interrupt)
6. Software reads ISTR to check status, then reads WD SCSI_STATUS via SCMD to
   clear INT
7. Software writes CINT strobe to clear latched SDMAC state

## Emulator Implications

- The DMAC is the SCSI boot path for A3000. Without it, KS 2.x on A3000
  cannot detect or boot from hard disks.
- Selection timeout must be immediate when no target is present. KS probes all
  7 SCSI IDs during boot — each absent target should timeout instantly.
- The indirect register access pattern (SASR then SCMD) with auto-increment is
  easy to get wrong. KS reads consecutive registers by writing SASR once then
  reading SCMD multiple times.
- CDB assembly for SELECT+XFER uses WD registers $03–$0E (from TOTAL_SECTORS
  through CYL_LO). The CDB bytes are read from these registers, not from a
  separate command buffer.
- The CINT strobe is write-only — the address and value don't matter, only the
  act of writing to offset $1A.

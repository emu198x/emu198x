# Buster / Super Buster — Zorro II/III Bus Controller

Buster manages the Zorro expansion bus. Every Amiga with expansion slots has
some form of Buster — the original handles Zorro II (24-bit, A500/A2000), and
Super Buster extends it to Zorro III (32-bit, A3000/A4000).

## Autoconfig Protocol

Autoconfig is the Amiga's plug-and-play mechanism. At boot, expansion.library
scans the autoconfig space at $E80000 to discover, configure, and assign base
addresses to expansion boards.

### How It Works

1. **Discovery:** The OS reads the autoconfig descriptor at $E80000. This is a
   nybble-packed ROM that describes the board's type, size, manufacturer, and
   serial number.

2. **Configuration:** The OS writes a base address to the board via the
   autoconfig registers. The board latches this address and begins responding at
   the assigned range.

3. **Next board:** After configuration, the next unconfigured board becomes
   visible at $E80000. The process repeats until all boards are configured.

4. **Shut-up:** If the OS cannot or does not want to configure a board, it
   writes to the SHUTUP register ($E8004C). The board stops responding to
   autoconfig and the next board becomes visible.

### Descriptor Format

The autoconfig descriptor is 64 bytes of nybble-packed, inverted data at even
byte addresses ($E80000, $E80002, $E80004, ...):

| Offset | Content |
|--------|---------|
| $00/$02 | Type byte (board present, chained, Zorro II/III, size code) |
| $04/$06 | Product number (8-bit, two nybbles) |
| $08/$0A | Flags (can't-shut-up, extended size for Zorro III) |
| $10–$16 | Manufacturer ID (16-bit, four nybbles) |
| $18–$26 | Serial number (32-bit, eight nybbles) |

All nybble values are bitwise inverted as stored. The OS reads a byte, inverts
it, and extracts the relevant 4 bits.

### Address Assignment

| Register | Offset | Purpose |
|----------|--------|---------|
| BASE_HI | $48 | Zorro II: A23–A16 of base address |
| BASE_LO | $4A | Zorro II: A15–A8 (for boards ≤ 64 KB) |
| Z3_BASE_HI | $44 | Zorro III: A31–A24 of base address |
| Z3_BASE_LO | $48 | Zorro III: A23–A16 (completes configuration) |
| SHUTUP | $4C | Skip this board |

For Zorro II, writing BASE_HI immediately configures the board and advances to
the next. For Zorro III, both Z3_BASE_HI and Z3_BASE_LO must be written; the
write to Z3_BASE_LO completes configuration.

## Zorro II (Buster)

Zorro II boards occupy the 24-bit address space between $200000 and $9FFFFF.
Each board is assigned a base address within this range, aligned to its size.

### Board Sizes

| Code | Size |
|------|------|
| 000 | 8 MB |
| 001 | 64 KB |
| 010 | 128 KB |
| 011 | 256 KB |
| 100 | 512 KB |
| 101 | 1 MB |
| 110 | 2 MB |
| 111 | 4 MB |

### Board Types

The type byte identifies the board category:

- **$C0** — Zorro II memory (RAM expansion)
- **$C1** — Zorro II I/O (peripherals)

The emulator currently supports RAM expansion boards. The autoconfig descriptor
uses manufacturer $0198 ("EMU198X") and product 1.

## Zorro III (Super Buster)

Super Buster extends the protocol to the 32-bit address space. Zorro III boards
can be mapped anywhere above $01000000, giving access to the full 68030/040
address range.

### Configuration Phases

Super Buster configures boards in two phases:

1. **Zorro III phase:** All Zorro III boards are configured first. The type byte
   has bit 5 set ($E0 instead of $C0) to identify Zorro III boards.

2. **Zorro II phase:** After all Zorro III boards are configured (or shut up),
   Zorro II boards become visible at $E80000 and are configured using the
   standard 24-bit protocol.

The phase advances automatically as boards are configured or shut up.

### Zorro III Sizes

Zorro III extends the size codes beyond Zorro II:

| Code | Size |
|------|------|
| $00 | 16 MB |
| $01 | 32 MB |
| $02 | 64 MB |
| $03 | 128 MB |
| $04 | 256 MB |
| $05 | 512 MB |
| $06 | 1 GB |

Zorro II size codes ($000–$111) are also valid for Zorro III boards that fit
in the 8 MB or smaller range.

### Address Assignment

Zorro III base addresses are 32-bit. The OS writes A31–A24 to Z3_BASE_HI
($E80044), then A23–A16 to Z3_BASE_LO ($E80048). The write to Z3_BASE_LO
completes configuration and advances to the next board.

## Board I/O Dispatch

After configuration, boards respond to their assigned address range:

- **Read:** `board_read(addr)` returns `Some(byte)` if any configured board
  claims the address, `None` if unclaimed.
- **Write:** `board_write(addr, val)` returns `true` if a board claimed the
  address.

The emulator's memory dispatcher calls these after Gary's decode identifies
the address as being in the expansion range. For Zorro III, the 32-bit address
is checked against Z3 boards before falling through to the 24-bit decode.

## Reset Behaviour

On reset, all boards become unconfigured:

- Base addresses are cleared
- The autoconfig index resets to the first board
- Super Buster returns to the Zorro III phase

Boards retain their descriptors and RAM contents through reset — only the
configuration state is lost. The OS must re-run the autoconfig sequence.

## Emulator Implications

- Autoconfig must present boards one at a time. Reading $E80000 before any
  writes shows the first board's descriptor. After configuration or shut-up,
  the next board appears. After all boards are done, reads return $FF.
- The nybble-inversion scheme is a common source of bugs. Each byte read from
  the autoconfig ROM must be bitwise inverted before the OS extracts meaningful
  data.
- Zorro II base addresses must be within $200000–$9FFFFF. Zorro III base
  addresses must be above $01000000.
- Super Buster's two-phase protocol means Zorro III boards appear first at
  $E80000. If the emulator mixes the phases (showing a Z2 board during Z3
  phase), expansion.library will misidentify the board type.
- RAM boards must be writable after configuration. The board stores data in a
  Vec and responds to reads/writes within its configured range.
- When autoconfig is complete and no board claims an address in the expansion
  range, the read falls through to Gary's Unmapped chip-select (returns 0).

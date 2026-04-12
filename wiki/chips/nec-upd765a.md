# NEC µPD765A (Floppy Disk Controller)

Floppy disk controller used in the ZX Spectrum +3 (and +2A with disk drive). Handles the command/execution/result protocol for reading and writing floppy disks.

## Crate

`nec-upd765a`

## Architecture

Multi-phase protocol:
1. **Command phase** — host sends command bytes
2. **Execution phase** — controller reads/writes disk data
3. **Result phase** — controller returns status bytes

## Disk geometry

| Parameter | Value |
|-----------|-------|
| Tracks | 40 |
| Sectors per track | 9 |
| Bytes per sector | 512 |
| Sides | 1 (single-sided) |

## Disk image formats

Consumes a structured `DiskImage` (tracks × sides × sectors), with sectors looked up by their address-mark ID (R) rather than by physical position — non-IBM sector layouts (e.g. CPC 0xC1..0xC9) work correctly.

Standard DSK and Extended DSK (EDSK) parsing lives in the `format-amstrad-dsk` crate. Non-uniform tracks and weak/protected sector layouts are not yet supported; the parser errors clearly when it sees them.

## Implemented commands

Recalibrate, Seek, Specify, Sense Interrupt Status, Sense Drive Status, Read ID, Read Data. Write Data is decoded but not yet executed.

## I/O ports (Spectrum +3)

| Port | Decode | Direction | Function |
|------|--------|-----------|----------|
| `$2FFD` | `port & 0xF002 == 0x2000` | Read | Main status register |
| `$3FFD` | `port & 0xF002 == 0x3000` | Read/Write | Data register |

## Serialisation

`#[derive(serde::Serialize, serde::Deserialize)]` on all state. Disk images are `#[serde(skip)]` — they must be re-inserted after restore.

## Used by

- [Spectrum +3](../systems/spectrum/variants.md) — via `machine-sinclair-zx-spectrum-plus`

The Beta disk interface used by Pentagon/Scorpion clones is a separate chip ([WD1793](../systems/spectrum/variants.md)), not this one.

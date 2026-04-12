# Floppy Drive — Motor Control, Data Path, Track Format

The Amiga's floppy drive is not a custom chip, but it is the other half of
Paula's disk DMA system and the primary boot device. Understanding its timing
and data format is essential for getting disk boot to work.

## Data Layer Model

Three layers sit between a disk image file and the hardware DMA:

```
┌─────────────────────────────────────────────────────────┐
│  Disk Image File                                        │
│  ADF: raw sector data, 512 bytes × 11 × 2 × 80 = 880K │
│  IPF: pre-encoded MFM tracks with timing metadata       │
└──────────────────────────┬──────────────────────────────┘
                           │
                    encode_mfm_track()  (ADF only — IPF is already encoded)
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│  Raw MFM Track                                          │
│  ~13,630 bytes of MFM-encoded data per track            │
│  Includes sync words, headers, checksums, gap filler    │
│  This is what the drive's read head "sees"              │
└──────────────────────────┬──────────────────────────────┘
                           │
                    Paula disk DMA (DSKSYNC match, word assembly)
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│  Chip RAM (DSKPT buffer)                                │
│  Raw MFM words land here via DMA                        │
│  trackdisk.device decodes them back into sector data    │
└─────────────────────────────────────────────────────────┘
```

**ADF** is the simple case: 901,120 bytes of raw sector data, no encoding, no
headers. The `format-adf` crate handles byte offset arithmetic. The emulator's
`encode_mfm_track()` wraps each sector with sync words, headers, checksums, and
the odd/even bit split that Paula expects. KS trackdisk.device then parses the
raw DMA data back into sector buffers. If you are only thinking at the ADF
level, MFM is invisible.

**IPF** (SPS/CAPS preservation format) stores pre-encoded track data with
timing information — copy-protected disks use non-standard MFM patterns that
can't be reconstructed from sector data alone. IPF tracks go straight to the
drive without re-encoding. This is why `DiskImage` is a trait: ADF encodes
on the fly, IPF provides pre-encoded data.

**File systems** are not the emulator's concern. The OS handles OFS/FFS
interpretation. The only file system structure the emulator touches is the
bootblock checksum that KS verifies before jumping to boot code (covered in
the Kickstart boot docs).

## Physical Interface

The floppy drive connects to the system through two CIAs and Paula:

| Signal | Chip | Port/Bit | Direction | Purpose |
|--------|------|----------|-----------|---------|
| /SEL0–3 | CIA-B | PRA 3–6 | Out | Drive select (active low) |
| /MTR | CIA-B | PRA 7 | Out | Motor on (active low) |
| /SIDE | CIA-B | PRA 2 | Out | Head select (0=upper, 1=lower) |
| DIR | CIA-B | PRA 1 | Out | Step direction (0=inward, 1=outward) |
| /STEP | CIA-B | PRA 0 | Out | Step pulse (active low edge) |
| /DSKRDY | CIA-A | PRA 5 | In | Drive ready (motor at speed) |
| /DSKTRACK0 | CIA-A | PRA 4 | In | Head at cylinder 0 |
| /DSKPROT | CIA-A | PRA 3 | In | Disk write-protected |
| /DSKCHANGE | CIA-A | PRA 2 | In | Disk changed since last step |
| /INDEX | CIA-A | FLAG | In | Index pulse (once per revolution) |
| MFM data | Paula | Disk DMA | In/Out | Raw MFM bitstream |

All control signals are active-low. The boolean sense in the emulator is
inverted: `true` means the signal is asserted (pin driven low).

## Drive Mechanics

### Motor Control

Motor state latches when the drive is selected:

1. Software selects the drive (/SELx low) with /MTR asserted
2. Motor begins spinning — not immediately at speed
3. After ~500ms spin-up, /DSKRDY asserts (drive ready)
4. Deselecting the drive preserves motor state

The spin-up timer runs at E-clock rate (709 kHz). The constant is 350,000
ticks, approximately 500ms.

```
MOTOR_SPINUP_TICKS = 350,000
Ready delay = 350,000 / 709,379 ≈ 493ms
```

KS waits for /DSKRDY before starting disk DMA. If the emulator's motor
spin-up is too slow (or /DSKRDY never asserts), boot hangs.

### Head Stepping

Head movement is edge-triggered:

1. Software sets DIR (inward or outward)
2. Software pulses /STEP (deasserted → asserted = falling edge)
3. Drive steps one cylinder in the specified direction
4. Range: cylinder 0 to cylinder 79 (80 tracks per side)

Step events clear the /DSKCHANGE flag when a disk is present. This is how
the OS detects that a new disk has been acknowledged — it steps the head and
checks whether /DSKCHANGE went inactive.

### Head Selection

/SIDE selects which disk surface to read:
- /SIDE asserted (low) → upper head (head 1)
- /SIDE deasserted (high) → lower head (head 0)

Track number = cylinder × 2 + head.

### Disk Change Detection

/DSKCHANGE is active (low) when a disk has been removed or inserted since the
last head step. The state machine:

1. Power-on: /DSKCHANGE active (no disk)
2. Disk inserted: /DSKCHANGE active (changed since last step)
3. Head step: /DSKCHANGE clears (if disk present)
4. Disk ejected: /DSKCHANGE active immediately

KS polls /DSKCHANGE to detect disk insertions and uses it to trigger the
"insert disk" prompt.

## MFM Track Format

The Amiga uses a unique MFM encoding that differs from the IBM PC format. Data
is stored with an odd/even bit-split: for each 32-bit longword, odd-position
bits are transmitted first, then even-position bits. Each half is MFM-encoded
separately.

### Sector Layout

Each track contains 11 sectors (DD) or 22 sectors (HD). Per sector:

| Field | Size (words) | Content |
|-------|-------------|---------|
| Gap | 2 | $AAAA (MFM clock-only filler) |
| Sync | 2 | $4489 (MFM-encoded $A1 with missing clock) |
| Header info | 4 | Format ($FF), track, sector, sectors-to-gap |
| Sector label | 16 | 16 zero bytes (odd/even split + MFM) |
| Header checksum | 4 | XOR of header + label, masked $55555555 |
| Data checksum | 4 | XOR of data, masked $55555555 |
| Data | 512 | 512 bytes (odd/even split + MFM) |

Total per sector: 544 words = 1,088 bytes. Eleven sectors ≈ 11,968 bytes,
plus inter-sector gap filler to fill the track.

### Sync Word ($4489)

The sync word is the magic pattern that disk DMA searches for. It is the MFM
encoding of $A1 with a deliberately missing clock bit — a pattern that cannot
occur in normal MFM data. This is how the controller finds sector boundaries
in the raw bitstream.

Paula's DSKSYNC register holds this value ($4489 by default). When the disk
controller sees this pattern, it sets DSKBYTR.WORDEQUAL and (if DMA is active)
begins transferring data.

### Odd/Even Bit Split

The Amiga's MFM encoding splits each longword:

```
Original: b31 b30 b29 ... b1 b0

Odd bits:  b31 b29 b27 ... b3 b1  (transmitted first)
Even bits: b30 b28 b26 ... b2 b0  (transmitted second)
```

Each 16-bit half is then MFM-encoded: a clock bit is inserted between each data
bit, set to 1 only if both adjacent data bits are 0.

### Checksum ($55555555 Mask)

All checksums are computed by XOR-ing the raw MFM longwords, then masking with
$55555555 to strip clock bits. The mask selects only the data bits from the MFM
stream. This was a critical emulator bug — without the mask, KS trackdisk.device
rejects every sector as having a bad checksum.

### Track Capacity

| Format | Sectors | Data per track | Raw MFM bytes |
|--------|---------|---------------|---------------|
| DD | 11 | 5,632 bytes | ~13,630 bytes |
| HD | 22 | 11,264 bytes | ~27,260 bytes |

The raw track size includes gap filler ($AA bytes) to pad out the space between
the last sector and the index pulse.

## Disk DMA Integration

Paula and the floppy drive interact through the disk DMA system described in
[paula.md](paula.md):

1. Software sets DSKLEN (two-write protocol)
2. Paula searches the MFM bitstream for the sync word
3. On sync: Paula assembles 16-bit words and transfers via DMA slots
4. Agnus services disk DMA at CCK $04–$06
5. Data lands in chip RAM at DSKPT
6. When the word count reaches 0, DSKBLK interrupt fires

### Boot Sequence Timing

A typical cold-boot disk read:

```
T = 0ms:      KS starts motor, selects drive 0
T ≈ 500ms:    Motor reaches speed (/DSKRDY asserts)
T ≈ 500ms:    KS steps to track 0 (verifies /DSKTRACK0)
T ≈ 500ms:    KS starts disk DMA for track 0 (2 sectors = bootblock)
T ≈ 700ms:    Disk completes one revolution, sync found, data transferred
T ≈ 700ms:    DSKBLK interrupt — bootblock in memory
T ≈ 700ms:    KS verifies bootblock checksum, jumps to boot code
```

The total cold-boot time to start executing bootblock code is approximately
700ms (dominated by motor spin-up + one revolution).

## Write Path

Writing reverses the DMA direction:

1. Software fills chip RAM with MFM-encoded data
2. DSKLEN bit 14 set (write mode), two-write protocol
3. Paula reads words from chip RAM via DMA
4. Words are presented to the drive's write head
5. Drive writes MFM data to the disk surface

The emulator captures write data in a buffer, decodes MFM sectors, and writes
decoded data back to the disk image. This flush happens when the write transfer
completes or the motor stops.

## Emulator Implications

- Motor spin-up timing is critical. If /DSKRDY asserts instantly (no delay),
  some timing-sensitive software breaks. The 350K E-clock tick delay matches
  real hardware.
- /DSKCHANGE must clear on step (not on insert). KS relies on this — it steps
  the head to acknowledge a disk change.
- The $4489 sync word must be present and correctly positioned in the MFM data.
  Without it, Paula never starts transferring and the DSKBLK interrupt never
  fires.
- The $55555555 checksum mask is mandatory. Omitting it was a real bug that
  caused all disk reads to fail.
- Track encoding must produce the exact number of bytes that fill one revolution.
  Too few bytes means the sync word is never found on the second revolution;
  too many means sectors overlap.
- Four drives (DF0–DF3) share the same control lines via /SEL0–3. Only the
  selected drive responds to /STEP and /MTR. Software must select the correct
  drive before issuing commands.

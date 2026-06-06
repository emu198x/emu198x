# Paula — Interrupts, Audio DMA, Disk DMA

Paula manages three systems: the interrupt priority controller, four audio DMA
channels, and floppy disk DMA. Unlike Agnus and Denise, Paula is identical
across OCS, ECS, and AGA.

## Interrupt Controller

Paula maps 14 interrupt sources to 6 CPU interrupt levels (IPL1-IPL6). The
68000 reads the interrupt level from its IPL pins and vectors to the
corresponding autovector handler.

### Interrupt Priority Map

| Level | IPL | Sources | Typical use |
|-------|-----|---------|-------------|
| 1 | 001 | TBE, DSKBLK, SOFT | Serial transmit, disk block done, software INT |
| 2 | 010 | PORTS (CIA-A) | CIA-A: keyboard, timer A/B, TOD, serial |
| 3 | 011 | COPER, VERTB, BLIT | Copper, vertical blank, blitter finished |
| 4 | 100 | AUD0, AUD1, AUD2, AUD3 | Audio channel interrupts |
| 5 | 101 | RBF, DSKSYN | Serial receive, disk sync word matched |
| 6 | 110 | EXTER (CIA-B) | CIA-B: disk index, timer A/B, TOD |

Level 7 (NMI) is directly exposed on a pin and is active-low — it's not managed
by Paula.

### INTENA and INTREQ

Both registers use SET/CLR semantics (bit 15):

**INTENA** ($DFF09A write, $DFF01C read) — interrupt enable:

| Bit | Name | Level | Source |
|-----|------|-------|--------|
| 14 | INTEN | — | Master interrupt enable (gates ALL interrupts) |
| 13 | EXTER | 6 | CIA-B external interrupt |
| 12 | DSKSYN | 5 | Disk sync word found |
| 11 | RBF | 5 | Serial receive buffer full |
| 10 | AUD3 | 4 | Audio channel 3 |
| 9 | AUD2 | 4 | Audio channel 2 |
| 8 | AUD1 | 4 | Audio channel 1 |
| 7 | AUD0 | 4 | Audio channel 0 |
| 6 | BLIT | 3 | Blitter finished |
| 5 | VERTB | 3 | Vertical blank (start of frame) |
| 4 | COPER | 3 | Copper (copper writes COPJMP or copper interrupt) |
| 3 | PORTS | 2 | CIA-A I/O ports (keyboard, timers) |
| 2 | SOFT | 1 | Software interrupt (set by writing INTREQ) |
| 1 | DSKBLK | 1 | Disk block finished |
| 0 | TBE | 1 | Serial transmit buffer empty |

**INTREQ** ($DFF09C write, $DFF01E read) — interrupt request:

Same bit layout as INTENA. Hardware sets bits when events occur; software
clears them by writing with bit 15 = 0.

### Interrupt Delivery

The output interrupt level is the highest level where at least one source has
both INTENA and INTREQ bits set, AND INTEN (bit 14) is set:

```
active = INTENA & INTREQ & {INTEN replicated to all bits}
level = highest priority level with any active bit
```

If no interrupts are active, the IPL pins read 0 (no interrupt).

**Timing:** There is a ~2-CCK delay between setting INTREQ and the CPU seeing
the interrupt on its IPL pins. Software must not assume instant delivery.

## Audio DMA

Paula drives four 8-bit audio channels with independent DMA. Each channel
outputs one sample at a programmable rate, and DMA automatically refills the
sample buffer from chip RAM.

### Per-Channel Registers

| Register | Address | Access | Content |
|----------|---------|--------|---------|
| AUDxLC | $DFF0A0+ | Write | Location — 18-bit chip RAM pointer (high, low) |
| AUDxLEN | $DFF0A4+ | Write | Length — number of words in sample (1-65535) |
| AUDxPER | $DFF0A6+ | Write | Period — output rate in colour clocks |
| AUDxVOL | $DFF0A8+ | Write | Volume — 0 (silent) to 64 (full) |
| AUDxDAT | $DFF0AA+ | Write | Data — DMA writes here, or software poke |

Channel offsets: ch0 = $A0, ch1 = $B0, ch2 = $C0, ch3 = $D0.

### DMA Pipeline

Audio DMA uses a two-word buffer with a return-latency model:

1. **DMA request:** When the channel's buffer needs data, it sets a DMA request
   flag. Agnus services this request during the channel's fixed DMA slot
   (CCK $07-$0A).

2. **Return latency:** After the DMA slot is serviced, the data takes 14 CCKs
   to become available to the channel (models the chip-bus return path). During
   these 14 CCKs, certain bus conditions stall the countdown:
   - DMA slots owned by Agnus (refresh, disk, sprite, audio, bitplane) stall
   - Copper slots stall only if the copper actually performs a chip-bus fetch
   - CPU/free slots do not stall

3. **Word consumption:** The channel consumes one word at a time, outputting
   the high byte first, then the low byte. Each byte is output for AUDxPER
   colour clocks.

4. **Block repeat:** When all words in the current block are consumed (counter
   reaches 0), the channel reloads the pointer from AUDxLC and the length from
   AUDxLEN, and raises the audio interrupt (INTREQx). This enables continuous
   playback using double-buffered interrupt handlers.

### Period and Sample Rate

AUDxPER sets the number of colour clocks between samples:
- Minimum period: 124 CCKs (practical — below this, DMA can't keep up)
- Output rate = CCK_frequency / (PER × 2)
  - PER=124 → ~28.6 kHz (PAL)
  - PER=428 → ~8.3 kHz (speech quality)
  - PER=256 → ~13.9 kHz (music quality)

The ×2 factor is because each word provides two samples (high byte and low
byte), and each sample plays for PER colour clocks.

### Volume

AUDxVOL ranges from 0 (silent) to 64 (full volume). Values above 64 are
clamped. The output amplitude is sample × volume / 64.

### Channel Assignment

Channels are hardwired to stereo outputs:
- Left: channels 0 and 3
- Right: channels 1 and 2

### Modulation (ADKCON)

ADKCON ($DFF09E) enables amplitude and period modulation between adjacent
channels:

| Bit | Name | Effect |
|-----|------|--------|
| 0 | USE0V1 | Channel 0 output modulates channel 1 volume |
| 1 | USE1V2 | Channel 1 output modulates channel 2 volume |
| 2 | USE2V3 | Channel 2 output modulates channel 3 volume |
| 4 | USE0P1 | Channel 0 output modulates channel 1 period |
| 5 | USE1P2 | Channel 1 output modulates channel 2 period |
| 6 | USE2P3 | Channel 2 output modulates channel 3 period |

When a channel is used as a modulation source, its audio output is muted
(the modulation value replaces its DAC output).

### DAC Non-Linearity

The A500's audio output stage uses a resistor-ladder DAC with slight
non-linearity. The transfer function is approximately:

```
y = x - 0.02 × x³
```

where x is the normalised sample value (-1.0 to +1.0). This produces ~2%
compression at the peaks and a slightly sharper zero-crossing, giving the
Amiga its characteristic warm sound.

## Disk DMA

Paula handles the timing of floppy disk DMA while Agnus provides the bus
slots. Disk DMA reads or writes raw MFM data between chip RAM and the
floppy drive.

### Disk DMA Registers

| Register | Address | Access | Content |
|----------|---------|--------|---------|
| DSKPTH/L | $DFF020/022 | Write | DMA buffer pointer in chip RAM |
| DSKLEN | $DFF024 | Write | DMA length and control |
| DSKSYNC | $DFF07E | Write | Sync word for read operations |
| DSKBYTR | $DFF01A | Read | Current byte and status |
| DSKDATR | $DFF008 | Read | Disk data register |

### DSKLEN Protocol

Disk DMA uses a two-write protocol to prevent accidental activation:

1. Write DSKLEN with bit 15 (DMAEN) set and the desired word count
2. Write DSKLEN again with the same value

DMA starts only after the second write matches the first. This prevents
stray writes from starting unwanted disk operations.

- Bit 15: DMA enable (1 = start after second write)
- Bit 14: Write mode (1 = write to disk, 0 = read from disk)
- Bits 13-0: Word count (0-16383)

Writing DSKLEN with bit 15 = 0 immediately stops disk DMA.

### Read DMA Timing

Disk data arrives one MFM bit per disk revolution clock (~2µs per bit for DD,
~1µs for HD). Paula assembles bits into 16-bit words:

1. Wait for sync word match (DSKSYNC value found in bitstream)
2. Once sync found, begin assembling words
3. Each complete word is transferred via Agnus DMA slot (CCK $04-$06)
4. Word count decrements; when zero, DSKBLK interrupt fires

**Byte timing:** One word takes either 14 CCKs (fast disk, ADKCON bit 8 set)
or 28 CCKs (normal DD speed). The DMA slot allocation is fixed regardless of
disk speed — the slot availability in CCK $04-$06 determines maximum
throughput.

### Sync Word

DSKSYNC ($DFF07E) holds the 16-bit pattern that the disk controller searches
for in the raw MFM bitstream. The standard Amiga sync word is $4489 (the MFM
encoding of the $A1 sync byte used in sector headers).

When the sync word is detected:
1. DSKBYTR WORDEQUAL flag is set
2. If disk DMA is active, data transfer begins
3. DSKSYN interrupt fires (INTREQ bit 12)

### DSKBYTR ($DFF01A)

Status register for byte-level disk access:

| Bit | Name | Meaning |
|-----|------|---------|
| 15 | BYTEREADY | New byte available |
| 14 | DMAON | Disk DMA is running |
| 13 | DISKWRITE | DMA is in write mode |
| 12 | WORDEQUAL | Last word matched DSKSYNC |
| 7-0 | DATA | Current disk data byte |

Software polling DSKBYTR allows byte-by-byte disk access without DMA, used
by some copy-protection schemes and diagnostic tools.

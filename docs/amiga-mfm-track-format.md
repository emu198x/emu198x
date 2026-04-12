# Amiga MFM Track Format: Definitive Reference

This document describes the low-level MFM (Modified Frequency Modulation) encoding used by the Amiga floppy disk controller, the byte-by-byte track layout, disk DMA operation, and the ADF file format. It covers the layer *below* the AmigaDOS filesystem (OFS/FFS blocks) and *above* the raw magnetic flux transitions on the disk surface.

All details are extracted from working emulator source code: **vAmiga** (Dirk W. Hoffmann) and **WinUAE** (Toni Wilen / Bernd Schmidt), cross-referenced for accuracy. Source citations use the format `(vAmiga file:line)` and `(WinUAE disk.cpp:line)`.


---

## 1. MFM Encoding Fundamentals

### 1.1 What MFM Is

MFM encodes one data bit into two bit cells: a **clock bit** followed by a **data bit**. The clock bit provides self-clocking capability so the disk controller can recover data without a separate clock track.

The encoding rule:

- The **data bit** occupies the even-numbered bit positions (positions 1, 3, 5, 7, 9, 11, 13, 15 within a 16-bit MFM word).
- The **clock bit** occupies the odd-numbered bit positions (positions 0, 2, 4, 6, 8, 10, 12, 14).
- A clock bit is **1** only when *both* adjacent data bits (the one before it and the one after it) are **0**.
- If either adjacent data bit is 1, the clock bit is 0.

This guarantees that no two consecutive bit cells are both 1, which limits the maximum flux transition density and prevents inter-symbol interference.

### 1.2 Encoding a Byte to MFM (Data Bit Spreading)

The first step spreads one data byte (8 bits) across two MFM bytes (16 bits), placing each data bit into the data-bit position and leaving clock-bit positions as 0:

```
Source byte:  d7 d6 d5 d4 d3 d2 d1 d0

MFM word:     0  d7  0  d6  0  d5  0  d4  0  d3  0  d2  0  d1  0  d0
              ^      ^      ^      ^      ^      ^      ^      ^
              clock positions (initially 0)
```

From vAmiga (`MFM.cpp:17-33`):

```cpp
void encodeMFM(u8 *dst, const u8 *src, isize count)
{
    for(isize i = 0; i < count; i++) {
        auto mfm =
        ((src[i] & 0b10000000) << 7) |
        ((src[i] & 0b01000000) << 6) |
        ((src[i] & 0b00100000) << 5) |
        ((src[i] & 0b00010000) << 4) |
        ((src[i] & 0b00001000) << 3) |
        ((src[i] & 0b00000100) << 2) |
        ((src[i] & 0b00000010) << 1) |
        ((src[i] & 0b00000001) << 0);

        dst[2*i+0] = HI_BYTE(mfm);
        dst[2*i+1] = LO_BYTE(mfm);
    }
}
```

Each bit is shifted left by its position index, producing the interleaved pattern. One data byte becomes two MFM bytes (one 16-bit MFM word).

### 1.3 Adding Clock Bits

After data bits are placed, clock bits fill the remaining positions according to the MFM rule. From vAmiga (`MFM.cpp:89-105`):

```cpp
u8 addClockBits(u8 value, u8 previous)
{
    // Clear all previously set clock bits
    value &= 0x55;  // 0x55 = 0b01010101, keeps only data bits

    // Compute clock bits (clock bit values are inverted)
    u8 lShifted = (u8)(value << 1);
    u8 rShifted = (u8)(value >> 1 | previous << 7);
    u8 cBitsInv = (u8)(lShifted | rShifted);

    // Reverse the computed clock bits
    u8 cBits = cBitsInv ^ 0xAA;  // 0xAA = 0b10101010, clock positions

    // Return original value with the clock bits added
    return value | cBits;
}
```

The algorithm:

1. Mask to keep only data bits (`& 0x55`).
2. Shift data bits left and right by 1 to get the *neighbours* of each clock position.
3. If either neighbour is 1, the clock bit must be 0; if both are 0, the clock bit must be 1.
4. The boundary between bytes requires the last bit of the previous byte, handled by the `previous` parameter.

WinUAE uses an equivalent approach (`disk.cpp:2059-2069`):

```cpp
static void mfmcode(uae_u16 *mfm, int words)
{
    uae_u32 lastword = 0;
    while (words--) {
        uae_u32 v = (*mfm) & 0x55555555;
        uae_u32 lv = (lastword << 16) | v;
        uae_u32 nlv = 0x55555555 & ~lv;
        uae_u32 mfmbits = (nlv << 1) & (nlv >> 1);
        *mfm++ = v | mfmbits;
        lastword = v;
    }
}
```

Same logic at word granularity: a clock bit is 1 only where both data-bit neighbours are 0.

### 1.4 Decoding MFM Back to Data

Decoding strips clock bits by masking with `0x5555` (keeping every other bit) and compacting. From vAmiga (`MFM.cpp:37-55`):

```cpp
void decodeMFM(u8 *dst, const u8 *src, isize count)
{
    for(isize i = 0; i < count; i++) {
        u16 mfm = HI_LO(src[2*i], src[2*i+1]);
        auto decoded =
        ((mfm & 0b0100000000000000) >> 7) |
        ((mfm & 0b0001000000000000) >> 6) |
        ((mfm & 0b0000010000000000) >> 5) |
        ((mfm & 0b0000000100000000) >> 4) |
        ((mfm & 0b0000000001000000) >> 3) |
        ((mfm & 0b0000000000010000) >> 2) |
        ((mfm & 0b0000000000000100) >> 1) |
        ((mfm & 0b0000000000000001) >> 0);

        dst[i] = (u8)decoded;
    }
}
```

Each data bit is extracted from its MFM position and shifted back into a contiguous byte.

### 1.5 The Amiga Odd/Even Interleave

The Amiga does **not** use standard MFM encoding for sector data. Instead, it uses a proprietary **odd/even split** encoding that separates the odd and even bits of each data byte into two separate halves.

For N data bytes, the encoder produces 2N MFM bytes arranged as:

```
Bytes 0..N-1:     odd bits of data bytes  (each data bit >> 1, masked with 0x55)
Bytes N..2N-1:    even bits of data bytes  (each data bit masked with 0x55)
```

From vAmiga (`MFM.cpp:58-79`):

```cpp
void encodeOddEven(u8 *dst, const u8 *src, isize count)
{
    // Encode odd bits
    for(isize i = 0; i < count; i++)
        dst[i] = (src[i] >> 1) & 0x55;

    // Encode even bits
    for(isize i = 0; i < count; i++)
        dst[i + count] = src[i] & 0x55;
}

void decodeOddEven(u8 *dst, const u8 *src, isize count)
{
    // Decode odd bits
    for(isize i = 0; i < count; i++)
        dst[i] = (u8)((src[i] & 0x55) << 1);

    // Decode even bits
    for(isize i = 0; i < count; i++)
        dst[i] |= src[i + count] & 0x55;
}
```

This odd/even interleave is a pre-step before clock bits are added. After odd/even encoding, the data is already in MFM data-bit positions (only every other bit is set), so clock bits can be inserted directly.

**Why odd/even?** The Amiga's disk controller hardware (the Paula chip) operates on 16-bit words. The odd/even split allows a longword (32 bits) of data to be encoded as two consecutive MFM longwords without any bit-level shifting within words -- just a mask and a shift-by-1. This simplifies the hardware and makes checksum computation efficient.

WinUAE confirms identical logic (`disk.cpp:2209-2251`), encoding a 32-bit data longword as two MFM longwords:

```cpp
deven = ((secbuf[4] << 24) | (secbuf[5] << 16)
    | (secbuf[6] << 8) | (secbuf[7]));
dodd = deven >> 1;
deven &= 0x55555555;
dodd &= 0x55555555;

mfmbuf[4] = dodd >> 16;   // Odd bits, high word
mfmbuf[5] = dodd;         // Odd bits, low word
mfmbuf[6] = deven >> 16;  // Even bits, high word
mfmbuf[7] = deven;        // Even bits, low word
```

To reconstruct the original longword from two MFM longwords (`disk.cpp:2594-2597`):

```cpp
odd = getmfmlong(mbuf, shift);     // Mask with 0x55555555
even = getmfmlong(mbuf + 2, shift);
id = (odd << 1) | even;            // Recombine
```

### 1.6 Why $4489 Is the Sync Word

The sync word `$4489` (binary `0100 0100 1000 1001`) is special because it **violates the MFM clock insertion rule**. Specifically, it contains a missing clock bit where the MFM rule demands one.

Breaking it down bit by bit (C = clock, D = data):

```
Bit position: 15 14 13 12 11 10  9  8  7  6  5  4  3  2  1  0
Bit value:     0  1  0  0  0  1  0  0  1  0  0  0  1  0  0  1
Role:          C  D  C  D  C  D  C  D  C  D  C  D  C  D  C  D

Data bits:        1     0     1     0     0     0     1     1
Clock bits:    0     0     0     0     1     0     0     0
```

The data bits decode to `$A3` (not meaningful as data). The violation is at **bit position 4** (a clock bit): the adjacent data bits are at positions 5 (data=0) and 3 (data=0). Since both neighbours are 0, the MFM rule says this clock bit should be 1 -- but it is 0.

If we were to properly MFM-encode the byte `$A1` (`1010 0001`), the result would be:

```
Data:    1  0  1  0  0  0  0  1
MFM:  0  1  0  0  0  1  0  0  1  0  1  0  1  0  0  1
                                    ^  ^
                            These clock bits should be 1 by the MFM rule
```

Proper MFM encoding of `$A1` gives `$44A9`, not `$4489`. The sync value `$4489` differs from `$44A9` at bit 5: the clock bit between data-0 and data-0 is missing. This missing clock is what makes `$4489` impossible to produce through normal MFM encoding.

**Why this matters:** The disk controller continuously shifts bits through a 16-bit register looking for a match against DSKSYNC. Because `$4489` cannot appear in properly clock-inserted MFM data at *any* bit alignment, it serves as an unambiguous frame synchronisation marker. No matter what data is written to the disk, the pattern `$4489` can only appear where it was deliberately placed.

**Double sync:** The standard Amiga format uses *two* consecutive `$4489` words (`$44894489`). The first sync word resets the controller's bit alignment (it now knows where word boundaries are). The second confirms the alignment. After the second sync word, the controller reads subsequent bytes in perfect alignment.

The DSKSYNC register (`$07E`) defaults to `$4489` on reset. Writing a non-standard value to DSKSYNC is flagged as unusual by both emulators -- vAmiga logs it as `xfiles("DSKSYNC: Unusual sync mark $%04X\n")` (`DiskControllerRegs.cpp:152`).

### 1.7 MFM Encoding Lookup Table

WinUAE provides a lookup table for encoding nibbles (4-bit values) directly to MFM (`disk.cpp:2072-2074`):

```cpp
static const uae_u8 mfmencodetable[16] = {
    0x2a, 0x29, 0x24, 0x25, 0x12, 0x11, 0x14, 0x15,
    0x4a, 0x49, 0x44, 0x45, 0x52, 0x51, 0x54, 0x55
};
```

Each nibble maps to an 8-bit MFM pattern. This table is used for PC-DOS format encoding (which uses standard MFM rather than the Amiga's odd/even split), but it also serves as a reference for the mapping between data nibbles and their MFM representations:

| Nibble | Binary | MFM (hex) | MFM (binary) |
|--------|--------|-----------|--------------|
| 0      | 0000   | $2A       | 00101010     |
| 1      | 0001   | $29       | 00101001     |
| 2      | 0010   | $24       | 00100100     |
| 3      | 0011   | $25       | 00100101     |
| 4      | 0100   | $12       | 00010010     |
| 5      | 0101   | $11       | 00010001     |
| 6      | 0110   | $14       | 00010100     |
| 7      | 0111   | $15       | 00010101     |
| 8      | 1000   | $4A       | 01001010     |
| 9      | 1001   | $49       | 01001001     |
| A      | 1010   | $44       | 01000100     |
| B      | 1011   | $45       | 01000101     |
| C      | 1100   | $52       | 01010010     |
| D      | 1101   | $51       | 01010001     |
| E      | 1110   | $54       | 01010100     |
| F      | 1111   | $55       | 01010101     |

Note: these values assume the preceding clock context is 0. The first clock bit may differ depending on the previous data.


---

## 2. Track Layout (Byte-by-Byte)

### 2.1 Overview

A standard Amiga 3.5" DD track contains **11 sectors** of **512 bytes** each. The track is a continuous loop of MFM-encoded data that the disk controller reads as the platter spins.

### 2.2 Sector Size Constant

From vAmiga (`AmigaEncoder.cpp:18-19`):

```cpp
static constexpr isize bsize  = 512;  // Block size in bytes
static constexpr isize ssize  = 1088; // MFM sector size in bytes
```

Each sector occupies **1,088 MFM bytes** (544 MFM words). This breaks down as:

| Component              | Byte Offset | Size (bytes) | Size (words) |
|------------------------|-------------|-------------- |--------------|
| Pre-sync gap           | 0           | 4             | 2            |
| Sync words             | 4           | 4             | 2            |
| Track & sector info    | 8           | 8             | 4            |
| OS recovery / reserved | 16          | 32            | 16           |
| Header checksum        | 48          | 8             | 4            |
| Data checksum          | 56          | 8             | 4            |
| Sector data            | 64          | 1,024         | 512          |
| **Total**              |             | **1,088**     | **544**      |

### 2.3 Sector Layout in Detail

From vAmiga (`AmigaEncoder.cpp:74-138`), with the layout comment at line 76:

```
Block header layout:

                         Start  Size   Value
    Bytes before SYNC    00      4     0xAA 0xAA 0xAA 0xAA
    SYNC mark            04      4     0x44 0x89 0x44 0x89
    Track & sector info  08      8     Odd/Even encoded
    Unused area          16     32     0xAA
    Block checksum       48      8     Odd/Even encoded
    Data checksum        56      8     Odd/Even encoded
    Sector data          64   1024     Odd/Even encoded
```

#### 2.3.1 Pre-Sync Gap (Bytes 0-3)

Four bytes of `$AA`. This is the MFM idle pattern (all clock bits set, all data bits clear). It provides a buffer zone between sectors for timing tolerance and ensures the sync detector has a clean run-in.

```
$AA $AA $AA $AA
```

In the first sector after the track gap, the first byte may be `$2A` instead of `$AA` depending on whether the last bit of the previous sector was 1 (this affects the first clock bit). vAmiga notes this edge case in a comment at `AmigaEncoder.cpp:88` but uses `$AA` unconditionally and later fixes it with `rectifyClockBit()`.

#### 2.3.2 Sync Words (Bytes 4-7)

Two copies of the sync word `$4489`, totalling 4 bytes:

```
$44 $89 $44 $89
```

The disk controller continuously scans the incoming bit stream for this pattern. When the DSKSYNC register matches and WORDSYNC is enabled in ADKCON, DMA transfer begins.

WinUAE confirms (`disk.cpp:2206-2207`):

```cpp
mfmbuf[2] = mfmbuf[3] = 0x4489;
```

#### 2.3.3 Track and Sector Info (Bytes 8-15)

A 4-byte info block, odd/even encoded into 8 MFM bytes.

The 4 raw bytes are:

| Byte | Content                          | Value                |
|------|----------------------------------|----------------------|
| 0    | Format type                      | `$FF` (AmigaDOS)     |
| 1    | Track number                     | 0-159                |
| 2    | Sector number                    | 0-10 (DD), 0-21 (HD)|
| 3    | Sectors until gap                | 11 - sector_number   |

From vAmiga (`AmigaEncoder.cpp:101-102`):

```cpp
u8 info[4] = { 0xFF, (u8)t, (u8)s, (u8)(11 - s) };
MFM::encodeOddEven(&it[8], info, sizeof(info));
```

WinUAE (`disk.cpp:2193-2198`):

```cpp
secbuf[4] = 0xff;
secbuf[5] = tr;       // track number (0-159)
secbuf[6] = sec;      // sector number (0-10)
secbuf[7] = drv->num_secs - sec;  // sectors to gap
```

After odd/even encoding, the 8 MFM bytes at offset 8 contain:

- Bytes 8-11: odd bits of the info longword
- Bytes 12-15: even bits of the info longword

WinUAE makes the odd/even split explicit (`disk.cpp:2209-2218`):

```cpp
deven = ((secbuf[4] << 24) | (secbuf[5] << 16)
    | (secbuf[6] << 8) | (secbuf[7]));
dodd = deven >> 1;
deven &= 0x55555555;
dodd &= 0x55555555;

mfmbuf[4] = dodd >> 16;   // MFM word at sector offset 4 (byte offset 8)
mfmbuf[5] = dodd;         //                              (byte offset 10)
mfmbuf[6] = deven >> 16;  //                              (byte offset 12)
mfmbuf[7] = deven;        //                              (byte offset 14)
```

#### 2.3.4 OS Recovery / Sector Label (Bytes 16-47)

32 bytes reserved for the operating system. Standard AmigaDOS sets these to `$00` (they encode as MFM `$AA` after odd/even + clock insertion). These bytes are sometimes called the "sector label" and were intended to hold OS-level metadata such as file system recovery information. In practice, almost no software uses them.

From vAmiga (`AmigaEncoder.cpp:104-106`):

```cpp
// Unused area
for (isize i = 16; i < 48; i++)
    it[i] = 0xAA;
```

WinUAE (`disk.cpp:2200-2201`):

```cpp
for (i = 8; i < 24; i++)
    secbuf[i] = 0;
```

WinUAE supports ADF files with non-zero sector headers via the `ADF_NORMAL_HEADER` format type, which stores 16 extra bytes per sector (`disk.cpp:2617-2630`). The sector header longwords are encoded with the same odd/even interleave as the info field.

**Note:** The "unused area" is 16 bytes of raw data (forming 4 longwords) that get odd/even encoded into 32 MFM bytes. The encoding produces 8 MFM words for odd bits + 8 MFM words for even bits = 16 MFM words = 32 MFM bytes.

#### 2.3.5 Header Checksum (Bytes 48-55)

A 4-byte checksum of the header, odd/even encoded into 8 MFM bytes.

The checksum is computed by XORing all MFM bytes from offset 8 through 47 (the info field and the OS recovery area), taken 4 bytes at a time:

From vAmiga (`AmigaEncoder.cpp:112-119`):

```cpp
u8 bcheck[4] = { 0, 0, 0, 0 };
for(isize i = 8; i < 48; i += 4) {
    bcheck[0] ^= it[i];
    bcheck[1] ^= it[i+1];
    bcheck[2] ^= it[i+2];
    bcheck[3] ^= it[i+3];
}
MFM::encodeOddEven(&it[48], bcheck, sizeof(bcheck));
```

WinUAE computes it over MFM words (`disk.cpp:2231-2233`):

```cpp
for (i = 4; i < 24; i += 2) {
    hck ^= (mfmbuf[i] << 16) | mfmbuf[i + 1];
}
```

This covers words 4-23 (byte offsets 8-47), matching vAmiga exactly. The result is then odd/even split into words 24-27 (byte offsets 48-55).

**Verification during decode:** The decoder XORs all header MFM longwords and checks for zero. If the checksum field is correct, the running XOR of words 4-27 should equal zero. WinUAE (`disk.cpp:2610-2640`) checks this explicitly.

#### 2.3.6 Data Checksum (Bytes 56-63)

A 4-byte checksum of the sector data, odd/even encoded into 8 MFM bytes.

The checksum covers the 1,024 MFM bytes of sector data (offset 64 through 1,087):

From vAmiga (`AmigaEncoder.cpp:122-129`):

```cpp
u8 dcheck[4] = { 0, 0, 0, 0 };
for(isize i = 64; i < ssize; i += 4) {
    dcheck[0] ^= it[i];
    dcheck[1] ^= it[i+1];
    dcheck[2] ^= it[i+2];
    dcheck[3] ^= it[i+3];
}
MFM::encodeOddEven(&it[56], dcheck, sizeof(dcheck));
```

WinUAE (`disk.cpp:2252-2254`):

```cpp
for (i = 32; i < 544; i += 2) {
    dck ^= (mfmbuf[i] << 16) | mfmbuf[i + 1];
}
```

Words 32-543 correspond to byte offsets 64-1087, matching vAmiga.

**Important ordering note:** The data checksum bytes (offset 56-63) appear *before* the data they checksum (offset 64-1087). This means the encoder must compute the data checksum *after* encoding the data, then place it at offset 56. The decoder reads data first, computes the checksum, and compares.

#### 2.3.7 Sector Data (Bytes 64-1087)

512 bytes of user data, odd/even encoded into 1,024 MFM bytes.

From vAmiga (`AmigaEncoder.cpp:109`):

```cpp
MFM::encodeOddEven(&it[64], bytes.data(), bsize);
```

The 512 data bytes produce:

- Bytes 64-575: odd bits of data (512 MFM bytes)
- Bytes 576-1087: even bits of data (512 MFM bytes)

In word terms (WinUAE, `disk.cpp:2241-2251`):

```cpp
for (i = 0; i < 512; i += 4) {
    deven = ((secbuf[i + 32] << 24) | (secbuf[i + 33] << 16)
        | (secbuf[i + 34] << 8) | (secbuf[i + 35]));
    dodd = deven >> 1;
    deven &= 0x55555555;
    dodd &= 0x55555555;
    mfmbuf[(i >> 1) + 32] = dodd >> 16;      // Odd half starts at word 32
    mfmbuf[(i >> 1) + 33] = dodd;
    mfmbuf[(i >> 1) + 256 + 32] = deven >> 16; // Even half starts at word 288
    mfmbuf[(i >> 1) + 256 + 33] = deven;
}
```

Odd bits occupy MFM words 32-287 (256 words = 512 bytes), even bits occupy words 288-543 (256 words = 512 bytes). Total: 512 words = 1,024 bytes.

#### 2.3.8 Clock Bit Insertion

After all the above components are assembled with their odd/even encoded data, clock bits are added to everything after the sync mark. The sync mark itself is written verbatim (it deliberately violates clock rules).

From vAmiga (`AmigaEncoder.cpp:132-135`):

```cpp
for(isize i = 8; i < ssize; i++) {
    it[i] = MFM::addClockBits(it[i], it[i-1]);
}
```

Clock bits are *not* added to bytes 0-7 (the gap and sync mark). The gap is already `$AA` (valid MFM idle), and the sync mark is the special violation pattern.

### 2.4 Complete Track Layout

#### 2.4.1 DD Track (11 sectors)

```
Track total: 11 x 1,088 + gap = 12,668 MFM bytes
Sector data: 11 x 1,088 = 11,968 MFM bytes
Track gap:   12,668 - 11,968 = 700 MFM bytes (varies slightly)
```

From vAmiga (`FloppyDisk.h:63-70`):

```
A single track of a 3.5"DD disk consists
  - 11 * 1088 = 11,968 MFM bytes.
  - A track gap of about 700 MFM bytes (varies with drive speed).

Hence,
  - a track usually occupies 11,968 + 700 = 12,668 MFM bytes.
```

WinUAE (`disk.cpp:78-84`):

```cpp
#define FLOPPY_WRITE_LEN_PAL  12668  // 3546895 / (7 * 5 * 8)
#define FLOPPY_WRITE_LEN_NTSC 12784  // 3579545 / (7 * 5 * 8)
// ...
#define FLOPPY_GAP_LEN (FLOPPY_WRITE_LEN - 11 * 544)
```

The gap length depends on the video standard:

| Standard | Track Length (bytes) | Track Length (words) | Gap Length (bytes) | Gap Length (words) |
|----------|---------------------|---------------------|--------------------|--------------------|
| PAL      | 12,668              | 6,334               | 700                | 350                |
| NTSC     | 12,784              | 6,392               | 816                | 408                |

**Note on units:** WinUAE's `FLOPPY_WRITE_LEN` constants represent the track length in **MFM words** (16 bits each), hence the `/ 2` seen in some calculations. The `544` in `11 * 544` is the sector size in MFM words. vAmiga's constants are in **bytes**.

#### 2.4.2 HD Track (22 sectors)

```
Track total: 22 x 1,088 + gap = 24,636 MFM bytes (approximately)
```

From vAmiga (`FloppyDisk.cpp:37`):

```cpp
if (dia == Diameter::INCH_35 && den == Density::HD) numTrackBytes = 24636;
```

And the encoder (`AmigaEncoder.cpp:35`):

```cpp
auto trackBytes = count == 11 ? 12668 : 24636;
```

#### 2.4.3 ASCII Track Diagram (DD, 11 Sectors)

```
|<-- ~700 bytes gap -->|<-------- Sector 0 (1088 bytes) -------->|
|  $AA $AA $AA ... $AA |  Gap  Sync   Header   Data              |

Sector detail (1088 bytes = 544 words):

Offset  Size  Content
------  ----  -------------------------------------------------------
  0       4   Inter-sector gap: $AA $AA $AA $AA
  4       4   Sync mark: $44 $89 $44 $89
  8       8   Sector info (odd/even encoded): format, track, sector, gap_count
 16      32   OS recovery / sector label (normally all $AA)
 48       8   Header checksum (odd/even encoded)
 56       8   Data checksum (odd/even encoded)
 64    1024   Sector data (512 bytes, odd/even encoded into 1024 MFM bytes)

Full track layout:

+----------+----------+----------+     +----------+----------+
|          | Sector 0 | Sector 1 | ... | Sector 10| Track    |
| Pre-gap  | 1088 B   | 1088 B   |     | 1088 B   | Gap/Fill |
| ~700 B   |          |          |     |          |          |
+----------+----------+----------+     +----------+----------+
|<----- Total: 12,668 bytes (PAL) or 12,784 bytes (NTSC) --->|
```

The "pre-gap" at the start and the remaining space after the last sector are filled with `$AA` bytes. The gap before the first sector and the gap after the last sector together make up the total gap.

WinUAE positions the gap before the first sector (`disk.cpp:2175-2181`):

```cpp
int len = drv->num_secs * 544 + FLOPPY_GAP_LEN;
memset(dstmfmbuf, 0xaa, len * 2);
dstmfmoffset += FLOPPY_GAP_LEN;  // sectors start after the gap
```

#### 2.4.4 Sector Ordering

Sectors are laid out sequentially from sector 0 through sector 10 within each track. The physical order on the media matches the logical sector number. There is no sector interleaving in standard AmigaDOS format.

The "sectors until gap" field (byte 3 of the info block) counts down from 11 for sector 0 to 1 for sector 10, indicating how many more sectors follow before the track gap.


---

## 3. Track Geometry

### 3.1 Physical Layout

| Parameter               | DD (3.5")        | HD (3.5")        | DD (5.25")       |
|-------------------------|------------------|------------------|------------------|
| Cylinders (standard)    | 80               | 80               | 42               |
| Cylinders (maximum)     | 84               | 84               | 42               |
| Heads (sides)           | 2                | 2                | 2                |
| Tracks (standard)       | 160              | 160              | 84               |
| Tracks (maximum)        | 168              | 168              | 84               |
| Sectors per track       | 11               | 22               | 11               |
| Bytes per sector        | 512              | 512              | 512              |
| User data per track     | 5,632            | 11,264           | 5,632            |
| Total capacity          | 880 KB           | 1,760 KB         | 440 KB           |
| MFM track length        | 12,668 bytes     | 24,636 bytes     | 12,668 bytes     |
| MFM bits per track      | 101,344          | 197,088          | 101,344          |

From vAmiga (`FloppyDisk.h:179-181`):

```cpp
isize numCyls() const override { return diameter == Diameter::INCH_525 ? 42 : 84; }
isize numHeads() const override { return 2; }
isize numSectors(isize t) const override { return density == Density::DD ? 11 : 22; }
```

From vAmiga (`FloppyDisk.cpp:36-38`):

```cpp
if (dia == Diameter::INCH_35  && den == Density::DD) numTrackBytes = 12668;
if (dia == Diameter::INCH_35  && den == Density::HD) numTrackBytes = 24636;
if (dia == Diameter::INCH_525 && den == Density::DD) numTrackBytes = 12668;
```

### 3.2 Track Numbering

Tracks are numbered linearly from 0 to 167:

```
Track = Cylinder * 2 + Head
```

From vAmiga (`FloppyDisk.h:38-55`):

```
Cylinder  Track     Head      Sectors
---------------------------------------
0         0         0          0 - 10
0         1         1         11 - 21
1         2         0         22 - 32
1         3         1         33 - 43
                  ...
79        158       0       1738 - 1748
79        159       1       1749 - 1759

80        160       0       1760 - 1770   <--- beyond standard spec
80        161       1       1771 - 1781
                  ...
83        166       0       1826 - 1836
83        167       1       1837 - 1847
```

Cylinders 80-83 are physically accessible on most drives but are not part of the standard 880 KB format. Some copy-protected software and the Extended ADF format use these extra cylinders.

### 3.3 Bit Rate and Timing

For a standard DD drive at 300 RPM:

| Parameter                | Value              | Derivation                                   |
|--------------------------|--------------------|----------------------------------------------|
| Rotation speed           | 300 RPM            | Standard for 3.5" DD                         |
| Revolutions per second   | 5                  | 300 / 60                                     |
| Rotation period          | 200 ms             | 1 / 5 Hz                                     |
| Bit cell period          | ~2 µs              | 1 / (data rate)                              |
| MFM data rate (PAL)      | ~506 kbit/s        | 12,668 × 8 × 5 = 506,720                    |
| MFM clock rate           | 7 CCK per bit cell | From WinUAE (`disk.cpp:86`)                  |
| Byte transfer period     | ~15.8 µs           | 200 ms / 12,668 bytes                        |
| DMA clock (PAL)          | 3,546,895 Hz       | System clock / 2                             |
| Bytes per revolution     | 12,668 (PAL)       | DMA_clock / (7 × 5 × 8)                     |

From WinUAE (`disk.cpp:78-86`):

```cpp
/* Writable track length with normal 2us bitcell/300RPM motor */
/* DMA clock / (7 clocks per bit * 5 revs per second * 8 bits per byte) */
#define FLOPPY_WRITE_LEN_PAL 12668   // 3546895 / (7 * 5 * 8)
#define FLOPPY_WRITE_LEN_NTSC 12784  // 3579545 / (7 * 5 * 8)
/* 7 CCK per bit */
#define NORMAL_FLOPPY_SPEED (7 * 256)
```

From vAmiga (`DiskControllerEvents.cpp:43-55`):

```cpp
static constexpr double bytesPerTrack = 12668.0;

// How many revolutions per minute?
isize rpm = drive ? drive->config.rpm : 300;

// Compute the time span between two incoming bytes
double delay = (SEC(1) * 60.0) / (double)rpm / bytesPerTrack;
```

### 3.4 Mechanical Timing

From vAmiga (`FloppyDrive.cpp:609-685`):

| Parameter                    | A1010 Drive       | No Mechanics (turbo) |
|------------------------------|--------------------|-----------------------|
| Motor start delay            | 380 ms             | 0                     |
| Motor stop delay             | 80 ms              | 0                     |
| Step pulse delay             | 40 µs              | 0                     |
| Reverse step pulse delay     | 40 µs              | 0                     |
| Track-to-track delay         | 3 ms               | 0                     |
| Head settle time             | 9 ms               | 0                     |
| **Total seek + settle**      | **12 ms per step** | **0**                 |

The time to complete a step is `track-to-track delay + head settle time` = 3 ms + 9 ms = 12 ms. From `FloppyDrive.cpp:942`:

```cpp
latestStepCompleted = agnus.clock + getTrackToTrackDelay() + getHeadSettleTime();
```

During the settle period, reads return random data (`FloppyDrive.cpp:802`):

```cpp
if (agnus.clock < latestStepCompleted) return u8(amiga.random() & 0x55);
```

### 3.5 Index Pulse

Each disk revolution produces one index pulse. In vAmiga, crossing the end of the track triggers a CIA B flag pin falling edge, which generates an interrupt if enabled (`FloppyDrive.cpp:848-863`):

```cpp
void FloppyDrive::rotate()
{
    long last = disk ? disk->track[head.track()].size() : 12668 * 8;
    head.offset += 8;

    if (head.offset >= last) {
        head.offset %= 8;
        if (isSelected()) ciab.emulateFallingEdgeOnFlagPin();
    }
}
```

### 3.6 Drive Identification

Drives identify themselves through bit patterns read from the RDY line while the motor is stopped or spinning up (`FloppyDrive.cpp:564-595`):

| Drive Type           | ID Pattern     |
|----------------------|---------------|
| Internal (df0)       | `$00000000`   |
| External 3.5" DD     | `$FFFFFFFF`   |
| External 3.5" HD     | `$AAAAAAAA` (HD disk) / `$FFFFFFFF` (DD/empty) |
| External 5.25" SD    | `$55555555`   |


---

## 4. Disk DMA

### 4.1 Registers

| Register  | Address | R/W | Name     | Purpose                                   |
|-----------|---------|-----|----------|-------------------------------------------|
| DSKDATR   | $008    | R   | -        | Disk data read (strobe, not CPU-accessible)|
| DSKLEN    | $024    | W   | -        | DMA length and control                    |
| DSKDAT    | $026    | W   | -        | Disk data write                           |
| DSKBYTR   | $01A    | R   | -        | Disk byte and status                      |
| DSKSYNC   | $07E    | W   | -        | Disk sync pattern                         |
| ADKCON    | $09E    | W   | -        | Audio/disk control (bit 10 = WORDSYNC)    |

### 4.2 DSKLEN Register ($024, Write)

```
Bit 15     DMAEN    DMA enable
Bit 14     WRITE    Direction (1 = write to disk, 0 = read from disk)
Bits 13-0  LENGTH   Number of words to transfer
```

**Double-write safety:** DMA is only armed when bit 15 (DMAEN) is written as 1 *twice in succession*. A single write with bit 15 set does not start DMA. Writing bit 15 as 0 at any point disables DMA immediately.

For write DMA, bit 14 must also be set in both writes.

From vAmiga (`DiskControllerRegs.cpp:35-98`):

```cpp
void DiskController::setDSKLEN(u16 oldValue, u16 newValue)
{
    dsklen = newValue;

    // Disable DMA if bit 15 (DMAEN) is zero
    if (!(newValue & 0x8000)) {
        setState(DriveDmaState::OFF);
        clearFifo();
    }

    // Enable DMA if bit 15 (DMAEN) has been written twice
    if (oldValue & newValue & 0x8000) {

        if ((dsklen & 0x3FFF) == 0) { paula.raiseIrq(IrqSource::DSKBLK); return; }

        // Check if the WRITE bit (bit 14) also has been written twice
        if (oldValue & newValue & 0x4000) {
            setState(DriveDmaState::WRITE);
        } else {
            // Check the WORDSYNC bit in the ADKCON register
            if (GET_BIT(paula.adkcon, 10)) {
                setState(DriveDmaState::WAIT);   // Wait for sync
            } else {
                setState(DriveDmaState::READ);   // Start immediately
            }
        }
        clearFifo();
    }
}
```

WinUAE (`disk.cpp:4844-4908`) handles the same logic with more edge cases for compatibility (re-writes during active DMA, zero-length transfers, etc.).

### 4.3 DSKSYNC Register ($07E, Write)

Contains the 16-bit sync pattern that the controller searches for. Defaults to `$4489` on reset.

From vAmiga (`DiskController.cpp:37`):

```cpp
dsksync = 0x4489;
```

When the incoming bit stream matches DSKSYNC:

1. A DSKSYN interrupt (level 6 / bit 12 of INTREQ) is raised.
2. If the controller is in WAIT state (WORDSYNC enabled in ADKCON), DMA transitions to READ state.

From vAmiga (`DiskController.cpp:307-328`):

```cpp
void DiskController::readBit(bool bit)
{
    dataReg = (u16)((u32)dataReg << 1 | (u32)bit);

    if (++dataRegCount == 8) {
        writeFifo((u8)dataReg);
        dataRegCount = 0;
    }

    if (dataReg == dsksync || (config.autoDskSync && syncCounter++ > 8*20000)) {
        syncCycle = agnus.clock;
        paula.raiseIrq(IrqSource::DSKSYN);

        if (state == DriveDmaState::WAIT) {
            dataRegCount = 0;
            clearFifo();
            setState(DriveDmaState::READ);
        }
        syncCounter = 0;
    }
}
```

### 4.4 DSKBYTR Register ($01A, Read)

Provides status for polled I/O (non-DMA) disk access.

```
Bit 15     DSKBYT     Byte ready (set when a complete byte has been received)
Bit 14     DMAON      DMA is actually running (DSKLEN armed + DMACON enabled)
Bit 13     DISKWRITE  Matches the WRITE bit in DSKLEN
Bit 12     WORDEQUAL  DSKSYNC match detected (within last ~2 µs)
Bits 11-8             Unused
Bits 7-0   DATA       Last byte received from disk
```

From vAmiga (`DiskControllerRegs.cpp:119-143`):

```cpp
u16 DiskController::computeDSKBYTR() const
{
    u16 result = incoming;  // Bits 15 (DSKBYT) + 7-0 (DATA)

    // DMAON
    if (agnus.dskdma() && state != DriveDmaState::OFF) SET_BIT(result, 14);

    // DSKWRITE
    if (dsklen & 0x4000) SET_BIT(result, 13);

    // WORDEQUAL
    if (agnus.clock - syncCycle <= USEC(2)) SET_BIT(result, 12);

    return result;
}
```

Reading DSKBYTR clears the DSKBYT flag (bit 15) so the next read won't see it until a new byte arrives.

### 4.5 DMA Slots and Timing

The Amiga allocates **3 DMA slots per scan line** for disk transfers (slots DAS_D0, DAS_D1, DAS_D2). Each slot transfers one word (2 bytes). From vAmiga (`AgnusEvents.cpp:609-614`):

```cpp
case DAS_D0:
case DAS_D1:
case DAS_D2:
    paula.diskController.performDMA();
    break;
```

At 3 words per scan line and ~312 lines per frame (PAL):

- **Words per frame:** ~936
- **Bytes per frame:** ~1,872
- **Words per revolution (5 frames at 50 Hz):** ~4,680

This is more than enough to handle a full track of data (6,334 words for DD) within one disk revolution.

### 4.6 DMA Flow

#### Read DMA

1. Software writes DSKPT (disk DMA pointer) to the target buffer address.
2. Software writes DSKLEN twice with bit 15 set and bit 14 clear.
3. If WORDSYNC is enabled in ADKCON (bit 10), the controller enters WAIT state and scans for the DSKSYNC pattern.
4. On sync match, DMA enters READ state. The controller's FIFO fills from disk, and each DMA slot moves one word from the FIFO to the memory address in DSKPT.
5. DSKPT advances by 2 after each word transfer.
6. When the transfer count reaches zero, a DSKBLK interrupt is raised and DMA stops.

#### Write DMA

1. Software writes DSKPT to the source buffer address.
2. Software writes DSKLEN twice with both bit 15 and bit 14 set.
3. The controller enters WRITE state immediately (no sync waiting).
4. Each DMA slot reads one word from memory at DSKPT and pushes it into the FIFO.
5. The FIFO content is written to disk as the drive head rotates.
6. When the transfer count reaches zero, a DSKBLK interrupt is raised. The remaining FIFO content is flushed to disk.

From vAmiga (`DiskController.cpp:457-475`):

```cpp
// Flush the FIFO immediately
while (!fifoIsEmpty()) {
    u8 value = readFifo();
    if (drive) drive->write8AndRotate(value);
}
setState(DriveDmaState::OFF);
```

### 4.7 FIFO Buffer

The disk controller uses a FIFO buffer to decouple the disk rotation timing from the DMA slot timing. The FIFO holds up to 6 bytes. On each `DSK_ROTATE` event (timed to match the disk byte rate), one byte is transferred between the selected drive and the FIFO. On each DMA slot, one word (2 bytes) is transferred between the FIFO and memory.

From vAmiga (`DiskController.h:309-313`):

```cpp
bool fifoIsEmpty() const { return fifoCount == 0; }
bool fifoIsFull() const { return fifoCount == 6; }
bool fifoHasWord() const { return fifoCount >= 2; }
bool fifoCanStoreWord() const { return fifoCount <= 4; }
```

The FIFO serves as a 3-word buffer between two asynchronous processes:

1. **Disk side:** Bytes arrive from the disk at the rotation rate (~15.8 us per byte at 300 RPM). Each `DSK_ROTATE` event calls `transferByte()`, which reads one byte from the drive and pushes it into the FIFO.

2. **DMA side:** Three times per scan line (roughly every 7.1 us for PAL), the DMA engine calls `performDMA()`, which pulls one word (2 bytes) from the FIFO and writes it to Chip RAM via the DSKPT pointer.

If the FIFO overflows (more than 6 bytes), the oldest word is silently discarded. If the FIFO underflows during a DMA slot (fewer than 2 bytes available), the DMA slot is skipped and the word is transferred on the next slot. This self-correcting behaviour handles minor timing variations between the disk rotation and the DMA clock.

### 4.8 Typical Read DMA Sequence

A standard Amiga trackdisk.device read proceeds as follows:

```
1. CPU seeks drive head to desired cylinder
   - Write to CIA B PRB register to select drive and set step direction
   - Pulse the step line via CIA B PRB
   - Wait for head settle time (~12 ms)

2. CPU sets up DMA
   - Write buffer address to DSKPT ($020/$022, high/low words)
   - Write $9900 to DSKLEN ($024)    -- DMAEN=1, LENGTH=0x1900 = 6400 words
   - Write $9900 to DSKLEN ($024)    -- second write arms DMA

3. Controller enters WAIT state (WORDSYNC is enabled in ADKCON)
   - Incoming bits are shifted into the data register
   - Controller scans for DSKSYNC match ($4489)

4. Sync detected
   - DSKSYN interrupt fires (INT level 6, bit 12 of INTREQ)
   - Controller clears FIFO, transitions to READ state
   - From this point, bytes go into FIFO

5. DMA transfer
   - Each of the 3 DMA slots per scan line pulls a word from FIFO
   - Word is written to memory at DSKPT, then DSKPT += 2
   - Transfer counter decrements by 1

6. Transfer complete
   - When counter reaches 0, DSKBLK interrupt fires (bit 1 of INTREQ)
   - Controller returns to OFF state
   - CPU processes the MFM data in the buffer
```

The DMA length for a standard track read is typically `$1900` (6,400 words = 12,800 bytes), which is slightly more than one full track (6,334 words for PAL). This ensures the entire track is captured regardless of where the head happens to be when DMA starts.

### 4.9 Acceleration

vAmiga supports a configurable speed multiplier (`DiskControllerTypes.h:76-83`):

```cpp
/* Acceleration factor. This value equals the number of words that get
 * transferred into memory during a single disk DMA cycle. This value must
 * be 1 to emulate a real Amiga. If it set to, e.g., 2, the drive loads
 * twice as fast. A value of -1 indicates a turbo drive.
 */
i32 speed;
```

Valid values are -1 (turbo), 1, 2, 4, or 8. In turbo mode, the entire DMA transfer is completed instantly when DSKLEN is written, bypassing the per-scanline DMA slot mechanism entirely (`DiskController.cpp:492-530`).


---

## 5. ADF File Format

### 5.1 Standard ADF

A standard ADF file is a raw dump of sector data, stored in track order (track 0 first, track 159 last), with sectors in order within each track (sector 0 first, sector 10 last). No MFM encoding, no sync marks, no checksums -- just the 512-byte payload of each sector.

**File size:**

```
80 cylinders × 2 heads × 11 sectors × 512 bytes = 901,120 bytes (880 KB)
```

From vAmiga (`ADFFile.h:33-38`):

```cpp
static constexpr isize ADFSIZE_35_DD    = 901120;   //  880 KB
static constexpr isize ADFSIZE_35_DD_81 = 912384;   //  891 KB (+ 1 cyl)
static constexpr isize ADFSIZE_35_DD_82 = 923648;   //  902 KB (+ 2 cyls)
static constexpr isize ADFSIZE_35_DD_83 = 934912;   //  913 KB (+ 3 cyls)
static constexpr isize ADFSIZE_35_DD_84 = 946176;   //  924 KB (+ 4 cyls)
static constexpr isize ADFSIZE_35_HD    = 1802240;  // 1760 KB
```

**Extended cylinder ADFs:** Some ADFs include data for cylinders 80-83 (used by copy protection), resulting in file sizes of 912,384 to 946,176 bytes.

**HD ADF:** An HD floppy ADF is 1,802,240 bytes (80 × 2 × 22 × 512).

### 5.2 ADF to MFM Conversion

When an emulator loads an ADF file, it must convert the raw sector data into MFM-encoded tracks. The process for each track:

1. Start with a blank track filled with `$AA` (MFM idle pattern).
2. For each of the 11 sectors:
   a. Build the sector header (format byte, track, sector, gap count).
   b. Read 512 bytes from the ADF at the appropriate offset.
   c. Odd/even encode the header info, sector labels, and data.
   d. Compute header and data checksums over the encoded MFM.
   e. Odd/even encode the checksums.
   f. Add clock bits to everything after the sync mark.
   g. Write the sync mark (`$4489 $4489`) verbatim.
3. Fix clock bits at sector boundaries.

### 5.3 MFM to ADF Conversion

When writing back to an ADF file, the emulator must decode MFM tracks into raw sector data:

1. Scan the MFM bit stream for sync word `$4489`.
2. After sync, decode the header (odd/even → info bytes).
3. Extract the sector number from the header.
4. Verify the header checksum.
5. Decode the data area (odd/even → 512 raw bytes).
6. Verify the data checksum.
7. Write the 512 bytes to the correct ADF offset for that track and sector.

This is shown in WinUAE's `decode_buffer()` (`disk.cpp:2553-2690`), which handles both standard and extended ADF formats.

### 5.4 Extended ADF (EADF / UAE-1ADF)

The Extended ADF format preserves raw MFM data, allowing non-standard track formats (copy protection schemes) to be stored accurately.

From vAmiga (`EADFFile.h:16-43`):

```
File layout:

  1. Header section:

     8 bytes : ASCII signature "UAE-1ADF"
     2 bytes : Reserved
     2 bytes : Number of tracks (typically 2 x 80 = 160)

  2. Track header section (one entry per track):

     2 bytes : Reserved
     2 bytes : Track type
               0 = Standard AmigaDOS track
               1 = Raw MFM data (upper byte = number of disk revolutions - 1)
     4 bytes : Available space for track in bytes (must be even)
     4 bytes : Track length in bits

  3. Track data section:

     Raw track data for each track, stored consecutively.
```

Total header size: `12 + (12 × number_of_tracks)` bytes.

**Track types:**

- **Type 0 (standard):** The track data section contains raw sector data (like a standard ADF). The emulator MFM-encodes it on load.
- **Type 1 (raw MFM):** The track data section contains the raw MFM bit stream. The upper byte of the type field indicates the number of disk revolutions stored minus one (for multi-revolution captures). The "track length in bits" field specifies the exact bit count, which may not be byte-aligned.

From WinUAE (`disk.cpp:99-107`):

```
/* UAE-1ADF (ADF_EXT2)
* W reserved
* W number of tracks (default 2*80=160)
*
* W reserved
* W type, 0=normal AmigaDOS track, 1=raw MFM (upper byte = disk revolutions - 1)
* L available space for track in bytes (must be even)
* L track length in bits
*/
```

**Note:** An older format with signature "UAE--ADF" (ADF_EXT1) exists. It was created by Factor 5 for Turrican disk images. Neither vAmiga nor WinUAE actively supports it for new files. vAmiga explicitly rejects it (`EADFFile.cpp:90-94`).

### 5.5 ADF with Sector Headers

WinUAE supports a variant (`ADF_NORMAL_HEADER`) that stores 16 extra bytes per sector for the OS recovery / sector label area. The file size is `880 × 2 × (512 + 16)` bytes. This is used when the sector labels contain non-zero data that needs to be preserved.


---

## 6. Copy Protection Techniques

### 6.1 Overview

Amiga copy protection exploited the fact that the standard floppy controller can read and write arbitrary MFM data -- there is no hard-wired format like on IBM PC drives. This gave game publishers many ways to create disks that a standard copy program (which assumes AmigaDOS format) could not duplicate.

### 6.2 Non-Standard Sector Counts

A track can be formatted with fewer or more than 11 sectors. The "sectors until gap" field and the actual number of sync marks on the track can differ from the standard. Copy protection checks would verify the exact sector count.

Because the Amiga uses no fixed sector format at the hardware level (unlike the IBM PC's NEC 765 controller), software can write any number of sectors to a track. Common variations:

- **Fewer sectors (e.g., 10):** Leaves a larger gap, which might contain a signature pattern.
- **More sectors (e.g., 12):** Possible on slightly long tracks or by shrinking the gap to near-zero. The protection check reads all 12 sectors to verify the extra one exists.
- **Zero sectors:** The track contains only raw MFM data with no valid sync marks at all. The protection code reads the track via polled I/O (DSKBYTR) rather than DMA.

WinUAE uses a `DiskSpare` format (`disk.cpp:2279-2364`) that is an example of a non-standard sector layout: it uses a different header structure with only 4 bytes of header per sector and a 16-bit CRC instead of the AmigaDOS header/data checksum pair.

### 6.3 Long and Short Tracks

The physical track length is determined by the drive rotation speed and data rate, not by any format header. Protection schemes used tracks that were slightly longer or shorter than standard:

- **Long tracks:** Written at a slightly lower data rate or on drives with slightly slower rotation, squeezing more data onto a track. A standard copy would truncate the extra data.
- **Short tracks:** Written with fewer bytes than expected, causing read errors in specific positions.

WinUAE's `FLOPPY_WRITE_MAXLEN` (`disk.cpp:82`) sets the maximum track length at `0x3800` words (28,672 bytes), accommodating long tracks.

### 6.4 Unformatted Areas

Some protection schemes left portions of the track unformatted (random magnetic data). The protection check would verify that reading these areas produces unpredictable values.

vAmiga initialises unformatted disk areas with random data (`FloppyDisk.cpp:297-300`):

```cpp
void FloppyDisk::clearDisk()
{
    srand(0);
    for (usize i = 0; i < sizeof(data.raw); i++) {
        data.raw[i] = rand() & 0xFF;
    }
}
```

It also injects a specific magic value for at least one known title (`FloppyDisk.cpp:302-311`):

```cpp
/* In order to make some copy protected game titles work, we smuggle in
 * some magic values. E.g., Crunch factory expects 0x44A2 on cylinder 80.
 */
if (diameter == Diameter::INCH_35 && density == Density::DD) {
    for (isize t = 0; t < numTracks(); t++) {
        data.track[t][0] = 0x44;
        data.track[t][1] = 0xA2;
    }
}
```

### 6.5 Custom Sync Words

The DSKSYNC register can be changed to a non-standard value. Protection schemes would write tracks with unusual sync markers and then program DSKSYNC to match before reading.

Both emulators flag this. vAmiga (`DiskControllerRegs.cpp:150-159`):

```cpp
void DiskController::pokeDSKSYNC(u16 value)
{
    if (value != 0x4489) {
        xfiles("DSKSYNC: Unusual sync mark $%04X\n", value);
        if (config.lockDskSync) {
            return;  // Block the write
        }
    }
    dsksync = value;
}
```

vAmiga provides a `lockDskSync` option that prevents games from changing the sync word, which can help with compatibility but breaks protection checks.

### 6.6 Fuzzy Bits and Weak Sectors

Some protection schemes rely on bits that are deliberately written at the threshold of readability. Each time the disk is read, these bits may resolve differently, producing unpredictable values. A bit-perfect copy would always read the same value, revealing itself as a copy.

**How they work on real hardware:** A "fuzzy" bit is created by writing a flux transition with ambiguous timing -- exactly between two valid positions. The read head's analog circuitry resolves this transition to one position or the other depending on noise, temperature, and slight speed variations. Each read of the same track yields different values for the fuzzy bits.

**Emulator handling:** Preserving fuzzy bit behaviour requires either:

1. **Multi-revolution captures:** The EADF format stores multiple revolutions of the same track (indicated by revolution count > 1 in the track type field). Each revolution captures the bits as they were read in one specific pass. The emulator cycles through revolutions to simulate the variability.

2. **Explicit weak-bit markers:** The IPF (Interchangeable Preservation Format) and SCP (SuperCard Pro) formats explicitly mark regions containing weak bits. The emulator generates random data for these regions on each read. WinUAE's `caps_loadtrack()` and `scp_loadtrack()` functions (`disk.cpp:2439-2447`) handle this.

3. **Random fill for unformatted areas:** Both emulators fill unformatted track data with random bytes, which naturally produces different data on each read -- mimicking the behaviour of genuinely unformatted disk areas.

**Practical example:** The protection check on a game like "Dungeon Master" reads a specific track multiple times and compares the results. If the values are identical each time, the disk is a copy and the game refuses to run. If the values differ (as they would on the original disk with fuzzy bits), the game proceeds.

### 6.7 Extra Cylinders

Cylinders 80-83 are physically accessible but outside the standard format. Protection code can seek to these cylinders and verify specific data patterns. The standard ADF format (901,120 bytes) cannot store this data; the extended cylinder ADFs (up to 946,176 bytes) and the EADF format can.

### 6.8 How WinUAE Handles Non-Standard Disks

WinUAE supports multiple disk image formats for different levels of preservation:

| Format    | Type Constant   | MFM Preserved | Multi-Rev | Copy Prot Support |
|-----------|-----------------|---------------|-----------|-------------------|
| ADF       | `ADF_NORMAL`    | No            | No        | None              |
| ADF+hdr   | `ADF_NORMAL_HEADER` | No       | No        | Sector labels     |
| EADF      | `ADF_EXT2`      | Yes           | Yes       | Good              |
| IPF/CAPS  | `ADF_IPF`       | Yes           | Yes       | Excellent         |
| SCP       | `ADF_SCP`       | Yes           | Yes       | Excellent         |
| FDI       | `ADF_FDI`       | Yes           | Yes       | Good              |

When writing to a standard ADF, WinUAE attempts to decode the MFM track back into AmigaDOS sectors (`drive_write_adf_amigados`). If decoding fails (non-standard format), it can optionally convert the ADF to an extended ADF to preserve the raw MFM data (`convert_adf_to_ext2`, `disk.cpp:2863-2913`).

For turbo DMA mode, WinUAE skips non-standard ADF files to avoid compatibility issues (`disk.cpp:4991-4994`):

```cpp
if (drv->filetype != ADF_NORMAL && drv->filetype != ADF_KICK
    && drv->filetype != ADF_SKICK && drv->filetype != ADF_NORMAL_HEADER)
    break;
// ...
if (dr < MAX_FLOPPY_DRIVES) /* no turbo mode if non-standard ADF */
    return;
```


---

## 7. Definitive Constants Table

| Constant                       | Value         | Source                           |
|--------------------------------|---------------|----------------------------------|
| MFM sync word                  | `$4489`       | Both                             |
| Sector size (data bytes)       | 512           | Both                             |
| MFM sector size (bytes)        | 1,088         | vAmiga `AmigaEncoder.cpp:19`     |
| MFM sector size (words)        | 544           | WinUAE `disk.cpp:2186`           |
| Sectors per track (DD)         | 11            | Both                             |
| Sectors per track (HD)         | 22            | Both                             |
| Track data (MFM bytes, DD)     | 11,968        | 11 × 1,088                      |
| Track gap (PAL, MFM bytes)     | 700           | 12,668 - 11,968                  |
| Track gap (NTSC, MFM bytes)    | 816           | 12,784 - 11,968                  |
| Track length (PAL, bytes)      | 12,668        | Both                             |
| Track length (NTSC, bytes)     | 12,784        | WinUAE `disk.cpp:80`             |
| Track length (HD, bytes)       | 24,636        | vAmiga `FloppyDisk.cpp:37`       |
| Track length (PAL, bits)       | 101,344       | 12,668 × 8                      |
| Tracks per disk (standard)     | 160           | 80 cyl × 2 heads                |
| Tracks per disk (maximum)      | 168           | 84 cyl × 2 heads                |
| ADF file size (DD)             | 901,120       | 512 × 11 × 2 × 80               |
| ADF file size (HD)             | 1,802,240     | 512 × 22 × 2 × 80               |
| Motor start time (A1010)       | 380 ms        | vAmiga `FloppyDrive.cpp:615`     |
| Motor stop time (A1010)        | 80 ms         | vAmiga `FloppyDrive.cpp:628`     |
| Track-to-track step (A1010)    | 3 ms          | vAmiga `FloppyDrive.cpp:667`     |
| Head settle time (A1010)       | 9 ms          | vAmiga `FloppyDrive.cpp:680`     |
| Step pulse delay               | 40 µs         | vAmiga `FloppyDrive.cpp:641`     |
| Rotation speed                 | 300 RPM       | Both                             |
| CCK per MFM bit cell           | 7             | WinUAE `disk.cpp:86`             |
| Disk DMA slots per scan line   | 3             | Both (DAS_D0, DAS_D1, DAS_D2)   |
| Pre-sync gap                   | 4 bytes `$AA` | vAmiga `AmigaEncoder.cpp:88-91`  |
| Format byte (AmigaDOS)         | `$FF`         | Both                             |
| Sector header reserved area    | 32 bytes `$AA`| 16 raw bytes, odd/even encoded   |
| EADF header signature          | "UAE-1ADF"    | Both                             |
| EADF per-track header size     | 12 bytes      | Both                             |


---

## 8. Worked Example: Encoding Sector 3 on Track 5

This section walks through encoding a single sector to show how all the pieces fit together.

### Input

- **Track:** 5 (cylinder 2, head 1)
- **Sector:** 3
- **Data:** 512 bytes of payload (assume all zeros for simplicity)

### Step 1: Assemble Raw Info

```
info[0] = $FF       (format type)
info[1] = $05       (track number)
info[2] = $03       (sector number)
info[3] = $08       (sectors until gap: 11 - 3 = 8)
```

As a 32-bit longword: `$FF050308`

### Step 2: Odd/Even Encode Info

The info longword is `$FF050308`:

```
Binary:  11111111 00000101 00000011 00001000

Odd bits  = (value >> 1) & $55555555:
  Binary:  01010101 00000000 00000001 00000100
  Hex:     $55 $00 $01 $04

Even bits = value & $55555555:
  Binary:  01010101 00000101 00000001 00000000
  Hex:     $55 $05 $01 $00
```

These 8 bytes go to sector offset 8-15:

```
Offset  8: $55 (odd byte 0)
Offset  9: $00 (odd byte 1)
Offset 10: $01 (odd byte 2)
Offset 11: $04 (odd byte 3)
Offset 12: $55 (even byte 0)
Offset 13: $05 (even byte 1)
Offset 14: $01 (even byte 2)
Offset 15: $00 (even byte 3)
```

### Step 3: Write Reserved Area

The 16-byte OS recovery area is all zeros. After odd/even encoding (16 raw bytes become 32 MFM bytes), all bits are 0 in data positions, yielding 32 bytes of `$00` at offsets 16-47. These will become `$AA` after clock bit insertion (all clock bits set because all data bits are 0).

### Step 4: Odd/Even Encode Data

512 zero bytes produce 1,024 MFM bytes (offsets 64-1087) where all data bit positions are 0. Before clock insertion, these are `$00`. After clock insertion, they become `$AA`.

### Step 5: Compute Header Checksum

The header checksum covers MFM bytes 8-47 (the info and reserved areas), XORed 4 bytes at a time:

```
Accumulator starts at $00000000

XOR bytes 8-11:   $55 $00 $01 $04 → accumulator = $55000104
XOR bytes 12-15:  $55 $05 $01 $00 → accumulator = $00050004
XOR bytes 16-19:  $00 $00 $00 $00 → accumulator = $00050004
XOR bytes 20-23:  $00 $00 $00 $00 → accumulator = $00050004
... (all remaining groups are $00000000) ...
XOR bytes 44-47:  $00 $00 $00 $00 → accumulator = $00050004

Header checksum = $00050004
```

This checksum is then odd/even encoded into 8 bytes at offsets 48-55.

### Step 6: Compute Data Checksum

The data checksum covers MFM bytes 64-1087 (1,024 bytes = 256 longwords):

Since all 256 longwords are `$00000000` (before clock insertion), the XOR of an even number of identical values is `$00000000`.

Data checksum = `$00000000`

This is odd/even encoded into 8 bytes at offsets 56-63 (all zeros in data positions).

### Step 7: Add Clock Bits

Starting from byte 8, each byte gets clock bits. For byte 8 (`$55`):

```
Data bits only: $55 = 01010101
Previous byte (byte 7 = $89): last bit is 1

Clock computation:
  Left-shifted data:   10101010
  Right-shifted data:  00101010 | (prev_last_bit << 7) = 10101010
  Combined (inverted): NOT(10101010 | 10101010) & $AA = NOT($AA) & $AA = $00

Result: $55 | $00 = $55 (clock bits are all 0 because every clock position
                         has at least one adjacent data bit that is 1)
```

For a byte that was `$00` (e.g., offset 9, following `$55`):

```
Data bits only: $00 = 00000000
Previous byte ($55): last bit is 1

Clock computation:
  Left-shifted data:   00000000
  Right-shifted data:  00000000 | (1 << 7) = 10000000
  Combined:            00000000 | 10000000 = 10000000
  Inverted:            NOT(10000000) & $AA = 01111111 & 10101010 = 00101010

Result: $00 | $2A = $2A
```

### Step 8: Assemble Final Sector

```
Offset  0-3:      $AA $AA $AA $AA  (inter-sector gap)
Offset  4-7:      $44 $89 $44 $89  (sync marks, written verbatim)
Offset  8-15:     info with clock bits applied
Offset  16-47:    reserved area: $AA $AA ... (zeros with all clocks set)
Offset  48-55:    header checksum with clock bits
Offset  56-63:    data checksum with clock bits
Offset  64-1087:  sector data: $AA $AA ... (zeros with all clocks set)
```

Total: 1,088 bytes. This sector is placed at byte offset `gap_size + (sector_number × 1088)` within the track.

### Step 9: Boundary Clock Fix

After all 11 sectors are assembled, the encoder calls `rectifyClockBit()` at each sector boundary (offsets 0, 1088, 2176, ...) to ensure the first clock bit of each sector follows the MFM rule relative to the last data bit of the preceding sector.


---

## 9. Relationship to the Filesystem Layer

The MFM track format described in this document is the **physical encoding layer**. Above it sits the **logical filesystem layer** (documented in `amiga-dos-filesystem-disk.md`), which organises the 512-byte sectors into a filesystem with directories, files, and metadata.

### 9.1 Layer Diagram

```
+-----------------------------------------------------+
| Application Layer                                    |
|   Files, directories, programs                       |
+-----------------------------------------------------+
| AmigaDOS Filesystem (OFS / FFS)                      |
|   Root block, bitmap blocks, file header blocks,     |
|   data blocks, extension blocks                      |
|   Operates on 512-byte logical sectors               |
+-----------------------------------------------------+
| MFM Track Format (THIS DOCUMENT)                     |
|   11 sectors × 512 bytes per track                   |
|   Odd/even encoding, sync marks, checksums           |
|   Operates on MFM-encoded bit streams                |
+-----------------------------------------------------+
| Disk Controller Hardware (Paula)                     |
|   DMA engine, DSKSYNC matching, FIFO buffer          |
|   Operates on raw bit cells (2 us each)              |
+-----------------------------------------------------+
| Magnetic Media                                       |
|   Flux transitions on rotating disk surface          |
+-----------------------------------------------------+
```

### 9.2 Sector Numbering

The filesystem sees sectors as a flat linear space:

```
Block 0:     Track 0, Sector 0     (Boot block, part 1)
Block 1:     Track 0, Sector 1     (Boot block, part 2)
Block 2-10:  Track 0, Sectors 2-10
Block 11:    Track 1, Sector 0
Block 12:    Track 1, Sector 1
...
Block 879:   Track 79, Sector 10   (last standard block on side 0/1 of cyl 39)
Block 880:   Track 80, Sector 0    (root block area)
...
Block 1759:  Track 159, Sector 10
```

The conversion between block number and track/sector is:

```
track  = block / sectors_per_track
sector = block % sectors_per_track
```

Where `sectors_per_track` is 11 for DD and 22 for HD.

### 9.3 What the Filesystem Sees

The filesystem operates entirely on decoded sector data. It never sees MFM encoding, sync marks, or checksums. The trackdisk.device driver handles:

1. Seeking the drive head to the correct cylinder.
2. Setting up disk DMA to read one or more tracks into a buffer.
3. Locating sectors within the MFM data by sync mark scanning.
4. Decoding the odd/even MFM encoding to extract raw sector data.
5. Verifying header and data checksums.
6. Returning the 512-byte sector payload to the filesystem.

For writes, the process is reversed: the driver encodes raw sector data into MFM format and writes it back to the track.


---

## 10. Decoding Algorithm Detail

The sector-seeking algorithm is the heart of the MFM decoder. Given a raw MFM bit stream, it must locate each sector's data area by finding sync marks and parsing headers.

### 10.1 vAmiga Decoder

From `AmigaDecoder.cpp:96-136`, the algorithm:

```
1. Start at bit offset 0 (or a provided hint offset)
2. LOOP:
   a. Scan forward through the bit stream for the 32-bit pattern $44894489
      (two consecutive sync words)
   b. If not found, throw SEEK_ERR
   c. Read the next 8 MFM bytes (the info field)
   d. If byte 1 is $89, this is a DOS track sync (PC format), skip it
   e. Decode the info field using odd/even decoding → 4 raw bytes
   f. Extract the sector number from byte 2 of the decoded info
   g. If this sector number has been seen before, BREAK (we've gone around the track)
   h. Record the sector's data range:
      - Start: current position + 48 bytes (skip past remaining header)
      - End: start + 1024 bytes (the 512-byte data area, odd/even encoded)
   i. Continue the LOOP
3. Return the map of sector numbers → data bit ranges
```

The key insight: the decoder does not need to know how many sectors exist in advance. It keeps scanning until it encounters a sector number it has already seen (indicating a complete revolution).

### 10.2 WinUAE Decoder

WinUAE's `decode_buffer()` (`disk.cpp:2553-2690`) uses a similar approach but with additional features:

- **Bit-shift tolerance:** It can find sync words at any bit offset, not just byte-aligned positions, using the `shift` variable:

```cpp
while (getmfmword(mbuf, shift) != 0x4489) {
    shift++;
    if (shift == 16) { shift = 0; mbuf++; }
}
```

- **Skip duplicate syncs:** After finding the first `$4489`, it skips all consecutive `$4489` words before reading the header.

- **Header validation:** It checks that the track number in the header matches the expected track number. Mismatches are logged and the sector is skipped.

- **Sector label preservation:** For `ADF_NORMAL_HEADER` format, non-zero sector labels are saved to a separate buffer (`writesecheadbuffer`).

### 10.3 Checksum Verification

Both decoders verify checksums independently:

**Header checksum:** After decoding the info field and sector labels, the decoder XORs all MFM longwords from the info field through the sector label area. The result should match the header checksum field. If not, the sector is rejected.

**Data checksum:** After decoding all 512 bytes of sector data, the decoder XORs all MFM longwords of the data area. The result should match the data checksum field. If not, the sector is rejected.

WinUAE's verification for the header (`disk.cpp:2610-2640`):

```cpp
chksum = odd ^ even;  // Start with info longword
for (i = 0; i < 4; i++) {
    odd = getmfmlong(mbuf, shift);
    even = getmfmlong(mbuf + 8, shift);
    mbuf += 2;
    dlong = (odd << 1) | even;
    chksum ^= odd ^ even;
}
// ...
odd = getmfmlong(mbuf, shift);
even = getmfmlong(mbuf + 2, shift);
if (((odd << 1) | even) != chksum) {
    // Header checksum error!
}
```

The checksum is computed over the **MFM-encoded** data (odd and even halves separately XORed into a running accumulator), not over the decoded bytes. This is a subtle but important distinction: the checksum protects the MFM encoding itself, catching any corruption that might occur during the encoding or transmission process.


---

## 11. Differences Between vAmiga and WinUAE

### 9.1 Track Length Constants

Both emulators agree on PAL track length (12,668 bytes). WinUAE also defines an NTSC track length (12,784 bytes) which vAmiga does not explicitly distinguish -- vAmiga uses 12,668 unconditionally but adjusts the byte transfer rate based on the configurable RPM value.

### 9.2 Word vs Byte Orientation

WinUAE works primarily in 16-bit MFM words (its `bigmfmbuf` is `uae_u16[]`), while vAmiga works in bytes. The encoding logic is equivalent, just expressed at different granularities.

### 9.3 Gap Placement

WinUAE places the gap *before* the first sector (`dstmfmoffset += FLOPPY_GAP_LEN` at `disk.cpp:2180`). vAmiga lays out sectors sequentially from offset 0 and fills the remaining track space with `$AA`. Both approaches produce a valid circular track -- the gap's position relative to the index pulse differs, but since Amiga software uses sync marks rather than index positioning, this does not affect compatibility.

### 9.4 Clock Bit Boundary Handling

vAmiga has an explicit `rectifyClockBit()` function (`AmigaEncoder.cpp:141-145`) that fixes the clock bit at sector boundaries (where the last bit of one sector and the first bit of the next must follow MFM rules):

```cpp
void AmigaEncoder::rectifyClockBit(MutableBitView view, isize offset)
{
    auto it = view.cyclic_begin(offset);
    view.set(offset, it[-1] || it[1] ? 0 : 1);
}
```

WinUAE handles this within the `mfmcode()` function by passing `lastword` across sector boundaries and encoding one extra word at the end (`disk.cpp:2262-2272`).

### 9.5 Sector Label Handling

WinUAE's `ADF_NORMAL_HEADER` format preserves non-zero sector labels and round-trips them through encode/decode. vAmiga always writes zero sector labels and does not currently support the header-extended ADF format.

### 9.6 No Fundamental Disagreements

The two implementations agree on all critical format details:

- Sync word value (`$4489`)
- Sector structure and field ordering
- Odd/even encoding algorithm
- Checksum algorithm (XOR of MFM longwords)
- Sector size (1,088 MFM bytes)
- Track size (12,668 PAL, 11 sectors DD)
- DSKLEN double-write mechanism
- DSKBYTR register layout
- DMA slot count (3 per line)


---

## 12. References

### Source Files

**vAmiga (Dirk W. Hoffmann)**
- `Core/RetroVault/Images/Encoders/MFM.cpp` -- MFM encode/decode primitives
- `Core/RetroVault/Images/Encoders/MFM.h` -- MFM function declarations
- `Core/RetroVault/Images/Encoders/AmigaEncoder.cpp` -- Sector and track encoding
- `Core/RetroVault/Images/Encoders/AmigaDecoder.cpp` -- Sector and track decoding
- `Core/Components/Paula/DiskController/DiskController.cpp` -- DMA engine
- `Core/Components/Paula/DiskController/DiskControllerRegs.cpp` -- Register handling
- `Core/Components/Paula/DiskController/DiskControllerEvents.cpp` -- Timing
- `Core/Components/Paula/DiskController/DiskControllerTypes.h` -- State enums
- `Core/Peripherals/Drive/FloppyDisk.cpp` -- Track geometry and encoding dispatch
- `Core/Peripherals/Drive/FloppyDisk.h` -- Track layout documentation comment
- `Core/Peripherals/Drive/FloppyDrive.cpp` -- Drive mechanics and timing
- `Core/Peripherals/Drive/FloppyDriveTypes.h` -- Drive type enums and config
- `Core/RetroVault/Images/ADF/ADFFile.h` -- ADF size constants
- `Core/RetroVault/Images/EADF/EADFFile.h` -- Extended ADF format documentation
- `Core/RetroVault/Images/EADF/EADFFile.cpp` -- Extended ADF parsing

**WinUAE (Toni Wilen / Bernd Schmidt)**
- `disk.cpp` -- Complete floppy disk emulation (MFM, DMA, formats, protection)
- `diskutil.cpp` -- MFM decode utilities

### Related Documents
- `amiga-dos-filesystem-disk.md` -- AmigaDOS filesystem layer (OFS/FFS blocks)
- `amiga-hardware-reference.md` -- Custom chip register descriptions (DSKLEN, DSKSYNC, etc.)

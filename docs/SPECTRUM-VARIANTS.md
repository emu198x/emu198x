# Spectrum Variant Reference

## Clock Tree

| Variant | Crystal (Hz) | CPU ÷ | CPU (Hz) | AY ÷ | AY (Hz) | T/line | Lines | T/frame |
|---------|-------------|-------|----------|------|---------|--------|-------|---------|
| 48K / TC2048 | 14,000,000 | 4 | 3,500,000 | — | — | 224 | 312 | 69,888 |
| 128K / +2 | 17,734,475 | 5 | 3,546,895 | 10 | 1,773,448 | 228 | 311 | 70,908 |
| +2A / +2B / +3 | 17,734,475 | 5 | 3,546,895 | 10 | 1,773,448 | 228 | 311 | 70,908 |
| TC2068 (PAL) | 14,000,000 | 4 | 3,500,000 | 8 | 1,750,000 | 224 | 312 | 69,888 |
| TS2068 (NTSC) | 14,112,000 | 4 | 3,528,000 | 8 | 1,764,000 | 224 | 262 | 58,688 |
| Pentagon | 14,336,000 | 4 | 3,584,000 | 8 | 1,792,000 | 224 | 320 | 71,680 |
| Scorpion | 14,000,000 | 4 | 3,500,000 | 8 | 1,750,000 | 224 | 312 | 69,888 |

## Contention

| Variant | Pattern | Phase | I/O contention | Internal contention | Contended range |
|---------|---------|-------|---------------|--------------------|-----------------| 
| 48K / TC2048 | 6,5,4,3,2,1,0,0 | 0 | Yes (4 cases) | Yes (IR on bus) | $4000-$7FFF |
| TC2068 / TS2068 | 6,5,4,3,2,1,0,0 | 0 | Yes (4 cases) | Yes (IR on bus) | $4000-$7FFF |
| 128K / +2 | 6,5,4,3,2,1,0,0 | 1 | Yes (4 cases) | Yes (IR on bus) | $4000-$7FFF + odd banks at $C000 |
| +2A / +3 | 1,0,7,6,5,4,3,2 | 0 | **No** | **No** (MREQ-only) | $4000-$7FFF + banks 4-7 at $C000 |
| Pentagon | **None** | — | No | No | None |
| Scorpion | **None** | — | No | No | None |

### I/O Contention (48K / 128K only)

| High byte contended? | A0 | Pattern |
|---------------------|-----|---------|
| No | 0 (even) | N:1, C:3 |
| No | 1 (odd) | N:4 |
| Yes | 0 (even) | C:1, C:3 |
| Yes | 1 (odd) | C:1, C:1, C:1, C:1 |

N = no contention. C = apply delay from contention pattern.

## Memory Map

### 48K

| Range | Contents |
|-------|----------|
| $0000-$3FFF | ROM (16K, fixed) |
| $4000-$7FFF | RAM bank 5 (screen, contended) |
| $8000-$BFFF | RAM bank 2 |
| $C000-$FFFF | RAM bank 0 |

### 128K / +2

Port **$7FFD** (A15=0, A1=0):

| Bit | Function |
|-----|----------|
| 0-2 | RAM bank at $C000 (0-7) |
| 3 | Screen bank (0=bank 5, 1=bank 7) |
| 4 | ROM (0=128K editor, 1=48K BASIC) |
| 5 | Paging lock (irreversible until reset) |

### +2A / +2B / +3

Port **$7FFD** (A15=0, A14=1, A1=0) — same as 128K.

Port **$1FFD** (A15=0, A14=0, A12=1, A1=0):

| Bit | Function |
|-----|----------|
| 0 | Paging mode (0=normal, 1=special) |
| 1-2 | Special mode select (0-3) |
| 2 | ROM select high bit |
| 3 | Disk motor |
| 4 | Printer strobe |

ROM select = ($1FFD bit 2) << 1 | ($7FFD bit 4). Four ROMs: 0=128K editor, 1=syntax checker, 2=+3DOS, 3=48K BASIC.

**Special paging modes** (all RAM, no ROM):

| Mode | $0000 | $4000 | $8000 | $C000 |
|------|-------|-------|-------|-------|
| 0 | Bank 0 | Bank 1 | Bank 2 | Bank 3 |
| 1 | Bank 4 | Bank 5 | Bank 6 | Bank 7 |
| 2 | Bank 4 | Bank 5 | Bank 6 | Bank 3 |
| 3 | Bank 4 | Bank 7 | Bank 6 | Bank 3 |

## I/O Ports

| Port | Decode | Device | Present on |
|------|--------|--------|------------|
| $xxFE | A0=0 | ULA (border, beeper, keyboard) | All |
| $7FFD | A15=0, A1=0 (128K) / A15=0, A14=1, A1=0 (+3) | Memory paging | 128K+ |
| $1FFD | A15=0, A14=0, A12=1, A1=0 | Extended paging | +2A/+3 |
| $FFFD | A15=1, A14=1, A1=0 | AY register select / read | 128K+ |
| $BFFD | A15=1, A14=0, A1=0 | AY data write | 128K+ |

## ROMs

| Variant | Path | Files |
|---------|------|-------|
| 48K | sinclair-zx-spectrum-48k/ | 48.rom |
| 128K | sinclair-zx-spectrum-128k/ | 128-0.rom, 128-1.rom |
| +2 | sinclair-zx-spectrum-plus2/ | plus2-0.rom, plus2-1.rom |
| +2A/+3 | sinclair-zx-spectrum-plus3/ | plus3-0.rom .. plus3-3.rom |
| Pentagon | pentagon-128/ | pentagon-0.rom, pentagon-1.rom |
| Scorpion | scorpion-zs256/ | scorpion-0.rom .. scorpion-3.rom |
| TC2048 | timex-tc2048/ | tc2048.rom |
| TC2068/TS2068 | timex-ts2068/ | ts2068.rom, exrom.rom |

All Sinclair ZX Spectrum ROMs (48K, 128K, +2, +2A, +3) are distributable under Amstrad's 1999 blanket permission and are committed to the repo at `test-roms/`. Pentagon, Scorpion, and Timex ROMs are user-supplied due to less clear licensing.

## Floating Bus

| Variant | Behaviour |
|---------|-----------|
| 48K / 128K / +2 / TC2048 / TC2068 / TS2068 | Returns ULA data bus during screen fetch, $FF during border |
| +2A / +2B / +3 | Always $FF (Amstrad gate array killed floating bus) |
| Pentagon / Scorpion | Always $FF |

## Interrupt

All variants: INT asserted at the start of the vertical blanking interval for 32 T-states. IM 1 vector: $0038. IM 2 vector: (I << 8) | $FF on 48K (no device responds, bus floats high).

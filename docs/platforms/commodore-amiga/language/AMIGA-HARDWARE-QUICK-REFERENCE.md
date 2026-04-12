# Amiga Hardware Quick Reference

**Purpose:** Fast lookup for Amiga OCS/ECS chipset and hardware capabilities
**Audience:** Amiga programmers and curriculum designers
**For comprehensive details:** See Amiga Hardware Reference Manual

---

## Hardware Overview

### Chipset (OCS - Original Chip Set)

**Three Custom Chips:**
1. **Paula** - Audio, Disk, Serial I/O
2. **Agnus** - DMA Controller, Blitter, Copper
3. **Denise** - Display, Sprites, Playfields

**Models:**
- **Amiga 500** (1987): OCS chipset, 512KB Chip RAM, 68000 @ 7.14MHz
- **Amiga 600** (1992): ECS chipset, 1MB Chip RAM, 68000 @ 7.14MHz
- **Amiga 1200** (1992): AGA chipset, 2MB Chip RAM, 68020 @ 14MHz

**This reference focuses on OCS/ECS (Phase 0 curriculum target)**

### Memory Architecture

**Two Types of RAM:**

1. **Chip RAM** (0.5-2MB)
   - Accessible by CPU and custom chips
   - Used for graphics, sound, disk buffers
   - Shared bandwidth (DMA contention)
   - Slower access (7.14MHz)

2. **Fast RAM** (0-8MB, if installed)
   - CPU-only access
   - No DMA contention
   - Faster access (full CPU speed)
   - Not available on all models

**Memory Map (Simplified):**
```
$000000-$07FFFF: Chip RAM (512KB, Amiga 500)
$080000-$1FFFFF: Extended Chip RAM (if installed)
$200000-$9FFFFF: Fast RAM (if installed)
$BFD000-$BFEFFF: CIA-A & CIA-B registers
$DFF000-$DFFFFF: Custom chip registers
$F80000-$FFFFFF: Kickstart ROM (512KB)
```

---

## Paula - Audio and I/O

### Sound Capabilities

**4 DMA Sound Channels:**
- **8-bit samples** (256 volume levels)
- **Stereo separation** (channels 0+3 left, 1+2 right)
- **Sample rates:** ~3.5kHz to 28kHz typical
- **Hardware mixing** (all 4 channels mixed automatically)
- **Max sample rate:** 56kHz (Nyquist limit for 3.58MHz DMA)

**Channel Assignment (typical):**
```
Channel 0 (Left):  Sound effects, drums
Channel 1 (Right): Sound effects, melody
Channel 2 (Right): Bass, harmony
Channel 3 (Left):  Lead, harmony
```

**Audio Registers (Simplified):**
```
$DFF0A0-$DFF0DF: Audio channel 0-3 registers
  +$00: Start address (pointer to sample)
  +$04: Length (in words)
  +$06: Period (sample rate)
  +$08: Volume (0-64)
```

**Sample Rate Calculation:**
```
Period = 3579545 / Sample_Rate_Hz

Examples:
8kHz:  Period = 447
11kHz: Period = 325
16kHz: Period = 224
```

### Disk Controller

**3.5" Floppy Drive:**
- **Capacity:** 880KB (double density)
- **Format:** 80 tracks × 11 sectors × 512 bytes
- **Transfer rate:** 500 Kbps
- **Access time:** ~100ms (track-to-track)

**DMA Operation:**
- Disk reads/writes via DMA
- CPU free during transfer (after setup)
- Interrupts signal completion

---

## Agnus - DMA and Blitter

### DMA Controller

**DMA Channels:**
1. **Disk** - Floppy drive transfers
2. **Audio 0-3** - Four sound channels
3. **Blitter** - Fast memory copies
4. **Copper** - Display list processor
5. **Bitplane** - Screen display (1-6 bitplanes)
6. **Sprite 0-7** - Hardware sprites

**DMA Timing:**
- Horizontal resolution determines DMA slots
- Low res (320px): More CPU time
- High res (640px): Less CPU time
- 50Hz PAL: 312 scan lines per frame
- 60Hz NTSC: 262 scan lines per frame

### Blitter

**Purpose:** Ultra-fast memory operations

**Operations:**
- **Block copy** - Copy rectangular areas
- **Block fill** - Fill areas with colour
- **Line drawing** - Fast line algorithm
- **Logical ops** - AND, OR, XOR combinations
- **Masking** - Selective pixel operations

**Performance:**
- ~4 million pixels/second
- Much faster than CPU for graphics
- Can operate during CPU execution (parallel)

**AMOS Access:**
```amos
' Blitter used automatically by:
Bar x1,y1 To x2,y2           ' Filled rectangle
Disc x,y,radius              ' Filled circle
Polygon x1,y1 To x2,y2...    ' Filled polygon
Scroll, Screen Copy, etc.
```

### Copper

**Purpose:** Display list co-processor

**Capabilities:**
- **Change registers mid-frame** (colour cycling, splits)
- **Wait for specific scan line**
- **Trigger other DMA operations**
- **Create raster effects** (rainbows, parallax)

**AMOS Access:**
```amos
Rainbow line,start,end,speed  ' Copper-driven colour bars
Screen Display positioning    ' Copper display list
```

---

## Denise - Display and Sprites

### Display Modes

**Resolutions (OCS):**

| Mode | Resolution | Colours | Description |
|------|-----------|---------|-------------|
| Low Res | 320×256 PAL | 32 | Standard game mode |
| | 320×200 NTSC | 32 | US standard |
| Med Res | 640×256 PAL | 16 | Detailed graphics |
| | 640×200 NTSC | 16 | US detailed |
| High Res | 640×512 PAL | 4 | Interlaced, Workbench |
| | 640×400 NTSC | 4 | US Workbench |

**Colour Depth (bitplanes):**
- 1 bitplane = 2 colours
- 2 bitplanes = 4 colours
- 3 bitplanes = 8 colours
- 4 bitplanes = 16 colours
- 5 bitplanes = 32 colours
- 6 bitplanes = 64 colours (ECS Extra Half-Brite)

**HAM Mode (Hold And Modify):**
- 6 bitplanes = 4096 colours on screen
- Limited to modifying R, G, or B from previous pixel
- Creates unique colour palette effects
- Slower to update (sequential dependency)

### Bitplanes

**Planar Memory Organization:**

Unlike chunky pixels (1 byte = 1 pixel), Amiga uses **planar** format:

```
4-colour display (2 bitplanes):

Bitplane 0: 00110011...  (bit 0 of each pixel)
Bitplane 1: 01010101...  (bit 1 of each pixel)

Pixel colours:
  00 = Colour 0
  01 = Colour 1
  10 = Colour 2
  11 = Colour 3
```

**Memory Layout:**
- Each bitplane = separate memory area
- Width in bytes = (pixels + 15) / 16 * 2
- 320px width = 40 bytes per line
- 640px width = 80 bytes per line

**Example (320×256, 32 colours = 5 bitplanes):**
```
Bitplane 0: 40 bytes × 256 lines = 10,240 bytes
Bitplane 1: 40 bytes × 256 lines = 10,240 bytes
Bitplane 2: 40 bytes × 256 lines = 10,240 bytes
Bitplane 3: 40 bytes × 256 lines = 10,240 bytes
Bitplane 4: 40 bytes × 256 lines = 10,240 bytes
Total: 51,200 bytes (~50KB)
```

### Palette

**Colour Registers (OCS):**
- 32 colour registers (0-31)
- 12-bit colour: 4 bits each for R, G, B
- Format: `$0RGB` (each component 0-15)
- 4096 possible colours (16×16×16)

**Colour Calculation:**
```
Red   = (Colour >> 8) & $F    (bits 8-11)
Green = (Colour >> 4) & $F    (bits 4-7)
Blue  = Colour & $F           (bits 0-3)

Example: $0F4A
  Red   = $F (15) = Bright red
  Green = $4 (4)  = Dim green
  Blue  = $A (10) = Bright blue
  Result: Bright pink/magenta
```

**AMOS Palette:**
```amos
Colour 0,$000       ' Black
Colour 1,$F00       ' Red
Colour 2,$0F0       ' Green
Colour 3,$00F       ' Blue
Colour 4,$FF0       ' Yellow
Colour 5,$0FF       ' Cyan
Colour 6,$F0F       ' Magenta
Colour 7,$FFF       ' White
Colour 8,$888       ' Grey
Colour 9,$F80       ' Orange
```

### Hardware Sprites

**8 Hardware Sprites:**
- **16 pixels wide** (15 colours + transparent)
- **Any height** (1-256 lines typical)
- **Attached mode:** Combine 2 sprites = 15 colours
- **Independent movement** (smooth per-pixel)
- **Automatic hardware collision detection**
- **Zero CPU overhead** (DMA driven)

**Sprite Pairs:**
- Sprites 0+1, 2+3, 4+5, 6+7 can attach
- Attached: 4 bitplanes = 15 colours + transparent
- Detached: 2 bitplanes = 3 colours + transparent each

**Performance:**
- Move: Hardware register write (instant)
- No screen redraw needed
- No background restoration needed
- Limited quantity (8 max)

**AMOS Sprite:**
```amos
' Hardware sprites used automatically
Sprite 0,x,y,image   ' Sprite 0 at position (x,y)
Move X 0,x+2         ' Move sprite horizontally
' Collision detection in hardware
If Col(0,1) Then Print "SPRITES COLLIDED"
```

---

## Display Timing

### PAL Timing (50Hz)

**Frame Structure:**
```
Total lines:     312 (625 interlaced)
Visible lines:   256 (512 interlaced)
Horizontal:      320 or 640 pixels
VBlank period:   ~56 lines (18ms)
Frame time:      20ms (50 fps)
```

**DMA Slots:**
- Low res: ~70% CPU time available
- Med res: ~35% CPU time available
- High res: ~10% CPU time available

### NTSC Timing (60Hz)

**Frame Structure:**
```
Total lines:     262 (525 interlaced)
Visible lines:   200 (400 interlaced)
Horizontal:      320 or 640 pixels
VBlank period:   ~62 lines (16ms)
Frame time:      16.7ms (60 fps)
```

### Synchronization

**Wait Vbl:**
```amos
Wait Vbl         ' Wait for vertical blank
' Screen not being drawn, safe to update
```

**Double Buffering:**
```amos
Double Buffer    ' Enable double buffering
Autoback 0       ' Manual swap

Main_Loop:
  Cls 0          ' Clear work buffer
  ' ... draw frame ...
  Screen Swap    ' Swap display/work buffers
  Wait Vbl       ' Sync to refresh
  Goto Main_Loop
```

---

## CIA (Complex Interface Adapter)

**Two CIA Chips:**

### CIA-A ($BFE001)

**Functions:**
- **Parallel port** (printer)
- **Serial port** (modem)
- **Game port 1** (joystick/mouse)
- **Keyboard** interface
- **LED** control (power LED)

### CIA-B ($BFD000)

**Functions:**
- **Disk drive** control
- **Serial port** (continued)
- **Game port 2** (joystick/mouse)
- **Timer interrupts**

### Game Ports

**2 Game Ports:**
- Port 1: Mouse or joystick
- Port 2: Mouse or joystick

**Joystick Directions:**
```
Bit 0: Up
Bit 1: Down
Bit 2: Left
Bit 3: Right
Bit 6: Fire button (low = pressed)
```

**AMOS Access:**
```amos
' Joystick
If Jup(1) Then y=y-2
If Jdown(1) Then y=y+2
If Jleft(1) Then x=x-2
If Jright(1) Then x=x+2
If Fire(1) Then Shoot

' Mouse
x=X Mouse : y=Y Mouse
If Mouse Key=1 Then Left_Click
If Mouse Key=2 Then Right_Click
```

---

## Copper Lists

**Display List Programming:**

The Copper can:
1. **Wait** for specific scan line
2. **Write** to hardware registers
3. **Jump** to another Copper list

**Common Uses:**
- **Colour cycling** (change palette mid-frame)
- **Raster bars** (horizontal colour bands)
- **Split screen** (different resolutions on same screen)
- **Parallax scrolling** (multiple scrolling layers)
- **Starfield effects** (modify playfield mid-frame)

**AMOS Copper:**
```amos
Rainbow line,start,end,speed  ' Automatic copper bars

' Manual copper effects require assembly
```

**Example Copper List (conceptual):**
```
Wait line 100                 ; Wait for line 100
Write $DFF180,$0F00           ; Set colour 0 to red
Wait line 150                 ; Wait for line 150
Write $DFF180,$00F0           ; Set colour 0 to green
Wait line 200                 ; Wait for line 200
Write $DFF180,$000F           ; Set colour 0 to blue
End list
```

---

## Memory Bandwidth

### DMA Cycles

**Horizontal Line (PAL, Low Res):**
- Total cycles: 227.5 DMA cycles
- Bitplane DMA: ~40 cycles (for 320px display)
- CPU available: ~187 cycles (82%)

**Bandwidth Usage:**

| Operation | DMA Cycles | Impact |
|-----------|-----------|--------|
| 1 bitplane | ~40/line | Light |
| 5 bitplanes | ~200/line | Heavy |
| 8 sprites | ~16/line | Light |
| Audio (4ch) | ~4/line | Minimal |
| Blitter | Variable | Can stall CPU |

**Optimization:**
- Use fewer bitplanes (32 colours vs 64)
- Low res over high res (more CPU time)
- Disable unused DMA channels
- Sync blitter ops to VBlank

---

## Performance Considerations

### CPU (68000 @ 7.14MHz)

**Cycle Times:**
- Simple instruction: ~4 cycles (560ns)
- Memory access: ~4 cycles (if no DMA)
- Multiply: ~70 cycles (~10μs)
- Divide: ~140 cycles (~20μs)

**Integer Performance:**
- 16-bit: Native (fast)
- 32-bit: Software (slower)
- Floating point: Software library (very slow)

**Optimization Tips:**
1. **Use integers** - 16-bit is native word size
2. **Avoid divide** - Use shifts for powers of 2
3. **Minimize DMA** - Fewer bitplanes = more CPU time
4. **Cache values** - Store sin/cos tables
5. **Assembly for hotspots** - 10-100× speedup possible

### Graphics Performance

**Rendering Methods (fastest to slowest):**

1. **Hardware sprites** (instant, 8 max)
   ```
   Move X 0,x+2        ' Instant, no CPU
   ```

2. **Copper effects** (zero CPU)
   ```
   Rainbow, colour cycling, splits
   ```

3. **Blitter operations** (fast, parallel with CPU)
   ```
   Bar, Disc, Polygon, Block copies
   ```

4. **CPU drawing** (slow, blocks everything)
   ```
   Plot, Draw (CPU bit manipulation)
   ```

**Typical Frame Budget (50 FPS):**
- Total: 20ms
- VBlank operations: ~2ms
- Game logic: ~8ms
- Drawing: ~8ms
- Reserve: ~2ms (DMA, interrupts)

---

## Common Hardware Limits

### Display Limits

| Limit | OCS Value |
|-------|-----------|
| Max sprites | 8 |
| Sprite width | 16 pixels |
| Max bitplanes | 6 (5 for games typically) |
| Max colours (normal) | 32 (5 bitplanes) |
| Max colours (HAM) | 4096 (6 bitplanes) |
| Max resolution | 640×512 interlaced |
| Colour palette | 4096 colours (12-bit) |

### Memory Limits

| Component | Size |
|-----------|------|
| Chip RAM (A500) | 512KB |
| Chip RAM (A600) | 1MB |
| Fast RAM (max) | 8MB |
| Screen buffer (320×256×5) | ~50KB |
| Audio sample (1 sec, 8kHz) | ~8KB |

### Audio Limits

| Limit | Value |
|-------|-------|
| Channels | 4 |
| Sample depth | 8-bit |
| Max sample rate | ~56kHz |
| Min sample rate | ~122Hz |
| Volume levels | 65 (0-64) |
| Max sample size | 128KB (practical) |

---

## Kickstart ROM

**Operating System in ROM:**
- **Kickstart 1.2/1.3** (256KB): A500 standard
- **Kickstart 2.0+** (512KB): A600/1200 standard

**Includes:**
- Exec (multitasking kernel)
- Intuition (GUI)
- DOS (file system)
- Graphics library
- Audio library
- Device drivers

**AMOS Access:**
```amos
' ROM routines used automatically
Load, Save, Bload, Bsave      ' DOS functions
Screen Open, Sprite, Bob      ' Graphics functions
Play, Music                   ' Audio functions
```

---

## Interrupts

**Interrupt Levels:**
- **Level 1:** Software, Disk block done
- **Level 2:** CIA-A (keyboard, serial)
- **Level 3:** Copper, VBlank, Blitter
- **Level 4:** Audio channels
- **Level 5:** Disk sync
- **Level 6:** CIA-B (disk, serial)

**VBlank Interrupt (most important):**
- Triggers at start of vertical blank
- ~50 times per second (PAL)
- Used for game timing, updates

**AMOS VBlank:**
```amos
Wait Vbl         ' Automatic VBlank sync
' AMOS game loop runs at 50 FPS
```

---

## Expansion

### Amiga 500 Expansion

**Trapdoor Slot:**
- 512KB RAM expansion (most common)
- Real-time clock
- Memory total: 1MB (512KB Chip + 512KB Fast)

**Side Expansion Port:**
- Hard drive controllers
- Accelerator cards (faster CPU)
- Additional memory

### Amiga 600/1200 Expansion

**PCMCIA Slot:**
- Memory cards
- Network adapters
- Modem cards

**IDE Controller (A600/1200):**
- Internal hard drive (up to 4GB)
- Much faster than floppy

---

## Register Quick Reference

### Essential Registers

| Address | Name | Purpose |
|---------|------|---------|
| $DFF000-$DFF01E | BPLCON | Bitplane control |
| $DFF02A | VPOSR | Vertical position (read) |
| $DFF088-$DFF09E | COPJMP | Copper jump |
| $DFF0A0-$DFF0DF | AUD0-3 | Audio channels |
| $DFF100-$DFF11E | BPLPT | Bitplane pointers |
| $DFF120-$DFF13E | SPR0-7 | Sprite pointers |
| $DFF180-$DFF1BE | COLOR | Colour palette |

### Custom Chip Registers

**Most programming via AMOS commands, not direct register access.**

**Assembly programmers:** See Amiga Hardware Reference Manual for complete register documentation.

---

## Quick Checklist

### Starting a Project

- [ ] Screen resolution chosen (320×256 typical)
- [ ] Colour depth chosen (32 colours typical)
- [ ] Double buffering enabled (smooth animation)
- [ ] Palette defined (colours 0-31)
- [ ] VBlank sync (Wait Vbl in loop)

### Optimizing Performance

- [ ] Use hardware sprites (not bobs) when possible
- [ ] Minimize bitplanes (5 max for games)
- [ ] Cache calculations (sin/cos tables)
- [ ] Profile hotspots (assembly for 5-10% of code)
- [ ] Test on real hardware (emulator ≠ real timing)

### Memory Management

- [ ] Chip RAM for graphics/sound
- [ ] Fast RAM for code/data (if available)
- [ ] Free banks when done (Erase)
- [ ] Monitor memory usage

---

**Version:** 1.0
**Created:** 2025-10-24
**For:** Amiga Phase 0 AMOS Programming

**See Also:**
- AMOS-COMMANDS-QUICK-REFERENCE.md (language reference)
- AMIGA-ASSEMBLY-REFERENCE.md (68000 programming)

**Complete:** Amiga reference documentation set finished!

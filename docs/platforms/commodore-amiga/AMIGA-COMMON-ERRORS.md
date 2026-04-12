# Amiga/AMOS Common Errors and Pitfalls

**Purpose:** Document common mistakes in AMOS BASIC programming and Amiga hardware quirks
**Audience:** Amiga curriculum designers and AMOS programmers
**Last Updated:** 2025-10-30

---

## Critical Hardware Understanding

### 1. Chip RAM vs Fast RAM

**Problem:** Not all memory is equal on the Amiga. Custom chips can only access Chip RAM.

**Symptoms:**
- Sprites/bobs don't display
- Sound doesn't play
- DMA operations fail silently

**Key Difference:**
- **Chip RAM:** Accessible by CPU AND custom chips (graphics, sound, blitter)
- **Fast RAM:** CPU only, cannot be used for graphics/audio data

**Wrong Assumption:**
```amos
Rem WRONG - Assuming all memory works for graphics
Load "sprites.abk"  : Rem May load to Fast RAM
Sprite 0,100,100,1  : Rem Sprite data not in Chip RAM = nothing displays!
```

**Correct Understanding:**
```amos
Rem AMOS handles this automatically in most cases
Rem But be aware: graphics/sound data MUST be in Chip RAM
Rem Standard Amiga 500: 512KB Chip RAM (all graphics/audio here)
Rem Fast RAM (if installed): Programs/variables only
```

**Rule:** Graphics data, sound samples, and sprite/bob banks must reside in Chip RAM. AMOS usually handles this, but understanding prevents confusion.

---

### 2. Sprites Are Hardware Limited (8 Only)

**Problem:** Amiga OCS has exactly 8 hardware sprites. Attempting to use more fails silently or causes odd behavior.

**Symptoms:**
- Sprite commands work for 0-7, then nothing
- Ninth sprite doesn't appear
- No error message

**Wrong:**
```amos
Rem Trying to use 10 sprites
For I=0 To 9
   Sprite I,X,Y,1  : Rem Sprites 8-9 don't exist!
Next I
```

**Correct (Use Bobs for more objects):**
```amos
Rem 8 hardware sprites for player/important objects
For I=0 To 7
   Sprite I,X,Y,1  : Rem Hardware sprites (very fast)
Next I

Rem Use Bobs for enemies/bullets (unlimited but slower)
For I=0 To 20
   Bob I,X,Y,2     : Rem Software sprites (blitter-based)
Next I
```

**When to Use What:**
- **Sprites (0-7):** Player character, cursor, critical fast-moving objects
- **Bobs (unlimited):** Enemies, bullets, particles, background objects

**Rule:** Hardware sprites = 8 maximum. Use Bobs for additional objects.

---

### 3. Hardware Sprites Are 16 Pixels Wide (Fixed)

**Problem:** Sprite width is hardwired to 16 pixels. Cannot be changed.

**Symptoms:**
- Wider sprite graphics get clipped
- Multiple sprites needed for wide objects

**Hardware Limitation:**
```amos
Rem Hardware sprite limitations:
Rem - Width: ALWAYS 16 pixels (cannot change)
Rem - Height: Any value (1-screen height)
Rem - Colors: 15 + transparent (4 palettes × 3 colors each + 1 shared)
```

**Workarounds:**
```amos
Rem Option 1: Use multiple sprites side-by-side
Sprite 0,X,Y,1    : Rem Left half of wide object
Sprite 1,X+16,Y,2 : Rem Right half (16px offset)

Rem Option 2: Use Bobs for wider objects
Bob 0,X,Y,1       : Rem Can be any width
```

**Rule:** Hardware sprites = 16 pixels wide, always. Use Bobs or multiple sprites for wider objects.

---

## AMOS Language Pitfalls

### 4. Screen Swap Without Double Buffer

**Problem:** Using Screen Swap without enabling Double Buffer first causes unpredictable results.

**Symptoms:**
- Screen flickers
- Tearing visible
- Graphics corruption

**Wrong:**
```amos
Screen Open 0,320,256,32,Lowres
Rem Missing: Double Buffer

Main_Loop:
   Cls 0
   Circle 160,128,50
   Screen Swap  : Rem ERROR - no double buffer enabled!
   Wait Vbl
   Goto Main_Loop
```

**Correct:**
```amos
Screen Open 0,320,256,32,Lowres
Cls 0
Double Buffer    : Rem MUST enable double buffering
Autoback 0       : Rem Manual swap (0=off, 1=auto)

Main_Loop:
   Cls 0
   Circle 160,128,50
   Screen Swap    : Rem Now correctly swaps buffers
   Wait Vbl
   Goto Main_Loop
```

**Rule:** Always enable `Double Buffer` before using `Screen Swap`.

---

### 5. Missing Wait Vbl in Animation Loops

**Problem:** Animation loops without Wait Vbl run as fast as possible, causing excessive speed and screen tearing.

**Symptoms:**
- Animation too fast to see
- Flickering graphics
- Wasted CPU cycles

**Wrong:**
```amos
Main_Loop:
   Bob Off
   X=X+1
   Bob 0,X,Y,1
   Bob Draw
   Goto Main_Loop  : Rem Runs thousands of times per second!
```

**Correct:**
```amos
Main_Loop:
   Bob Off
   X=X+1
   Bob 0,X,Y,1
   Bob Draw
   Wait Vbl        : Rem Sync to 50Hz (PAL) or 60Hz (NTSC)
   Goto Main_Loop
```

**Best Practice:**
```amos
Main_Loop:
   Bob Off
   X=X+Speed       : Rem Movement speed independent of frame rate
   Bob 0,X,Y,1
   Bob Draw
   Screen Swap     : Rem If double buffered
   Wait Vbl        : Rem ALWAYS sync to vertical blank
   Goto Main_Loop
```

**Rule:** EVERY animation loop should have `Wait Vbl` to sync with display refresh.

---

### 6. Colour Values Must Be $RGB (12-bit)

**Problem:** Amiga OCS uses 12-bit RGB color ($RGB format), not 24-bit or other formats.

**Symptoms:**
- Wrong colors displayed
- Colors too bright/dark
- Syntax errors

**Color Format:**
```
$RGB where R, G, B are each 0-15 (4 bits)
$000 = Black   ($0 red, $0 green, $0 blue)
$F00 = Red     ($F red, $0 green, $0 blue)
$0F0 = Green   ($0 red, $F green, $0 blue)
$00F = Blue    ($0 red, $0 green, $F blue)
$FFF = White   ($F red, $F green, $F blue)
$888 = Grey    ($8 red, $8 green, $8 blue)
```

**Wrong (24-bit RGB or decimal):**
```amos
Colour 1,255     : Rem WRONG - not decimal
Colour 2,$FF0000 : Rem WRONG - not 24-bit RGB
```

**Correct (12-bit $RGB):**
```amos
Colour 0,$000    : Rem Black
Colour 1,$F00    : Rem Bright red
Colour 2,$0F0    : Rem Bright green
Colour 3,$00F    : Rem Bright blue
Colour 4,$F80    : Rem Orange ($F red, $8 green)
Colour 5,$08F    : Rem Light blue
```

**Rule:** Amiga colors = $RGB format (12-bit), not 24-bit RGB.

---

### 7. Palette Register Limits

**Problem:** Number of available color registers depends on screen bit depth.

**Palette Sizes:**
- 1 bitplane = 2 colors (registers 0-1)
- 2 bitplanes = 4 colors (registers 0-3)
- 3 bitplanes = 8 colors (registers 0-7)
- 4 bitplanes = 16 colors (registers 0-15)
- 5 bitplanes = 32 colors (registers 0-31)
- 6 bitplanes = 64 colors (registers 0-63, ECS with EHB)

**Wrong:**
```amos
Screen Open 0,320,256,16,Lowres  : Rem 16 colors (4 bitplanes)
Colour 20,$F00                    : Rem ERROR - register 20 doesn't exist!
```

**Correct:**
```amos
Screen Open 0,320,256,16,Lowres  : Rem 16 colors
For I=0 To 15                     : Rem Registers 0-15 only
   Colour I,$F00
Next I
```

**Rule:** Available palette registers = number of colors in screen mode. 16-color mode = registers 0-15.

---

## Bobs vs Sprites Confusion

### 8. Mixing Bob Commands and Sprite Commands

**Problem:** Bobs and Sprites use different command sets. Mixing them causes errors.

**Key Differences:**
| Feature | Sprites | Bobs |
|---------|---------|------|
| Command prefix | `Sprite` | `Bob` |
| Maximum count | 8 | Unlimited |
| Width | 16 pixels fixed | Any width |
| Speed | Very fast (hardware) | Slower (blitter) |
| Collision | `Col()` | `Bobcol()` |
| Update | `Sprite Update` | `Bob Update` / `Bob Draw` |

**Wrong (Mixing commands):**
```amos
Sprite 0,100,100,1  : Rem Hardware sprite
Bob Update          : Rem WRONG - Bob Update doesn't affect sprites!
```

**Correct (Separate commands):**
```amos
Rem Hardware sprites
Sprite 0,100,100,1
Sprite 1,120,100,2
Sprite Update       : Rem Update sprites

Rem Software bobs
Bob 0,50,50,1
Bob 1,70,50,2
Bob Draw            : Rem Draw bobs
```

**Rule:** Sprites use Sprite commands. Bobs use Bob commands. Don't mix.

---

### 9. Bob Draw vs Bob Update

**Problem:** `Bob Draw` and `Bob Update` are NOT interchangeable.

**Difference:**
- **`Bob Draw`:** Draws bobs to screen immediately (use in non-double-buffered mode)
- **`Bob Update`:** Updates internal bob list (use with Autoback or double buffering)

**Wrong:**
```amos
Double Buffer
Main_Loop:
   Bob Off
   Bob 0,X,Y,1
   Bob Draw        : Rem WRONG with double buffering - doesn't swap
   Wait Vbl
   Goto Main_Loop
```

**Correct:**
```amos
Double Buffer
Autoback 1         : Rem Auto-swap after drawing
Main_Loop:
   Bob Off
   Bob 0,X,Y,1
   Bob Update      : Rem Updates and triggers swap (via Autoback)
   Wait Vbl
   Goto Main_Loop
```

**Or:**
```amos
Double Buffer
Autoback 0         : Rem Manual swap
Main_Loop:
   Bob Off
   Bob 0,X,Y,1
   Bob Draw        : Rem Draw to back buffer
   Screen Swap     : Rem Manual swap
   Wait Vbl
   Goto Main_Loop
```

**Rule:** `Bob Update` with Autoback, OR `Bob Draw` + manual `Screen Swap`.

---

## Sound and Audio Issues

### 10. Sample Play Channels (0-3 Only)

**Problem:** Paula audio chip has 4 channels (0-3). Using channel 4+ fails silently.

**Symptoms:**
- Sound doesn't play
- No error message

**Wrong:**
```amos
Sam Play 5,1  : Rem Channel 5 doesn't exist!
```

**Correct:**
```amos
Sam Play 0,1  : Rem Channel 0 (left)
Sam Play 1,2  : Rem Channel 1 (right)
Sam Play 2,3  : Rem Channel 2 (right)
Sam Play 3,4  : Rem Channel 3 (left)
```

**Stereo Channels:**
- Channels 0 and 3: **Left speaker**
- Channels 1 and 2: **Right speaker**

**Rule:** Audio channels = 0-3 only. Plan stereo accordingly.

---

### 11. Sample Rates and Memory

**Problem:** Higher sample rates sound better but use more memory and CPU.

**Trade-offs:**
```
Sample Rate | Quality | Memory Use | CPU Use
8 kHz       | Low     | Low        | Low
11 kHz      | Medium  | Medium     | Medium
16 kHz      | High    | High       | High
22 kHz      | Very High | Very High | Very High
```

**Best Practice:**
```amos
Rem Game sound effects: 8-11 kHz (balance quality/memory)
Rem Music: 11-16 kHz (better quality worth the memory)
Rem Speech: 8 kHz (comprehensible, low memory)
```

**Rule:** Lower sample rates for effects, higher for music. Balance quality vs memory.

---

## Memory and Performance

### 12. Blitter Busy Flag

**Problem:** Blitter operations are asynchronous. Starting new operation while blitter is busy causes corruption.

**Symptoms:**
- Graphics corruption
- Crashes
- Incomplete blits

**Wrong (Rare in AMOS, but possible with direct blitter access):**
```amos
Bar 0,0 To 100,100      : Rem Starts blitter
Bar 100,100 To 200,200  : Rem May start before first finishes!
```

**AMOS Protection:**
```amos
Rem AMOS automatically waits for blitter to finish
Rem But be aware: rapid-fire blits can slow down
Bar 0,0 To 100,100      : Rem AMOS waits internally
Bar 100,100 To 200,200  : Rem Safe - first blit finished
```

**Rule:** AMOS handles blitter synchronization. Don't worry unless using direct hardware access.

---

### 13. DMA Contention (Chip RAM Slowdown)

**Problem:** CPU and custom chips share Chip RAM bandwidth. Complex graphics = slower CPU.

**DMA Usage by Mode:**
```
Mode           | Bitplanes | DMA Cycles | CPU Time
Low res, 2BP   | 2         | Low        | ~70%
Low res, 5BP   | 5         | High       | ~40%
High res, 4BP  | 4         | Very High  | ~30%
```

**Symptoms:**
- Game slows down with complex graphics
- More colors = slower
- Higher resolution = slower

**Optimization:**
```amos
Rem Use fewer bitplanes (fewer colors)
Screen Open 0,320,256,16,Lowres  : Rem 4 bitplanes = good balance

Rem Instead of:
Screen Open 0,320,256,32,Lowres  : Rem 5 bitplanes = slower CPU
```

**Rule:** More bitplanes/higher resolution = more DMA = slower CPU. Find balance.

---

### 14. Screen Open Memory Allocation

**Problem:** Each screen allocates Chip RAM. Multiple screens = memory exhaustion.

**Memory Requirements (approximate):**
```
Screen Mode         | Memory
320×256, 16 colors  | ~40KB
320×256, 32 colors  | ~50KB
640×256, 16 colors  | ~80KB
640×512, 4 colors   | ~80KB (interlaced)
```

**Wrong (Running out of Chip RAM):**
```amos
Rem Opening too many screens on 512KB Amiga 500
Screen Open 0,320,256,32,Lowres  : Rem ~50KB
Screen Open 1,320,256,32,Lowres  : Rem ~50KB
Screen Open 2,320,256,32,Lowres  : Rem ~50KB
Screen Open 3,320,256,32,Lowres  : Rem ~50KB
Screen Open 4,320,256,32,Lowres  : Rem ~50KB
Rem Total: ~250KB just for screens!
Rem Plus sprite/bob banks, sound samples...
Rem May run out of Chip RAM!
```

**Correct (Conservative memory use):**
```amos
Rem Use 1-2 screens max
Screen Open 0,320,256,32,Lowres
Double Buffer  : Rem Uses same screen, less memory
```

**Rule:** Each screen uses Chip RAM. Minimize screen count. Use double buffering instead of multiple screens.

---

## Animation Issues

### 15. Anim Sequences: Loop vs End

**Problem:** Animation sequences must explicitly loop or end. Forgetting causes animation to stop.

**Sequence Syntax:**
- Numbers: Frame numbers (e.g., `1,2,3,4`)
- `L`: Loop forever
- `E`: End and stop
- `(n,m)`: Repeat frames n to m

**Wrong (Animation stops after one cycle):**
```amos
Anim 1,"1,2,3,4",5  : Rem Plays once, then stops!
Channel To Bob 0,1
Anim On 1
```

**Correct (Looping animation):**
```amos
Anim 1,"(1,2,3,4) L",5  : Rem Loops forever
Channel To Bob 0,1
Anim On 1
```

**Rule:** Always end animation sequences with `L` (loop) or `E` (end). Otherwise they play once and stop.

---

## Procedure and Structure Issues

### 16. Procedure Variables vs Global

**Problem:** Variables inside procedures are local by default. Changes don't affect main program unless `Global` or `Shared`.

**Wrong:**
```amos
X=100
Draw_Sprite
Print X  : Rem Still 100 - procedure didn't change it!

Procedure Draw_Sprite
   X=200  : Rem LOCAL variable X, not global!
End Proc
```

**Correct (Use Global or Shared):**
```amos
Global X  : Rem Declare global at top
X=100
Draw_Sprite
Print X  : Rem Now 200 - global variable changed

Procedure Draw_Sprite
   X=200  : Rem Changes global X
End Proc
```

**Or use parameters:**
```amos
X=100
Draw_Sprite[X]
Print X  : Rem 100 (parameter passed by value)

Procedure Draw_Sprite[PX]
   PX=200  : Rem Changes local copy only
End Proc
```

**Rule:** Use `Global` or `Shared` for variables that procedures need to modify.

---

### 17. Goto vs Gosub in Procedures

**Problem:** Using Goto to call procedures bypasses the call stack. Return has nowhere to return to.

**Wrong:**
```amos
Goto My_Procedure  : Rem Jump without return address
Print "Back"       : Rem Never executes

My_Procedure:
   Print "In procedure"
   Return          : Rem ERROR - no return address!
```

**Correct:**
```amos
Gosub My_Procedure : Rem Call with return address
Print "Back"       : Rem Executes after Return

My_Procedure:
   Print "In procedure"
   Return          : Rem Returns to line after Gosub
```

**Or use Procedure:**
```amos
My_Procedure      : Rem Call procedure
Print "Back"

Procedure My_Procedure
   Print "In procedure"
End Proc          : Rem Auto-return
```

**Rule:** Gosub for labels with Return. Procedure name for Procedure...End Proc blocks.

---

## Summary of Critical Rules

1. ✅ **Sprites = 8 maximum (hardware limit)**
2. ✅ **Sprite width = 16 pixels (fixed)**
3. ✅ **Use Double Buffer before Screen Swap**
4. ✅ **Wait Vbl in every animation loop**
5. ✅ **Colors = $RGB format (12-bit, not 24-bit)**
6. ✅ **Palette registers match color count** (16 colors = 0-15)
7. ✅ **Audio channels = 0-3 only**
8. ✅ **Bob Update with Autoback, OR Bob Draw with Screen Swap**
9. ✅ **Bobs = software, Sprites = hardware** (different commands)
10. ✅ **Animation sequences need L or E**
11. ✅ **Chip RAM required for graphics/sound**
12. ✅ **More bitplanes = slower CPU** (DMA contention)
13. ✅ **Use Global/Shared for procedure variables**
14. ✅ **Gosub for labels, Procedure name for Procedure blocks**
15. ✅ **Conserve Chip RAM** (minimize screens)

---

**This document should be consulted before writing any AMOS lesson code to avoid these common pitfalls.**

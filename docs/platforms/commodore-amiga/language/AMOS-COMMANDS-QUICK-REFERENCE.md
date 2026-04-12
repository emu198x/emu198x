# AMOS Commands Quick Reference

**Purpose:** Fast lookup for AMOS Professional BASIC commands and syntax
**Audience:** Amiga AMOS programmers and curriculum designers
**For comprehensive details:** See AMOS Professional Manual

---

## Essential Commands

### Program Control

| Command | Syntax | Purpose |
|---------|--------|---------|
| RUN | `Run` | Execute program from start |
| STOP | `Stop` | Halt program execution |
| DIRECT | `Direct` | Enter direct mode |
| GOTO | `Goto label` | Jump to label |
| GOSUB | `Gosub label` | Call subroutine |
| RETURN | `Return` | Return from subroutine |
| ON GOTO | `On expression Goto label1,label2...` | Computed jump |
| WAIT | `Wait n` | Wait n VBLs (50Hz PAL, 60Hz NTSC) |
| WAIT VBL | `Wait Vbl` | Wait for next vertical blank |

**AMOS uses labels, not line numbers:**
```amos
Main_Loop:
   Print "Hello"
   Goto Main_Loop
```

### Variables and Input

| Command | Syntax | Purpose |
|---------|--------|---------|
| LET | `variable=expression` | Assign value (LET keyword optional) |
| INPUT | `Input variable` | Get user input |
| INPUT prompt | `Input "prompt";variable` | Prompted input |
| DIM | `Dim array(size)` | Declare array |
| GLOBAL | `Global variable` | Declare global variable |
| SHARED | `Shared variable` | Share variable with procedures |

**Variable Types:**
- Integer: `score%`, `x%`, `y%` (16-bit signed: -32768 to 32767)
- Float: `speed#`, `angle#` (single precision)
- String: `name$`, `text$`
- No suffix: Float by default

### Procedures and Functions

| Command | Syntax | Purpose |
|---------|--------|---------|
| PROCEDURE | `Procedure name[parameters]` | Define procedure |
| END PROC | `End Proc` | End procedure |
| DEF FN | `Def Fn name[parameters]` | Define function |
| CALL | (implicit) | Call by name: `name[arguments]` |

**Example:**
```amos
Procedure Draw_Paddle[X,Y]
   Box X,Y To X+8,Y+32
End Proc

Main_Loop:
   Draw_Paddle[10,100]
   Goto Main_Loop
```

### Conditionals and Loops

| Command | Syntax | Purpose |
|---------|--------|---------|
| IF...THEN | `If condition Then statement` | Conditional execution |
| IF...ELSE...END IF | `If...Else...End If` | Block conditional |
| FOR...NEXT | `For var=start To end` | Counting loop |
| FOR...STEP | `For var=start To end Step inc` | Loop with increment |
| NEXT | `Next var` | End loop |
| WHILE...WEND | `While condition...Wend` | Conditional loop |
| REPEAT...UNTIL | `Repeat...Until condition` | Loop with exit test |
| EXIT IF | `Exit If condition` | Exit loop early |

**Examples:**
```amos
If Score>1000 Then Print "WIN!"

If Lives=0
   Print "GAME OVER"
Else
   Print "Lives: ";Lives
End If

For I=1 To 10
   Print I
Next I

While Key$<>"Q"
   Key$=Inkey$
Wend

Repeat
   X=X+1
Until X>255
```

---

## Screen and Graphics

### Screen Management

| Command | Syntax | Purpose |
|---------|--------|---------|
| SCREEN OPEN | `Screen Open screen,width,height,colours,mode` | Create screen |
| SCREEN CLOSE | `Screen Close screen` | Close screen |
| SCREEN | `Screen screen` | Make screen current |
| SCREEN DISPLAY | `Screen Display screen,x,y,width,height` | Position screen |
| CLS | `Cls [colour]` | Clear screen |
| DOUBLE BUFFER | `Double Buffer` | Enable double buffering |
| SCREEN SWAP | `Screen Swap` | Swap display/work buffers |
| AUTOBACK | `Autoback on/off` | Auto-swap after draw |

**Common Screen Modes:**
- Low res: 320×256 (32 colours, 5 bitplanes)
- Medium res: 640×256 (16 colours, 4 bitplanes)  
- High res: 640×512 (4 colours, 2 bitplanes)

**Example:**
```amos
Screen Open 0,320,256,32,Lowres
Cla 0
Double Buffer
Autoback 0

Main_Loop:
   Cls 0
   ' ... draw frame ...
   Screen Swap
   Wait Vbl
   Goto Main_Loop
```

### Drawing Primitives

| Command | Syntax | Purpose |
|---------|--------|---------|
| PLOT | `Plot x,y[,colour]` | Draw pixel |
| DRAW | `Draw x1,y1 To x2,y2[,colour]` | Draw line |
| BOX | `Box x1,y1 To x2,y2[,colour]` | Draw rectangle outline |
| BAR | `Bar x1,y1 To x2,y2[,colour]` | Draw filled rectangle |
| CIRCLE | `Circle x,y,radius[,colour]` | Draw circle outline |
| DISC | `Disc x,y,radius[,colour]` | Draw filled circle |
| POLYLINE | `Polyline x1,y1 To x2,y2 To ...` | Draw connected lines |
| POLYGON | `Polygon x1,y1 To x2,y2 To ...` | Draw filled polygon |
| PAINT | `Paint x,y[,colour]` | Flood fill |
| POINT | `Point(x,y)` | Read pixel colour |

**Examples:**
```amos
Plot 160,128,15        : Rem White pixel at centre
Draw 0,0 To 319,255,2  : Rem Red diagonal line
Box 50,50 To 100,100   : Rem Rectangle
Bar 120,120 To 200,200,4 : Rem Filled blue rectangle
Circle 160,128,50,3    : Rem Magenta circle
Disc 160,128,30,5      : Rem Filled cyan disc
```

### Colour Commands

| Command | Syntax | Purpose |
|---------|--------|---------|
| INK | `Ink colour` | Set foreground colour |
| PEN | `Pen colour` | Alias for INK |
| PAPER | `Paper colour` | Set background colour |
| COLOUR | `Colour register,rgb` | Set palette entry |
| GET COLOUR | `colour=Colour(register)` | Read palette entry |
| FADE | `Fade distance` | Fade palette |
| COLOUR BACK | `Colour Back` | Restore palette |
| RAINBOW | `Rainbow line,start,end,speed` | Animated colour bars |

**RGB Format:** `$RGB` where R, G, B are 0-15 (4 bits each)

**Examples:**
```amos
Colour 0,$000       : Rem Black
Colour 1,$F00       : Rem Red
Colour 2,$0F0       : Rem Green
Colour 3,$00F       : Rem Blue
Colour 4,$FF0       : Rem Yellow
Colour 5,$F0F       : Rem Magenta
Colour 6,$0FF       : Rem Cyan
Colour 7,$FFF       : Rem White

Ink 2 : Paper 0     : Rem Green on black
Print "Hello"

Fade 1              : Rem Fade out one step
```

---

## Sprites and Bobs

### Sprite Commands

| Command | Syntax | Purpose |
|---------|--------|---------|
| SPRITE | `Sprite number,x,y,image` | Create/move sprite |
| MOVE | `Move number,x,y` | Move sprite to position |
| MOVE X/Y | `Move X number,x` / `Move Y number,y` | Move on single axis |
| SPRITE OFF | `Sprite Off [number]` | Hide sprite(s) |
| SPRITE UPDATE | `Sprite Update` | Update all sprites |
| SPRITE BASE | `Sprite Base address` | Set sprite data bank |
| GET SPRITE PALETTE | `Get Sprite Palette [start,end]` | Load sprite colours |

**Hardware Sprites:**
- Amiga OCS: 8 hardware sprites
- 16 pixels wide (15 colours + transparent)
- Any height
- Automatic collision detection
- Very fast (no CPU overhead)

**Examples:**
```amos
Load "sprites.abk",1    : Rem Load sprite bank
Get Sprite Palette      : Rem Load sprite colours

Sprite 0,100,80,1       : Rem Sprite 0 at (100,80), image 1
Sprite 1,150,80,2       : Rem Sprite 1 at (150,80), image 2

Main_Loop:
   Move X 0,X Sprite(0)+2  : Rem Move sprite 0 right
   Sprite Update           : Rem Update display
   Wait Vbl
   Goto Main_Loop
```

### Bob Commands (Blitter Objects)

| Command | Syntax | Purpose |
|---------|--------|---------|
| BOB | `Bob number,x,y,image` | Create/move bob |
| BOB OFF | `Bob Off [number]` | Hide bob(s) |
| BOB UPDATE | `Bob Update` | Update all bobs |
| BOB CLEAR | `Bob Clear` | Clear all bobs |
| BOB DRAW | `Bob Draw` | Draw bobs to screen |
| SET BOB | `Set Bob number,x,y[,image]` | Set bob position/image |
| BOB BASE | `Bob Base address` | Set bob data bank |

**Bobs vs Sprites:**
- **Sprites:** Hardware, 8 max, 16px wide, very fast
- **Bobs:** Software, unlimited, any size, slower, more flexible

**Examples:**
```amos
Load "bobs.abk",2       : Rem Load bob bank to bank 2
Bob 0,100,100,1         : Rem Bob 0 at (100,100), image 1
Bob 1,200,150,2         : Rem Bob 1 at (200,150), image 2

Main_Loop:
   Bob Off               : Rem Hide all bobs
   Move 0,X Bob(0)+4,Y Bob(0)  : Rem Move bob 0 right
   Bob Draw              : Rem Draw bobs
   Wait Vbl
   Goto Main_Loop
```

### Collision Detection

| Command | Syntax | Purpose |
|---------|--------|---------|
| COL | `c=Col(sprite1,sprite2)` | Test sprite collision |
| BOBCOL | `c=Bobcol(bob1,bob2)` | Test bob collision |
| SPRITEBOB | `c=Spritebob(sprite,bob)` | Test sprite-bob collision |

**Returns:** 0 = no collision, -1 = collision

---

## Animation

### Animation Commands

| Command | Syntax | Purpose |
|---------|--------|---------|
| ANIM | `Anim number,"sequence"[,delay]` | Define animation |
| ANIM ON | `Anim On [number]` | Start animation(s) |
| ANIM OFF | `Anim Off [number]` | Stop animation(s) |
| ANIM FREEZE | `Anim Freeze [number]` | Pause animation(s) |
| CHANNEL TO BOB | `Channel To Bob number,channel` | Assign anim to bob |

**Animation Sequences:**
- Numbers: Frame numbers (e.g., "1,2,3,4")
- `L`: Loop sequence
- `E`: End and stop
- `(n,m)`: Repeat frames n to m

**Examples:**
```amos
Rem Walking animation (4 frames, loop)
Anim 1,"(1,2,3,4,3,2) L",4

Rem Explosion animation (8 frames, end)
Anim 2,"(1,2,3,4,5,6,7,8) E",2

Channel To Bob 0,1      : Rem Assign anim 1 to bob 0
Anim On 1               : Rem Start animation
```

---

## Sound and Music

### Sound Effects

| Command | Syntax | Purpose |
|---------|--------|---------|
| PLAY | `Play frequency,duration` | Play tone |
| PLAY OFF | `Play Off [channel]` | Stop sound |
| BELL | `Bell [volume]` | Play bell sound |
| BOOM | `Boom` | Play explosion |
| SHOOT | `Shoot` | Play laser |
| NOISE | `Noise channel,volume,frequency,duration` | White noise |
| VOLUME | `Volume channel,volume` | Set channel volume (0-63) |

**4 Sound Channels:** 0-3 (Paula sound chip)

**Examples:**
```amos
Play 440,50             : Rem 440Hz (A) for 50 VBLs
Bell 32                 : Rem Medium volume bell
Boom                    : Rem Explosion
Shoot                   : Rem Laser
Noise 3,32,100,20       : Rem White noise on channel 3
Volume 0,63             : Rem Max volume on channel 0
```

### Music (Tracker Modules)

| Command | Syntax | Purpose |
|---------|--------|---------|
| MUSIC | `Music song` | Play tracker module |
| MUSIC OFF | `Music Off` | Stop music |
| MUSIC STOP | `Music Stop` | Pause music |
| MUSIC CONT | `Music Cont` | Resume music |
| TEMPO | `Tempo speed` | Set music tempo |
| VOLUME | `Volume volume` | Set music volume (0-63) |

**Example:**
```amos
Load "game_music.mod",3  : Rem Load music to bank 3
Music 3                  : Rem Play music
Tempo 125                : Rem Default tempo
Volume 48                : Rem 75% volume
```

---

## Input

### Keyboard

| Command | Syntax | Purpose |
|---------|--------|---------|
| INKEY$ | `key$=Inkey$` | Read key (no wait) |
| INPUT | `Input variable` | Wait for input + RETURN |
| SCANCODE | `code=Scancode` | Read raw key scan code |
| KEY STATE | `state=Key State(scancode)` | Test if key down |
| WAIT KEY | `Wait Key` | Wait for any keypress |
| CLEAR KEY | `Clear Key` | Clear keyboard buffer |

**Common Scancodes:**
- Arrow keys: Up=76, Down=77, Left=79, Right=78
- Space=64, Return=68, Esc=69
- Q=16, A=32, Z=49

**Examples:**
```amos
Key$=Inkey$
If Key$="Q" Then Up=True
If Key$="A" Then Down=True
If Key$=" " Then Fire=True

If Key State(76) Then Y=Y-2    : Rem Up arrow
If Key State(77) Then Y=Y+2    : Rem Down arrow
If Key State(64) Then Fire=True : Rem Space
```

### Mouse and Joystick

| Command | Syntax | Purpose |
|---------|--------|---------|
| X MOUSE | `x=X Mouse` | Read mouse X position |
| Y MOUSE | `y=Y Mouse` | Read mouse Y position |
| MOUSE KEY | `button=Mouse Key` | Read mouse buttons (1=left, 2=right) |
| MOUSE CLICK | `clicks=Mouse Click` | Count mouse clicks |
| JLEFT | `state=Jleft(port)` | Test joystick left |
| JRIGHT | `state=Jright(port)` | Test joystick right |
| JUP | `state=Jup(port)` | Test joystick up |
| JDOWN | `state=Jdown(port)` | Test joystick down |
| FIRE | `state=Fire(port)` | Test joystick fire button |

**Joystick Ports:** 0 or 1

**Examples:**
```amos
X=X Mouse : Y=Y Mouse
If Mouse Key=1 Then Print "LEFT CLICK"
If Mouse Key=2 Then Print "RIGHT CLICK"

If Jup(1) Then Paddle_Y=Paddle_Y-4
If Jdown(1) Then Paddle_Y=Paddle_Y+4
If Fire(1) Then Launch_Ball
```

---

## Data and Memory

### DATA Statements

| Command | Syntax | Purpose |
|---------|--------|---------|
| DATA | `Data value1,value2,...` | Define data |
| READ | `Read variable` | Read next data value |
| RESTORE | `Restore [label]` | Reset data pointer |

**Example:**
```amos
Read Name$,Age%,Score%
Print Name$;" is ";Age%;" with score ";Score%

Level_Data:
Data "Alice",25,1500
Data "Bob",30,2000
Data "Carol",22,1800
```

### Memory Management

| Command | Syntax | Purpose |
|---------|--------|---------|
| RESERVE AS WORK | `Reserve As Work bank,size` | Allocate memory bank |
| ERASE | `Erase bank` | Free memory bank |
| POKE | `Poke address,byte` | Write byte to memory |
| PEEK | `value=Peek(address)` | Read byte from memory |
| DOKE | `Doke address,word` | Write word (2 bytes) |
| DEEK | `value=Deek(address)` | Read word |
| LOKE | `Loke address,long` | Write longword (4 bytes) |
| LEEK | `value=Leek(address)` | Read longword |

**Memory Banks:**
- Banks 1-15: User-accessible
- Bank 0: Screen
- Negative banks: System use

---

## File Operations

### Disk Access

| Command | Syntax | Purpose |
|---------|--------|---------|
| LOAD | `Load "filename"[,bank]` | Load file |
| SAVE | `Save "filename"[,bank]` | Save file |
| BLOAD | `Bload "filename",address` | Load to address |
| BSAVE | `Bsave "filename",address,length` | Save from address |
| DIR$ | `file$=Dir$(pattern)` | Get first matching file |
| DFREE | `free=Dfree` | Get free disk space |
| EXIST | `exists=Exist("filename")` | Test if file exists |
| KILL | `Kill "filename"` | Delete file |

**File Types:**
- `.abk`: AMOS bank file (sprites, bobs, music, icons)
- `.amos`: AMOS source code
- `.bin`: Binary data

**Examples:**
```amos
Load "sprites.abk",1           : Rem Load sprites to bank 1
Load "level1.bin",5            : Rem Load level data to bank 5

If Exist("highscore.dat")
   Bload "highscore.dat",Start(10)
Else
   High_Score=0
End If

Bsave "highscore.dat",Start(10),4  : Rem Save 4 bytes
```

---

## Strings and Text

### String Functions

| Function | Syntax | Purpose |
|----------|--------|---------|
| LEN | `length=Len(string$)` | String length |
| LEFT$ | `s$=Left$(string$,n)` | First n characters |
| RIGHT$ | `s$=Right$(string$,n)` | Last n characters |
| MID$ | `s$=Mid$(string$,start,n)` | Substring |
| UPPER$ | `s$=Upper$(string$)` | Convert to uppercase |
| LOWER$ | `s$=Lower$(string$)` | Convert to lowercase |
| CHR$ | `c$=Chr$(code)` | Character from ASCII |
| ASC | `code=Asc(string$)` | ASCII of first char |
| STR$ | `s$=Str$(number)` | Number to string |
| VAL | `number=Val(string$)` | String to number |
| INSTR | `pos=Instr(string$,search$)` | Find substring position |

**Examples:**
```amos
Name$="AMIGA"
Print Len(Name$)                : Rem 5
Print Left$(Name$,2)            : Rem "AM"
Print Right$(Name$,3)           : Rem "IGA"
Print Mid$(Name$,2,3)           : Rem "MIG"
Print Lower$(Name$)             : Rem "amiga"

Score$=Str$(1500)               : Rem "1500"
Number=Val("42")                : Rem 42
```

### Text Output

| Command | Syntax | Purpose |
|---------|--------|---------|
| PRINT | `Print expression` | Display text |
| LOCATE | `Locate x,y` | Set cursor position |
| CENTRE | `Centre "text"` | Print centred text |
| TEXT | `Text x,y,"text"` | Print text at pixel position |
| SET TEXT | `Set Text colour` | Set text colour |
| SET BACK | `Set Back colour` | Set text background |
| WRITING | `Writing mode` | Set text mode (0-3) |

**Text Coordinates:**
- LOCATE: Character positions (40 columns × 32 rows in low res)
- TEXT: Pixel positions (anywhere on screen)

**Examples:**
```amos
Locate 0,0 : Print "TOP LEFT"
Locate 10,15 : Print "SCORE: ";Score
Centre "GAME OVER"
Text 160,128,"CENTRE"
Set Text 2 : Set Back 0        : Rem Red on black
Writing 1                       : Rem XOR mode
```

---

## Mathematical Functions

| Function | Syntax | Purpose |
|----------|--------|---------|
| ABS | `ABS(x)` | Absolute value |
| INT | `INT(x)` | Integer part (floor) |
| SQRT | `SQRT(x)` | Square root |
| SIN | `SIN(x)` | Sine (degrees) |
| COS | `COS(x)` | Cosine (degrees) |
| TAN | `TAN(x)` | Tangent (degrees) |
| ATN | `ATN(x)` | Arctangent (radians) |
| LOG | `LOG(x)` | Natural log |
| EXP | `EXP(x)` | e^x |
| PI | `Pi#` | π constant |
| RND | `RND(max)` | Random integer 0 to max-1 |

**Note:** AMOS trig functions use **degrees**, not radians (unlike most BASICs)

**Examples:**
```amos
Dice=Rnd(6)+1              : Rem 1-6
X=160+Cos(Angle)*50        : Rem Circular motion
Y=128+Sin(Angle)*50
Angle=(Angle+2) Mod 360

Distance=Sqr((X2-X1)^2+(Y2-Y1)^2)  : Rem Pythagorean distance
```

---

## Program Structure

### Typical Game Loop

```amos
'===============================================
' PONG - Simple game example
'===============================================

Curs Off : Hide On : Flash Off : Click Off

'--- Setup Screen ---
Screen Open 0,320,256,32,Lowres
Cla 0
Double Buffer : Autoback 0

'--- Setup Colours ---
Colour 0,$000       : Rem Black
Colour 1,$FFF       : Rem White
Ink 1 : Paper 0

'--- Initialize Game Variables ---
Global Ball_X,Ball_Y,Ball_DX,Ball_DY
Global Paddle1_Y,Paddle2_Y
Global Score1,Score2

Ball_X=160 : Ball_Y=128
Ball_DX=2 : Ball_DY=1
Paddle1_Y=100 : Paddle2_Y=100

'--- Main Loop ---
Main_Loop:
   Cls 0
   
   '--- Input ---
   Gosub Handle_Input
   
   '--- Update ---
   Gosub Update_Ball
   Gosub Update_Paddles
   
   '--- Draw ---
   Gosub Draw_Game
   
   '--- Display ---
   Screen Swap
   Wait Vbl
   
   Goto Main_Loop

'===============================================
' SUBROUTINES
'===============================================

Handle_Input:
   If Key State(76) Then Paddle1_Y=Paddle1_Y-4  : Rem Up arrow
   If Key State(77) Then Paddle1_Y=Paddle1_Y+4  : Rem Down arrow
Return

Update_Ball:
   Ball_X=Ball_X+Ball_DX
   Ball_Y=Ball_Y+Ball_DY
   
   '--- Bounce off top/bottom ---
   If Ball_Y<4 or Ball_Y>252 Then Ball_DY=-Ball_DY
Return

Update_Paddles:
   '--- Keep paddles on screen ---
   If Paddle1_Y<0 Then Paddle1_Y=0
   If Paddle1_Y>224 Then Paddle1_Y=224
Return

Draw_Game:
   '--- Draw paddles ---
   Bar 10,Paddle1_Y To 18,Paddle1_Y+32
   Bar 302,Paddle2_Y To 310,Paddle2_Y+32
   
   '--- Draw ball ---
   Circle Ball_X,Ball_Y,4
   
   '--- Draw score ---
   Locate 10,1 : Print Score1
   Locate 30,1 : Print Score2
Return
```

---

## Common Patterns

### Double Buffering

```amos
Screen Open 0,320,256,32,Lowres
Double Buffer
Autoback 0           : Rem Manual swap

Main_Loop:
   Cls 0              : Rem Clear work buffer
   '... draw to work buffer ...
   Screen Swap        : Rem Swap buffers
   Wait Vbl           : Rem Wait for vertical blank
   Goto Main_Loop
```

### Smooth Movement

```amos
'--- Fixed timestep (50 FPS PAL) ---
X#=X#+DX#            : Rem Use floating point for smooth movement
Y#=Y#+DY#
X=Int(X#) : Y=Int(Y#)  : Rem Convert to integer for drawing
```

### Collision Detection

```amos
'--- Rectangle collision ---
Function Check_Collision[X1,Y1,W1,H1,X2,Y2,W2,H2]
   If X1<X2+W2 and X1+W1>X2
      If Y1<Y2+H2 and Y1+H1>Y2
         End =True
      End If
   End If
   End =False
End Function
```

---

## Performance Tips

### Optimization

1. **Use integers** - `%` suffix for integer variables (16-bit, faster)
2. **Minimize screen updates** - Only redraw changed areas
3. **Use hardware sprites** - Much faster than bobs
4. **Cache calculations** - Store sin/cos tables
5. **Double buffering** - Eliminate flicker
6. **WAIT VBL** - Sync to 50Hz (PAL) or 60Hz (NTSC)

### Memory Management

- **Banks 1-15**: User memory (sprites, bobs, music, data)
- **Reserve As Work**: Pre-allocate memory for data
- **Erase**: Free memory when done
- **CHIP RAM**: Required for DMA (graphics, sound)
- **FAST RAM**: For code and non-DMA data

---

## Error Handling

**Common Errors:**
- `Out of Memory` - Too many sprites/bobs or large graphics
- `Disc Error` - File not found or disk issue
- `Variable not defined` - Use `Global` or `Shared`
- `Bank not reserved` - Allocate bank before use
- `Screen not open` - Open screen before drawing

**Error Recovery:**
```amos
On Error Goto Error_Handler

'... program code ...

Error_Handler:
   Print "Error: ";Errn$;" on line ";Errl
   Resume Next
```

---

## Quick Reference Tables

### Screen Modes

| Mode | Resolution | Colours | Use Case |
|------|----------|---------|----------|
| Lowres | 320×256 | 32 (64 HAM) | Games, demos |
| Medres | 640×256 | 16 | Detailed graphics |
| Hires | 640×512 | 4 | Workbench, tools |
| Laced | 320×512 | 32 | Interlaced displays |

### Sound Channels

| Channel | Usage |
|---------|-------|
| 0 | Sound effects (left) |
| 1 | Sound effects (right) |
| 2 | Sound effects (right) |
| 3 | Sound effects (left) |

**Note:** Channels 0+3 = left, 1+2 = right (stereo separation)

### Common Scancodes

| Key | Code | Key | Code |
|-----|------|-----|------|
| ESC | 69 | Space | 64 |
| Return | 68 | Backspace | 65 |
| Up | 76 | Down | 77 |
| Left | 79 | Right | 78 |
| F1-F10 | 80-89 | Q | 16 |
| A | 32 | Z | 49 |

---

**Version:** 1.0
**Created:** 2025-10-24
**For:** Amiga Phase 0 AMOS Programming

**See Also:**
- AMIGA-HARDWARE-QUICK-REFERENCE.md (chipset details)
- AMOS-PROFESSIONAL-MANUAL.md (comprehensive guide)

**Next:** Amiga Hardware Quick Reference

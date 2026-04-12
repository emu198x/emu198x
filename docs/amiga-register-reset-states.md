# Amiga Custom Chip Register Reset States

Definitive reset values extracted from WinUAE and vAmiga emulator source code.
This fills a gap in the Amiga Hardware Reference Manual, which does not specify
power-on reset values for most custom registers.

## Power-On vs Soft Reset

Both emulators distinguish hard reset (power-on) from soft reset:

- **WinUAE**: `custom_reset(bool hardreset, ...)` -- hard reset clears everything;
  soft reset preserves color registers and LOF state.
- **vAmiga**: `SerResetter(bool hard)` -- `serialize(worker)` zeros all serialized
  fields to 0 on both hard and soft reset. The `operator <<(SerResetter&)` override
  then sets specific post-reset values. Some fields are only reset on hard reset
  (guarded by `isSoftResetter` early return).

Unless noted otherwise, values below are for **hard reset (power-on)**.

### What "Soft Reset" Preserves

In WinUAE, soft reset (`hardreset == false`) skips these operations that
hard reset performs:

- Color register randomization (COLOR00-31 keep their current values)
- LOF state preservation (`lof_store`, `lof_display` keep current values)
- ECS beam counter register initialization (HTOTAL, VTOTAL, HBSTRT/STOP,
  VBSTRT/STOP, etc. keep their programmed values)
- Serial port data register (SDR) is preserved in CIA reset

In vAmiga, soft reset zeros all fields listed in the `serialize()` method
up to the `if (isSoftResetter(worker)) return;` guard. Fields listed after
that guard (typically `clock` and timing-related state) are only reset on
hard reset. The key practical difference: CIA timers, counters, and data
registers are reset on both hard and soft; the CIA clock cycle counter and
sleep state are only reset on hard.

## Sources

- **WinUAE**: `custom.cpp:custom_reset()`, `cia.cpp:CIA_reset()`,
  `audio.cpp:audio_reset()`, `blitter.cpp:blitter_reset()`,
  `disk.cpp:DISK_reset()`, `drawing.cpp:denise_reset()`
- **vAmiga**: Component `operator <<(SerResetter&)` methods in Agnus.cpp,
  CIA.cpp, TOD.cpp, DiskController.cpp, Denise.h/Denise.cpp, Paula.h,
  Blitter.h, Copper.h, StateMachine.h, Memory.cpp


---

## Custom Chip Registers ($DFF000--$DFF1FE)

All registers sorted by address offset from $DFF000.

Key:
- **R** = read-only, **W** = write-only, **RW** = read-write
- "Undefined" = hardware does not define a value; emulators zero it or leave random
- Source agreement: **Both** = WinUAE and vAmiga agree, otherwise noted

### Read-Only Registers

| Register  | Offset | Dir | Reset Value | Source | Notes |
|-----------|--------|-----|-------------|--------|-------|
| BLTDDAT   | $000   | R   | N/A         | Both   | Dummy address; early read of blitter dest. Returns last bus value. |
| DMACONR   | $002   | R   | $0000       | Both   | Reflects DMACON. All DMA off at reset. |
| VPOSR     | $004   | R   | $0000+      | Both   | Returns current beam V8-V10 + LOF + chip ID. LOF=0 at hard reset (WinUAE); LOF=1 (vAmiga sets pos.lof=true). Chip ID bits are read from hardware revision. |
| VHPOSR    | $006   | R   | $0000       | Both   | Returns current beam position (V7-V0, H8-H0). Position is 0,0 at reset. |
| DSKDATR   | $008   | R   | N/A         | Both   | Dummy address; early read of disk data. |
| JOY0DAT   | $00A   | R   | $0000       | Both   | Joystick/mouse 0 position counters. Zero at reset. |
| JOY1DAT   | $00C   | R   | $0000       | Both   | Joystick/mouse 1 position counters. Zero at reset. |
| CLXDAT    | $00E   | R   | $0000       | Both   | Collision data. Cleared at reset. Read-and-clear. |
| ADKCONR   | $010   | R   | $0000       | Both   | Reflects ADKCON. Zero at reset. |
| POT0DAT   | $012   | R   | $0000       | Both   | Potentiometer counter pair 0. Zero at reset. |
| POT1DAT   | $014   | R   | $0000       | Both   | Potentiometer counter pair 1. Zero at reset. |
| POTGOR    | $016   | R   | $0000       | Both   | Pot pin data read. Returns current pin state. |
| SERDATR   | $018   | R   | varies      | Both   | Serial port data and status. Depends on serial state. |
| DSKBYTR   | $01A   | R   | $0000       | Both   | Disk byte and status. No disk activity at reset. |
| INTENAR   | $01C   | R   | $0000       | Both   | Reflects INTENA. All interrupts disabled at reset. |
| INTREQR   | $01E   | R   | $0000       | Both   | Reflects INTREQ. No pending interrupts at reset. |
| LISAID    | $07C   | R   | varies      | Both   | ECS Denise chip ID. Not a resettable register; reflects chip revision. |
| COPINS    | $08C   | R   | $0000       | Both   | Copper instruction fetch identify. |
| HHPOSR    | $1DA   | R   | $0000       | Both   | ECS only. DUAL mode hires H beam counter read. |

### Write-Only Registers -- DMA Pointers

These are DMA address pointer registers. They are **not explicitly cleared** by
the reset hardware; emulators zero them as part of clearing all state. In practice,
Kickstart always sets these before use. Treat as **undefined at power-on**.

| Register  | Offset | Reset Value | Source | Notes |
|-----------|--------|-------------|--------|-------|
| DSKPTH    | $020   | $0000       | Both   | Disk pointer high. Undefined until set by software. |
| DSKPTL    | $022   | $0000       | Both   | Disk pointer low. |
| AUD0LCH   | $0A0   | $0000       | Both   | Audio channel 0 location high. |
| AUD0LCL   | $0A2   | $0000       | Both   | Audio channel 0 location low. |
| AUD1LCH   | $0B0   | $0000       | Both   | Audio channel 1 location high. |
| AUD1LCL   | $0B2   | $0000       | Both   | Audio channel 1 location low. |
| AUD2LCH   | $0C0   | $0000       | Both   | Audio channel 2 location high. |
| AUD2LCL   | $0C2   | $0000       | Both   | Audio channel 2 location low. |
| AUD3LCH   | $0D0   | $0000       | Both   | Audio channel 3 location high. |
| AUD3LCL   | $0D2   | $0000       | Both   | Audio channel 3 location low. |
| BPL1PTH   | $0E0   | $0000       | Both   | Bitplane 1 pointer high. |
| BPL1PTL   | $0E2   | $0000       | Both   | Bitplane 1 pointer low. |
| BPL2PTH   | $0E4   | $0000       | Both   | Bitplane 2 pointer high. |
| BPL2PTL   | $0E6   | $0000       | Both   | Bitplane 2 pointer low. |
| BPL3PTH   | $0E8   | $0000       | Both   | Bitplane 3 pointer high. |
| BPL3PTL   | $0EA   | $0000       | Both   | Bitplane 3 pointer low. |
| BPL4PTH   | $0EC   | $0000       | Both   | Bitplane 4 pointer high. |
| BPL4PTL   | $0EE   | $0000       | Both   | Bitplane 4 pointer low. |
| BPL5PTH   | $0F0   | $0000       | Both   | Bitplane 5 pointer high. |
| BPL5PTL   | $0F2   | $0000       | Both   | Bitplane 5 pointer low. |
| BPL6PTH   | $0F4   | $0000       | Both   | Bitplane 6 pointer high. |
| BPL6PTL   | $0F6   | $0000       | Both   | Bitplane 6 pointer low. |
| BPL7PTH   | $0F8   | $0000       | Both   | AGA only. Bitplane 7 pointer high. |
| BPL7PTL   | $0FA   | $0000       | Both   | AGA only. Bitplane 7 pointer low. |
| BPL8PTH   | $0FC   | $0000       | Both   | AGA only. Bitplane 8 pointer high. |
| BPL8PTL   | $0FE   | $0000       | Both   | AGA only. Bitplane 8 pointer low. |
| SPR0PTH   | $120   | $0000       | Both   | Sprite 0 pointer high. |
| SPR0PTL   | $122   | $0000       | Both   | Sprite 0 pointer low. |
| SPR1PTH   | $124   | $0000       | Both   | Sprite 1 pointer high. |
| SPR1PTL   | $126   | $0000       | Both   | Sprite 1 pointer low. |
| SPR2PTH   | $128   | $0000       | Both   | Sprite 2 pointer high. |
| SPR2PTL   | $12A   | $0000       | Both   | Sprite 2 pointer low. |
| SPR3PTH   | $12C   | $0000       | Both   | Sprite 3 pointer high. |
| SPR3PTL   | $12E   | $0000       | Both   | Sprite 3 pointer low. |
| SPR4PTH   | $130   | $0000       | Both   | Sprite 4 pointer high. |
| SPR4PTL   | $132   | $0000       | Both   | Sprite 4 pointer low. |
| SPR5PTH   | $134   | $0000       | Both   | Sprite 5 pointer high. |
| SPR5PTL   | $136   | $0000       | Both   | Sprite 5 pointer low. |
| SPR6PTH   | $138   | $0000       | Both   | Sprite 6 pointer high. |
| SPR6PTL   | $13A   | $0000       | Both   | Sprite 6 pointer low. |
| SPR7PTH   | $13C   | $0000       | Both   | Sprite 7 pointer high. |
| SPR7PTL   | $13E   | $0000       | Both   | Sprite 7 pointer low. |
| COP1LCH   | $080   | $0000       | Both   | Copper list 1 location high. |
| COP1LCL   | $082   | $0000       | Both   | Copper list 1 location low. |
| COP2LCH   | $084   | $0000       | Both   | Copper list 2 location high. |
| COP2LCL   | $086   | $0000       | Both   | Copper list 2 location low. |

### Write-Only Registers -- Control and Data

| Register  | Offset | Reset Value | Source | Notes |
|-----------|--------|-------------|--------|-------|
| DSKLEN    | $024   | $0000       | Both   | WinUAE calls `DSKLEN(0,0)`. Disk DMA length = 0, DMA disabled. |
| DSKDAT    | $026   | N/A         | Both   | Dummy address; disk DMA data write. |
| REFPTR    | $028   | $0000/$1FFFFE | WinUAE | OCS/ECS: $0000. AGA: $1FFFFE. vAmiga does not model this. |
| VPOSW     | $02A   | N/A         | Both   | Write beam V position. Not a latched register in the traditional sense. |
| VHPOSW    | $02C   | N/A         | Both   | Write beam H+V position. |
| COPCON    | $02E   | $0000       | Both   | Copper control. CDANG=0, Copper cannot access upper register range. |
| SERDAT    | $030   | $0000       | Both   | Serial port data write. |
| SERPER    | $032   | $0000       | Both   | Serial port period and control. |
| POTGO     | $034   | $0000       | Both   | Pot count start / pin drive enable. vAmiga zeros potgo. |
| JOYTEST   | $036   | N/A         | Both   | Write to joystick counters. Strobe register, no latch. |
| STREQU    | $038   | N/A         | Both   | Strobe: horiz sync with VB and EQU. |
| STRVBL    | $03A   | N/A         | Both   | Strobe: horiz sync with VB. |
| STRHOR    | $03C   | N/A         | Both   | Strobe: horiz sync. |
| STRLONG   | $03E   | N/A         | Both   | Strobe: long line identification. |

### Write-Only Registers -- Blitter

| Register  | Offset | Reset Value | Source | Notes |
|-----------|--------|-------------|--------|-------|
| BLTCON0   | $040   | $0000       | Both   | Blitter control 0. All function bits clear. |
| BLTCON1   | $042   | $0000       | Both   | Blitter control 1. No fill, no line mode. |
| BLTAFWM   | $044   | $0000       | Both   | First word mask for source A. |
| BLTALWM   | $046   | $0000       | Both   | Last word mask for source A. |
| BLTCPTH   | $048   | $0000       | Both   | Blitter source C pointer high. |
| BLTCPTL   | $04A   | $0000       | Both   | Blitter source C pointer low. |
| BLTBPTH   | $04C   | $0000       | Both   | Blitter source B pointer high. |
| BLTBPTL   | $04E   | $0000       | Both   | Blitter source B pointer low. |
| BLTAPTH   | $050   | $0000       | Both   | Blitter source A pointer high. |
| BLTAPTL   | $052   | $0000       | Both   | Blitter source A pointer low. |
| BLTDPTH   | $054   | $0000       | Both   | Blitter dest D pointer high. |
| BLTDPTL   | $056   | $0000       | Both   | Blitter dest D pointer low. |
| BLTSIZE   | $058   | $0000       | Both   | Blitter size (triggers blit). Not triggered at reset. |
| BLTCON0L  | $05A   | $0000       | Both   | ECS Agnus. Lower 8 bits of BLTCON0. |
| BLTSIZV   | $05C   | $0000       | Both   | ECS Agnus. Blitter V size for 15-bit vert. |
| BLTSIZH   | $05E   | $0000       | Both   | ECS Agnus. Blitter H size (triggers blit). |
| BLTCMOD   | $060   | $0000       | Both   | Blitter source C modulo. |
| BLTBMOD   | $062   | $0000       | Both   | Blitter source B modulo. |
| BLTAMOD   | $064   | $0000       | Both   | Blitter source A modulo. |
| BLTDMOD   | $066   | $0000       | Both   | Blitter dest D modulo. |
| BLTCDAT   | $070   | $0000       | Both   | Blitter source C data. |
| BLTBDAT   | $072   | $0000       | Both   | Blitter source B data. |
| BLTADAT   | $074   | $0000       | Both   | Blitter source A data. |

### Write-Only Registers -- Display Control

| Register  | Offset | Reset Value | Source | Notes |
|-----------|--------|-------------|--------|-------|
| DIWSTRT   | $08E   | $0000       | Both   | Display window start. vAmiga zeros diwstrt. |
| DIWSTOP   | $090   | $0000       | Both   | Display window stop. |
| DDFSTRT   | $092   | $0000       | Both   | Display data fetch start. |
| DDFSTOP   | $094   | $0000       | Both   | Display data fetch stop. |
| DMACON    | $096   | $0000       | Both   | DMA control write. All DMA channels off. Master enable off. |
| CLXCON    | $098   | $0000       | Both   | Collision control. WinUAE calls `CLXCON(0)`. vAmiga zeros clxcon. |
| INTENA    | $09A   | $0000       | Both   | Interrupt enable. All interrupts disabled. Master enable off. |
| INTREQ    | $09C   | $0000       | Both   | Interrupt request. No pending interrupts. |
| ADKCON    | $09E   | $0000       | Both   | Audio/disk/UART control. All bits clear. |
| COPJMP1   | $088   | N/A         | Both   | Strobe: restart Copper at COP1LC. Not triggered at reset. |
| COPJMP2   | $08A   | N/A         | Both   | Strobe: restart Copper at COP2LC. |

### Write-Only Registers -- Audio Channel Data

Audio channels are zeroed by `memset` (WinUAE) or serialize-to-zero (vAmiga),
with WinUAE then setting period to PERIOD_MAX (effectively infinite).

| Register  | Offset | Reset Value | Source | Notes |
|-----------|--------|-------------|--------|-------|
| AUD0LEN   | $0A4   | $0000       | Both   | Audio ch 0 length. |
| AUD0PER   | $0A6   | $0000*      | Both   | Audio ch 0 period. WinUAE internal: PERIOD_MAX. Register value: 0. |
| AUD0VOL   | $0A8   | $0000       | Both   | Audio ch 0 volume. |
| AUD0DAT   | $0AA   | $0000       | Both   | Audio ch 0 data. |
| AUD1LEN   | $0B4   | $0000       | Both   | Audio ch 1 length. |
| AUD1PER   | $0B6   | $0000*      | Both   | Audio ch 1 period. |
| AUD1VOL   | $0B8   | $0000       | Both   | Audio ch 1 volume. |
| AUD1DAT   | $0BA   | $0000       | Both   | Audio ch 1 data. |
| AUD2LEN   | $0C4   | $0000       | Both   | Audio ch 2 length. |
| AUD2PER   | $0C6   | $0000*      | Both   | Audio ch 2 period. |
| AUD2VOL   | $0C8   | $0000       | Both   | Audio ch 2 volume. |
| AUD2DAT   | $0CA   | $0000       | Both   | Audio ch 2 data. |
| AUD3LEN   | $0D4   | $0000       | Both   | Audio ch 3 length. |
| AUD3PER   | $0D6   | $0000*      | Both   | Audio ch 3 period. |
| AUD3VOL   | $0D8   | $0000       | Both   | Audio ch 3 volume. |
| AUD3DAT   | $0DA   | $0000       | Both   | Audio ch 3 data. |

### Write-Only Registers -- Bitplane Control

| Register  | Offset | Reset Value | Source | Notes |
|-----------|--------|-------------|--------|-------|
| BPLCON0   | $100   | $0000       | Both   | Bitplane control 0. Zero planes, lores, no genlock, etc. |
| BPLCON1   | $102   | $0000       | Both   | Bitplane scroll. No scroll offset. |
| BPLCON2   | $104   | $0000       | Both   | Bitplane priority. Playfield 1 priority, sprites behind. |
| BPLCON3   | $106   | $0C00       | Both   | ECS Denise. WinUAE: `bplcon3 = 0x0C00`. vAmiga: serialize zeros it (OCS has no BPLCON3; ECS/AGA Denise would need Kickstart to set it). Note: WinUAE forces $0C00 as reset default. |
| BPL1MOD   | $108   | $0000       | Both   | Bitplane modulo (odd). |
| BPL2MOD   | $10A   | $0000       | Both   | Bitplane modulo (even). |
| BPLCON4   | $10C   | $0011       | WinUAE | AGA only. WinUAE: `bplcon4 = 0x0011` to force AGA into ECS compat mode. vAmiga zeros it (does not model AGA). |
| CLXCON2   | $10E   | $0000       | Both   | AGA extended collision control. WinUAE calls `CLXCON2(0)`. |

### Write-Only Registers -- Bitplane Data

| Register  | Offset | Reset Value | Source | Notes |
|-----------|--------|-------------|--------|-------|
| BPL1DAT   | $110   | $0000       | Both   | Bitplane 1 data. Triggers shift register load. |
| BPL2DAT   | $112   | $0000       | Both   | Bitplane 2 data. |
| BPL3DAT   | $114   | $0000       | Both   | Bitplane 3 data. |
| BPL4DAT   | $116   | $0000       | Both   | Bitplane 4 data. |
| BPL5DAT   | $118   | $0000       | Both   | Bitplane 5 data. |
| BPL6DAT   | $11A   | $0000       | Both   | Bitplane 6 data. |
| BPL7DAT   | $11C   | $0000       | Both   | AGA only. Bitplane 7 data. |
| BPL8DAT   | $11E   | $0000       | Both   | AGA only. Bitplane 8 data. |

### Write-Only Registers -- Sprite Position, Control, and Data

All sprite registers are zeroed at reset. WinUAE: `memset(spr, 0, sizeof spr)`.
vAmiga: serialize zeros all sprpos[], sprctl[], sprdata[], sprdatb[].

| Register  | Offset | Reset Value | Source | Notes |
|-----------|--------|-------------|--------|-------|
| SPR0POS   | $140   | $0000       | Both   | Sprite 0 V/H start position. |
| SPR0CTL   | $142   | $0000       | Both   | Sprite 0 control (V stop, attach, etc). |
| SPR0DATA  | $144   | $0000       | Both   | Sprite 0 image data A. |
| SPR0DATB  | $146   | $0000       | Both   | Sprite 0 image data B. |
| SPR1POS   | $148   | $0000       | Both   | Sprite 1 position. |
| SPR1CTL   | $14A   | $0000       | Both   | Sprite 1 control. |
| SPR1DATA  | $14C   | $0000       | Both   | Sprite 1 data A. |
| SPR1DATB  | $14E   | $0000       | Both   | Sprite 1 data B. |
| SPR2POS   | $150   | $0000       | Both   | Sprite 2 position. |
| SPR2CTL   | $152   | $0000       | Both   | Sprite 2 control. |
| SPR2DATA  | $154   | $0000       | Both   | Sprite 2 data A. |
| SPR2DATB  | $156   | $0000       | Both   | Sprite 2 data B. |
| SPR3POS   | $158   | $0000       | Both   | Sprite 3 position. |
| SPR3CTL   | $15A   | $0000       | Both   | Sprite 3 control. |
| SPR3DATA  | $15C   | $0000       | Both   | Sprite 3 data A. |
| SPR3DATB  | $15E   | $0000       | Both   | Sprite 3 data B. |
| SPR4POS   | $160   | $0000       | Both   | Sprite 4 position. |
| SPR4CTL   | $162   | $0000       | Both   | Sprite 4 control. |
| SPR4DATA  | $164   | $0000       | Both   | Sprite 4 data A. |
| SPR4DATB  | $166   | $0000       | Both   | Sprite 4 data B. |
| SPR5POS   | $168   | $0000       | Both   | Sprite 5 position. |
| SPR5CTL   | $16A   | $0000       | Both   | Sprite 5 control. |
| SPR5DATA  | $16C   | $0000       | Both   | Sprite 5 data A. |
| SPR5DATB  | $16E   | $0000       | Both   | Sprite 5 data B. |
| SPR6POS   | $170   | $0000       | Both   | Sprite 6 position. |
| SPR6CTL   | $172   | $0000       | Both   | Sprite 6 control. |
| SPR6DATA  | $174   | $0000       | Both   | Sprite 6 data A. |
| SPR6DATB  | $176   | $0000       | Both   | Sprite 6 data B. |
| SPR7POS   | $178   | $0000       | Both   | Sprite 7 position. |
| SPR7CTL   | $17A   | $0000       | Both   | Sprite 7 control. |
| SPR7DATA  | $17C   | $0000       | Both   | Sprite 7 data A. |
| SPR7DATB  | $17E   | $0000       | Both   | Sprite 7 data B. |

### Write-Only Registers -- Color

| Register  | Offset | Reset Value | Source | Notes |
|-----------|--------|-------------|--------|-------|
| COLOR00   | $180   | $000 (OCS) / $FFF (ECS) | WinUAE | COLOR00 is special. WinUAE: ECS Denise (non-AGA) or Denise A1000 sets COLOR00=$FFF; OCS and AGA set COLOR00=$000. vAmiga: zeros all colors via serialize. COLOR01-31: random values (WinUAE fills with `uaerand()`). |
| COLOR01   | $182   | random      | WinUAE | Filled with random 12-bit values. vAmiga zeros them. |
| COLOR02   | $184   | random      | WinUAE | Same as above. |
| COLOR03   | $186   | random      | WinUAE | Same as above. |
| COLOR04   | $188   | random      | WinUAE | Same as above. |
| COLOR05   | $18A   | random      | WinUAE | Same as above. |
| COLOR06   | $18C   | random      | WinUAE | Same as above. |
| COLOR07   | $18E   | random      | WinUAE | Same as above. |
| COLOR08   | $190   | random      | WinUAE | Same as above. |
| COLOR09   | $192   | random      | WinUAE | Same as above. |
| COLOR10   | $194   | random      | WinUAE | Same as above. |
| COLOR11   | $196   | random      | WinUAE | Same as above. |
| COLOR12   | $198   | random      | WinUAE | Same as above. |
| COLOR13   | $19A   | random      | WinUAE | Same as above. |
| COLOR14   | $19C   | random      | WinUAE | Same as above. |
| COLOR15   | $19E   | random      | WinUAE | Same as above. |
| COLOR16   | $1A0   | random      | WinUAE | Same as above. |
| COLOR17   | $1A2   | random      | WinUAE | Same as above. |
| COLOR18   | $1A4   | random      | WinUAE | Same as above. |
| COLOR19   | $1A6   | random      | WinUAE | Same as above. |
| COLOR20   | $1A8   | random      | WinUAE | Same as above. |
| COLOR21   | $1AA   | random      | WinUAE | Same as above. |
| COLOR22   | $1AC   | random      | WinUAE | Same as above. |
| COLOR23   | $1AE   | random      | WinUAE | Same as above. |
| COLOR24   | $1B0   | random      | WinUAE | Same as above. |
| COLOR25   | $1B2   | random      | WinUAE | Same as above. |
| COLOR26   | $1B4   | random      | WinUAE | Same as above. |
| COLOR27   | $1B6   | random      | WinUAE | Same as above. |
| COLOR28   | $1B8   | random      | WinUAE | Same as above. |
| COLOR29   | $1BA   | random      | WinUAE | Same as above. |
| COLOR30   | $1BC   | random      | WinUAE | Same as above. |
| COLOR31   | $1BE   | random      | WinUAE | Same as above. |

### Write-Only Registers -- ECS/AGA Beam Counter Programmable Registers

These registers exist only on ECS Agnus and ECS/AGA Denise. On OCS hardware,
writes are ignored. WinUAE initialises most to $FFFF, which effectively
disables them -- the "never match" sentinel ensures the standard hardwired
sync/blank timing is used until software programs them via BEAMCON0.

The two "total" registers use specific constants:
- HTOTAL: `MAXHPOS_ROWS - 1` = 255 ($FF). Max horizontal counter value.
- VTOTAL: `MAXVPOS_LINES_ECS - 1` = 2047 ($7FF). Max vertical counter value.

| Register  | Offset | Reset Value | Source | Notes |
|-----------|--------|-------------|--------|-------|
| HTOTAL    | $1C0   | $00FF       | WinUAE | ECS Agnus. MAXHPOS_ROWS(256)-1 = 255. Maximum horizontal count. vAmiga does not model programmable beam counters. |
| HSSTOP    | $1C2   | $0000       | WinUAE | ECS Denise. Horizontal sync stop position. |
| HBSTRT    | $1C4   | $FFFF       | WinUAE | ECS Denise. Horizontal blank start. $FFFF = never match (disabled). |
| HBSTOP    | $1C6   | $FFFF       | WinUAE | ECS Denise. Horizontal blank stop. $FFFF = never match (disabled). |
| VTOTAL    | $1C8   | $07FF       | WinUAE | ECS Agnus. MAXVPOS_LINES_ECS(2048)-1 = 2047. Maximum vertical count. |
| VSSTOP    | $1CA   | $FFFF       | WinUAE | ECS Agnus. Vertical sync stop. $FFFF = never match (disabled). |
| VBSTRT    | $1CC   | $FFFF       | WinUAE | ECS Agnus. Vertical blank start. $FFFF = never match (disabled). |
| VBSTOP    | $1CE   | $FFFF       | WinUAE | ECS Agnus. Vertical blank stop. $FFFF = never match (disabled). |
| SPRHSTRT  | $1D0   | $FFFF       | WinUAE | ECS Agnus. UHRES sprite region start. Disabled. |
| SPRHSTOP  | $1D2   | $FFFF       | WinUAE | ECS Agnus. UHRES sprite region stop. Disabled. |
| BPLHSTRT  | $1D4   | $FFFF       | WinUAE | ECS Agnus. UHRES bitplane region start. Disabled. |
| BPLHSTOP  | $1D6   | $FFFF       | WinUAE | ECS Agnus. UHRES bitplane region stop. Disabled. |
| HHPOSW    | $1D8   | $0000       | Both   | ECS Agnus. DUAL mode hires H counter write. |
| BEAMCON0  | $1DC   | $0000 (NTSC) / $0020 (PAL) | Both | ECS Agnus. Bit 5 (PAL) set for PAL systems. BEAMCON0_PAL = $0020. All programmable beam features disabled (VARBEAMEN=0, etc). |
| HSSTRT    | $1DE   | $0000       | WinUAE | ECS Denise. Horizontal sync start. WinUAE comment: "jtxrules / illusion assumes HSSTRT==0". |
| VSSTRT    | $1E0   | $FFFF       | WinUAE | ECS Denise. Vertical sync start. $FFFF = never match (disabled). |
| HCENTER   | $1E2   | $FFFF       | WinUAE | ECS Denise. Horizontal center (for interlace vsync). Disabled. |
| DIWHIGH   | $1E4   | $0000       | Both   | ECS Agnus+Denise. Display window upper bits for start/stop. Zero extends DIWSTRT/STOP to 11-bit resolution. |

### Write-Only Registers -- Disk Sync and Fetch Mode

| Register  | Offset | Reset Value | Source | Notes |
|-----------|--------|-------------|--------|-------|
| DSKSYNC   | $07E   | $0000 / $4489 | **Disagree** | WinUAE: 0 (disk.cpp does not set dsksync explicitly; it's part of memset-cleared state). vAmiga: `dsksync = 0x4489` (standard MFM sync word). See Disagreements. |
| FMODE     | $1FC   | $0000       | Both   | AGA fetch mode. WinUAE: `FMODE(0)`. vAmiga: serialize zeros it (OCS only). |
| NULL      | $1FE   | N/A         | Both   | No-op register / last refresh cycle indicator. |

### Unused/Reserved Registers

These offsets have no associated hardware function. Reading them returns the
last value on the data bus. Writing to them has no effect. They are listed here
for completeness to account for every word in the $DFF000-$DFF1FE range.

| Offset(s)           | Notes |
|---------------------|-------|
| $068, $06A, $06C, $06E | Between blitter modulo and blitter data registers. |
| $076                | Between BLTADAT and UHRES registers. |
| $078                | UHRES sprite pointer/data identifier (ext logic, never implemented). |
| $07A                | UHRES bitplane identifier (ext logic, never implemented). |
| $0AC, $0AE          | After AUD0DAT, before AUD1LCH. |
| $0BC, $0BE          | After AUD1DAT, before AUD2LCH. |
| $0CC, $0CE          | After AUD2DAT, before AUD3LCH. |
| $0DC, $0DE          | After AUD3DAT, before BPL1PTH. |
| $1E6                | UHRES bitplane modulo (never implemented). |
| $1E8, $1EA          | UHRES sprite pointer (never implemented). |
| $1EC, $1EE          | VRAM UHRES bitplane pointer (never implemented). |
| $1F0-$1FA           | Reserved (6 word-aligned slots). |


---

## CIA Registers

Both CIA-A ($BFE001, directly addressable at odd bytes) and CIA-B ($BFD000,
directly addressable at even bytes) use the MOS 8520 CIA chip. The Amiga maps
each CIA register at every 256-byte boundary within its address space.

**CIA-A base**: $BFE001 (active on odd bytes, accent-decoded via A12)
**CIA-B base**: $BFD000 (active on even bytes, accent-decoded via A13)

### Register Offset Map

| 8520 Offset | Register | CIA-A Address | CIA-B Address |
|-------------|----------|---------------|---------------|
| $0          | PRA      | $BFE001       | $BFD000       |
| $1          | PRB      | $BFE101       | $BFD100       |
| $2          | DDRA     | $BFE201       | $BFD200       |
| $3          | DDRB     | $BFE301       | $BFD300       |
| $4          | TALO     | $BFE401       | $BFD400       |
| $5          | TAHI     | $BFE501       | $BFD500       |
| $6          | TBLO     | $BFE601       | $BFD600       |
| $7          | TBHI     | $BFE701       | $BFD700       |
| $8          | TODLO    | $BFE801       | $BFD800       |
| $9          | TODMID   | $BFE901       | $BFD900       |
| $A          | TODHI    | $BFEA01       | $BFDA00       |
| $B          | (unused) | $BFEB01       | $BFDB00       |
| $C          | SDR      | $BFEC01       | $BFDC00       |
| $D          | ICR      | $BFED01       | $BFDD00       |
| $E          | CRA      | $BFEE01       | $BFDE00       |
| $F          | CRB      | $BFEF01       | $BFDF00       |

### Reset Values

WinUAE: `CIA_reset()` in cia.cpp -- `memset(&cia, 0, sizeof(cia))` then sets
specific fields. vAmiga: `CIA::operator<<(SerResetter&)` -- serialize zeros all
fields, then overrides counterA, counterB, latchA, latchB, cnt, irq.

| Register | CIA-A Reset | CIA-B Reset | Source | Notes |
|----------|-------------|-------------|--------|-------|
| PRA      | $00         | $8C         | WinUAE | CIA-A: All zero. Bit 0 (OVL) = 0 (ROM overlay enabled). Bit 1 (/LED) = 0 (power LED bright). Bits 2-7 are inputs reflecting drive and fire button state. CIA-B: WinUAE sets `cia[1].pra = 0x8C` (bits 7,3,2 set: /DTR inactive, step direction outward, side 0 selected). vAmiga: serialize zeros pra, then `updatePA()/updatePB()` recomputes from external pin state. |
| PRB      | $00         | $00/$FF     | **Disagree** | WinUAE: memset zeros both. Calls `DISK_select_set(cia[1].prb)`. CIA-B PRB controls drive selection (/SEL0-3), motor, direction, side, step. vAmiga DiskController sets `prb = 0xFF` at reset (all drives deselected, motor off, all active-low signals high = inactive). |
| DDRA     | $00         | $00         | Both   | Data direction: all pins configured as inputs. Kickstart boot code sets CIA-A DDRA to $03 (bits 0-1 as outputs for OVL and /LED). |
| DDRB     | $00         | $00         | Both   | Data direction: all pins configured as inputs. Kickstart configures CIA-B DDRB for drive control outputs. |
| TALO     | $FF         | $FF         | Both   | Timer A low byte. Part of 16-bit counter. |
| TAHI     | $FF         | $FF         | Both   | Timer A high byte. Full 16-bit value = $FFFF. WinUAE: `cia[n].t[0].timer = 0xffff`. vAmiga: `counterA = 0xFFFF`. 8520 datasheet confirms timers reset to $FFFF. |
| TBLO     | $FF         | $FF         | Both   | Timer B low byte. |
| TBHI     | $FF         | $FF         | Both   | Timer B high byte. Full value = $FFFF. |
| Latch A  | $FFFF       | $FFFF       | Both   | Timer A latch (reload value). WinUAE: `cia[n].t[0].latch = 0xffff`. vAmiga: `latchA = 0xFFFF`. |
| Latch B  | $FFFF       | $FFFF       | Both   | Timer B latch. Same pattern. |
| TODLO    | $00         | $00         | Both   | TOD counter low byte. |
| TODMID   | $00         | $00         | Both   | TOD counter mid byte. |
| TODHI    | $00 / $01   | $00 / $01   | **Disagree** | WinUAE: memset zeros TOD entirely ($000000). vAmiga hard reset: `tod.hi = 0x1` giving TOD = $010000. See Disagreements section. |
| Alarm    | $000000     | $000000     | Both   | TOD alarm register zeroed. |
| TOD Latch | $000000    | $000000     | Both   | TOD latch register zeroed. Unlatched state. |
| SDR      | $00 (hard) / preserved (soft) | Same | WinUAE | Serial data register. WinUAE preserves SDR on soft reset: saves `cia[n].sdr` before memset, restores after. Hard reset zeros it. vAmiga: serialize zeros sdr on both hard and soft. |
| ICR      | $00         | $00         | Both   | Interrupt control register (read clears). No pending interrupts. |
| IMR      | $00         | $00         | Both   | Interrupt mask register. All interrupt sources disabled. |
| CRA      | $00         | $00         | Both   | Control register A. Timer A stopped (START=0), continuous mode (RUNMODE=0), PB6 output disabled (PBON=0). |
| CRB      | $00 / $04   | $00 / $04   | **Disagree** | WinUAE: memset zeros CRA/CRB. vAmiga source comment: "UAE initializes CRB with 4 (which I think is wrong)." vAmiga sets CRB=$04 only when `debug::MIMIC_UAE` is true; otherwise CRB=$00. Bit 2 of CRB = PBON (PB7 output mode). See Disagreements. |

### CIA Internal State

| State     | CIA-A       | CIA-B       | Source | Notes |
|-----------|-------------|-------------|--------|-------|
| CNT pin   | High (1)    | High (1)    | vAmiga | vAmiga: `cnt = true`. |
| IRQ line  | High (1) = inactive | Same | vAmiga | vAmiga: `irq = 1` (active low; 1 = not asserted). |
| TOD running | Stopped   | Stopped     | vAmiga | vAmiga hard reset: `stopped = true`. TOD does not run until written. |
| TOD matching | Yes      | Yes         | vAmiga | vAmiga hard reset: `matching = true`. |
| Timer input pipe | $00 | $00         | WinUAE | Timer input pipeline cleared via memset. |
| Keyboard state | 0     | N/A         | WinUAE | `kbstate = 0` (idle, waiting for keyboard). |


---

## Internal State at Reset

### Copper

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| Program counter      | $000000     | Both   | WinUAE: `memset(&cop_state, 0, ...)`. vAmiga: serialize zeros coppc. |
| Copper list select   | 1           | vAmiga | vAmiga initializes `copList = 1` (default). WinUAE: no explicit set; starts from COP1LC on first COPJMP1. |
| COP1LC               | $000000     | Both   | Zeroed. Kickstart sets this before starting Copper. |
| COP2LC               | $000000     | Both   | Zeroed. |
| COPCON (CDANG)       | 0           | Both   | Copper cannot access blitter/upper regs. |
| Copper state machine | COP_stop    | Both   | WinUAE: `cop_state.state = COP_stop`. Copper halted until COPJMP1 strobe. |
| Skip flag            | false       | vAmiga | No pending skip. |
| Active in frame      | false       | vAmiga | Copper has not yet been activated. |
| COP1INS              | $0000       | Both   | Instruction registers zeroed. |
| COP2INS              | $0000       | Both   | |

### Blitter

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| Busy (BBUSY)         | 0 / false   | Both   | Blitter not busy. WinUAE: `blt_info.blit_main = 0`. vAmiga: `bbusy = false`. |
| Running              | false       | vAmiga | `running = false`. |
| Zero flag (BZERO)    | false       | vAmiga | Serialize zeros it. |
| Blit pending         | 0           | WinUAE | `blt_info.blit_pending = 0`. |
| Blit interrupt       | 1           | WinUAE | `blt_info.blit_interrupt = 1` -- blitter can generate interrupts. |
| Blit queued           | 0           | WinUAE | `blt_info.blit_queued = 0`. |
| Fill carry           | false       | vAmiga | `fillCarry = false`. |
| Shifters             | 0 / false   | WinUAE | All blitter shifter state cleared. |
| All pipeline regs    | $0000       | Both   | anew, bnew, aold, bold, ahold, bhold, chold, dhold, ashift, bshift = 0. |

### Audio State Machines (Channels 0-3)

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| State machine state  | 0 (idle)    | Both   | WinUAE: memset zeros audio_channel. vAmiga: `state = 0`. |
| Period (internal)    | PERIOD_MAX (WinUAE) / 0 (vAmiga) | **Disagree** | WinUAE sets `cdp->per = PERIOD_MAX - 1` (effectively infinite). vAmiga: serialize zeros audper. The register value is 0 in both cases; WinUAE uses PERIOD_MAX as the internal countdown value to prevent audio from triggering. |
| Volume               | 0           | Both   | Channel silent. |
| Data                 | $0000       | Both   | No sample data. |
| Length               | $0000       | Both   | |
| DMA request          | false       | vAmiga | `audDR = false`. |
| Interrupt request 2  | false       | vAmiga | `intreq2 = false`. |
| Buffer               | $0000       | vAmiga | Output buffer empty. |

### Disk DMA and Controller

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| DSKLEN               | $0000       | Both   | WinUAE: `DSKLEN(0, 0)`. Disk DMA disabled (bit 15 = 0), length = 0. Two consecutive writes with bit 15 set are needed to enable disk DMA. |
| DSKSYNC              | $0000 / $4489 | **Disagree** | WinUAE: implicitly 0 (cleared by memset). vAmiga: `dsksync = 0x4489` (standard MFM sync word). |
| Disk DMA enabled     | 0           | Both   | WinUAE: `dskdmaen = 0`. vAmiga: DriveDmaState zeroed. |
| Disk HPOS            | 0           | WinUAE | `disk_hpos = 0`. Internal position tracking. |
| Drive motor          | Off         | Both   | WinUAE: `drv->motoroff = 1` for all drives. |
| Selected drive       | None / -1   | Both   | WinUAE: selection via CIA-B PRB. vAmiga: `selected = -1` (no drive selected). |
| Drive state          | Off         | vAmiga | `state` zeroed (DriveDmaState::OFF). |
| FIFO                 | Empty       | vAmiga | `fifo = 0`, `fifoCount = 0`. |
| PRB (CIA-B copy)     | $FF         | vAmiga | `prb = 0xFF` (all drives deselected, all active-low signals high). |
| Data register        | $0000       | vAmiga | `dataReg = 0`, `dataRegCount = 0`. |
| Incoming byte        | $0000       | vAmiga | `incoming = 0`. No disk data available. |
| Sync cycle           | 0           | vAmiga | `syncCycle` zeroed. No recent sync match. |
| Sync counter         | 0           | vAmiga | `syncCounter = 0`. Watchdog for auto-sync feature. |

### Floppy Drive State (WinUAE per-drive)

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| Motor off            | 1 (off)     | WinUAE | `drv->motoroff = 1`. Motor not spinning. |
| ID bit               | 0           | WinUAE | `drv->idbit = 0`. Drive ID shift register position. |
| Drive ID             | 0           | WinUAE | `drv->drive_id = 0`. Gets set from drive type config. |
| Drive ID shift count | 0           | WinUAE | `drv->drive_id_scnt = 0`. |
| Index hack mode      | 0 or 1      | WinUAE | Drive 0 with dfxtype=0 (internal): `indexhackmode = 1`. Others: 0. Delays diskready until motor at full speed. |
| Disk change time     | 0           | WinUAE | `drv->dskchange_time = 0`. |
| Disk change flag     | false       | WinUAE | `drv->dskchange = false`. |
| Last data track      | -1          | WinUAE | `drv->lastdataaccesstrack = -1` (no previous access). |
| AMAX                 | 0           | WinUAE | `drv->amax = 0`. |

### Beam Position

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| VPOS                 | 0           | Both   | WinUAE: agnus_hpos = 0 (but see restore logic). vAmiga: pos zeroed, then `pos.lof = true`. |
| HPOS                 | 0           | Both   | |
| LOF (long frame)     | 0 (WinUAE) / 1 (vAmiga) | **Disagree** | WinUAE hard reset: `lof_store = lof_display = 0` (short frame). vAmiga: `pos.lof = true` (long frame). |
| Frame type           | PAL / NTSC  | Both   | Determined by system config, not a reset value per se. |

### Interrupts

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| INTENA               | $0000       | Both   | All interrupt enables off. Master enable off. |
| INTREQ               | $0000       | Both   | No pending interrupt requests. |
| INTENA2 (pipeline)   | $0000       | WinUAE | WinUAE: `intena2 = 0`. |
| INTREQ2 (pipeline)   | $0000       | WinUAE | WinUAE: `intreq2 = 0`. |
| IPL pipe             | $0000000000000000 | vAmiga | vAmiga: `iplPipe = 0`. CPU sees IPL=0 (no interrupt). |
| Scheduled INTREQ     | NEVER       | vAmiga | All `setIntreq[16]` entries zeroed. |

### Display Window State

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| VDIW state           | waiting_start | WinUAE | `vdiwstate = DIW_waiting_start`. Vertical display window not yet opened. |
| HDIW state           | waiting_start | WinUAE | `hdiwstate = DIW_waiting_start`. Does not reset at vblank. |
| Denise HFLOP         | false       | vAmiga | Horizontal display window flipflop off (border active). |
| Border buffer dirty  | 0           | vAmiga | No pending border updates. |
| Denise blanking       | all false   | WinUAE | `denise_hblank`, `denise_vblank`, `denise_blank_active` all false. |
| External blank       | false       | WinUAE | `exthblank = false`, `extblank = false`. |

### Sequencer (vAmiga)

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| BPL events           | re-initialised | vAmiga | `initBplEvents()` called during reset. |
| DAS events           | re-initialised | vAmiga | `initDasEvents()` called during reset. |
| DDF state            | zeroed      | vAmiga | Data fetch state serialized to zero, then recalculated. |
| Vertical DIW flipflop | off        | vAmiga | `ddf.bpv = false` until vstrt line is reached. |

### Denise Rendering Buffers (vAmiga)

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| dBuffer (raw bitplane data) | all zero | vAmiga | `memset(dBuffer, 0, ...)` in `_didReset()`. |
| bBuffer (border mask) | all $FF    | vAmiga | `memset(bBuffer, 0xFF, ...)` -- $FF means "no border" (border drawing off). |
| iBuffer (color index) | all zero   | vAmiga | `memset(iBuffer, 0, ...)`. |
| mBuffer (multiplexed) | all zero   | vAmiga | `memset(mBuffer, 0, ...)`. |
| zBuffer (depth/meta)  | all zero   | vAmiga | `memset(zBuffer, 0, ...)`. |

### Agnus Event Scheduler (vAmiga)

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| All event triggers   | NEVER       | vAmiga | All SLOT_COUNT trigger cycles set to NEVER. |
| All event IDs        | 0           | vAmiga | No events pending. |
| SLOT_CIAA            | scheduled   | vAmiga | Immediately scheduled: `CIA_CYCLES(AS_CIA_CYCLES(clock))`. |
| SLOT_CIAB            | scheduled   | vAmiga | Same as CIAA. |
| SLOT_IRQ             | NEVER       | vAmiga | No pending IRQ check. |
| SLOT_SRV             | 0.5s out    | vAmiga | Server launch daemon scheduled 0.5s after reset. |
| Clock                | 0           | vAmiga | Hard reset: `clock == 0` asserted. |

### Memory

| State                | Reset Value | Source | Notes |
|----------------------|-------------|--------|-------|
| Chip RAM             | Init pattern | vAmiga | Hard reset: `fillRamWithInitPattern()`. Simulates power-on RAM content. |
| Overlay (OVL)        | On          | Both   | WinUAE: `oldovl = true` then `map_overlay(0)`. ROM visible at $000000. vAmiga: CIA-A PA0 controls this; Memory::_didReset calls `updateMemSrcTables()`. |
| WOM locked           | false       | vAmiga | Write-Once Memory unlocked at power-on. |


---

## Disagreements and Ambiguities

### 1. DSKSYNC: $0000 vs $4489

- **WinUAE**: Not explicitly set; implicitly zero from memset-clearing disk state.
- **vAmiga**: Explicitly sets `dsksync = 0x4489` (standard MFM sync word).
- **Analysis**: The 8520 datasheet does not define DSKSYNC. The HRM states
  $4489 as the standard sync word but does not say it is the hardware default.
  vAmiga's choice is pragmatic (most software expects $4489); WinUAE leaves it
  to Kickstart to set. Neither is definitively "correct" for bare hardware.
  **Recommendation**: Treat as undefined; Kickstart always sets it.

### 2. LOF (Long Frame): 0 vs 1

- **WinUAE**: `lof_store = lof_display = 0` (short frame on hard reset).
- **vAmiga**: `pos.lof = true` (starts with long frame).
- **Analysis**: The initial LOF state depends on when exactly the chip starts
  counting after power-on. PAL long frames are 313 lines; short frames are 312.
  vAmiga's choice to start with a long frame may better match hardware behavior
  where interlace starts with the long field. Both approaches are reasonable.

### 3. COLOR00: $000 vs $FFF

- **WinUAE**: ECS Denise (non-AGA) and Denise A1000 set COLOR00 = $FFF (white).
  OCS Denise and AGA set COLOR00 = $000 (black).
- **vAmiga**: Zeros all color registers (OCS model).
- **Analysis**: This is chipset-revision-dependent. The ECS Denise apparently
  powers on with a white background, while OCS powers on black. WinUAE's
  distinction is well-researched.

### 4. COLOR01-31: Random vs Zero

- **WinUAE**: Fills COLOR01-31 with random 12-bit values from `uaerand()`.
  This simulates uninitialized SRAM in the color palette.
- **vAmiga**: Zeros all colors.
- **Analysis**: Real hardware color registers are indeed random at power-on.
  WinUAE is more accurate here. For emulation purposes, Kickstart clears
  colors during boot, so the difference is visible only in the brief
  pre-Kickstart display.

### 5. CRB (CIA): $00 vs $04

- **WinUAE**: CRB = $00 (via memset).
- **vAmiga**: CRB = $04 only when `MIMIC_UAE` mode is active; otherwise $00.
  Comment: "UAE initializes CRB with 4 (which I think is wrong)."
- **Analysis**: The 8520 datasheet says all control register bits reset to 0.
  vAmiga's default ($00) is correct per the datasheet. WinUAE's memset also
  yields $00, contradicting vAmiga's comment about UAE setting CRB=4.

### 6. CIA-B PRA: $8C vs $00

- **WinUAE**: `cia[1].pra = 0x8C` after memset. Bits 7,3,2 set.
  Bit 7: /DTR (active low -- high = DTR inactive).
  Bit 3: /DIR (step direction, active low -- high = outward).
  Bit 2: /SIDE (disk side, active low -- high = side 0).
- **vAmiga**: PRA zeroed via serialize, then `updatePA()` recomputes from
  external pin state.
- **Analysis**: CIA-B PRA reflects external pin states for bits configured as
  inputs. WinUAE's $8C represents the actual electrical state of the pins at
  power-on with no drive activity. This is not a register default but a
  reflection of hardware state.

### 7. BPLCON3: $0C00

- **WinUAE**: Forces $0C00 at reset.
- **vAmiga**: Zeros it (OCS model does not have BPLCON3).
- **Analysis**: $0C00 is the ECS/AGA default value. Bits 11-10 (BANK) = %11,
  selecting color bank 0 in the normal way. This is an ECS Denise feature.

### 8. REFPTR: $0000 vs $1FFFFE

- **WinUAE**: OCS/ECS: `refptr = 0`. AGA: `refptr = 0x1ffffe`.
- **vAmiga**: Does not model REFPTR (OCS only emulator).
- **Analysis**: REFPTR is the internal memory refresh pointer. AGA sets it to
  $1FFFFE (top of 2MB chip RAM space). Not directly observable by software.

### 9. Audio Period Internal Value

- **WinUAE**: Sets internal period counter to `PERIOD_MAX - 1` (ULONG_MAX - 1)
  after memset zeroes the register. This prevents audio DMA from triggering.
- **vAmiga**: Zeros the internal period (audper = 0).
- **Analysis**: The register value is $0000 in both cases. The internal counter
  is an emulator implementation detail. WinUAE's PERIOD_MAX prevents spurious
  audio events; vAmiga handles this through its state machine (state 0 = idle).

### 10. TOD High Byte: $00 vs $01

- **WinUAE**: Memset zeros TOD entirely.
- **vAmiga**: Hard reset sets `tod.hi = 0x1`.
- **Analysis**: The 8520 datasheet does not specify the TOD reset value.
  vAmiga's TOD value of $01xxxx may be an implementation choice to avoid
  immediate TOD/alarm match at reset (alarm defaults to $000000).


---

## Source Map

| Component    | WinUAE Source                         | vAmiga Source                          |
|-------------|--------------------------------------|---------------------------------------|
| Custom regs  | custom.cpp:6713 `custom_reset()`     | Agnus.h:238 `serialize()`, Agnus.cpp:90 `operator<<(SerResetter&)` |
| Denise       | drawing.cpp:3488 `denise_reset()`    | Denise.h:403 `serialize()` (all fields zeroed) |
| CIA          | cia.cpp:2319 `CIA_reset()`           | CIA.cpp:55 `operator<<(SerResetter&)`, CIA.h:389 `serialize()` |
| TOD          | cia.cpp (part of CIA struct memset)  | TOD.cpp:23 `operator<<(SerResetter&)` |
| Audio        | audio.cpp:2059 `audio_reset()`       | StateMachine.h:146 `serialize()` |
| Blitter      | blitter.cpp:2291 `blitter_reset()`   | Blitter.h:278 `serialize()` |
| Copper       | custom.cpp:6972 `cop_state` memset   | Copper.h:133 `serialize()` |
| Disk         | disk.cpp:5468 `DISK_reset()`         | DiskController.cpp:30 `operator<<(SerResetter&)` |
| Memory       | (external memory reset functions)     | Memory.cpp:287 `operator<<(SerResetter&)` |
| Paula/IRQ    | custom.cpp:6903-6905 intreq/intena   | Paula.h:155 `serialize()` |
| Register map | identify.cpp:134 `custd[]`           | N/A (registers defined across component headers) |


---

---

## Boot Sequence Context

The register states documented above represent the hardware state immediately
after the reset signal is released. In practice, several things happen before
any user-visible activity:

1. **CPU reads reset vectors from $000000-$000007**: Because OVL (overlay) is
   active at reset (CIA-A PRA bit 0 = 0), the ROM at $F80000 is mirrored to
   $000000. The CPU fetches the initial SSP from $000000-$000003 and the
   initial PC from $000004-$000007.

2. **Kickstart early init**: The first thing Kickstart does is:
   - Set CIA-A DDRA to $03 (make OVL and /LED outputs)
   - Clear OVL (PRA bit 0 = 1) to unmap the ROM overlay
   - Set up the initial Copper list
   - Configure DMACON to enable master DMA + Copper + bitplane DMA
   - Set INTENA to enable interrupts needed for boot
   - Set display window registers (DIWSTRT, DIWSTOP, DDFSTRT, DDFSTOP)
   - Initialize color registers (the "hand" or "insert floppy" screen)
   - Set DSKSYNC to $4489 for MFM disk reading

3. **CIA-B drive control**: Kickstart sets CIA-B DDRB to configure drive
   control outputs, then begins drive detection by toggling select lines.

This means that for any emulator implementation, the exact reset values of
write-only registers like DIWSTRT, DDFSTRT, BPLxPT, etc. are irrelevant in
practice -- Kickstart always initialises them before use. The reset values
matter primarily for:

- Registers that affect behavior *before* Kickstart runs (DMACON, INTENA,
  INTREQ must be zero to prevent spurious DMA and interrupts)
- Registers where incorrect reset state could cause visible glitches
  (COLOR00 background color, LOF long/short frame)
- CIA timer/latch values ($FFFF) that determine behavior if software reads
  timers before programming them
- The overlay state (OVL must be on for the CPU to find Kickstart ROM)


---

## Register Reset Methodology Notes

### WinUAE approach

WinUAE's `custom_reset()` function (custom.cpp:6713) follows this pattern:

1. Clear various internal pipeline and RGA state with memset
2. Zero beam position counters (agnus_hpos, vpos_prev, etc.)
3. If not restoring from savestate (`!savestate_state`):
   a. Call `blitter_reset()` and `denise_reset(true)`
   b. Zero all sprite state with `memset(spr, 0, sizeof spr)`
   c. Set `dmacon = 0`, `intreq = intreq2 = 0`, `intena = intena2 = 0`
   d. Set `copcon = 0`, call `DSKLEN(0, 0)`
   e. Set `bplcon0 = 0`, `bplcon3 = 0x0C00`, `bplcon4 = 0x0011`
   f. Initialize ECS beam registers to $FFFF sentinels
   g. Randomize color registers (hard reset only)
   h. Set `refptr` (0 for OCS/ECS, $1FFFFE for AGA)
   i. Call `FMODE(0)`, `CLXCON(0)`, `CLXCON2(0)`
   j. Set `beamcon0` based on PAL/NTSC
4. Call `audio_reset()` which memsets all channels and sets per to PERIOD_MAX
5. Clear `cop_state` with memset, set `cop_state.state = COP_stop`
6. Set `adkcon = 0`

### vAmiga approach

vAmiga uses a serialization-based reset. The `SerResetter` class calls
`operator<<` on every serialized field, which calls `RESET(type)` -- a macro
that assigns `(type)0` to the field. This zeros ALL serialized state in one
pass.

Each component then overrides `operator<<(SerResetter&)` to set specific
non-zero values after the blanket zeroing:

- **Agnus**: Sets `pos.lof = true`, reinitializes event slots, schedules
  CIA and DAS events
- **CIA**: Sets `counterA/B = 0xFFFF`, `latchA/B = 0xFFFF`, `cnt = true`,
  `irq = 1`
- **TOD**: Hard reset sets `stopped = true`, `matching = true`, `tod.hi = 0x1`
- **DiskController**: Sets `prb = 0xFF`, `selected = -1`, `dsksync = 0x4489`
- **Sequencer**: Reinitializes BPL and DAS event tables
- **Denise**: `_didReset()` fills bBuffer with $FF, zeros other pixel buffers
- **Paula**: `_didReset()` sets all `setIntreq[]` to NEVER, asserts IPL=0
- **Memory**: Hard reset fills chip RAM with power-on init pattern

All other fields (Blitter, Copper, audio state machines, interrupt registers,
DMA pointers, control registers) remain at zero from the serialize pass.


---

## Summary Statistics

- **Total custom register offsets documented**: 256 (offsets $000--$1FE, every word)
- **Named registers**: 193 (remainder are unused/reserved)
- **CIA registers per chip**: 16 (PRA, PRB, DDRA, DDRB, TALO, TAHI, TBLO, TBHI, TODLO, TODMID, TODHI, SDR, ICR, CRA, CRB + unused)
- **Internal state entries**: 45+
- **Disagreements between sources**: 10 documented above
- **Registers where both sources agree on $0000**: ~170 (the vast majority)

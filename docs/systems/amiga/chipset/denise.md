# Denise — Video Output, Bitplanes, Sprites, Collisions

Denise receives bitplane data from Agnus and converts it into pixel output.
It handles colour lookup, sprite compositing, playfield priority, collision
detection, and the special display modes (HAM, EHB, dual playfield). Denise
has no DMA capability of its own — all data arrives via Agnus.

## Bitplane Shift Registers

Denise has one 16-bit shift register per bitplane (6 for OCS/ECS, 8 for AGA).
Each shift register produces one bit per pixel clock.

### Load Timing

When Agnus fetches BPL1DAT (the last plane in the fetch order), Denise
transfers all pending bitplane data into the shift registers simultaneously.
This is the "parallel load" — all planes start shifting from the same pixel
position.

The sequence within each fetch group:
1. Agnus fetches BPL4, BPL6, BPL2, BPL3, BPL5 into holding latches (Denise
   stores these but does not shift them yet)
2. Agnus fetches BPL1 — this triggers the parallel load
3. All shift registers start outputting pixels

**Why this matters:** If BPL1DAT isn't written (e.g. plane count is wrong),
the shift registers never load and the display shows garbage or the previous
line's data.

### Scroll (BPLCON1)

BPLCON1 ($DFF102) delays the shift register output by 0-15 lowres pixels
independently for odd and even playfields:

- Bits 3-0: PF1 scroll (odd planes: 1, 3, 5, 7)
- Bits 7-4: PF2 scroll (even planes: 2, 4, 6, 8)

The delay is applied after the parallel load — the shift registers still load
at the same time, but output is delayed by the scroll value.

## Colour Resolution

Each pixel clock, Denise reads the shift register output bits to form a colour
index, then looks up the colour in the palette.

### Standard Mode

Shift register bits form an index directly:
- 1 plane: 2 colours (index 0-1)
- 2 planes: 4 colours (index 0-3)
- 3 planes: 8 colours
- 4 planes: 16 colours
- 5 planes: 32 colours (full OCS palette)
- 6 planes: 64 colours (EHB mode, see below)

### Extra Half-Brite (EHB)

When 6 planes are active and EHB is not killed (BPLCON3 KILLEHB on ECS/AGA),
bit 5 (plane 6) acts as a half-brightness flag:
- Bit 5 = 0: use colour from palette[index 0-31] normally
- Bit 5 = 1: use palette[index 0-31] at half brightness (shift RGB right by 1)

EHB gives 64 apparent colours from a 32-entry palette. ECS can disable this
with KILLEHB in BPLCON3, making all 6 planes contribute to a 64-entry index
(not useful on OCS/ECS which only have 32 palette entries, but matters on AGA).

### Hold-and-Modify (HAM)

HAM uses the top 2 bits as control bits instead of colour index bits:

**HAM6 (OCS/ECS, 6 planes):**
- Bits 5-4 = 00: Set colour from palette[bits 3-0] (16-entry subset)
- Bits 5-4 = 01: Modify blue — keep previous R and G, set B from bits 3-0
- Bits 5-4 = 10: Modify red — keep previous G and B, set R from bits 3-0
- Bits 5-4 = 11: Modify green — keep previous R and B, set G from bits 3-0

Previous colour is tracked per-pixel, reset to palette[0] at the start of
each line. HAM6 produces 4096 apparent colours (12-bit RGB) but with
characteristic fringing on colour transitions.

**HAM8 (AGA, 8 planes):**
Same control bit encoding but with 6 data bits instead of 4:
- Bits 7-6 = 00: Set colour from palette[bits 5-0] (64-entry subset)
- Bits 7-6 = 01/10/11: Modify component using 6 bits (placed in high 6 bits
  of the 8-bit component, low 2 bits from the previous value)

HAM8 produces 262144 apparent colours (18-bit effective) with much less visible
fringing than HAM6.

### AGA 24-bit Palette

AGA expands the palette to 256 entries of 24-bit colour (8 bits per component).
The palette is written through the same COLOR registers ($DFF180-$DFF1BE) but
with bank selection:

1. BPLCON3 bits 15-13 select the palette bank (0-7, each holding 32 entries)
2. Write the high nibbles of R, G, B to COLORxx
3. Set BPLCON3 bit 9 (LOCT)
4. Write the low nibbles of R, G, B to COLORxx

This two-pass scheme fits 24-bit colour into 12-bit registers. The palette
write is not atomic — intermediate states may be visible if the copper changes
colours during the active display.

### BPLCON4 XOR (AGA)

BPLCON4 ($DFF10C) bits 15-8 (BPLAM) XOR the colour index before palette
lookup. This remaps all bitplane colours without changing the bitplane data
or the copper list. Used by games for palette cycling effects.

The XOR applies to the 8-bit index from the bitplanes, not to sprite colours.

## Playfield Priority (BPLCON2)

BPLCON2 ($DFF104) controls the layering of sprites and playfields:

- Bits 2-0 (PF2PRI): Playfield 2 priority relative to sprites
- Bit 6 (PF2P2): Playfield 2 over playfield 1 (dual playfield mode)

### Sprite-Playfield Priority

Sprites are grouped into pairs for priority: (0,1), (2,3), (4,5), (6,7).
Each pair sits at a fixed priority level between the playfields:

```
Back → Front (PF2PRI=0, PF1 in front):
  Background → PF2 → Sprites 0-1 → PF1 → Sprites 2-3 → ... → Sprites 6-7

Back → Front (PF2PRI=4, PF2 in front of some sprites):
  Background → Sprites 0-1 → ... → PF2 → Sprites 4-5 → PF1 → Sprites 6-7
```

The exact ordering depends on PF2PRI value (0-7) and whether dual playfield
mode is active.

### Dual Playfield Mode

BPLCON0 bit 10 (DPF) enables dual playfield mode:
- Playfield 1: odd planes (1, 3, 5) — 8 colours from palette 0-7
- Playfield 2: even planes (2, 4, 6) — 8 colours from palette 8-15
- Each playfield scrolls independently (via BPLCON1 PF1/PF2 scroll)
- PF2P2 bit in BPLCON2 controls which playfield is in front

Colour 0 in either playfield is transparent, revealing the playfield behind it
(or the background/sprite layer).

## Sprites

Denise manages 8 hardware sprites, each 16 pixels wide (OCS/ECS) or 16/32/64
pixels wide (AGA via FMODE).

### Sprite Data Format

Each sprite line has two 16-bit words:
- SPRxDATA: Bit 0 of each pixel (16 pixels)
- SPRxDATB: Bit 1 of each pixel (16 pixels)

Together they form a 2-bit index per pixel:
- 00: Transparent
- 01: Sprite colour 1
- 10: Sprite colour 2
- 11: Sprite colour 3

### Sprite Colours

Each sprite pair shares a 4-colour palette (including transparent):
- Sprites 0-1: palette entries 17-19 (16 is shared transparent)
- Sprites 2-3: palette entries 21-23
- Sprites 4-5: palette entries 25-27
- Sprites 6-7: palette entries 29-31

### Sprite Positioning

SPRxPOS and SPRxCTL registers define the sprite's position:
- SPRxPOS bits 15-8: VSTART (V7-V0)
- SPRxPOS bits 7-0: HSTART (H8-H1)
- SPRxCTL bits 15-8: VSTOP (V7-V0)
- SPRxCTL bit 7: VSTART bit 8 (V8)
- SPRxCTL bit 6: VSTOP bit 8
- SPRxCTL bit 2: Attach (pairs with next sprite for 16-colour mode)
- SPRxCTL bit 0: HSTART bit 0 (H0)

Horizontal positioning is in lowres pixels (1 pixel = 2 hires pixels = 4
superhires pixels). The minimum horizontal position is limited by sprite DMA
slot timing — sprites can't appear in the far-left border area.

### Sprite DMA Sequence

On each line where a sprite is active (VPOS between VSTART and VSTOP):
1. Agnus fetches SPRxPOS/SPRxCTL from the sprite pointer (DMA slot 1)
2. Agnus fetches SPRxDATA/SPRxDATB from the next word (DMA slot 2)
3. Sprite pointer advances by 4 bytes
4. Denise arms the sprite — it will display at the specified HSTART

When the sprite's VSTOP is reached, sprite DMA stops fetching data and waits
for the next occurrence of VSTART on a subsequent frame.

### AGA Wide Sprites

FMODE bits 3-2 control sprite fetch width:
- 00: 16 pixels (1 word per plane, standard)
- 01: 32 pixels (2 words per plane)
- 1x: 64 pixels (4 words per plane)

Wider sprites consume proportionally more DMA bandwidth.

### Attached Sprites

SPRxCTL bit 2 (ATTACH) pairs adjacent sprites for 16-colour mode:
- Sprites 0+1 combine to form a 4-bit index (16 colours from palette 16-31)
- Sprites 2+3, 4+5, 6+7 similarly

The even sprite provides bits 0-1, the odd sprite provides bits 2-3. Both
sprites must have the same position and size.

## Collision Detection

Denise detects pixel-level collisions between sprites and playfields:

### CLXCON ($DFF098, write)

Configures which bitplanes participate in collision detection and what value
they must match:
- Bits 5-0: Enable bits for planes 1-6
- Bits 11-6: Match bits for planes 1-6
- Bits 13-12: Enable sprite odd pairs for collision

### CLXDAT ($DFF00E, read)

Returns collision results (cleared on read):
- Bits 14-0: Collision flags for all sprite-sprite and sprite-playfield pairs

Collision detection happens at the pixel level — the result reflects actual
overlapping non-transparent pixels, not bounding boxes.

## DENISEID ($DFF07C)

Read-only register that identifies the Denise variant:
- $FF: OCS Denise (no ID register — open bus returns $FF)
- $FC: ECS Super Denise
- $F8: AGA Lisa

Software reads this once during graphics.library init to determine the chipset
generation and enable appropriate features.

## Display Modes Summary

| Mode | Planes | Colours | BPLCON0 bits | Notes |
|------|--------|---------|-------------|-------|
| Lowres | 1-5 | 2-32 | — | Standard |
| Hires | 1-4 | 2-16 | HIRES (bit 15) | Half horizontal resolution per plane |
| EHB | 6 | 64 | — | Plane 6 = half-bright flag |
| HAM6 | 6 | 4096 | HAM (bit 11) | Hold-and-modify, 12-bit |
| Dual PF | 6 | 8+8 | DPF (bit 10) | Two independent 3-plane playfields |
| Interlace | any | same | LACE (bit 2) | Double vertical resolution |
| SuperHires | 1-2 | 2-4 | SHRES (ECS) | 4× horizontal resolution |
| HAM8 | 8 | 262144 | HAM (AGA) | Hold-and-modify, 18-bit effective |
| 8-plane | 8 | 256 | BPU=8 (AGA) | Full AGA palette |

### BPLCON0 Key Bits

| Bit | Name | Meaning |
|-----|------|---------|
| 15 | HIRES | Hires mode (4 CCK fetch groups instead of 8) |
| 14-12 | BPU2-0 | Bitplane count (0-6 on OCS/ECS, 0-8 on AGA with bit 4) |
| 11 | HAM | Hold-and-modify mode |
| 10 | DPF | Dual playfield mode |
| 9 | COLOR | Composite colour burst enable |
| 4 | BPU3 | Bitplane count bit 3 (AGA only, extends to 8) |
| 2 | LACE | Interlace enable (LOF toggles per frame) |
| 1 | ERSY | External resync (ECS/AGA) |
| 0 | — | (reserved) |

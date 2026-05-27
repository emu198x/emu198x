# AGA Workbench palette — handoff (Stage AF)

**One-line summary**: DENISEID was wrong ($FFF8 → fixed to $00F8). Palette is still EGA because MrgCop reads color table entries 0–3 (EGA primaries) instead of 8–11 (WB grey/black/white/blue). Patching entries 0–3 of the color table with WB values before MrgCop runs produces the correct display.

## What changed this session

### Fix applied: DENISEID value

`commodore-denise-aga/src/lib.rs` — `LISA_DENISE_ID` changed from `0xFFF8` to `0x00F8`.

**Evidence**: WinUAE returns `$00F8` for A1200 AGA, `$FCF8` for A4000 AGA (`custom.cpp:2347`). The high byte matters: KS 3.1 at `$00F8B510` computes `NOT(DENISEID) & $0300 >> 8` and stores at GfxBase+454. With `$FFF8` this produces 0; with `$00F8` it produces 3. The value 3 is the sprite-width capability index that downstream code expects for Lisa.

**Downstream effects observed**:
- GfxBase+454: 0 → 3 (sprite width capability)
- GfxBase+237: 0 → 3 (same, set during MakeVPort at `$00F94BE4`)
- ColorMap+18 (SpriteResDefault): 0 → 1
- ColorMap+28 (CoerceDisplayInfo): NULL → $00005DD0 (DblPAL:HIRES $00029000)
- Palette: **unchanged** — EGA primaries still in COLOR00–03

### Proof-of-concept: two working patches

**Patch A — copper list**: manually poke WB values ($0AAA/$0000/$0FFF/$068B) into the copper list COLOR00–03 MOVE entries. Screenshot: `/tmp/wb31-aga-patched-copperlist.png`.

**Patch B — color table entries 0–3**: poke WB values into entries 0–3 of both ColorTable and LowColorBits right after GetColorMap returns (before MrgCop). MrgCop then picks up the correct values. Screenshot: `/tmp/wb31-aga-patched-table-entries.png`.

Both produce a correct grey/black/white/blue Workbench display. Patch B is the cleaner proof: **entries 0–3 need WB values, and MrgCop reads from entries 0–3 honestly.**

## The remaining bug: nothing writes WB values to entries 0–3

### Color table layout

```
ColorMap at $00001B04, Type=2, Count=32
  ColorTable    → $00001B78 (32 entries × 2 bytes)
  LowColorBits  → $00001B38 (identical layout)

Table entries after full boot:
  0-7:  EGA primaries $0000/$0F00/$00F0/$0FF0/$000F/$0F0F/$00FF/$0FFF
        (written by GetColorMap at $00F852A6, NEVER overwritten)
  8-11: WB palette $0AAA/$0000/$0FFF/$068B
        (written by LoadRGB32 at $00F85758/$00F8577C, +16 byte offset)
  12-15: system colors (from GetColorMap ROM template)
  16-31: sprite/gradient (template + LoadRGB32 sprite-color patches)
```

### LoadRGB32's +16 offset is by design

LoadRGB32 (`$00F856E8`, LVO -882) uses `lea (16,A3,D0.w),A5` — constant +16 displacement — putting "user color 0" at table entry 8. The first 8 entries are reserved (EGA system colors). Same ROM code runs on real hardware.

### What definitely writes to the color tables

Full-table memory watch ($00001B38, 128 bytes covering both tables) captured exactly **14 writes** during the 3000-frame boot after GetColorMap:

| PC | Entries | Values | Source |
|---|---|---|---|
| `$00F85758` | 8-11 of table B | `$0AAA $0000 $0FFF $068B` | LoadRGB32 (WB palette) |
| `$00F8577C` | 8-11 of table A | `$0AAA $0000 $0FFF $068B` | LoadRGB32 (WB palette) |
| `$00F85758` | 25-27 of table B | `$0E44 $0000 $0EEC` | LoadRGB32 (sprite colors) |
| `$00F8577C` | 25-27 of table A | `$0E44 $0000 $0EEC` | LoadRGB32 (sprite colors) |

**Zero writes to entries 0–7 in either table.** On real hardware, something must write WB values to entries 0–3.

### VPModeID

VPModeID = `$00008000` (bare HIRES_KEY), written as MOVE.L D7,-(A3) at `$00F9731C` (PC observed as `$00F97322` in watch due to pipelining). This is the requested mode, not the coerced mode. CoerceDisplayInfo at `$00005DD0` contains mode `$00029000` (DblPAL:HIRES) at its offset +16 — coercion IS working.

### AGA monitor database IS populated

Scan confirmed DblPAL ($00021000) = 5 hits, DblPAL:HIRES ($00029000) = 6 hits, DblPAL:HIRESLACE ($00029004) = 4 hits in chip RAM. The monitor database has AGA modes.

### Intuition palette setup function ($00FE24B6)

This function calls the LoadRGB32-wrapping `$00FE2528` up to THREE times:

1. **Call 1** (always): 4 colors from IntuitionBase+2914[0..3] at firstcolor=0 → entries 8–11 (via +16 offset) → WB grey/black/white/blue
2. **Call 2** (if D3 > 2): 4 colors from IntuitionBase+2914[4..7] at firstcolor=(Screen+612 - 3) → entries 8+ of unknown offset — additional palette colors
3. **Call 3** (always): 3 colors from IntuitionBase+2914[8..10] at firstcolor=17 → entries 25–27 (via +16 offset) → sprite highlight colors

D3 = 12 on our emulator (read from offset 5 of the struct at Screen+88), so call 2 does execute. None of these calls write to entries 0–3 because of the +16 constant in LoadRGB32's lea.

### The IntuitionBase system palette

At IntuitionBase+2914 ($0000A8B6):
```
[0] AA AA AA = grey     [1] 00 00 00 = black    [2] FF FF FF = white
[3] 66 88 BB = blue     [4] EE 44 44 = red      [5] 55 DD 55 = green
[6] 00 44 DD = blue     [7] EE 99 00 = orange   [8] EE 44 44 = red
[9] 00 00 00 = black    [10] EE EE CC = cream
```

These are the CORRECT WB system colors. They get written to entries 8+ via LoadRGB32. They need to ALSO reach entries 0–3.

### ROM is verified correct

ROM: `kick31a1200.rom`, version 40.68 (A1200 KS 3.1), 512KB. MD5 `646773759326fbac3b2311fd8c8793ee`. Contains `card.resource` (PCMCIA — A1200-specific). Confirmed distinct from A4000/A3000/A500 ROMs.

## What's ruled out

- **DENISEID**: Fixed ($00F8). Necessary but not sufficient.
- **ROM**: Correct A1200 KS 3.1 (40.68).
- **Renderer**: Correct — displays whatever the copper list loads.
- **Color table population**: Correct — GetColorMap + LoadRGB32 produce the right values at entries 8–11.
- **BPLCON4.BPLAM=$08**: Doesn't help alone because the copper list doesn't load COLOR08–11 either.
- **SetRGB4/SetRGB32 calls**: No writes to entries 0–7 during entire boot.
- **AGA monitor database**: DblPAL modes ARE registered.
- **CoerceDisplayInfo**: Correctly points to DblPAL:HIRES ($00029000).
- **BPLCON3.BANK**: Always 0.

## ColorMap private fields

| Offset (from ColorMap) | Field | Value | Expected |
|---|---|---|---|
| +44 | SpriteBase_Even | 16 | 16 ✓ |
| +46 | SpriteBase_Odd | 16 | 16 ✓ |
| +48 | Bp_0_base | **0** | **8?** |
| +50 | Bp_1_base | 8 | 8 ✓ |

`Bp_0_base` is never written during the entire boot (confirmed with memory watch). If MakeVPort uses it to compute BPLCON4.BPLAM and/or palette load range, its zero value explains both BPLAM=$00 and the limited COLOR00–03 load range.

## Highest-priority next steps

1. **Cross-validate against FS-UAE or WinUAE**: Run the exact same ROM + WB disk on a known-working AGA emulator. Capture:
   - ColorTable entries 0–11 after boot
   - BPLCON4 value in the copper list
   - Bp_0_base value in the ColorMap
   
   This tells us definitively WHETHER real AGA uses BPLAM=$08 + wider palette load, OR has WB colors at entries 0–3. Both can fix the display but via different mechanisms.

2. **Trace who should set Bp_0_base**: If cross-validation shows Bp_0_base=8 on real hardware, find the code that sets it. It's likely inside MakeVPort's AGA monitor handler, gated by a condition our emulator doesn't satisfy. The MakeVPort dispatch goes through function pointers loaded from the display record chain (`$00F8D1F4-$00F8D212`).

3. **Check for a CPU execution bug**: The user raised whether we might be accidentally skipping an instruction. The palette-setting code paths involve complex function-pointer dispatch, tagged parameter lists, and DBRA loops. A CPU bug (wrong return address, wrong DBRA count, wrong address calculation) could cause a palette-setting subroutine to be skipped. Compare our 68020 execution against Musashi or a cycle-accurate reference for the critical MakeVPort call.

## Reproduce

```bash
cargo build --release -p emu198x-amiga
# Boot A1200 with WB 3.1 disk, 3000+ frames
# Verify: color table entries 0-3 have EGA ($0000/$0F00/$00F0/$0FF0)
# Verify: copper list COLOR00-03 have same EGA values
# Patch proof: poke WB values into table entries 0-3 BEFORE MrgCop → correct display
```

## Reference: call chains

```
GetColorMap:
  intuition $00FD042A → wrapper $00FE7E48 → GetColorMap (LVO -570) $00F85224

LoadRGB32 (palette):
  intuition $00FE24DE → $00FE2528 → wrapper $00FE7F70 → LoadRGB32 (LVO -882) $00F856E8

VPModeID write:
  graphics $00F9731C: MOVE.L D7,-(A3) where D7=$00008000, A3 → ColorMap+40

Screen setup:
  intuition $00FD052E → $00FDB2A4 (calls MakeVPort + MrgCop internally)
```

# AGA Workbench palette — handoff (Stage AF) — RESOLVED

**Status: FIXED.** Root cause was the 68020 never decoding **full-format extension words** (bit 8 of the index extension word). LoadRGB32's palette pointer `lea (A3,D0.w*2),A5` (`$4BF3 $0310`, at `$00F85732`) was mis-decoded as the brief form `(16,A3,D0.w)` — a constant +16 displacement — so with `A3=$1B78` and `D0.w=0` it wrote to entry **8** instead of entry **0**, shifting the whole WB palette +8 entries and rendering the desktop in EGA primaries.

**Verified end-to-end**: WB 3.1 now boots to the correct grey desktop. Framebuffer top colours are `$AAAAAA` (89%, grey), `$6688BB` (blue), `$FFFFFF` (white), `$000000` (black) — byte-matching the FS-UAE reference below. (A separate geometry/text-garbling issue remains in the unsettled boot frame — not palette-related.)

**The fix** (motorola-68000/68010/68020): implemented full-format extension word EA decode following WinUAE `get_disp_ea_020` — scaled index, base/outer displacement (word/long), base/index suppress, and pre/post-indexed memory indirection. Lives in `motorola-68000/src/ea.rs` (`ff_begin`/`ff_after_bd`/`ff_indirect_read`), `decode.rs` (`TAG_EA_FF_*` followups), and `disasm.rs` (full-format disassembly). Gated on `variant_scaled_index` (68020+).

**Why every prior verification missed it**: both Tom Harte harnesses structurally exclude full-format words. `harte_real_hw.rs` runs the *68000* corpus and skips every `(d8,` case; the m68k-generated 68020 corpus only emits brief words (`d8 = random u8 & 0xFE` → bit 8 always 0). New coverage: `motorola-68020/tests/full_format_ea.rs` (10 hand-computed cases) + 2 disasm cases in `motorola-68000/src/disasm.rs`.

---

## Original investigation (kept for context)

**One-line summary**: DENISEID fixed ($FFF8 → $00F8). The remaining palette bug is now decisively characterised by a save-state diff against FS-UAE: the WB palette lands at color table entries **8-11** on our emulator but entries **0-3** on FS-UAE — an exact +8-entry (+16-byte) shift. All inputs (ROM, ColorMap struct, firstcolor, computed depth) are byte-identical between the two, so this is a **CPU execution divergence** in the screen-open / LoadRGB32 path, not a chipset-register or OS-struct issue.

## Fix applied this session: DENISEID

`commodore-denise-aga/src/lib.rs` — `LISA_DENISE_ID` `0xFFF8` → `0x00F8` (committed). WinUAE/FS-UAE both return `$00F8` for A1200 (`custom.cpp:2347`). Fixes GfxBase+454 (0→3) and enables CoerceDisplayInfo. Necessary but does NOT fix the palette.

Also fixed stale `$FFF8` comment in `machine-commodore-amiga-a1200/src/lib.rs:1494`.

## DECISIVE: FS-UAE save-state comparison

FS-UAE 3.2.35 uses the **identical ROM** (`kick31_40_068_a1200.rom`, MD5 `646773759326fbac3b2311fd8c8793ee` — byte-for-byte same as ours). Booted WB 3.1, saved state, parsed the `CRAM` (zlib chip RAM) and `AGAC` (AGA palette) chunks. Dump saved at `/tmp/fsuae-chipram-wb31.bin`.

### Color table (`$00001B78`), entries 0-15:

| Entry | FS-UAE (correct) | Our emulator |
|---|---|---|
| 0-3 | **`0AAA 0000 0FFF 068B`** (WB) | `0000 0F00 00F0 0FF0` (template) |
| 4-7 | `000F 0F0F 00FF 0FFF` | `000F 0F0F 00FF 0FFF` (identical) |
| 8-11 | `0620 0E50 09F1 0EB0` (template) | **`0AAA 0000 0FFF 068B`** (WB) |
| 12-15 | `055F 092F 00F8 0CCC` | `055F 092F 00F8 0CCC` (identical) |
| 16-31 | (grey/sprite ramp) | identical |

The WB palette (`0AAA 0000 0FFF 068B` = grey/black/white/blue) is written by LoadRGB32 to entries **0-3 on FS-UAE** and entries **8-11 on ours**. Everything else in the table is the identical GetColorMap template.

### Hardware palette (`AGAC` chunk) on FS-UAE:
`palette[0]=AAAAAA palette[1]=000000 palette[2]=FFFFFF palette[3]=6688BB` — i.e. grey/black/white/blue at registers 0-3, BPLAM=0. Real AGA displays directly from palette[0-3]; no BPLAM remapping involved.

### Structs are byte-identical (this is the key):
| Field | FS-UAE | Ours |
|---|---|---|
| ColorMap addr | `$00001B04` | `$00001B04` |
| ColorMap.Type | 2 | 2 |
| ColorMap.ColorTable (+4) | `$00001B78` | `$00001B78` |
| ColorMap.LowColorBits (+12) | `$00001B38` | `$00001B38` |
| VPModeID (+36) | `$00008000` | `$00008000` |
| Bp_0_base (+48) | 0 | 0 |
| Bp_1_base (+50) | 8 | 8 |
| Screen+612 | 3 | 3 |

## Why this rules out everything previously chased

The earlier hypotheses are all **dead ends**, disproven by the identical structs:
- **VPModeID / monitor database / DblPAL coercion**: VPModeID is bare `$8000` on FS-UAE too. Not the cause.
- **Bp_0_base / BPLCON4.BPLAM**: Bp_0_base is 0 on FS-UAE too; FS-UAE's BPLAM is 0 and it still displays correctly because palette[0-3] directly holds WB. Not the cause.
- **MakeVPort palette handlers**: would have to produce different structs; they don't.

## The narrowed paradox (next session starts here)

The LoadRGB32 write uses `lea (16,A3,D0.w),A5` at `$00F85732` — a **constant +16 displacement**. On our emulator, at this instruction during the WB call:
- A3 = `$00001B78` (ColorTable), D0.w = `0000` (firstcolor=0) → A5 = `$00001B88` = **entry 8**.

Confirmed inputs on our side (live trap at `$00F85732`):
- Source RGB32 table at `$0003E0C4`: `0004 0000` (ncolors=4, firstcolor=0) followed by grey/black/white/blue 24-bit triples — the genuine WB call.
- D0=`$00030000` (D0.w=0), A3=`$00001B78`, A4=`$00001B38`.

For FS-UAE to land WB at entry 0 with the **same ROM, same A3, same +16**, its D0.w must be `-16` ($FFF0) at that instruction — i.e. a different firstcolor — yet the intuition call that builds the source table **hardcodes firstcolor=0** (`clr.l` at `$00FE24D4`), and every upstream input we can compare (Screen+612=3, etc.) is identical.

**Therefore the divergence is a transient CPU-execution difference somewhere in the screen-open path** (it leaves no trace in the final structs). This matches the "are we skipping an instruction / returning to the wrong place?" hypothesis. Candidate areas:
1. The intuition palette function `$00FE24B6` → `$00FE2528` → LoadRGB32 wrapper `$00FE7F70` → LoadRGB32 `$00F856E8`. Something in this chain computes a different firstcolor on real hardware, OR our CPU mis-executes producing firstcolor=0/entry-8.
2. The LoadRGB32 inner loop ($00F8570A-$00F8577A) uses instructions the disassembler mis-decodes (ROXL, DBRA, MOVEP). Worth verifying our 68020 executes each correctly — especially anything that could feed D0 or the write pointer.
3. GetColorMap loop ($00F85298-$00F852A4, DBRA-driven) — verified to produce the correct template, but the EGA-vs-data split (entries 0-7 algorithmic, 8-31 from ROM data) interacts with where LoadRGB32 writes.

## Recommended next step

Trace the WB LoadRGB32 call **instruction-by-instruction** on our emulator from `$00FE24B6` through the `$00F85732` lea, recording D0/A0/A1/A3 at each step. Cross-check each instruction's effect against 68020 semantics. The bug is the single instruction (or skipped call/branch) that makes firstcolor resolve to 0→entry-8 instead of the value that yields entry-0. FS-UAE's WinUAE-core debugger can produce the reference trace for the same call if an instruction-level audit on our side isn't conclusive.

## Reproduce / artifacts

```bash
# FS-UAE reference capture (config at /tmp/wb31-capture.fs-uae):
#   amiga_model=A1200, kick31_40_068_a1200.rom, WB3.1 disk, save_state_compression=0
# Save state: ~/Documents/FS-UAE/Save States/A1200 - KS3.1 - 2MB Chip/Saved State 1.uss
# Parsed chip RAM: /tmp/fsuae-chipram-wb31.bin (2 MiB, raw)
#   ColorTable at offset 0x1B78, ColorMap at 0x1B04.

# Our emulator: boot A1200 + WB3.1, trap $00F85732 → D0.w=0, A3=$1B78 → writes entry 8.
```

## Reference: call chains

```
GetColorMap:  intuition $00FD042A → wrapper $00FE7E48 → GetColorMap (-570) $00F85224
LoadRGB32:    intuition $00FE24DE → $00FE2528 → wrapper $00FE7F70 → LoadRGB32 (-882) $00F856E8
  WB write lea: $00F85732  lea (16,A3,D0.w),A5   [A3=$1B78, D0.w=0 → entry 8 (ours)]
Screen setup: intuition $00FD052E → $00FDB2A4 (MakeVPort + MrgCop)
```

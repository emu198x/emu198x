# Stage AE — AGA HIRES rendering: handoff

Session of 2026-05-25 took the A1200 emulator from "appears wedged"
to "KS 3.1 + WB 3.1 boots through OS init, fs-uae cross-check
proves our render pipeline correct, two chipset reporting bugs
fixed". The remaining gap is **HIRES rendering**, exposed once the
Alice agnus_id was corrected to `$2300` (PAL) / `$3300` (NTSC).

## Where we are right now

Run the MCP boot probe:

```
printf '%s\n%s\n%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_frames","arguments":{"frames":300}}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"insert_media","arguments":{"path":"/Users/stevehill/Projects/198x/assets/amiga/Operating Systems/Workbench/Workbench v3.1 rev 40.42 (1996)(ESCOM)(M10)(Disk 2 of 6)(Workbench).zip"}}}' \
  '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"run_frames","arguments":{"frames":3000}}}' \
  '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"dump_framebuffer","arguments":{"path":"/tmp/wb.png"}}}' \
  | ./target/release/emu198x-amiga --mcp
```

Result: mostly black framebuffer with just the disk-icon outlines
visible at top-left. fs-uae produces a proper grey Workbench from
the same inputs (see `tools/fs-uae-cross-check.sh` and
`docs/amiga-fs-uae-cross-check.png`).

The chipset state at the boot point:

```
bplcon0  = $A302     (HIRES + BPU=2 + COLOR + GAUD + ERSY)
diwstrt  = $2C81     (vstart 44, hstart 129)
diwstop  = $2CC1     (vstop 300 — PAL 256-line)
ddfstrt  = $0038
ddfstop  = $00D8
bpl1mod  = 76
bpl2mod  = 76
bpl_pt[0]= $41E4E
bpl_pt[1]= $41E9E    (= bpl_pt[0] + 80 — interleaved layout)
fmode    = $0000     (16-bit fetch — OCS-compatible)
num_bitplanes = 2
```

## What is correct (don't re-investigate)

- All CPU instruction handling (68000/68010/68020 Tom Harte green)
- BSR / RTE / supervisor mode / interrupts (Stages L–N)
- Memory + bus + overlay + chip RAM access
- Trackdisk + MFM decode + floppy IRQs (KS reads the disk fully)
- AGA detection: DENISEID = `$FFF8` returned at `$DFF07C` ✓
- AGA agnus_id: `$2300` PAL / `$3300` NTSC (Stage AD)
- FMODE readback returns stored value (Stage AC)
- BPLCON3 + BPLCON4 + COLOR write routing through AGA wrapper
- AGA 24-bit palette tracking via `BPLCON3 BANK + LOCT`
- The framebuffer pixel pipeline: **proven by `poke_word` forcing
  grey into `bpl_pt[0..3]` and getting a pixel-perfect WB 3.1
  desktop** (Stage AB, `docs/amiga-forced-grey-palette.png`)
- HIRES bitplane DMA arbitration in `dma_claim` (Stage AE-a)
- Agnus-side HIRES_DDF_TO_PLANE bitplane scheduling
  (`commodore-agnus-ocs`, was correct all along)

## What is broken

KS programs a HIRES 4-color screen with an interleaved bitplane
layout. We:
- Schedule bitplane DMA at the right CCKs ✓
- Fetch bitplane words into the shift registers ✓
- Emit pixels through `output_pixel_with_beam_and_playfield_gate`
- Resolve colours through `palette[final_color_idx]`

But the visible result is almost all black. Some hypothesis to
test, ordered by likelihood:

### (a) HIRES shift-register clocking — most likely

In HIRES the pixel rate is 2× lores. Look at
`commodore-denise-ocs/src/chip.rs::output_pixel_with_beam_and_playfield_gate`
and the surrounding `tick` / shift register code.

`commodore-denise-ocs::DeniseOcs::shift_count` decrements once per
output pixel. In HIRES, the shift register should advance 2× as
fast — once per master/4 tick instead of once per CCK. If we
output the same lores pixel for both halves of a CCK in HIRES
mode, we're losing every other hires pixel.

How to test:
- Boot, query: which lines have non-zero pixels in the framebuffer?
- Read the bitplane memory at `bpl_pt[0]` directly (`memory_read`)
  to see what's actually in the bitmap.
- Compare: render uses bytes [0..N], but the bitmap has data
  through byte [80*256-1]. Where does the data stop being
  rendered? (Hint: probably at the first CCK where the shift
  register runs out.)

### (b) Interleaved bitmap modulo

KS sets `BPL1MOD = BPL2MOD = 76`. For the interleaved layout to
work, per-line advance for each plane must be:
`fetched_bytes + bpl_mod = 2 * line_width_per_plane`

For 640px HIRES with `line_width_per_plane = 80 bytes` and 2
planes interleaved: `80 + 80 = 160`. So `bpl_mod` should be 80.

Our `bpl1mod = 76`. That's 80 − 4. The 4 might be slack from
HIRES overscan (672 wide instead of 640 = 84 bytes per line, then
`84 + 76 = 160`). Need to verify what the actual fetched bytes
per line are vs what we apply the modulo to.

Look at the end-of-line modulo path in
`common-commodore-amiga/src/denise.rs::tick` around line 270.

### (c) AGA palette resolve

`DeniseAga::resolve_color_rgb12` currently delegates to the ECS
inner (which uses 12-bit `palette[0..31]`). For full AGA, it
should consult `palette_24` (already populated) and downsample.

This won't fix the visibility issue (the colours from the OCS
palette match palette_24 high-nybble anyway), but it's prerequisite
for the BPLAM XOR remap.

### (d) BPLCON4 BPLAM XOR remap

After (c), apply `bitplane_idx XOR (bplcon4 >> 8)` at pixel emit
before palette lookup. KS currently writes `BPLCON4 = $0011`
(BPLAM = 0, sprite base 17), so this won't change anything in
this specific boot — but it's a load-bearing AGA feature for
Workbench colour banks.

## Tools available

All built into `./target/release/emu198x-amiga --mcp`. JSON-RPC
over stdio, one call per line.

### Inspection
- `query_cpu` / `query_chipset` / `query_agnus` / `query_aga`
  / `query_paula` / `query_cia` / `query_blitter` / `query_disk`
  / `query_stack` / `query_copper_list`
- `memory_read addr len` / `memory_read_long addr`
- `disasm addr count`

### Control
- `run_frames frames` / `run_ticks ticks`
- `run_until_pc target` / `run_until_any_pc targets[]`
- `run_until_mem_change addrs[]`
- `step count`
- `reset`

### Capture
- `dump_framebuffer path` — PNG + colour histogram + hash
- `start_video_recording path` / `stop_video_recording` — MP4
  via ffmpeg

### Diagnostics
- `bplcon0_log unique:true` — every BPLCON0 write with BPU histogram
- `palette_log unique:true [color_idx_range:[lo,hi]]` — every
  COLOR/BPLCON3/BPLCON4 write with BANK + LOCT
- `chipset_read_log [offset, dedupe, cck_min/max]` — every chipset
  register read with returned value
- `watch_memory addr len` / `watch_memory_log` / `watch_memory_clear`
  — chip-RAM byte-range write watchpoint

### Backdoors
- `poke_word addr val` — force a word write anywhere through the bus
- `insert_media path [entry] [kind=adf] [change_pending]`
- `eject_media`
- `restart [exit_code]` — exit so a host re-spawns the new binary

## Files most relevant to Stage AE

- `crates/commodore-denise-ocs/src/chip.rs` — `output_pixel_*`,
  shift register, palette resolution. This is where the HIRES
  bug almost certainly lives.
- `crates/common-commodore-amiga/src/denise.rs` — board-level
  wrapper, per-CCK tick that does the bitplane fetch and pixel
  emission.
- `crates/commodore-agnus-ocs/src/agnus.rs` — bus plan
  (`cck_bus_plan`, `current_slot`), HIRES_DDF_TO_PLANE.
- `crates/commodore-denise-aga/src/lib.rs` — AGA wrapper, where
  the AGA palette resolve will land.

## Cross-check workflow

`tools/fs-uae-cross-check.sh` already automates: boot fs-uae +
our emulator with the same ROM+ADF, capture screenshots from
both, compose a side-by-side PNG. Re-run after each substantive
change to see whether the gap is closing.

## Today's commits (S → AE-a)

```
9fbaf4a  S    zip-aware insert_media + STOP-instruction reframing
ebd6e5b  T    DENISEID/FMODE/BPLCON3 routing — KS sees AGA
d52a786  U    AGA palette via BPLCON3 BANK + LOCT
ca8a21f  V    BPLCON0 write trace
6de0d71  W    PNG framebuffer dump
51728b7  X    VideoRecorder integration
76ac575  Y+Z  palette log + restart tool
d9caeb3  AA   fs-uae cross-check script + screenshot
4eeec95  AB   watch_memory + poke_word — render path proven correct
ff8a8f0  AC   chipset_read_log + Alice agnus_id (PAL=$30 was wrong)
8170239  AD   Alice agnus_id corrected to PAL=$2300/NTSC=$3300
f27f7ce  AE-a HIRES dma_claim schedule (correct but invisible to WB)
```

## What to do first when picking back up

1. **Read** `commodore-denise-ocs/src/chip.rs` end to end — focus
   on `output_pixel_with_beam_and_playfield_gate` and how
   `shift_count` is managed.
2. **Trace** what `source_pixels_per_fb_pixel` is for a HIRES
   tick in our current code. If it returns 1 in HIRES mode
   (when it should be 2), or if the shift register advances by
   16 pixels per CCK (when in HIRES it should advance by 16
   pixels per *half*-CCK), that's the bug.
3. **Verify** by reading bitmap memory at `bpl_pt[0]` and
   forming the expected pixel pattern, then comparing to what
   ends up in the framebuffer at the corresponding fb_x/fb_y.

The user's instinct on "framebuffer index mapping" was wrong but
adjacent — the issue is in the **timing** of when pixels are
shifted out and rendered, not the *mapping* from index to colour.

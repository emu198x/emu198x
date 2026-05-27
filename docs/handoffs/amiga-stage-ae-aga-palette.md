# AGA Workbench palette — handoff (Stage AE end of session)

**One-line summary**: Workbench 3.1 fully boots on AGA in our emulator. The desktop renders correctly EXCEPT colours 0-3 are EGA primaries (black/red/green/yellow) instead of the standard WB grey/black/white/blue. This is purely a palette-layout divergence between our AGA emulation and real AGA hardware.

## Big reframings from this session

1. **There is no wedge.** The "WB doesn't install view" framing from the AE-p handoff was wrong. WB *does* install. All-tasks-in-WAIT is the *correct* idle state of a fully-booted Amiga waiting for user input. Every config (OCS+KS1.3+WB1.3, ECS+KS3.1+WB3.1, AGA+KS3.1+WB3.1) reaches this exact same idle steady state.
2. **The IPrefs investigation was misdirected.** IPrefs sitting in Wait with `sig_wait=$0000F000` and `msg_count=0` is the correct state — it's waiting for prefs file notifications that don't fire because the user never changes prefs. Force-waking IPrefs via `wake_task` made it run one iteration, find nothing to do, and re-park. Healthy.
3. **The chipset-detection register fixes (AE-j/k/l) were real bugs, just not the bugs that mattered for WB display.**
4. **First diagnostic that would have caught this in 5 minutes: `dump_framebuffer`.** Captured as feedback memory `feedback-screenshot-first.md`.

## What's actually wrong

On AGA, the visible Workbench desktop palette is (from `dump_framebuffer`):

| Colour | Wrong (our AGA) | Right (real AGA / FS-UAE) |
|---|---|---|
| 0 | black ($000000) | grey ($AAAAAA) |
| 1 | red ($FF0000) | black ($000000) |
| 2 | green ($00FF00) | white ($FFFFFF) |
| 3 | yellow ($FFFF00) | blueish ($6688BB) |

Screenshots: `/tmp/wb-screenshots/wb31-aga.png` (broken), `wb31-ecs.png` (correct grey/blue), `wb13-ocs.png` (correct blue).

The displayed values are the OS-written ones. The OS writes red/green/yellow into colour slots 0-3 in our emulator's AGA boot. On real AGA hardware the same OS code writes grey/black/white/blue into slots 0-3. Some chipset query inside graphics.library / intuition.library returns differently on our AGA vs real AGA, and the OS picks a different palette source.

## Mechanism, in detail

Chip RAM layout after boot (AGA):

```
$00001B00..$00001B37   struct ColorMap (32-entry, Type=$02, two table pointers)
                          +0    self-pointer $00001B00
                          +5    Type = $02
                          +6    Count = 32
                          +8    table A pointer → $00001B78 (ROM-template-loaded)
                          +16   table B pointer → $00001B38 (zero-cleared then patched)

$00001B38..$00001B77   ColorTable B (32 entries × 2 bytes = 64 bytes)
   slots 0-7    $0000 $0F00 $00F0 $0FF0 $000F $0F0F $00FF $0FFF   ← EGA primaries (writer 1)
   slots 8-15   $0AAA $0000 $0FFF $068B $055F $092F $00F8 $0CCC   ← WB system palette (writer 2)
   slots 16-31  grey/sprite gradient

$00001B78..$00001BB7   ColorTable A (identical layout — high/low nibble pair)
```

The screen opens at `BPLCON0 = $A302` (HIRES, BPU=2, LACE, 4 colours visible from slots 0-3). The visible slots get the EGA primaries.

## Code path identified

In graphics.library ROM:

| Address | Role | What it does |
|---|---|---|
| `$00F85224` | LVO entry — `GetColorMap` or similar AGA-aware allocator | Allocates 56-byte ColorMap struct + two N×2-byte ColorTable buffers via `exec.AllocMem`. Loops through `lea ($00F852C8, PC), A1` template to populate table A. **Makes NO chipset reads.** Same allocation path regardless of chipset. |
| `$00F85276` | ColorMap pointer write | `movea.l D0, A3` after the second AllocMem — table B base captured. |
| `$00F852A6` | Writer 1 (post-loop) | End of the loop that fills slots 0-7 with EGA primaries **algorithmically from the index bits** (it's not a data table — it's `(N&1)*$F00 | ((N>>1)&1)*$0F0 | ((N>>2)&1)*$00F`). Slots 8-31 from the ROM template at `$00F852C8`. |
| `$00F85758` | Writer 2 | Patches slots 8-11 (and a few in 13-15) with WB grey/black/white/blue. Destination offset is hard-coded: `lea (16, A3, D0.w*1), A5` — that's `+16 bytes = +8 slots`. **The `+8` offset is the most concrete clue.** |
| `$00F91F54` / `$00F91F60` | `MrgCop` / copper-list builder | Reads the table B via `(A3)`, emits MOVE entries into the copper list at `$00011DBC`, `$00011E3A`. |
| `$00F925B8` | Final copper-list builder | Emits MOVE entries into cop2lc at `$000121EC`, `$00012240`. |

So the bug is **at the source of the table layout, not the renderer**. The OS code at writer 1 + writer 2 deliberately writes EGA at slots 0-7 and WB at slots 8-15. On real AGA, this same code must produce a different layout (WB at slots 0-3).

## What we ruled out

- **Our renderer**: reads from `inner.palette` (the ECS 12-bit cache), which is correctly populated from the LOCT=0 (high-nibble) writes. Renderer is honest about what the OS wrote.
- **BPLCON4.BPLAM**: always `$00` throughout the boot (84 writes captured, all `$0011`). The hypothetical "real AGA writes BPLAM=$08 to remap bitplane data 0-3 → slots 8-11" doesn't hold — the OS isn't writing BPLAM either.
- **BPU / screen depth**: stable at BPU=2 (4 colours). No SHRES or BPU3 magic.
- **BPLCON3.BANK**: always bank 0. The LOCT dance works (2039 LOCT=0 + 2057 LOCT=1 writes), but produces the EGA palette into bank 0 slots 0-3.
- **AGA chipset detection (DENISEID, agnus_id)**: confirmed correct by AE-j/k/l fixes. The OS knows it's running on AGA — it's just picking the wrong AGA palette template.
- **ROM image**: byte-exact match with the on-disk `kick31a1200.rom`. The ROM template data at `$00F852C8` is loaded correctly.

## The narrow target

**Inside graphics.library, somewhere upstream of `$00F85224`, a chipset query made by some caller returns a value that picks "EGA-primaries-at-0-7 + WB-at-8-15" template instead of "WB-at-0-3" template.**

The chipset query isn't in `$00F85224..$00F852C8` itself — that routine is chipset-agnostic. It's in the caller (which sets D0 to 32 — the entry count). The caller's chipset query (DENISEID? BPLCON3 read-back? FMODE behaviour? A custom-register read we don't return correctly?) decides what the OS treats as the default screen depth + colour scheme.

## What's needed for the next session

1. **Find the caller of `$00F85224`**. Set a watch / breakpoint at the LVO entry, look at the return address on the stack, walk back to find the chipset query.
2. **OR cross-reference with `vAmiga/Emulator/Components/...`** — vAmiga emulates Lisa correctly, the same OS code on vAmiga produces grey/blue, so a side-by-side `chipset_read_log` diff between us and vAmiga will surface the divergent register.
3. **OR disasm + analyse the OS structure at GfxBase+420**. `$00F85228 movea.l (420, A6), A6` — this loads ExecBase from a GfxBase field. The structure at GfxBase contains pointers to chipset-specific data tables. Investigating which fields differ between OCS/ECS/AGA paths is another way in.

The MCP toolkit assembled this session (AE-q through AE-w) is sufficient for this work. In particular:
- `read_task_stack` to walk callers off the stack when stopped at `$00F85224` entry
- `disasm_around` for clean disasm of the caller chain
- `address_to_library` to attribute caller PCs to their libraries
- `memory_scan` to find data structures referenced
- `chipset_read_log` to see what registers the relevant code path queries

## Reference: commits this session

- `AE-p`: handoff doc (initial misframing — see `amiga-stage-ae-aga-wb-install.md`, kept for historical context)
- `AE-q`: `memory_scan` MCP tool
- `AE-r`: `resolve_lvo` MCP tool + NDK 3.2 LVO tables
- `AE-s`: `Process` struct decoder (NT_PROCESS detection)
- `AE-t`: `read_task_stack` + `query_library` + `address_to_library`
- `AE-u`: `disasm_around` + `dump_msgport_messages`
- `AE-v`: `signal_task` (write-only signal injection)
- `AE-w`: `wake_task` (full WAIT → READY transition)

## Reproduce

```bash
cargo build --release -p emu198x-amiga
~/Projects/198x/Emu198x/target/release/emu198x-amiga --model a1200 --headless
# Insert: ~/.emu198x/media/commodore-amiga/wb31/Workbench v3.1 rev 40.42 (1996)(ESCOM)(M10)(Disk 2 of 6)(Workbench).adf
# Run ~3000 frames after disk insert. Screenshot shows black/red/green/yellow desktop.
```

Or via MCP:

```bash
# Probe scripts from this session in /tmp/:
#   /tmp/palette-diff-v2.py    — ECS vs AGA palette write histogram
#   /tmp/copper-dump2.py        — cop2lc with palette MOVEs
#   /tmp/find-source.py         — locate ColorTable in chip RAM
#   /tmp/colormap-diff.py       — locate the EGA palette source
#   /tmp/find-table-writer-late.py — find the writers
#   /tmp/disasm-w1-aligned.py   — disasm writer 1
#   /tmp/disasm-w2.py            — disasm writer 2
#   /tmp/disasm-full-routine.py  — full ColorMap allocator routine
```

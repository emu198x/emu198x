# Stage AE — AGA HIRES rendering: handoff

Session of 2026-05-25 took the A1200 emulator from "appears wedged"
to "KS 3.1 + WB 3.1 boots through OS init, fs-uae cross-check
proves our render pipeline correct, two chipset reporting bugs
fixed". The remaining gap was first attributed to **HIRES rendering**
once the Alice agnus_id was corrected to `$2300` (PAL) / `$3300`
(NTSC) in Stage AD.

The session of 2026-05-25 (evening) re-investigated that hypothesis,
disproved it, found the real blocker is upstream of rendering, and
backed out the AGA-side chipset-identification change so WB renders
again — with the palette gap as the next thing to fix.

## What is true now (post Stage AE-b revert)

- HIRES rendering is **correct** (poke test below).
- KS 3.1 + WB 3.1 (Disk 2 ADF) boots through to a HIRES Workbench
  desktop with title bar, "Copyright …" text, Ram Disk + Workbench3.1
  icons, scrollbars. Same content as Stage AB; HIRES geometry now.
- Palette is **wrong** — KS writes the AGA test-pattern colours
  (`$0F00 / $00F0 / $0FF0` at COLOR1/2/3) instead of grey/blue.
  Identical to the Stage AA / Stage AB fs-uae cross-check gap.
- agnus_id reports the OCS default (`$10` PAL / `$00` NTSC). KS
  sees AGA Denise (`DENISEID = $FFF8`) + OCS Agnus = mismatched
  chipset, takes a downgrade boot path that does *not* try to
  install the broken AGA WB display.

`docs/amiga-fs-uae-cross-check.png` from Stage AA still represents
the gap accurately: our left panel shows real WB content in wrong
colours; fs-uae right panel shows correct grey Workbench.

## What was wrong about the original Stage AE hypothesis

The previous handoff identified HIRES shift-register clocking (a),
interleaved modulo (b), AGA palette resolve (c), and BPLCON4 BPLAM
XOR (d) as ordered hypotheses, with (a) "most likely". None of
them is the gap.

**Empirical disproof of (a):** booted the A1200 to the documented
AE state (BPLCON0 = `$A302`, HIRES + BPU=2, bpl_pt[0] = `$37E4E`),
then `poke_word` wrote a known pattern to the bitmap:

```
plane 0 row 0: $FFFF $AAAA $5555 $FF00 $00FF
plane 1 row 0: $FFFF $AAAA
```

Every fb_x position in the dump matches the expected colour exactly
across the visible width — confirmed at the pixel level:

```
fb_x 82..96  yellow at even positions    word 2 $AAAA $AAAA → 1,0,1,0 → yellow,black,yellow,black ✓
fb_x 99..113 red at every-other          word 3 $5555 $0000 → 0,1,0,1 → black,red,black,red ✓
fb_x 113..121 solid red (9 px)           word 4 $FF00 $0000 high bits → red ✓
fb_x 138..145 solid red (8 px)           word 5 $00FF $0000 low bits → red ✓
fb_x 706..721 solid red (16 px)          overfetch wraps to plane 1's $FFFF, plane 0=$0 → red ✓
```

DIW H gate opens correctly at fb_x = 82 (beam_x_lores = 129). The
4-CCK commit cadence is correct. The interleaved-modulo path is
correct. quad[0] / quad[1] mapping is correct. **HIRES rendering
is not the bug.**

## What the real gap turned out to be

The Stage AE framebuffer shows just two disk-icon outlines on black.
The bitmap memory at `bpl_pt[0]` is **empty**. WB has not drawn to
its bitmap. The "icons" are KS's "Insert Workbench Disk" prompt,
left over from before WB took over.

In OCS-fallback (Stage AB), WB *does* draw — and switches `cop2lc`
to its own copper list at `$10878` that points to its bitmap at
`$30F56`. WB calls graphics.library's MakeVPort / MrgCop, then
intuition's OpenScreen, then LoadView; LoadView writes the new
COP1LC / COP2LC and the new display takes effect.

In the AGA path (Stages AC–AE-a), WB never installs its view. The
cop2lc at `$121E0` stays unchanged across 600+ frames. Only one
BPL1PTH MOVE pair exists in chip RAM (KS's). No WB copper list
got built.

**Why** WB stops in AGA mode is unknown. Things we ruled out:

- HIRES rendering (poke test pixel-perfect).
- Chip RAM size: 512 KB → 2 MB → 2 MB + 4 MB fast all give the
  identical empty-bitmap result, same framebuffer hash, same disk
  stuck at cylinder 40.
- Disk loading isn't the differentiator — cylinder 40 / motor off
  is identical at AB (works) and AE (broken).
- Chipset register reads look normal: DMACONR, INTENAR, INTREQR,
  VPOSR, VHPOSR, POTGOR, JOY0DAT in the polling pattern KS always
  does; DENISEID and FMODE return the values KS expects.

Still possible: a specific AGA chipset behaviour (sprite-DMA timing
under AGA Alice? a BPL fetch quirk we don't model? a library probe
that hits a register we mishandle?) that graphics.library or
intuition.library blocks on. Pinning it down needs CPU-trace work:
look for where WB.Workbench task gets scheduled, then trace
forward until it stops calling library functions.

## What this commit (Stage AE-b) does

Reverts the agnus_id assignment in `AmigaA1200`:

```rust
// Stage AD value (correct AGA, but WB doesn't install view):
a.agnus_id = match region {
    AgnusRegion::Pal => 0x2300,
    AgnusRegion::Ntsc => 0x3300,
};

// Stage AE-b: drop the assignment; OCS Agnus default ($10/$00)
// stays in place.
```

Effect: KS sees AGA Denise (`DENISEID = $FFF8`) + OCS Agnus
(`agnus_id = $10`) = mismatched chipset. KS takes the
OCS-compatibility WB boot path. WB installs its view, draws the
desktop, scrollbars, icons.

All other Stage S → AE-a work is preserved:

- diagnostic tooling: `chipset_read_log`, `bplcon0_log`,
  `palette_log`, `watch_memory`, `poke_word`, `dump_framebuffer`,
  `start/stop_video_recording`, `restart`
- correctness: FMODE readback (Stage AC), DENISEID = `$FFF8`
  (Stage T), AGA palette via BPLCON3 BANK + LOCT (Stage U), HIRES
  `dma_claim` schedule (Stage AE-a)

`ks31_boot` + `mcp_smoke` green. Full workspace test sweep green
(386 test sets pass).

## What's left

### Immediate: wrong palette on the OCS-fallback path

The Stage AB cross-check picture (left panel) shows the gap: WB
renders with yellow / red / green primaries at COLOR1/2/3 instead
of WB's standard grey/blue scheme. Stage AB documented this via
the `poke_word` proof in `docs/amiga-forced-grey-palette.png` —
forcing the palette to grey/black/white/mid-grey produces a
pixel-perfect WB desktop.

The reason KS writes the test-pattern values is upstream of our
chipset reads — something different from what fs-uae's KS does on
the same ROM. Stage AC's hypothesis was that this was caused by
the mismatched chipset (which AC / AD then attempted to remove).
After this Stage AE-b revert we're back on the mismatched path,
so that hypothesis can be tested differently: find what palette-
init code path KS takes when it sees this chipset combination,
and what we report that drives the wrong colours.

### Medium-term: full AGA WB boot path

When we want to remove the OCS-fallback workaround, we need to
identify why WB doesn't install its view when KS reports full AGA.
The investigation needs CPU-execution tracing rather than
chipset-register snapshots — see "Still possible" list above.

## How to reproduce + verify

Boot probe (renders WB now):

```
printf '%s\n%s\n%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_frames","arguments":{"frames":300}}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"insert_media","arguments":{"path":"<path to WB 3.1 Disk 2 zip>"}}}' \
  '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"run_frames","arguments":{"frames":3000}}}' \
  '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"dump_framebuffer","arguments":{"path":"/tmp/wb.png"}}}' \
  | ./target/release/emu198x-amiga --mcp
```

Cross-check: `tools/fs-uae-cross-check.sh`.

HIRES rendering verification (any time, no boot needed beyond
reaching the AGA HIRES screen): poke a known pattern into
`bpl_pt[0]` and dump the framebuffer. Every fb_x position in the
visible width must match the expected colour from the pattern bits
of both planes. The 2026-05-25 evening session captured the
expected pattern above for reference.

## Recent commits (S → AE-b)

```
ad-hoc   AE-b  agnus_id revert — WB renders again, palette is next
f27f7ce  AE-a  HIRES dma_claim schedule (kept; verified correct)
8170239  AD    Alice agnus_id corrected to PAL=$2300/NTSC=$3300 (reverted in AE-b)
ff8a8f0  AC    chipset reads + Alice agnus_id (agnus_id reverted; FMODE readback kept)
4eeec95  AB    watch_memory + poke_word — render path proven correct
d9caeb3  AA    fs-uae cross-check script + screenshot
76ac575  Y+Z   palette log + restart tool
d52a786  U     AGA palette via BPLCON3 BANK + LOCT
ebd6e5b  T     DENISEID/FMODE/BPLCON3 routing
9fbaf4a  S     zip-aware insert_media + STOP-instruction reframing
```

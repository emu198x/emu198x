# UI boot-verification sweep — 2026-06-22

Discharges the **boot-verification debt** from the emu198x-ui migration (see
[`../plans/ui-harness-migration-resume.md`](../plans/ui-harness-migration-resume.md)).
Lists A and B shipped their first UIs on *smoke-launch* alone ("window opens,
runs N s, no stderr"), which never proved a machine reached its expected screen.
This sweep captured a real framebuffer per system via the headless
`--screenshot` path and inspected each one.

## Method

Built each runner `--no-default-features` (headless; the `--screenshot` /
`--frames` path lives in `script.rs`, no `ui` feature needed) and ran:

```
emu198x-<system> --frames 300 --screenshot out.png   # 600 for re-checks
```

Default firmware resolved from `~/.emu198x/roms/<system>/`. The headless
renderer is sound — confirmed by the many crisp text-screen passes below
(Oric, MSX, PET, etc.), so a wrong capture is a real machine/render fault, not
a screenshot artifact.

## Results

### Booting correctly (12)

| System | Screen reached |
|--------|----------------|
| Jupiter Ace | blank screen + cursor on the bottom input line (correct — the Ace prints no banner) |
| Mattel Aquarius | `BASIC` / `Press RETURN key to start` |
| Oric Atmos | `ORIC EXTENDED BASIC V1.1uk` / `Ready` |
| Memotech MTX | `Ready` + cursor |
| Tatung Einstein | `*** EINSTEIN ***` / `TATUNG/Xtal MOS 1.2` / `Ready` / `>` |
| Acorn Atom | `ACORN ATOM` / `>` |
| Spectravideo SVI-328 | `SPECTRAVIDEO` boot splash (proceeds to BASIC) |
| ColecoVision | `COLECOVISION ™` BIOS screen (`© 1982 COLECO`) |
| MSX | `MSX BASIC version 1.0` / `Ok` + function-key bar |
| Commodore PET | `### COMMODORE BASIC ###` / `READY.` |
| Commodore VIC-20 | `**** CBM BASIC V2 ****` / `READY.` |
| Acorn Electron | `Acorn Electron` / `BASIC` / `>` |

### Problems found (2)

- **Sinclair ZX81** and **Sinclair ZX80** — instead of the near-blank screen +
  cursor a freshly booted machine shows, the lower ~⅔ of the screen is a stable
  checkerboard of a repeating glyph (unchanged at 600 frames). Identical on both
  siblings, which share the ULA display generation. The area below the collapsed
  display file rendered RAM contents instead of blank. **Fixed** in PR #624 — a
  double-NEWLINE-advance in `render_display`. One fix covered both.

### Re-investigated: BBC Micro — sweep false alarm (corrected 2026-06-22)

The original sweep flagged the BBC as stalling at the `BBC Computer 32K` banner
without entering BASIC. **That was a methodology error in the sweep, not a
machine bug.** The headless `--screenshot` path does *not* auto-install a
language ROM (the interactive UI best-effort installs BASIC into bank 15;
headless callers pass `--sideways` explicitly). My capture ran the bare MOS with
no language ROM, so it correctly showed only the banner.

Re-run with `--sideways 15=basic.rom`, the BBC **boots into BASIC**: a
PC-histogram + OSWRCH trace confirmed it enters bank 15, runs BASIC, and prints
`BBC Computer 32K` + `BASIC`. The interactive UI (which installs BASIC) was fine
all along.

The narrower follow-up (BASIC entered but never emitted the `>` prompt) is now
also **fixed**. Root cause: there was no 6850 ACIA model, so reads of the serial
status register at `$FE08` returned open-bus `0xFF` — bit 7 set, which the MOS
read as "the ACIA is interrupting". The IRQ handler serviced that phantom serial
interrupt on every IRQ and never cleared the System VIA's 100 Hz timer, so the
CPU IRQ stayed asserted continuously (an interrupt storm) and BASIC's foreground
was starved at `$8ADD` before it could print `>`. Modelling the ACIA (idle =
TDRE set, interrupt bit clear, faithful to b-em's `acia.c`) clears the storm; the
BBC now boots to `BBC Computer 32K` / `BASIC` / `>`. Found with a PC histogram +
OS-call trace + per-VIA IFR dump; the b-em reference confirmed the `$80` status
interrupt-bit semantics.

### Cart-only consoles — verified with TOSEC carts (5 of 5)

These runners require `--cart`. Tested against commercial titles from the full
TOSEC at `/Volumes/Data/Library/ROMs/TOSEC/`:

- **Sega Master System — ✅.** Alex Kidd in Miracle World (US) boots straight to
  its title screen — VDP, mapper, Z80 all good. So the earlier **black** screen
  with `rachel.sms` is the **Rachel build**, not the core (relevant to the Rachel
  cross-platform goal: investigate Rachel's SMS image — mapper/header/entry —
  not our emulator).
- **Sega SG-1000 — ✅.** Congo Bongo (1983)(Sega) reaches its title screen
  (`© SEGA 1983`).
- **Atari 7800 — ✅.** Asteroids (1987)(Atari)(NTSC) reaches its title + starfield
  (`COPYRIGHT ATARI 1984`); the `.a78` header is parsed.
- **Atari 5200 — ✅ (after a fix).** Defender (1982)(Atari)(US) boots to its
  difficulty menu. It was black until the runner's BIOS auto-resolution was
  fixed to find `5200.rom` (it only looked for `bios.rom`, and the optional BIOS
  failed silently → blank). Fixed in both UI + headless (PR #630).
- **Sord M5 — ✅.** Dig Dug (1982)(Namco) and Mappy (1983)(Namco) reach their
  title screens (`© NAMCO`). No code change needed — the M5 boots its monitor
  ROM first and only hands off to the cart after ~1,000 frames, so it reads as
  black at the 300-frame sweep default but is fine given time.

Reference now vendored: Genesis Plus GX (`emulators/multi-system/genesis-plus-gx/`),
covering SMS, SG-1000 and Game Gear.

## Follow-ups

- ~~ZX80/ZX81 lower-screen render~~ — **fixed** (PR #624).
- ~~BBC Micro `>` REPL prompt not emitted~~ — **fixed** (6850 ACIA model; was an
  interrupt storm from open-bus `$FE08` reads).
- ~~SMS: stage a commercial cart, retest~~ — **done, core verified** (Alex Kidd
  boots). Remaining: investigate why the Rachel SMS *build* renders black.
- ~~Stage carts for SG-1000 / 5200 / 7800 and re-run~~ — **done, all boot**
  (5200 needed the BIOS-resolution fix, PR #630).
- ~~Re-test Sord M5 with a cart~~ — **done** (Dig Dug + Mappy boot).

**Every list-A/B system is confirmed booting**, and all render/timing/boot faults
the sweep surfaced are fixed. The boot-verification sweep is complete; the only
non-emulator follow-up is the Rachel SMS *build* (renders black on a verified-good
SMS core — investigate the image, not the emulator).

## Harness-migrated bespoke systems — boot sweep (2026-06-23)

After the four bespoke runners moved onto the `emu198x-ui` harness (Spectrum #638,
Dragon #639, C64 #640, Amiga #642), each was re-verified — **not** by the
window-opens smoke-launch the migrations used (that only proves "runs without
crashing"), but by the deterministic frame-counted headless `save_screenshot`
path, with the rendered frame inspected.

| System | Reached | Frames to boot screen |
|--------|---------|-----------------------|
| Sinclair Spectrum 48K | `© 1982 Sinclair Research Ltd` | 89 |
| Commodore 64 | `**** COMMODORE 64 BASIC V2 ****` / `64K RAM SYSTEM` | 109 |
| Commodore Amiga (A500, Kickstart 1.3, no disk) | the insert-disk hand holding the *AMIGA Workbench V1.3* floppy | **~600** |
| Dragon 32 | **unverified** — no Dragon ROM staged | — |

**Method.** Spectrum / C64 via `--script` with a `wait_for_boot` + `save_screenshot`
step; Amiga via `--frames N --screenshot` (a fixed budget, *not* `--wait-for-boot`).
The runtimes are unchanged by the migrations, so a headless boot is what the
harness UI displays.

**Key lesson — boot budgets vary wildly; don't trust a flag or a wall-clock smoke.**
The Amiga's `boot.detected` reason is `display-active`, which fires the instant
the display turns on — a dark grey early-boot frame, ~hundreds of frames *before*
the Kickstart insert-disk screen renders. A `--wait-for-boot` screenshot (or the
5-second smoke-launch the migration used) "passes" on that blank frame. Only a
**fixed ~600-frame** budget reaches the real screen. So per-system boot
verification must use a frame-counted screenshot with a generous, per-machine
budget and an *inspected* frame — never `display-active` alone, never wall-clock.

**Open:** Dragon 32 is unverified — there is no Dragon ROM in `~/.emu198x/roms/`
(its conventional path is `~/.emu198x/roms/dragon/dragon32.rom`) nor in the asset
library. It is otherwise covered by its 74 passing crate tests + clean build.
Stage a Dragon 32 BASIC ROM there to close this.

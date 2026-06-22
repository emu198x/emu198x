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

One **narrower** real issue remains: BASIC enters and prints its startup but
never emits the `>` REPL prompt (OSWRCH stops after `BASIC\r\n\r\n`; the CPU
loops inside BASIC near `$8ADC`). That needs BBC BASIC ROM-level tracing and is a
separate, smaller follow-up — the machine boots and runs BASIC, it just doesn't
present the interactive prompt yet.

### Needs a cartridge to verify (5)

These runners require `--cart` and can't boot to anything cartless:

- **Sega Master System** — the only cart available locally (`rachel.sms`, the
  Rachel build) renders **black** through 1200 frames. Can't tell core-vs-cart
  without a commercial title to cross-check; flagged for follow-up. (Relevant to
  the Rachel cross-platform goal — Rachel on the SMS core shows nothing.)
- **Sega SG-1000**, **Atari 5200**, **Atari 7800**, **Sord M5** — no test
  cartridge staged. Cartless the M5 shows black (expected — cart-only); the
  others won't run without `--cart`. Verification deferred until carts are
  staged.

## Follow-ups

- ~~ZX80/ZX81 lower-screen render~~ — **fixed** (PR #624).
- BBC Micro `>` REPL prompt not emitted (boots + runs BASIC; needs BASIC ROM trace).
- SMS: stage a commercial cart, retest; investigate Rachel-renders-black.
- Stage carts for SG-1000 / 5200 / 7800 / Sord M5 and re-run.

The 12 confirmed-good systems — plus the BBC (boots BASIC) — need no further boot
work; the ZX80/81 render is fixed.

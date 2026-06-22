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

### Problems found (3)

- **Sinclair ZX81** and **Sinclair ZX80** — instead of the near-blank screen +
  cursor a freshly booted machine shows, the lower ~⅔ of the screen is a stable
  checkerboard of a repeating glyph (unchanged at 600 frames). Identical on both
  siblings, which share the ULA display generation. Looks like the area below
  the collapsed display file is rendering RAM contents instead of blank — a
  ZX80/81 display-generation accuracy bug. One fix likely covers both.
- **Acorn BBC Micro** — prints the `BBC Computer 32K` banner and then stops; no
  `BASIC` / `>` prompt even at 600 frames (~12 s), whereas the Electron (same
  Acorn BASIC) reaches `BASIC` / `>` correctly. The MOS boots but the language
  ROM (BASIC, best-effort installed into sideways bank 15) is not entered. Boot
  stalls after the banner.

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

- ZX80/ZX81 lower-screen render (one bug, both machines).
- BBC Micro BASIC-entry stall after the banner.
- SMS: stage a commercial cart, retest; investigate Rachel-renders-black.
- Stage carts for SG-1000 / 5200 / 7800 / Sord M5 and re-run.

The 12 confirmed-good systems need no further boot work.

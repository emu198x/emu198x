# Fleet index

At-a-glance roll-up of the per-system status pages. One row per system; follow
the link for the full page (boot detail, accuracy gaps, known unknowns, timing,
tooling, peripherals/connectivity). Conventions and template in
[`README.md`](README.md).

**Columns.** *Timing* = which model is realised (the goal is `hc`-driven master
clock; relaxed models are honest debt — see RULES §51-64). *Drive* = `win+mcp`
(native window, primary tier) / `mcp` (headless, `--script`+`--mcp`) / `—` (no
binary). *Net* = the standalone internet-capability verdict. *Rachel* = runs the
Rachel client (offline/AI — a **separate axis** from Net; see
[`rachel-readiness.md`](rachel-readiness.md)).

## Primary (native window, CPU oracles green)

| System | Boot today | Timing | Drive | Net | Rachel |
|--------|-----------|--------|-------|-----|--------|
| [ZX Spectrum](sinclair/zx-spectrum/index.md) | Playable, 11 variants | **hc-driven (reference)** | win+mcp | Yes | ✅ |
| [Commodore 64](commodore/c64.md) | Fully functional | **hc-driven (per-cycle VIC-II)** | win+mcp | Yes | ✅ |
| [Nintendo NES](nintendo/nes.md) | SMB renders; 135/155 sweep | hc-driven (dot 2C02) | win+mcp | Yes | ✅ |
| [Commodore Amiga](commodore/amiga/index.md) | Workbench 3.1 | hc-driven (DMA-exact) | win+mcp | Yes | ✅ |
| [Nintendo Game Boy](nintendo/game-boy.md) | DMG verifier | PPU dot (mooneye 75/75 + acid2) | win+mcp | Yes | ✅ |
| [Dragon 32](dragon/index.md) | Boots BASIC | beam VDG (partial) | win+mcp | Yes | ✅ |

## Extended (headless; script + MCP parity)

| System | Boot today | Timing | Drive | Net | Rachel |
|--------|-----------|--------|-------|-----|--------|
| [Atari 800XL](atari/800xl.md) | BASIC READY; types | fixed-DMA (relaxed) | mcp | Yes | ✅ |
| [Commodore VIC-20](commodore/vic-20.md) | READY; PRG autoload | relaxed | mcp | Yes | ✅ |
| [MSX1](msx/index.md) | MSX BASIC | per-dot VDP (3:2 CPU phase relaxed) | mcp | Yes | ✅ |
| [Acorn Electron](acorn/electron.md) | BASIC `>`; types | 1 MHz RAM contention + modes-0-3 fetch halt | mcp | Marginal | ✅ |
| [Acorn BBC Micro](acorn/bbc-micro.md) | MODE 7 banner renders | 1 MHz-bus contention | mcp | Yes | ✅ |
| [Oric Atmos](oric/atmos.md) | BASIC; types | per-scanline | mcp | Marginal | ✅ |
| [Sord M5](sord/m5.md) | Carts play; Dig Dug | per-dot VDP | mcp | Marginal | — |
| [Memotech MTX](memotech/mtx.md) | BASIC `Ready` | per-dot VDP | mcp | Marginal | — |
| [Tatung Einstein](tatung/einstein.md) | MOS prompt; types | per-dot VDP (exact 4 MHz:5.37 MHz) | mcp | Marginal | — |
| [Spectravideo SVI-328](spectravideo/svi-328.md) | SV-BASIC; types | per-dot VDP | mcp | Marginal | — |
| [Commodore PET](commodore/pet.md) | BASIC `READY.` | relaxed (1× CRTC) | mcp | Marginal | — |
| [Jupiter Ace](jupiter/ace.md) | Forth; interactive | hc-ish; e-o-f display | mcp | No | target (planned) |
| [Mattel Aquarius](mattel/aquarius.md) | BASIC + carts play | end-of-frame (NTSC/PAL) | mcp | No | — |
| [Sega Master System](sega/master-system.md) | Cart title (Mode 4) | per-dot VDP (3:2 interleave) | mcp | Marginal | ✅ |
| [Sega SG-1000](sega/sg-1000.md) | Cart title | per-dot VDP (3:2 phase relaxed) | mcp | No | — |
| [ColecoVision](coleco/colecovision.md) | BIOS title | per-dot VDP (3:2 phase) | mcp | Marginal | ✅ |
| [Atari 2600](atari/2600.md) | Combat playable | pixel-level TIA | mcp | Yes | ✅ |
| [Atari 5200](atari/5200.md) | Pac-Man menu | fixed-DMA (relaxed) | mcp | No | ✅ (offline) |
| [Atari 7800](atari/7800.md) | Asteroids renders | zone-DMA | mcp | No | — |
| [Acorn Atom](acorn/atom.md) | Prompt; types | relaxed (text VDG) | mcp | Marginal | — |
| [Sinclair ZX81](sinclair/zx81.md) | Boot screen | relaxed (NMI/HALT) | mcp | Marginal | — |
| [Sinclair ZX80](sinclair/zx80.md) | Boot (FAST only) | relaxed (FAST only) | mcp | No | — |

## Not started (core not built)

| System | Rachel repo | Timing | Net | Rachel |
|--------|-------------|--------|-----|--------|
| [Atari ST](atari/st.md) | `rachel-atari-st` | — (68000 shared) | Yes | target (core WIP) |
| [Sega Mega Drive](sega/mega-drive.md) | `rachel-sega-genesis` | — | Yes | target (core WIP) |
| [TRS-80 CoCo](tandy/coco.md) | `rachel-coco` | — (6809 shared) | Yes | target (core WIP) |

## Code198x curriculum

How far the teaching mission ([Code198x](https://code198x.stevehill.xyz)) reaches
each machine. Three tiers (from the Code198x repo):

- **Curriculum** (deep: code-samples + build skills + dev repo) — the four launch
  platforms: **ZX Spectrum, Commodore 64, NES, Amiga**.
- **Platform doc** (`docs/platforms/`, pedagogical framing) — additionally:
  Game Boy, MSX, BBC Micro, Atari 8-bit (800XL), Atari ST, Master System,
  Mega Drive.
- **Vault entry** (encyclopaedia only) — additionally: Dragon 32, VIC-20, PET,
  Acorn Electron, Atari 2600/5200/7800, ColecoVision, Oric, ZX81.
- **Not in Code198x yet** — Sord M5, Memotech MTX, Tatung Einstein, SVI-328,
  Jupiter Ace, ZX80, SG-1000, Aquarius, Acorn Atom.

## ROM availability & shipping (survey — verify before relying)

Whether a learner can run a system without sourcing a copyrighted ROM, and Steve's
leaning to **embed redistributable ROMs in the binary** (see
`project_rom_shipping_strategy.md`). **First pass — confirm each licence before it
drives a shipping decision:**

- **No system BIOS needed** → NES, Atari 2600/7800, Master System, SG-1000, Game
  Boy (boot ROM optional). Embeddable trivially / N/A.
- **Free open replacement** → MSX (C-BIOS), Atari 800XL + 5200 (AltirraOS),
  Atari ST (EmuTOS), C64 / VIC-20 / PET (Open ROMs), Amiga (AROS vs proprietary
  Kickstart). Candidate to embed.
- **Vendor-permitted** → ZX Spectrum (Amstrad permits ROM redistribution for
  emulation); ZX80/81 likely same lineage — verify.
- **Proprietary, user-supplied** → Dragon/CoCo, BBC Micro, Electron, Atom, Oric,
  Sord M5, MTX, Einstein, SVI-328, Jupiter Ace, Aquarius, ColecoVision.

The embed-vs-supply call is per-ROM and needs the actual licence checked; this
table is the starting point, not the verdict.

---

Cross-cutting views: [`rachel-readiness.md`](rachel-readiness.md) (Rachel client/
netplay), [`../status/current-system-usability.md`](../status/current-system-usability.md)
(launch-practical usability), [`../status/outstanding-work.md`](../status/outstanding-work.md)
(per-machine open items).

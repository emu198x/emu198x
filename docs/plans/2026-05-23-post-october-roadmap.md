# Post-October Roadmap and Feature Gap Analysis

**Status:** Working plan
**Date:** 2026-05-23
**Supersedes:** nothing. Complementary to
[`2026-04-12-emulator-suite-coherent-development-plan.md`](2026-04-12-emulator-suite-coherent-development-plan.md)
(architectural framing) and
[`2026-04-28-october-runup-plan.md`](2026-04-28-october-runup-plan.md)
(tactical Oct sequencing). This document picks up *after* both —
the longer arc once Spectrum SOLID has shipped and the project is
public.

## Purpose

Two things, in one document:

1. **Honest feature-gap inventory** across every shipping system and
   every system in scope. Names what's missing, what's overclaimed,
   and what's invisible to the catalogue tests.
2. **A sequenced roadmap** covering the next ~18 months — through the
   four-anchor engineering bar, Wave 2 systems, cross-system polish,
   and into the world-systems backlog.

Aimed at: project decision-making about what to land next. Not aimed
at: users (the README does that) or contributors (CONTRIBUTING.md
does that).

## Source-of-truth hierarchy

When this disagrees with anything, use:

1. [`RULES.md`](../../RULES.md)
2. [`knowledge/decisions/`](../../knowledge/decisions/)
3. [`2026-04-12-emulator-suite-coherent-development-plan.md`](2026-04-12-emulator-suite-coherent-development-plan.md)
4. [`2026-04-28-october-runup-plan.md`](2026-04-28-october-runup-plan.md)
5. this plan
6. README and per-system overview docs (state claims)

This plan should never silently override a `knowledge/decisions/`
record. If a roadmap item below contradicts a decision, the decision
wins and this plan needs updating.

---

# Part 1 — Where we actually are (May 2026)

## Per-system reality vs claims

The README lists six systems as the "current implementation focus."
Internally, each is at a different level of completeness. The
table below maps README claim → reality → headline gap. Reality is
sourced from the architecture-review decision records, recent
commits, and the catalogue manifest state.

| System | README claim | Reality (May 2026) | Headline gap |
|---|---|---|---|
| **Spectrum 48K** | Production-ready | 48K is production-ready; 7 other variants exist as crates but the README hides them | Variant catalogue assertions per SOLID criterion 1, 8 variants × 10 titles |
| **Spectrum 128K / +2 / +2A / +2B / +3 / 16K / Spectrum+** | Not mentioned in intro | Crates exist; coverage and catalogue completion in progress | Per-variant SOLID closure |
| **Spectrum Pentagon / Scorpion / Timex** | Not mentioned | Crates exist; explicitly deferred post-October | TR-DOS (Pentagon), DOCK (Timex) format coverage |
| **C64** | Boots; tape; disk; 1541 live | All true; 1541 read-only; cartridge support absent | Seam 1 BA/RDY (landed); CRT cartridge; REU; mouse/paddle; PAL/NTSC parity |
| **NES** | 14 mappers; nestest; SMB renders | All true; 627/629 ROMs survive 300 frames | Blargg PPU test ROM suite in CI (Seam 1 architecture review); FDS; NSF; Zapper; mapper coverage past 14 |
| **Amiga A500 OCS PAL** | Boots Kickstart 1.3; Workbench desktop; ADF mount | All true; full chip stack; snapshot round-trip | A500+ (ECS) catalogue closure; A600/A2000/A3000/A4000; HD floppy; hard drive (RDB); DF1/DF2/DF3; AHI/RTG |
| **Amiga A1200 (AGA)** | Not in README | Mid-flight; KS 3.1 reaches reboot trampoline but doesn't boot | Wack-style debug entry investigation; full AGA chipset (agnus-aga, denise-aga scaffolds exist) |
| **Amiga CDTV / CD32** | Not in README | Not started | Akiko chunky-to-planar; CD-ROM (CDXL); CD32 controller |
| **Game Boy DMG** | Boots; PPU/APU; battery RAM | All true | GBC (Color); SGB; link cable; long-tail mappers (MBC6/7, HuC1/3, Camera/Printer) |
| **Dragon 32** | Boots; CAS; ROM cartridge; VDK; 11/12 XRoar match | All true; DragonDOS WD2797 partial | Dragon 64; CoCo line; DragonDOS write; OS-9; stereo cartridge audio |

**The README under-claims for Spectrum** (hides 7 in-flight variants
under the "48K only" framing) and **over-claims for the line "fresh
Rust workspace" implication** (the workspace has 113 crates including
chips that aren't yet in any user-facing machine, which is a feature,
not a bug — but readers may not infer this).

## Architectural seam status

Each anchor system has a "five seams" architecture review in
`knowledge/decisions/`. Status as of 2026-05-23:

| System | Seam 1 | Seam 2 | Seam 3 | Seam 4 | Seam 5 |
|---|---|---|---|---|---|
| Spectrum | Engine-side landed; strict Float48K not in CI | (joystick routing) landed | landed | landed | landed |
| C64 | Audited; six lock-down tests | landed | landed | landed | landed |
| NES | Partial — blargg PPU ROMs not in CI | landed | landed | landed | landed |
| Amiga (OCS) | Closed all five 2026-05-21 | — | — | — | — |
| Amiga (full family) | landed | proposed | proposed | proposed | proposed |

**Single biggest accuracy gap left** across the architecture reviews:
**NES blargg PPU test ROMs are not in CI** (Seam 1, NES review). The
ROMs are ~50 KB, redistributable, and the harness is documented.
This would catch a class of latent bugs that the catalogue cannot.

## Code quality and verification state

- **Tom Harte:** Z80 100%, 6502 100%, 68000 100% (1,000,058 vectors).
  Bar holds.
- **ZEX:** ZEXDOC + ZEXALL pass.
- **Coverage gate:** 72% workspace floor; SOLID criterion 11 targets
  ≥90% on Spectrum-specific crates. **~18 percentage points to
  close on Spectrum side.**
- **`.unwrap()` policy:** denied workspace-wide. SOLID criterion 9
  asserts no stubs in Spectrum library code.
- **`unsafe_code`:** forbidden workspace-wide.
- **Snapshot round-trip:** working for every anchor system × every
  in-scope variant per the existing catalogue.

## Distribution and release state (new since 2026-05-22)

- **License:** GPL-2.0-or-later (workspace-level, all 113 crates)
- **Public repo:** github.com/emu198x/emu198x (pushed 2026-05-22)
- **CI:** fmt + clippy + coverage + zex on Linux; smoke-build on
  macOS arm64+x86_64, Windows, Linux x86_64
- **Release pipeline:** cargo-dist on `v*` tags builds 6 native
  shells across 4 targets; release-plz opens auto-bump PRs on push
  to main when conventional-commits markers present
- **GitHub App:** release-plz installed (full chain works end-to-end)
- **crates.io:** no crates published yet

---

# Part 2 — Where the existing plan already takes us

The April 12 coherent plan + April 28 October-runup plan + the
[product-roadmap](../../knowledge/decisions/product-roadmap.md)
decision already cover the near arc.

**Public October (Crash! Live):** Spectrum SOLID only (eight variants,
80 manifest entries, single binary, MCP server, CRT filter, native
UI, save state on every variant × title cell, ≥90% coverage, no
stubs, Tom Harte/ZEX green).

**Engineering bar (post-October-public, no deadline):** C64, NES,
Amiga catalogues each at the same 10-title bar.

**Wave 2 (post-engineering-bar):** Atari 2600, BBC Micro, MSX, Master
System. Ordered by historical significance and chip reuse
(Z80 family or 6502 family already exists in the workspace).

**Wave 3+ (grouped by shared chips):**

- Z80 family: CPC, ZX80, ZX81, ColecoVision, SG-1000
- 6502 family: Atari 800XL/5200/7800, Electron, Atom, VIC-20, PET, Oric
- 68000 family: Atari ST, Mega Drive
- 6809 family: Dragon 64 (variant of current), CoCo, Vectrex
- Long tail: Jupiter Ace, Aquarius, MTX, Sord M5, SVI-328, Einstein

**Per the chip-reuse map** in product-roadmap.md, the Z80 already
exists, the 6502 exists, the 68000 exists, the 6809 exists,
AY-3-8912 exists, TMS9918 does *not* yet (would unlock MSX +
ColecoVision + SG-1000), SN76489 does not (would unlock SMS + MD +
ColecoVision + BBC).

## What the existing plan doesn't cover

Five categories sit outside the current plans. This document's main
contribution is naming them and proposing where they fit.

1. **Cross-system features** (rewind, debugger UI, cheats, netplay,
   game library, achievements, gamepad remap UI)
2. **Format gaps inside already-shipping systems** (CRT cartridges
   for C64, NSF/FDS for NES, CDXL for Amiga CD32, etc.)
3. **Distribution-tier gaps** (crates.io publishing of chip
   libraries, Homebrew tap, Windows installer, WASM build)
4. **Systems with donor code in `Emu198x-Oldest` not yet ported**
   (Atari 2600/5200/7800/800XL, MSX, ColecoVision, BBC Micro,
   Acorn Atom/Electron, Sega Master System/SG-1000, Oric, ZX80/81,
   Jupiter Ace, Memotech MTX, Mattel Aquarius, Tatung Einstein,
   Spectravideo SVI-328, Sord M5 — 15 systems with substantive
   1,000+ LOC implementations per system)
5. **Systems in scope by the five axes but not in any current code**
   (Apple II line, TI-99/4A, Japanese 8-bit/16-bit, Eastern Bloc
   clones, Soviet, Sega Genesis/Saturn/Dreamcast, 3DO, N64, PSX,
   iQue Player)

---

# Part 3 — Cross-system gaps a serious emulator user expects

These are functional gaps across **all** shipping systems, not
per-system feature work. Most are absent across the catalogue today
and absent from any planning record. Some belong on the long-term
roadmap; some are explicit non-goals; one or two are quick wins.

| Feature | Where retro-emu projects typically land | Our position | Recommendation |
|---|---|---|---|
| **Save states** | Universal; manage via UI | Done as snapshots; no UI to browse / name | Per-system save-state browser. Wave 2 or after |
| **Rewind** | Modern emulators (RetroArch, Mesen2, openMSX) | Absent; not in any plan | **Could be a Phase 5 win.** Requires per-tick state delta, not full snapshot; large engineering investment |
| **Cheats / Game Genie** | Universal in retro emulators | Absent | Per-system; small individually, ~3-5 days per system |
| **Debugger / disassembler UI** | RetroArch / Mesen2 / openMSX have full debuggers | MCP query surface exists (`amiga.cpu.pc`, etc); no user-facing debug UI | **Genuinely valuable for the project's audience** (Code198x learners using Emu198x). Worth designing |
| **Netplay** | RetroArch netplay, BizHawk netplay | Absent | Explicit non-goal probably; deterministic cores would make it feasible but the scope is enormous |
| **Achievements (RetroAchievements)** | Some emulators integrate | Absent | Out of scope; tied to a specific third-party |
| **Game library / picker UI** | All-in-one emulator frontends | Absent; per-system CLI | **Out of scope.** Per [`no-unified-launcher.md`](../../knowledge/decisions/no-unified-launcher.md) (2026-05-23) — per-system binaries are the product; third-party launchers (LaunchBox / OpenEmu / Playnite) handle library management |
| **Screen filters beyond CRT** | scanlines, NTSC composite, palette swaps | CRT preset exists; no other filters | Per-shader work; small per filter |
| **Gamepad remap UI** | Universal | Inputs hardcoded; no remap | Small UI work once native UI matures |
| **Mid-game video record** | Hotkey-driven in most | MCP tool exists (`start_video_recording`); no UI binding | Trivial once native UI matures |
| **WASM / web build** | itch.io WASM emulators; web embeds | Not started; demoted in April 12 plan | Post-launch. Real opportunity for Code198x curriculum embeds |
| **Mobile / touch** | Delta (iOS), various Android | Not started | Out of scope unless someone makes a case |

**Most credible quick wins:** cheat support (per-system, small per
system); screen-filter library (per-shader); gamepad remap UI per
system once native UI matures. (Game library / launcher was on this
list before 2026-05-23; retired per
[`no-unified-launcher.md`](../../knowledge/decisions/no-unified-launcher.md).)

**Most credible high-leverage build:** debugger UI. The project's
adjacent audience is Code198x learners who would benefit
enormously from a teaching-oriented debugger (single-step,
disassembly, register view, memory inspector, ULA/CIA/VIC-II live
state). The MCP query surface already exposes most of what a
debugger needs to read.

## Format gaps inside shipping systems

These are within-system gaps, but the architecture-review docs
mostly focus on accuracy rather than format breadth. Inventory:

| System | Format / capability gap | Effort |
|---|---|---|
| C64 | CRT cartridge format (no crate) | Small — well-documented format |
| C64 | REU (RAM Expansion Unit) | Medium — adds a chip with banking |
| C64 | 1351 mouse, paddles | Small per peripheral |
| C64 | 1541 write path | Medium-large — was explicitly out-of-scope for October |
| C64 | C128 | Large — 8502 CPU variant, VDC, second VIA |
| NES | FDS (Famicom Disk System) | Medium — adds disk drive |
| NES | NSF (NES Sound Format) playback | Small — APU is done, NSF is a thin loader |
| NES | Zapper / Power Pad | Small — input mapping |
| NES | Four-player adapter | Small |
| NES | Mapper coverage past 14 (60–80 documented) | Mapper-by-mapper, days each |
| NES | Game Genie / Pro Action Replay | Small once cheat system exists |
| Amiga | ECS (A500+) catalogue completion | Per architecture-full-family review |
| Amiga | AGA (A1200, A4000, CD32) | Mid-flight |
| Amiga | DF1, DF2, DF3 (second/third/fourth floppy) | Small once floppy is done — wiring only |
| Amiga | HD floppies (1.76MB DD) | Small — format only |
| Amiga | Hard drive (RDB, SCSI/IDE) | Medium-large |
| Amiga | CDTV / CD32 | Akiko chip; CDXL streaming; CD-ROM controller |
| Amiga | RTG (Picasso IV, Cybervision, etc.) | Medium per board |
| Amiga | AHI (audio expansion) | Medium |
| Amiga | Mouse on port-2 | Trivial |
| Amiga | Genlock | Niche |
| Game Boy | GBC (Color) | Large — different PPU mode set, double speed CPU |
| Game Boy | SGB enhancements | Medium |
| Game Boy | Link cable | Medium — multi-instance pairing |
| Game Boy | Camera, Printer | Small per peripheral |
| Dragon | Dragon 64 | Small — bigger ROM, banked RAM |
| Dragon | CoCo 1/2/3 | Medium — sibling architecture; large catalogue |
| Dragon | DragonDOS write | Small once read is complete |
| Dragon | OS-9 | Medium — operating system above the existing path |
| Spectrum | TR-DOS (Pentagon) | Medium — adds beta-disk controller |
| Spectrum | DOCK (Timex cartridge) | Small |
| Spectrum | Microdrive | Medium — niche but iconic |
| Spectrum | Interface 1 / Multiface 128 | Small per device |
| Spectrum | +D / DISCiPLE | Medium |
| Spectrum | AMX Mouse | Small |

---

# Part 4 — Distribution gaps

Public-repo release closed the obvious gaps (license, governance,
CI, cargo-dist). Two layers remain.

## Crates.io publishing

The workspace contains crates that are independently valuable to
other emulator projects:

- `mos-6502` — used by C64, NES, BBC, Atari 8-bit, Apple II, etc.
- `zilog-z80` — used by Spectrum, MSX, CPC, SMS, ColecoVision, ZX80/81
- `motorola-68000` — used by Amiga, Atari ST, Mega Drive, NeoGeo
- `motorola-6809` — used by Dragon, CoCo, Vectrex
- `format-sinclair-zx-spectrum-tap` / `tzx` / `sna` / `z80` — standard
  Spectrum format readers nobody else has packaged
- `format-nintendo-nes-ines` — iNES + NES 2.0 parsing
- `format-commodore-amiga-adf` — Amiga disk image
- `gi-ay-3-8912`, `mos-sid-6581`, `commodore-paula-8364` — chip
  implementations

Publishing these creates external interest, gets external bug
reports, and lets the chip-reuse map serve the wider Rust emulator
ecosystem.

**Blockers:**

- All workspace crates currently use `version.workspace = true`
  (lockstep) — publishing to crates.io requires per-crate versions
  to make sense
- Public APIs aren't curated; many crates expose internal types
- No crates.io README per published crate

**Recommendation:** Phase this in after first stable release.
Identify ~6–8 chip + format crates with the cleanest public API,
add per-crate `description`, `readme`, `keywords` metadata, switch
those to independent versioning, publish via `release-plz publish`.
Keep the rest workspace-internal until they have clear demand.

## Binary distribution tiers

- **GitHub Releases via cargo-dist:** done (just landed)
- **Homebrew tap:** not started. `brew tap emu198x/emu198x && brew
  install emu198x-spectrum` is a real user-facing improvement.
  cargo-dist supports auto-generating Homebrew formulae.
- **Windows installer:** cargo-dist supports MSI generation; not
  enabled (`installers = []` currently)
- **`cargo install emu198x-spectrum`:** works in principle once any
  crate is published to crates.io
- **WASM build:** not started; April 12 plan demotes it behind core
  correctness

**Recommendation:** Homebrew + Windows installer are cheap wins once
the first release ships and stays stable for a couple of weeks. WASM
is genuinely Phase 5+ work but has high payoff (Code198x curriculum
embeds, demoable web links from the README).

---

# Part 5 — Donor stash and unstarted systems

## `Emu198x-Oldest` substantive donor implementations

Per the umbrella CLAUDE.md, the frozen `Emu198x-Oldest` codebase
holds 1,000+ LOC implementations for **15 systems** plus the Amiga
AGA chipset scaffold:

| System | Donor state | Chip reuse | Effort to port |
|---|---|---|---|
| Atari 2600 | Substantive | TIA (donor), 6502 (current), RIOT (donor) | Medium — TIA is the hard part |
| Atari 5200 | Substantive | 6502, ANTIC, GTIA, POKEY (all donor) | Medium |
| Atari 7800 | Substantive | 6502, MARIA, POKEY, TIA | Medium |
| Atari 800XL | Substantive | 6502, ANTIC, GTIA, POKEY | Medium |
| MSX | Substantive | Z80 (current), TMS9918 (donor), AY (current) | Small once TMS9918 lands |
| ColecoVision | Substantive | Z80 (current), TMS9918, SN76489 (donor) | Small after TMS9918 + SN76489 |
| BBC Micro | Substantive | 6502, MOS Video (donor), SAA5050, SN76489 | Medium |
| Acorn Atom / Electron | Substantive | 6502, ULA per machine | Small per machine |
| Sega Master System / SG-1000 | Substantive | Z80, VDP (Sega), SN76489 | Small after SN76489 + Sega VDP land |
| Oric | Substantive | 6502, ULA, AY | Small |
| ZX80 / ZX81 | Substantive | Z80, ULA per machine | Very small (ULA is simple) |
| Jupiter Ace | Substantive | Z80, ULA | Small — Forth-first machine, unique |
| Memotech MTX | Substantive | Z80, TMS9918, AY | Small after TMS9918 |
| Mattel Aquarius | Substantive | Z80, TMS9918, AY | Small after TMS9918 |
| Tatung Einstein | Substantive | Z80, CRTC, AY | Small |
| Spectravideo SVI-328 | Substantive | Z80, TMS9918, AY | Small after TMS9918 |
| Sord M5 | Substantive | Z80, TMS9918, SN76489 | Small after TMS9918 + SN76489 |
| Amiga AGA scaffold | Lighter | Agnus-AGA, Denise-AGA | Per Amiga full-family review |

**Key insight: TMS9918 + SN76489 land = 7 systems become small ports.**
MSX, ColecoVision, SG-1000, Memotech MTX, Mattel Aquarius, SVI-328,
Sord M5 all need TMS9918 + AY or SN76489. Of these, Master System
and SG-1000 are Wave 2; the rest are Wave 3.

**The donor code is the cheapest path to breadth.** Per RULES.md
rule 25: "Check the archives before writing new code." And rule 26:
"Chip/CPU/cycle-accuracy code does not port" — but **format crates,
machine wiring, and shared peripherals are usually portable with
minor adaptation.**

## Systems beyond `Emu198x-Oldest` (no donor code yet)

In the five-axis scope but not in current code or donor codebases:

**Western / Anglosphere:**
- Apple II / II+ / IIe / IIc / IIgs
- TI-99/4A
- Amstrad CPC line (mentioned in product-roadmap Wave 3+; not yet
  in any code)
- Commodore VIC-20, PET, PLUS/4
- Mattel Intellivision
- Vectrex

**Japanese 8-bit and early 16-bit:**
- PC-88 series
- PC-98 series
- MZ-series
- X1
- X68000
- FM-7, FM-77, FM Towns
- Sharp pocket computers (PC-1500 etc.)

**Eastern Bloc and Soviet (per the cultural-scope axis):**
- TK85 (Brazilian Spectrum clone)
- CP-200 (Brazilian Z80)
- PMD 85 (Czechoslovak)
- HomeLab (Hungarian)
- Pravetz 8 (Bulgarian Apple II clone)
- Pravetz 16 (Bulgarian)
- Elektronika BK (Soviet)
- Корвет / Korvet (Soviet educational)
- Soviet Spectrum clones (Hobbit, Krista, Magic)
- Chinese systems (Xiang Zhan, Hua Min etc.)

**Consoles beyond Wave 2:**
- Sega Genesis / Mega Drive (chip reuse: Z80, 68000, SN76489, YM2612)
- Sega Saturn
- Sega Dreamcast
- Nintendo N64
- Sony PlayStation
- 3DO
- iQue Player
- TurboGrafx-16 / PC Engine
- Neo Geo (AES + MVS)

**Form factors not yet covered (per the form-factor axis):**
- Single-board / kit machines (KIM-1, AIM-65, OSI Challenger,
  Sinclair MK14, Acorn System 1, NASCOM 1/2)
- Pocket computers (Casio PB-1000, Sharp PC-1500, TRS-80 PC-1/2)
- Workstations (NeXT Cube, Sun-1, Lisa)
- Music workstations (Synclavier)
- Calculators with computer features

---

# Part 6 — Proposed sequencing

The April 12 + April 28 plans take us through Spectrum SOLID and
the four-anchor engineering bar. This sequencing picks up after
that and runs through the next ~18 months.

Each phase is sized small (~1 month) / medium (~3 months) / large
(~6+ months) by total engineering work.

## Phase A — Public release stabilisation (Now → 0.1.0)

**Goal:** First public release ships clean. Users can install,
launch, get something on screen with a ROM they sourced themselves.

**Work:**

- README screenshots per shipping system (small)
- README compatibility matrix per variant × media kind (small)
- Initial CHANGELOG.md (small)
- First conventional-commits-prefixed PR to bootstrap release-plz
  (trivial)
- v0.1.0 release through the cargo-dist pipeline (rehearsal of the
  end-to-end chain)
- Fix any cargo-dist Linux apt syntax issues that surface on first
  release run

**Size:** Small. ~1 week of focused work spread across normal cadence.

**Done when:** v0.1.0 binaries downloadable from the Releases page;
README has at least one screenshot per system; one external user
has successfully launched a system from a fresh clone.

## Phase B — Spectrum SOLID closeout (Now → October)

**Already planned.** See
[`october-catalogue.md`](../../knowledge/decisions/october-catalogue.md)
and
[`2026-04-28-october-runup-plan.md`](2026-04-28-october-runup-plan.md).
This document does not re-plan it.

**Size:** Medium. ~3 months at current cadence.

**Done when:** SOLID criteria 1–11 all green.

## Phase C — Engineering-bar completion across the four anchors (Oct 2026 → Feb 2027)

**Goal:** C64, NES, Amiga catalogues each at the same 10-title bar
as Spectrum. Per-system architecture-review seams closed. Cross-system
infrastructure proven against four systems.

**Work:**

- **C64:** complete the 10-title catalogue; close C64 review Seam
  1; CRT cartridge support; basic mouse/paddle peripherals
- **NES:** complete the 10-title catalogue; **land blargg PPU test
  ROMs in CI (Seam 1)**; expand mapper coverage past 14 toward the
  ~40 mappers needed for "most commercial titles"
- **Amiga:** A500+ (ECS) catalogue entries; the A1200 AGA work
  unblocking Kickstart 3.1 boot per the in-flight commits;
  Amiga full-family review Seams 2–5; second floppy (DF1)
- **Cross:** debugger UI design — single-step / disassembly /
  register view / memory inspector / live chip state. Built once
  against the existing MCP query surface; instantiated per system.

**Size:** Large. ~4–5 months total across all four systems.

**Done when:** All four anchors pass their 10-title catalogue; NES
blargg PPU suite green; A1200 boots Workbench 3.1; debugger UI
shipped for at least Spectrum (the system the audience uses most).

## Phase D — Game Boy completion + per-system polish (Feb → May 2027)

**Goal:** Game Boy graduates from "DMG-only" to "Game Boy family
covered." Per-system cross-cutting features (cheats, screen
filters, gamepad remap, save-state browser) land in each
per-system binary, since there is no unified launcher to host them
([`no-unified-launcher.md`](../../knowledge/decisions/no-unified-launcher.md)).

**Work:**

- **Game Boy:** GBC (Color); SGB enhancements; long-tail mappers
  (MBC6/7, HuC1/3, Camera/Printer)
- **Per-system features (each landed inside every shipping
  binary):** screen-filter library beyond CRT (CRT preset already
  exists; add scanlines / NTSC composite / palette swap shaders);
  cheat system (Game Genie / Pro Action Replay per system as the
  formats are system-specific); gamepad remap UI; save-state
  browser UI
- **Dragon:** Codex's Dragon work ships when ready (parallel track,
  not gating this phase)

**Size:** Medium. ~3 months.

**Done when:** Game Boy family covered; cheats / screen filters /
gamepad remap / save-state browser work in every shipping binary.

## Phase E — Wave 2 systems (May → Aug 2027)

**Goal:** First substantive expansion beyond the anchor four.

**Work:**

- **TMS9918 + SN76489 land first** (cheap chip ports that unlock
  many subsequent systems)
- **Atari 2600** (TIA, RIOT, 6502 already exists)
- **BBC Micro** (6502, MOS Video, SAA5050, SN76489 just landed)
- **MSX** (Z80, TMS9918, AY all exist)
- **Master System** (Z80, Sega VDP, SN76489 just landed)

**Size:** Large. ~3-4 months total — donor code accelerates this
significantly per RULES.md rule 25, but TIA and Sega VDP from
scratch are real work.

**Done when:** Each Wave 2 system passes its own 10-title catalogue
to the same SOLID bar applied to Spectrum.

## Phase F — Donor-stash sweep (Aug → Dec 2027)

**Goal:** Port the remaining 10+ donor-stash systems that become
small ports once their shared chips exist.

**Work (mostly small per system once shared chips land):**

- **ColecoVision, SG-1000, Memotech MTX, Mattel Aquarius, SVI-328,
  Sord M5** — all small after Wave 2 chips
- **Oric** — small
- **ZX80, ZX81, Jupiter Ace** — very small (Z80 + simple ULAs)
- **Acorn Atom, Electron** — small (6502 + machine-specific ULA)
- **Atari 5200, 7800, 800XL** — medium each (ANTIC + GTIA + POKEY
  + MARIA-as-needed)
- **Tatung Einstein** — small

**Size:** Medium overall (each system small individually).

**Done when:** 15+ additional systems boot to a credible first-screen
waypoint. Catalogue depth per system is post-launch work; getting
each to "it boots and runs" is the Phase F bar.

## Phase G — Distribution maturity (parallel with E/F)

**Goal:** The project is installable everywhere a user might
reasonably look for it.

**Work:**

- **crates.io publishing** of ~6-8 chip + format libraries with
  curated public APIs (`mos-6502`, `zilog-z80`, `motorola-68000`,
  `motorola-6809`, `format-sinclair-zx-spectrum-{tap,tzx,sna,z80}`,
  `format-nintendo-nes-ines`)
- **Homebrew tap** for `brew install emu198x-spectrum` etc.
- **Windows MSI installer** via cargo-dist (`installers = ["msi"]`)
- **WASM build** of at least one system (Spectrum likely) for
  Code198x curriculum embeds

**Size:** Small per item, cumulative Medium.

**Done when:** Homebrew works on macOS; MSI installs on Windows;
chip crates have non-trivial downloads from crates.io; one WASM
demo embedded in a Code198x page.

## Phase H — Wave 3 systems (2028)

The remaining systems in product-roadmap.md Wave 3:

- Z80 family additions: **Amstrad CPC** (well-documented, AY/CRTC/Gate Array)
- 6502 additions: **Commodore VIC-20, PET, Apple II family**
- 68000 additions: **Atari ST, Mega Drive**
- 6809 additions: **CoCo 1/2/3, Vectrex** (Dragon line already done
  via Codex by then)

**Size:** Large.

**Done when:** Each Wave 3 system boots and runs commercial software
end-to-end.

## Phase I+ — World systems and form-factor expansion (2028+)

Open-ended. Driven by community demand, regional interest, and
preservation priority rather than a fixed plan.

Likely high-value subsequent areas (per the cultural-geographic
axis from the project scope):

- **Japanese 8-bit:** PC-88, MZ, X1 (largest body of unique software)
- **X68000** (high-prestige Japanese 16/32-bit; Tom Harte tests are
  available)
- **Soviet / post-Soviet** Spectrum clones (BK, Pentagon variants
  beyond what's already in code)
- **Brazilian** scene (TK85, CP-200) — Spectrum-clone family
- **Apple II line** (large software corpus, cultural significance)
- **Single-board systems** (KIM-1, AIM-65) as the "computing before
  home computers" anchor

**Form-factor expansion:**

- Pocket computers (Casio PB-1000, Sharp PC-1500)
- Calculator-with-computer hybrids
- Workstations (NeXT, possibly Lisa)

**Size:** Open-ended. Each system Medium-to-Large depending on
chipset novelty.

---

# Cross-cutting initiatives

These don't fit cleanly in a single phase but should be tracked.

## Accuracy ladder per system

The product roadmap says "Same accuracy bar for every system. Non-
negotiable." But in practice, "accuracy" means different things per
system:

| System | Current accuracy bar | What "production-ready" looks like |
|---|---|---|
| Spectrum | Tom Harte 100%, ZEX, Float48K, 101-entry catalogue | SOLID criteria 1-11 |
| C64 | Tom Harte 100% 6502, KERNAL boot, badline-aware | 10-title catalogue + VICE-shape cycle accuracy |
| NES | Tom Harte 100%, nestest 100%, 627/629 ROMs | blargg PPU suite green, ≥40 mappers |
| Amiga | Tom Harte 100% 68000, KS 1.3 boot, full chip stack | full-family review Seams 2-5 closed |
| Game Boy | Blargg + mooneye | Mealybug Tearoom, dmg-acid2, cgb-acid2 |
| Dragon | XRoar 11/12 match | XRoar 12/12, OS-9 boot, write paths |

**Worth codifying** as a per-system "accuracy ladder" document
listing the specific test ROMs / waypoints / catalogue entries each
system commits to. This becomes the contract per system as we add
Wave 2/3.

## Test infrastructure investment

- **Blargg PPU ROMs in CI for NES** (Seam 1 NES review)
- **Mealybug Tearoom for Game Boy** (modern PPU regression set,
  more thorough than blargg's GB suite)
- **NeoTest / 68k test corpus** beyond Tom Harte for Amiga
- **Standardised "first-screen waypoint" harness** that every
  system implements — replaces ad-hoc per-system boot tests

## Internal quality investments

- **Coverage gate up from 72% to 90%** (Spectrum SOLID criterion 11
  forces this for Spectrum-side crates; rest of the workspace
  follows organically)
- **Drop the placeholder `roms/` directory** entirely from the
  default user layout (`~/.emu198x/roms/` is the convention; the
  repo-level placeholder is vestigial)
- **Cargo.lock review** for unmaintained transitive deps
- **Dependabot config** review (`.github/dependabot.yml` exists,
  should be audited for current relevance)
- **Pre-existing 102-commit-deep author-local paths in git history**
  — `git-filter-repo` cleanup if/when there's a clean stopping point
  to do it

## Community and contribution scaffolding

The first round of governance docs landed 2026-05-22. Further
investments as the project picks up external contributors:

- **First-good-issues label** + a handful of small, well-scoped
  tasks tagged for new contributors
- **Discussions** for "how do I" questions (decide pre-Phase E
  whether to enable; volume-dependent)
- **Per-crate doc.rs polish** for the published chip libraries
- **Mentioning Emu198x** in adjacent communities (NESdev, World of
  Spectrum, Lemon Amiga forum, CPC Wiki, Atari Age) once Phase B
  and C deliverables are stable

---

# Open questions and decisions to make

These don't have answers yet; flagging them so they don't surprise
us mid-phase.

1. **Independent versioning for crates.io publish** — switching from
   lockstep `version.workspace = true` to per-crate versions is a
   113-file edit. Worth doing once before first crates.io publish?
   Or maintain lockstep and use release-plz's per-crate-tag mode?
2. **Debugger UI architecture** — native (egui? cushy? pure winit?)
   or web (LSP-shape server + web client)? The MCP query surface
   strongly suggests "MCP + frontend" — could be HTML-over-MCP,
   could be native. Worth a proper brainstorm.
3. **WASM build** — which system first? Spectrum (smallest, October
   anchor)? Game Boy (highest game library appeal)? NES (most
   recognisable)?
4. ~~**Game library / launcher**~~ — **Resolved 2026-05-23.** No
   unified launcher; per-system binaries are the product. See
   [`no-unified-launcher.md`](../../knowledge/decisions/no-unified-launcher.md).
5. **CDTV / CD32 scope** — full Akiko + CD-ROM, or stop at Akiko
   and treat CD-ROM as a separate phase?
6. **A historical-significance vs technical-novelty axis** for system
   ordering — should TI-99/4A (historically significant but TMS9900
   is a unique CPU we don't have) come before SG-1000 (less
   historically prominent but cheap because of chip reuse)? The
   current product-roadmap implicitly ranks by chip reuse; worth
   confirming.
7. **OS-9 on Dragon and 6809-family** — natural fit once Dragon work
   completes (CoCo had OS-9 too); is this in scope or out?
8. **Apollo Vampire FPGA / AC68080** — mentioned as future-Amiga in
   the full-family review. Speculative; flagged.

---

# What this plan does NOT do

To keep the scope honest:

- It does not pre-decide Wave 2 ordering inside Phase E (Atari 2600
  / BBC / MSX / SMS — product-roadmap lists them but doesn't
  sequence within the wave)
- It does not commit to a debugger UI design or technology choice
- It does not commit to which systems get WASM builds and in what
  order
- It does not address the "should we accept contributions for
  systems we haven't started?" governance question
- It does not propose a CONTRIBUTING.md update for the Wave-2 and
  later expansion (currently CONTRIBUTING points at
  `docs/adding-a-system.md` which itself is a Phase-B+ deliverable)
- It does not address commercial considerations (sponsorships,
  GitHub Sponsors button, Liberapay) — out of scope here

---

# Drift triggers

If I'm about to suggest any of these, stop and re-consult before
silently acting:

- **Sliding Wave 2 systems forward** into the engineering-bar phase
  for the anchor four. The four-anchor catalogue closure is the
  gate.
- **Treating "debugger UI" as a small task.** It's a significant
  design effort and shouldn't be tucked into a single sprint.
- **Reordering phases A and B.** Public release (A) is small;
  Spectrum SOLID (B) is the existing plan. Both proceed in parallel
  at different paces.
- **Skipping the donor-stash sweep** to chase fresh systems with no
  donor code. The donor systems are *cheap* — skipping them in
  favour of greenfield Apple II / X68000 work is throwing away
  RULES.md rule 25.
- **Bundling crates.io publish with the first stable release.**
  Publishing requires curated public APIs per crate; first stable
  release should be the binary distribution alone, and crates.io
  comes later.
- **Promising community-contributed systems any priority** without
  the contributor having proven the system passes the same SOLID
  bar as the anchor four.

---

# Status

This is a working plan. It will rot. Re-read it at the close of each
phase. If the codebase has diverged from the assumptions here,
write a new dated plan superseding the relevant parts rather than
editing this one in place.

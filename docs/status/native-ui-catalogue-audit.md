# Native UI catalogue audit

Source audit dated 2026-09-08, against main `9bc8471a` plus the CPC/Einstein
and M5/SVI-328 migrations described here. This is a source-level coverage audit,
not a claim that every preset has booted or passed hardware validation.

## Coverage

All 30 registered machine binaries have a native `UiApp` adapter and enable the
UI feature by default. Ten expose a Machine-menu selector. Twelve other
binaries have multiple runtime profiles without an in-window selector; the
remaining eight have a single runtime profile.

The shared native menu is currently attached on **macOS only**. Windows menu
attachment remains unwired (the code still references the closed #549), and
Linux uses a stub. Therefore the selector
counts below describe the macOS menu surface, not equivalent controls on every
host. See [shared menu construction and attachment](../../crates/emu198x-ui/src/menu.rs).

The inventory comes from the [system registry](systems.toml), each binary's
`Cargo.toml`, `src/main.rs`, `src/app.rs` and `src/ui.rs`, and its runtime's
`Model` catalogue. Binary links below lead to launch policy; the adjacent UI
source defines menu coverage. A dash means there is no alternative profile to
select; “None” means the catalogue has alternatives but no menu to choose them.

| Binary (`emu198x-` prefix omitted) | Runtime profiles | Launch selection | Menu presets | Meaning of differences |
|---|---:|---|---|---|
| [acorn-atom](../../crates/emu198x-acorn-atom/src/app.rs) | 2 | `--ram-kb` | None | Base/full RAM presets |
| [acorn-bbc-micro](../../crates/emu198x-acorn-bbc-micro/src/app.rs) | 1 | Single profile | — | Model B |
| [acorn-electron](../../crates/emu198x-acorn-electron/src/app.rs) | 1 | Single profile | — | Electron |
| [amiga](../../crates/emu198x-amiga/src/app.rs) | 18 | `--model` | 18/18 | Six base machines; RAM/accelerator presets × region |
| [amstrad-cpc](../../crates/emu198x-amstrad-cpc/src/app.rs) | 1 | Single profile | — | CPC 464 |
| [atari-2600](../../crates/emu198x-atari-2600/src/app.rs) | 2 | `--region` | None | Region |
| [atari-5200](../../crates/emu198x-atari-5200/src/app.rs) | 1 | NTSC only | — | One model despite a region flag |
| [atari-7800](../../crates/emu198x-atari-7800/src/app.rs) | 2 | `--region` | None | Region |
| [atari-800xl](../../crates/emu198x-atari-800xl/src/app.rs) | 2 | `--region` | None | Region |
| [c64](../../crates/emu198x-c64/src/app.rs) | 4 | `--model` | 4/4 | Breadbin/C64C × region; RAM expansions independent |
| [colecovision](../../crates/emu198x-colecovision/src/app.rs) | 2 | `--region` | None | Region |
| [commodore-pet](../../crates/emu198x-commodore-pet/src/app.rs) | 2 | `--columns` | None | 40/80-column hardware profiles |
| [commodore-vic-20](../../crates/emu198x-commodore-vic-20/src/app.rs) | 2 | `--region` | None | Region; RAM expansion flags independent |
| [dragon](../../crates/emu198x-dragon/src/app.rs) | 2 | `--model` | 2/2 | Dragon 32/64 |
| [game-boy](../../crates/emu198x-game-boy/src/app.rs) | 5 | `--model` | None | DMG0/DMG/MGB/SGB/SGB2 post-boot profiles |
| [jupiter-ace](../../crates/emu198x-jupiter-ace/src/app.rs) | 3 | `--model` / `--ram-kb` | 3/3 | Stock 3 KiB / 16 KiB expansion / 48 KiB expansion |
| [mattel-aquarius](../../crates/emu198x-mattel-aquarius/src/app.rs) | 1 | Single profile | — | RAM expansion independent |
| [memotech-mtx](../../crates/emu198x-memotech-mtx/src/app.rs) | 2 | `--model` | 2/2 | MTX500/512 marketed models |
| [msx](../../crates/emu198x-msx/src/app.rs) | 2 | `--region` | None | MSX1 region |
| [nes](../../crates/emu198x-nes/src/app.rs) | 1 | Single profile | — | NTSC |
| [oric-atmos](../../crates/emu198x-oric-atmos/src/app.rs) | 2 | `--model` | None | Oric-1/Atmos |
| [sega-game-gear](../../crates/emu198x-sega-game-gear/src/app.rs) | 1 | `--variant` (one choice) | — | Game Gear |
| [sega-master-system](../../crates/emu198x-sega-master-system/src/app.rs) | 5 | `--variant` | None | Hardware revisions/market/region |
| [sega-sg-1000](../../crates/emu198x-sega-sg-1000/src/app.rs) | 2 | `--region` | None | Region |
| [sinclair-zx80](../../crates/emu198x-sinclair-zx80/src/app.rs) | 3 | `--model`; `--ram-bytes` override | 3/3 | One ZX80 base machine; USA strap and RAM-pack presets |
| [sinclair-zx81](../../crates/emu198x-sinclair-zx81/src/app.rs) | 3 | `--model` | 3/3 | ZX81/16 KiB RAM pack/Timex TS1000 |
| [sord-m5](../../crates/emu198x-sord-m5/src/app.rs) | 2 | `--model` / `--region` | 2/2 | Region |
| [spectravideo-svi-328](../../crates/emu198x-spectravideo-svi-328/src/app.rs) | 2 | `--model` / `--region` | 2/2 | Region |
| [spectrum](../../crates/emu198x-spectrum/src/app.rs) | 13 | `--machine` | 13/13 | Machine models and regional/revision profiles |
| [tatung-einstein](../../crates/emu198x-tatung-einstein/src/app.rs) | 1 | Single profile | — | Einstein |

## Model, region and configuration

A runtime profile is a selectable hardware preset, not necessarily another
machine. The Amiga catalogue now exposes all eighteen existing regional presets
through launch, scripts, MCP and menus. These are six base machines, nine
configuration presets and two regions. A501 and expanded RAM sit under A500;
the GVP A530 configuration also changes the CPU and remains explicitly labelled
as research. The A2000 preset specifies its Agnus and RAM configuration.

The Amiga menu groups by base machine and labels each entry with its region and
configuration. Legacy PAL ids remain stable; NTSC adds `-ntsc`. Host pacing uses
the live runtime's frame budget and clock, including after a region switch.
The enum's serialization order and the emulated hardware implementation are
unchanged. See [Amiga catalogue policy](../../knowledge/decisions/amiga-machine-catalogue.md).

RAM alone is not a universal way to classify variants. A RAM pack is an add-on,
whereas MTX500/512 and Dragon 32/64 are named machine models. Likewise region,
board revision and accelerator configuration should retain their own meaning.
These labels must follow each family's hardware model, not a naming heuristic.

Game Boy's SGB entries describe post-boot machine profiles, not a complete
Super Game Boy host implementation. Spectrum and Amiga catalogues also include
limited or research profiles. Menu presence is not evidence of boot usability.

## Remaining work

1. Migrate families with alternate profiles to the existing runtime-owned
   catalogue, firmware resolver and shared switch path. Dragon already has a
   menu but retains its own switching path. ZX80 now exposes its USA profile
   and RAM-pack configuration through the shared path. Continue with the other
   Z80 families, retaining each family's existing firmware and media semantics.
2. For cartridge systems, define and verify cartridge retention/reload on a
   hardware switch before adding selectors. Rebuilding a core alone does not
   establish that the running game remains usable.
3. Use optional shared menu groups where a family needs model/configuration
   separation. Keep firmware resolution and ids in the runtime. Avoid another
   per-binary model enum or switch implementation.
4. Wire the Windows and Linux selection surfaces, then verify actual native
   windows, firmware/media loading, input, sound, switching and pacing on each
   intended host. These checks are separate from catalogue completeness.

## Verification of this slice

Automated tests cover construction and unique ids for every Amiga runtime
profile using synthetic firmware, backward-compatible PAL ids, matching RAM
across regional counterparts, menu grouping and live regional pacing. The MCP
smoke test switches from AGA to PAL OCS and then an NTSC A501 configuration,
checking the installed model and session frame budget; it requires staged ROMs.

A local macOS Amiga process successfully started with native audio outside the
sandbox. The UI automation provider could not identify the unbundled executable,
so the actual menu appearance and click-through remain unverified. This audit
does not claim a visual pass for it or for the other 29 binaries.

## ZX80 follow-on and backlog alignment

The ZX80 migration advances [#1475](https://github.com/emu198x/emu198x/issues/1475)
and the UI/script/MCP parity goal in
[#456](https://github.com/emu198x/emu198x/issues/456). Its three existing profile
ids now select the same presets through launch, scripts, MCP and the native
menu. `--rom-dir`, `EMU198X_ZX80_ROM_DIR` and `--rom ID=PATH` use the shared
resolver. The existing `--rom PATH`, `EMU198X_ZX80_ROM` and conventional file
location continue to work; removing file environment variables is unnecessary
because the shared catalogue already supports them. Firmware pins win over the
file environment variable, which wins over directory lookup.

Switching constructs the target preset with conventional firmware and default
RAM, ejecting the tape and dropping launch overrides. Reports read live RAM and
tape state. Missing conventional firmware still allows blank MCP startup;
invalid images and explicitly requested missing paths produce errors.

In line with [#720](https://github.com/emu198x/emu198x/issues/720), verification
checks actual installed model, RAM, television strap, display height and frame
budget, rather than only the presence of a capability declaration. The audio
vocabulary/conformance problem in
[#1369](https://github.com/emu198x/emu198x/issues/1369) remains separate: this
silent machine gains no audio capability declaration. Hash-based firmware
lookup stays deferred under
[#1476](https://github.com/emu198x/emu198x/issues/1476).

Tests exercise the actual CLI, scripts and MCP with synthetic firmware,
including tape ejection and errors. Local staged-ROM launches ran all three
presets for 150 requested frames; inspected captures show the startup cursor
at 384×288 for the two 50 Hz presets and 384×240 for USA. This verifies the
headless boot/capture path, not native menu interaction or audio-device policy.

Two related issues constrain later UI work:
[#830](https://github.com/emu198x/emu198x/issues/830) requires truthful Amiga
support evidence across hardware dimensions; configuration labels and selectors
do not satisfy that gate.
[#1042](https://github.com/emu198x/emu198x/issues/1042) proposes shared keyboard
layout/legend metadata before a clickable renderer. That remains a separate
step toward a usable native interface, using the same machine input events
rather than adding a per-system keyboard UI.

## Ace/MTX rollout slice

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

Jupiter Ace and Memotech MTX now use runtime-owned ids, firmware sources and
frame budgets through launch, script, MCP and native menu selection. Ace's
three choices distinguish stock onboard RAM from fitted expansion RAM. MTX500
and MTX512 retain their marketed model names and existing command-line ids;
profile ids are also accepted as aliases.

Both families retain their file environment variables and conventional ROM
locations, and gain the shared directory and named-pin options. MTX's combined
OS/paged-ROM image is still any whole number of 8 KiB pages totalling at least
16 KiB. Its RS128 and additional firmware catalogue work remain tracked by
[#269](https://github.com/emu198x/emu198x/issues/269).

Ace's `--ram-kb` retains the existing thresholds: below 16 selects stock,
16–47 selects the 16 KiB expansion, and 48 or more selects the 48 KiB expansion.
It and `--model` select the same preset; the last flag wins. The report now
reads the live preset's `ram_kb`, including after a switch. The existing
`--ace` startup snapshot path remains in place. A hardware switch installs a
fresh preset using conventional firmware, replacing the running machine and
its snapshot state.

MCP now loads available conventional firmware for both families, including
Ace, which previously always started blank. Missing conventional firmware
still permits blank startup; invalid images and explicitly requested missing
paths fail. A before/after MCP tool-list comparison adds only `set_machine`
to each binary's existing 36 tools and removes none.

Tests exercise CLI/script/MCP selection, firmware options and failures, real
memory-map differences after a swap, failed-switch preservation and frame
budgets. Local staged-ROM launches ran all five presets for 150 frames;
inspected captures show the Ace startup cursor and MTX's Ready prompt.
Native menu click-through remains unverified.

## CPC/Einstein rollout slice

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

CPC464 and Tatung Einstein now have single-entry runtime firmware catalogues.
Both gain shared `--rom-dir` and `--rom ID=PATH` handling, retaining their
existing file environment variables and paths. Einstein's `--mos PATH` remains
an alias for pinning its MOS image. The new directory variables are
`EMU198X_CPC_ROM_DIR` and `EMU198X_EINSTEIN_ROM_DIR`.

Catalogue adoption now covers nine of 30 binaries, with 21 remaining. Selector
coverage stays at eight: these two machines each have one current profile,
so neither gains a selector or a `set_machine` tool. Before/after MCP tool
lists are identical (36 CPC tools and 39 Einstein tools).

The opt-in `build_variant_or_blank` shell helper centralises the existing
missing-conventional-firmware policy for ZX80, Ace, MTX, CPC and Einstein.
Explicit missing paths and invalid images still fail. CPC's `--tape` now
loads through the same code in normal and MCP startup, including retaining
the cassette across reset.

Verification covers the two binary/runtime suites, firmware options and
errors, the shared helper and the existing ZX80/Ace/MTX subprocess contracts.
Local staged-ROM captures after 150 frames show both machines at their Ready
prompts. These are headless boot captures, not native-window visual checks.

## Sord M5 and SVI-328 follow-on

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

Both families expose their two existing PAL/NTSC profiles through runtime-owned
ids, firmware sources and construction, shared script/MCP switching, and native
selectors. `--region` remains compatible; `--model` accepts the existing profile
ids. Shared `--rom-dir` and `--rom PATH|ID=PATH` options preserve the existing
file conventions, with `EMU198X_SORD_M5_ROM_DIR` and `EMU198X_SVI_328_ROM_DIR`
for family directory overrides. SVI-328 retains `--bios PATH`.

Switching boots a fresh target with conventional firmware, ejecting cartridges
and the SVI cassette and dropping launch overrides. Reset retains media; a failed
switch preserves the installed machine. Reports read live cartridge state.
Host frame budgets retain the existing values, pacing reads the live region,
and the SVI window uses the selected VDP's dimensions instead of a fixed NTSC
size. Sord's existing permissive ROM-size policy is unchanged; SVI still requires
32 KiB system firmware and limits cartridges to 16 KiB.

The shared window launcher now loads parsed startup media. An MCP startup-media
hook lets firmware-based apps use the same parsed media without reinterpreting
`--rom` as a cartridge. M5, SVI-328, ZX80, ZX81, Ace, MTX, CPC and Einstein opt in;
other launchers retain legacy media-flag discovery. This fixes valid MCP firmware
pins being rejected or loaded as cartridge data, and makes Ace/ZX80 snapshot
startup hooks available to MCP as well as scripts and the default window path.

Catalogue adoption covers eleven of 30 binaries, with 19 remaining. Ten expose
native selectors. Automated tests cover all four regional presets, actual
cartridge mapping, cassette presence, reset and failed-switch retention, live
pacing and framebuffer dimensions, CLI/MCP firmware precedence and errors, and
parsed cartridge loading into the window runtime. Native menu click-through
remains a separate verification step.

Local staged-ROM captures after 300 requested frames show the Dig Dug title
screen on both M5 regions and the BASIC `Ok` prompt on both SVI regions. Captures
are 280×240 NTSC and 278×288 PAL. MCP tool inventories add only `set_machine`
(36→37 M5, 39→40 SVI); existing definitions are unchanged. A synthetic NES
cartridge still loads through legacy MCP `--rom` discovery. These checks do not
validate native menu interaction or audio output.

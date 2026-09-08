# Native UI catalogue audit

Source audit dated 2026-09-08, covering the runtime catalogue migrations
described here. This is a source-level coverage audit, not a claim that every
preset has booted or passed hardware validation.

## Coverage

All 30 registered machine binaries have a native `UiApp` adapter and enable the
UI feature by default. Twenty-one expose a Machine-menu selector. One other
binary has multiple runtime profiles without an in-window selector; the
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
| [acorn-atom](../../crates/emu198x-acorn-atom/src/app.rs) | 2 | `--model` / `--ram-kb` | 2/2 | Base 2.5 KiB / expanded 32 KiB RAM presets |
| [acorn-bbc-micro](../../crates/emu198x-acorn-bbc-micro/src/app.rs) | 1 | Single profile | — | Model B |
| [acorn-electron](../../crates/emu198x-acorn-electron/src/app.rs) | 1 | Single profile | — | Electron |
| [amiga](../../crates/emu198x-amiga/src/app.rs) | 18 | `--model` | 18/18 | Six base machines; RAM/accelerator presets × region |
| [amstrad-cpc](../../crates/emu198x-amstrad-cpc/src/app.rs) | 1 | Single profile | — | CPC 464 |
| [atari-2600](../../crates/emu198x-atari-2600/src/app.rs) | 2 | `--model` / `--region` | 2/2 | Region |
| [atari-5200](../../crates/emu198x-atari-5200/src/app.rs) | 1 | NTSC only | — | One model despite a region flag |
| [atari-7800](../../crates/emu198x-atari-7800/src/app.rs) | 2 | `--model` / `--region` | 2/2 | Region |
| [atari-800xl](../../crates/emu198x-atari-800xl/src/app.rs) | 2 | `--model` / `--region` | 2/2 | Region; BASIC boot policy independent |
| [c64](../../crates/emu198x-c64/src/app.rs) | 4 | `--model` | 4/4 | Breadbin/C64C × region; RAM expansions independent |
| [colecovision](../../crates/emu198x-colecovision/src/app.rs) | 2 | `--model` / `--region` | 2/2 | Region |
| [commodore-pet](../../crates/emu198x-commodore-pet/src/app.rs) | 2 | `--model` / `--columns` | 2/2 | 40/80-column hardware profiles |
| [commodore-vic-20](../../crates/emu198x-commodore-vic-20/src/app.rs) | 2 | `--model` / `--region` | 2/2 | Region; RAM expansion flags independent |
| [dragon](../../crates/emu198x-dragon/src/app.rs) | 2 | `--model` | 2/2 | Dragon 32/64 |
| [game-boy](../../crates/emu198x-game-boy/src/app.rs) | 5 | `--model` | None | DMG0/DMG/MGB/SGB/SGB2 post-boot profiles |
| [jupiter-ace](../../crates/emu198x-jupiter-ace/src/app.rs) | 3 | `--model` / `--ram-kb` | 3/3 | Stock 3 KiB / 16 KiB expansion / 48 KiB expansion |
| [mattel-aquarius](../../crates/emu198x-mattel-aquarius/src/app.rs) | 1 | Single profile | — | RAM expansion independent |
| [memotech-mtx](../../crates/emu198x-memotech-mtx/src/app.rs) | 2 | `--model` | 2/2 | MTX500/512 marketed models |
| [msx](../../crates/emu198x-msx/src/app.rs) | 2 | `--model` / `--region` | 2/2 | MSX1 region |
| [nes](../../crates/emu198x-nes/src/app.rs) | 1 | Single profile | — | NTSC |
| [oric-atmos](../../crates/emu198x-oric-atmos/src/app.rs) | 2 | `--model` | 2/2 | Oric-1/Atmos |
| [sega-game-gear](../../crates/emu198x-sega-game-gear/src/app.rs) | 1 | `--model` / `--variant` (one choice) | — | Game Gear |
| [sega-master-system](../../crates/emu198x-sega-master-system/src/app.rs) | 5 | `--model` / `--variant` | 5/5 | Hardware revisions/market/region |
| [sega-sg-1000](../../crates/emu198x-sega-sg-1000/src/app.rs) | 2 | `--model` / `--region` | 2/2 | Region |
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

## Atom and Electron follow-on

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

Atom's two existing RAM presets now use a runtime-owned catalogue and shared
script/MCP switching, with a native selector labelled by installed RAM. Legacy
`--ram-kb` keeps its threshold: values below 12 select the base 2.5 KiB machine;
12 or more select the full 32 KiB expansion. `--model` accepts the existing
profile ids, and the last selector wins. The headless `ram_kb` report now describes
the installed preset (2.5 or 32), rather than echoing the launch argument;
`ram_bytes` gives its exact size. Cassette and printer capture flags remain intact.

A switch boots a fresh Atom with conventional firmware and ejects the cassette
and utility ROM. Reset retains media, and a failed switch leaves the current
machine intact. Frame budgets are unchanged. Electron remains a single-profile
family with no redundant selector or switch capability.

Both launchers use shared `--rom-dir` and firmware pins while retaining Atom's
`--rom PATH` and Electron's `--os` / `--basic` aliases. Directory variables are
`EMU198X_ACORN_ATOM_ROM_DIR` and `EMU198X_ELECTRON_ROM_DIR`; legacy per-file
variables still work. Electron requires named `--rom ID=PATH` pins because it
has two required images. MCP loads available firmware and can start blank when
conventional firmware is absent. An explicitly named partial firmware set is
an error, including when a per-file environment variable names only one image.

Catalogue adoption covers thirteen of 30 binaries, with 17 remaining. Eleven
expose native selectors. Tests exercise actual RAM mapping, cassette and utility
ROM lifecycle, legacy flags, firmware precedence and failure handling, and real
CLI/script/MCP entry points. Menu click-through remains unverified.

Local staged-ROM launches ran each of the three presets for exactly 300 requested
frames. Inspected captures show both Atom prompts at 372×288 and Electron's BASIC
prompt at 640×256. Atom's MCP inventory adds only `set_machine` (36→37 tools);
Electron's 36 tool definitions are unchanged. These are headless boot/capture
checks, not native menu or audio-device verification.

## Oric and Aquarius follow-on

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

Oric's two existing machine profiles now use runtime-owned firmware construction
and shared script/MCP switching, with a native selector. Canonical launch ids
remain `oric-1` and `atmos`; `oric1` and the `oric-atmos` profile id are aliases.
Within the conventional directory, `oric-1.rom` and `atmos.rom` take precedence
for their respective models, with `oric.rom` retained as a legacy fallback.
Explicit pins and `EMU198X_ORIC_ROM` still take precedence over directory lookup.
This is a filename convention, not ROM-version identification; the legacy
shared file may contain either firmware version. Hash-based identification is
still deferred under [#1476](https://github.com/emu198x/emu198x/issues/1476).

An Oric switch boots fresh with the target's conventional firmware, ejects tape
and drops launch overrides. Reset retains tape; failed switches retain the
current machine. Reports read the installed model rather than the launch choice.

Aquarius keeps its existing NTSC profile without a redundant selector. Its
runtime now owns the BIOS/character-ROM conventions and the unchanged host frame
budget. `--bios`, `--char` and their per-file environment variables remain
compatible with shared directory options and named `--rom ID=PATH` pins.
MCP now loads both physical ROMs and applies the requested RAM expansion;
parsed cartridge media follows the same path as scripts and the default window.
Reports describe live cartridge state and the runtime's capped expansion size.
The existing 16 KiB expansion cap and cartridge handling are unchanged.

Family directory variables are `EMU198X_ORIC_ROM_DIR` and
`EMU198X_AQUARIUS_ROM_DIR`. Both use the shared blank-start policy: absent
conventional firmware may start MCP blank, while explicit missing or malformed
firmware is an error. Aquarius requires both named firmware images.

Catalogue adoption covers fifteen of 30 binaries, with 15 remaining. Twelve
expose native selectors. Automated tests cover actual model installation, tape
lifecycle, firmware-name precedence, both-ROM loading and errors, cartridge
mapping, expansion RAM and reset, and character-ROM rendering in the Aquarius
window constructor. Native menu click-through remains unverified.

Local staged-ROM captures show both Oric profiles at BASIC `Ready` after 300
requested frames (240×224), using the same legacy `oric.rom`. This verifies the
construction and capture paths, not independent firmware-version correctness.
Aquarius reaches its `Press RETURN` screen at 600 frames; a scripted Return
followed by 300 more frames reaches BASIC `Ok` (352×232). Its earlier blank
captures were during the startup display sequence. MCP inventories add only
Oric's `set_machine` (39→40 tools); Aquarius's 39 definitions are unchanged.

## SG-1000 and ColecoVision follow-on

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

Both consoles now expose their existing PAL/NTSC profiles through the runtime
catalogue, `--model`, shared script/MCP switching and native selectors. `--region`
and both `--cart PATH` and positional cartridge paths remain available. The last
model or region selector wins. Parsed cartridge startup now also works in MCP;
reports read the runtime's installed cartridge state.

A region switch cold-boots with the cartridge already held in memory. It resets
CPU, RAM, video, audio and session state without rereading the cartridge file.
`FamilyRuntime::replacement` owns this policy; `build_replacement` and session
switching both use it. Other families retain their default fresh-machine policy.
A rejected switch leaves the running machine intact, including rejection while
video recording is active, which is now checked before installing a replacement.

SG-1000 needs no BIOS and resolves an empty firmware catalogue without HOME or a
ROM directory. Normal launch still requires a cartridge; MCP may start empty.
ColecoVision uses the shared BIOS resolver, retaining `--bios` and
`EMU198X_COLECO_BIOS`, and gains `--rom PATH|ID=PATH`, `--rom-dir` and
`EMU198X_COLECO_ROM_DIR`. Its firmware id is `colecovision-bios`. Switches resolve
the BIOS conventionally, dropping launch-time firmware pins. Missing conventional
firmware allows blank MCP startup; explicitly missing or invalid BIOS images fail.
The existing 8 KiB BIOS validation and cartridge handling are unchanged.

Catalogue adoption covers seventeen of 30 binaries, with 13 remaining. Fourteen
expose native selectors. Tests cover all four presets, live regional pacing and
framebuffer dimensions, cartridge mapping after switches and resets, fresh RAM,
failed-switch preservation, parsed window startup, and CLI/script/MCP errors.
An interactive MCP test removes the cartridge file after loading it and confirms
that switching still retains its bytes. Both MCP inventories add only
`set_machine` (33→34 tools); existing definitions are unchanged.

Local captures after 150 requested frames show SG-1000's committed synthetic
cartridge producing its white backdrop and the staged ColecoVision BIOS displaying
its no-cartridge screen. Both regions capture at their existing dimensions:
280×240 NTSC and 278×288 PAL. This is headless boot/capture evidence; commercial
cartridge gameplay, native menu click-through and audio-device output remain
unverified by this slice.

## Atari 5200 optional-firmware follow-on

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

Atari 5200 now uses a single-entry runtime catalogue for its existing NTSC model,
optional BIOS and unchanged native frame budget. Its legacy `--region ntsc` is
accepted and PAL remains rejected; it gains no redundant menu or switch tool.
Catalogue adoption covers eighteen of 30 binaries, with 12 remaining. Fourteen
expose native selectors.

`--bios` and `EMU198X_A5200_BIOS` remain available alongside shared
`--rom PATH|ID=PATH`, `--rom-dir` and `EMU198X_A5200_ROM_DIR`. The firmware id is
`atari-5200-bios`; directory lookup tries `bios.rom` before `5200.rom`. An explicit
missing file is an error in every mode. Absent conventional BIOS remains optional,
including with no HOME or ROM directory, and the existing permissive BIOS-size
handling is unchanged. Reports expose live `bios_loaded` state.

MCP now loads available BIOS firmware, which it previously ignored, and accepts
both flagged and positional startup cartridges. The window and scripts use the
same parsed cartridge path. Normal launch still requires a cartridge; MCP can
wait for one. Tests verify actual BIOS and cartridge mapping, retention on reset,
firmware precedence and errors, optional-firmware startup and the native window
constructor. The existing synthetic handover-BIOS boot test now constructs its
runtime through the catalogue before executing the cartridge.

The 33 existing MCP tool definitions are unchanged. A local 150-frame capture
using the committed synthetic handover BIOS and cartridge displays the Emu198x
plate at 374×240. This verifies the catalogue-to-boot path without private ROMs;
native window interaction and audio output remain unverified by this slice.

## Master System and Game Gear follow-on

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

Both systems retain distinct catalogues while sharing `SmsRuntime<M>` in the
Master System class crate. Each runtime crate supplies its `SmsModel` metadata
and a concrete alias. Core execution, debug tools, input, queries, snapshots and
replacement policy remain shared. Snapshot version 7 and its stored profile ids
are unchanged; the generic model parameter is not part of the serialized envelope.

Master System exposes all five existing profiles through launch, script, MCP and
native selection, including the Japanese model previously missing from the CLI.
Legacy `sms`, `sms1` and regional ids remain valid; full profile ids are aliases.
Game Gear accepts `game-gear`, `gg` and its full profile id, with no redundant
selector or switch tool. Both gain `--model` as an alias for `--variant` and use
empty firmware catalogues without requiring HOME or a ROM directory.

A Master System switch cold-boots while retaining the cartridge, all 32 KiB of
SRAM and its dirty flag. Clean loaded saves remain clean; unsaved changes remain
eligible for the existing sidecar writeback. The window's save path stays tied
to the launch cartridge. Normal launch and MCP now share parsed cartridge
construction; Master System MCP also restores the startup sidecar, and both
MCP launchers accept positional cartridge paths. Source cartridges are not written.

Catalogue adoption covers twenty of 30 binaries, with 10 remaining. Fifteen
expose native selectors. Tests verify all hardware selections, catalogue isolation,
SRAM lifecycle, snapshot round trips, failed-switch preservation, source-file
removal, real CLI/script/MCP calls and native adapter pacing and dimensions.
The Master System MCP inventory adds only `set_machine` (33→34 tools); Game
Gear's 33 definitions are unchanged. Native menu click-through and audio output
remain unverified by this slice.

Local 150-frame captures with the committed synthetic cartridges show the
expected green backdrop on all six profiles: 280×240 on NTSC Master Systems,
278×288 on PAL models and 160×144 on Game Gear. These are boot/capture checks,
not commercial-game compatibility evidence.

## BBC Micro firmware and language policy

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

BBC Micro's single Model B catalogue now owns the MOS, SAA5050 font and BASIC
file conventions and the unchanged native frame budget. MOS remains required;
the font and default language are optional. `--mos PATH` remains compatible with
`--rom ID=PATH`, `--rom-dir` and `EMU198X_BBC_ROM_DIR`. Existing per-file variables
remain supported. Firmware ids are `acorn-bbc-mos`, `acorn-bbc-saa5050` and
`acorn-bbc-basic`; directory filenames remain `os.rom`, `saa5050.rom` and `basic.rom`.
Named pins are required for `--rom` because the catalogue has three images.

The window installs staged BASIC into bank 15 by default; headless modes keep
their bare-MOS default. An explicit BASIC firmware pin selects the language in
all modes. Explicit `--sideways` banks are applied afterwards and win in the
window, scripts and MCP. MCP now honours these banks and the teletext font,
which its old startup ignored. Reports count occupied sideways banks in the live
machine, including BASIC and snapshot-restored ROMs, rather than launch flags.

The shell's existing firmware-construction helper is exposed as
`build_variant_with` so the BBC window can select its runtime language constructor
without duplicating the resolver. Missing conventional MOS still permits blank
MCP startup; explicit missing files and invalid MOS images fail. Existing font,
BASIC and sideways-ROM size handling is unchanged. No new hardware variant or
selector is introduced.

Catalogue adoption covers twenty-one of 30 binaries, with nine remaining.
Fifteen expose native selectors. Tests cover firmware precedence and errors,
window/headless language policy, explicit sideways overrides, reset retention,
font installation and actual ROM mapping through script and MCP. All 36 existing
MCP tool definitions are unchanged. A 150-frame staged-ROM capture reaches the
BBC BASIC prompt at 640×256; native menu interaction and audio output remain
unverified by this slice.

## MSX regional catalogue and cartridge lifecycle

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

MSX exposes its two existing regional profiles through the runtime catalogue,
`--model`, script/MCP switching and a native selector. `--region` remains an
alias; the last selector wins. The runtime owns the unchanged frame budgets,
and native pacing follows the live runtime after switching.

The shared BIOS resolver retains `--bios` and `EMU198X_MSX_BIOS`, and adds
`--rom PATH|ID=PATH`, `--rom-dir` and `EMU198X_MSX_ROM_DIR`. The firmware id
is `msx1-bios`, and the existing directory and filename are `microsoft-msx/msx.rom`.
The 32 KiB BIOS size requirement is unchanged. Missing conventional firmware
permits blank MCP startup; explicit missing or malformed firmware fails.

Both cartridge slots and explicit mapper choices now load through parsed startup
options in every mode. A region switch resolves conventional BIOS and cold-boots
while retaining both in-memory cartridges and their mapper choices, without
rereading source files. CPU, RAM, bank selections and peripheral state reset.
Failed switches preserve the running machine. Snapshot restore refreshes the
runtime's cached BIOS, cartridges and mappers from the installed machine, so
subsequent resets and switches retain restored media rather than stale launch
inputs. The version-5 snapshot envelope is unchanged. Cartridge reports describe
the runtime's installed media.

Catalogue adoption covers twenty-two of 30 binaries, with eight remaining.
Sixteen expose native selectors on macOS. Tests cover BIOS precedence and errors,
both mapped cartridges, snapshot/reset/replacement lifecycle, live regional
pacing and dimensions, and CLI/script/MCP paths. An interactive MCP test removes
both cartridge sources before switching, then removes the BIOS and verifies that
the rejected next switch preserves live memory. The MCP inventory adds only
`set_machine` (40→41 tools); all existing definitions are unchanged.

Local 300-frame captures with staged firmware reach MSX BASIC `Ok` in both
regions: 280×240 NTSC and 278×288 PAL. These are boot/capture checks; native
menu click-through, commercial-cartridge gameplay and audio-device output remain
unverified by this slice.

## Atari 2600 and 7800 regional catalogues

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

Both consoles now expose their existing NTSC/PAL profiles through the runtime
catalogue, `--model`, script/MCP switching and native selectors. `--region`
remains accepted; the last selector wins. Both runtimes retain their BIOS-less
boot policy and resolve empty firmware catalogues without HOME or ROM paths.
Normal launch requires a cartridge; MCP can start empty. Parsed flagged and
positional startup cartridges now load consistently in every mode, retaining
the existing readers, including the 2600's archive-member selection.

A region switch cold-boots the cartridge installed in the live machine without
rereading its source. Machine constructors share their existing power-on setup
with `cold_boot`; cartridge bank/RAM state resets while ROM and hardware
configuration survive. The 7800 preserves parsed A78 mapper, RAM and POKEY
configuration. The 2600 preserves its banking scheme and Supercharger image.
Runtime ROM caches are removed, so reset and switching also use snapshot-restored
cartridges. Snapshot envelopes remain unchanged (2600 version 2, 7800 version 4).
Rejected cartridge insertion leaves the previous machine intact and cannot
poison a subsequent reset or switch.

Catalogue adoption covers twenty-four of 30 binaries, with six remaining.
Eighteen expose native selectors on macOS. Tests cover actual ROM mapping through
CLI/script/MCP, failed-switch preservation, removal of source files before
switching, restored cartridge reset/replacement, A78 configuration, banked and
Supercharger images, and native selection, pacing and dimensions. Both MCP
inventories add only `set_machine` (33→34); existing definitions are unchanged.

Local 150-frame captures with the committed synthetic cartridges produce their
uniform backgrounds in all four profiles: yellow on NTSC and grey on PAL.
The 2600 captures are 160×240 and 160×288; the 7800 captures are 374×240 and
368×288. Existing frame budgets and pacing are unchanged. These are synthetic
boot/capture checks; commercial-game compatibility, native menu click-through
and audio-device output remain unverified by this slice.

## PET firmware catalogue and display profiles

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

PET's existing 40- and 80-column profiles now use shared catalogue construction,
script/MCP switching and a native selector. `--columns` remains compatible with
`--model`; the last selection wins, and unsupported numeric column counts now
fail instead of silently choosing 40 columns. Reports read the live model.

The runtime declares all four required ROMs and their existing size constraints.
Legacy flags and per-file environment variables remain supported alongside
`--rom ID=PATH`, `--rom-dir` and `EMU198X_PET_ROM_DIR`. Firmware ids are
`commodore-pet-kernal`, `commodore-pet-basic`, `commodore-pet-editor` and
`commodore-pet-char`. The directory remains `commodore-pet` and filenames remain
`kernal.rom`, `basic.rom`, `editor.rom` and `chargen.rom`. Both profiles retain
the existing filename convention; selecting a profile does not supply a matching
editor ROM automatically.

MCP now uses strict shared resolution for explicit missing or malformed firmware,
and can still start blank when conventional firmware is absent. Parsed PRG
startup uses the same autoload path in native, script and MCP modes. A profile
switch cold-boots conventional firmware and clears RAM and queued programs;
a failed switch preserves the machine and queued PRG. Reset uses the ROMs
installed in the live machine, including after snapshot restore, instead of
redundant launch-ROM caches. The snapshot version-2 envelope, PRG autoload
budget and native frame budget are unchanged.

Catalogue adoption covers twenty-five of 30 binaries, with five remaining.
Nineteen expose native selectors on macOS. Tests cover four-ROM precedence and
errors, live profile reports, PRG injection and switch lifecycle, failed firmware
switch preservation, snapshot/reset firmware retention and native construction
at both display sizes. The MCP inventory adds only `set_machine` (36→37);
all existing tool definitions are unchanged.

With the local staged ROM set, a 300-frame 40-column capture reaches a clean
BASIC `READY` prompt at 384×248. The 80-column capture reaches `READY` at 704×248
but has display corruption; its PNG is byte-for-byte identical to a capture from
the pre-migration build with the same ROM set. This confirms no rendering
regression from the migration, not successful 80-column firmware validation.
Native menu click-through remains unverified.

## VIC-20 regional catalogue and expansion policy

Parent issue: [#1475](https://github.com/emu198x/emu198x/issues/1475).
Broader epic: [#456](https://github.com/emu198x/emu198x/issues/456).

VIC-20's existing PAL/NTSC profiles now use shared firmware construction,
script/MCP switching and native selection. `--region` remains compatible with
`--model`; PAL stays the launch default. Fitted RAM blocks remain independent
configuration, including blocks added by the existing BASIC PRG load-address
policy. A regional replacement retains fitted RAM and the installed cartridge,
while cold-booting conventional firmware and clearing RAM contents and queued
programs. Serial/modem attachments end with the old runtime. A failed switch
preserves the running machine and queued PRG.

The runtime owns the existing KERNAL, BASIC and character-ROM conventions and
frame budgets. Legacy flags and per-file variables remain supported alongside
`--rom ID=PATH`, `--rom-dir` and `EMU198X_VIC20_ROM_DIR`. Firmware ids are
`commodore-vic-20-kernal`, `commodore-vic-20-basic` and `commodore-vic-20-char`;
filenames remain `kernal.rom`, `basic.rom` and `chargen.rom` under
`commodore-vic-20`. Both profiles retain the same filename convention; selecting
a region does not itself supply a region-matched KERNAL.

MCP now honours explicit RAM expansion, modem attachment and both PRG launch
modes. `--prg-sys` startup also works in the window, using its existing
150-frame boot/inject/SYS sequence. Ordinary BASIC PRGs retain their delayed
shared media path. Explicit missing or malformed firmware fails in every mode;
absent conventional firmware still permits blank MCP startup.

Reset and RAM reconfiguration now cold-boot the live machine's installed ROMs
and cartridge mappings, removing redundant firmware caches. Snapshot restore
refreshes the runtime's expansion selection from the restored machine, so reset
and replacement retain restored configuration. The version-5 snapshot envelope,
cartridge container retention and hardware execution are unchanged.

Catalogue adoption covers twenty-six of 30 binaries, with four remaining.
Twenty expose native selectors on macOS. Tests cover firmware precedence and
errors, PRG injection and launch commands, explicit and PRG-driven expansion,
failed-switch preservation, restored firmware/RAM/cartridge retention, modem
lifecycle and native startup. The MCP inventory adds only `set_machine` (36→37);
all existing definitions are unchanged.

Local 300-frame captures reach BASIC `READY` in both regions at 214×240 NTSC
and 230×288 PAL. The NTSC display clips its right edge with the staged KERNAL; its PNG is
byte-for-byte identical to the pre-migration build with the same ROMs. This
confirms no migration regression, while region-matched firmware validation and native menu click-through remain
outstanding. Audio-device output and live TCP connectivity were not exercised.


## Atari 800XL regional catalogue

The [800XL runtime](../../crates/runtime-atari-800xl/src/runtime.rs) owns the
NTSC/PAL catalogue, optional OS/BASIC firmware sources and existing native
frame budgets. Launch, scripts, MCP and the macOS Machine menu share this
selection path. Legacy `--region`, `--os`, `--basic`, `--cart`, `--disk` and
`--no-basic` flags remain; `--model`, `--rom ID=PATH` and `--rom-dir` use the
catalogue. The directory variable is `EMU198X_A800XL_ROM_DIR`; per-image
`EMU198X_A800XL_OS` and `EMU198X_A800XL_BASIC` still take precedence over it.
Files remain `atari-800xl/atarixl.rom` and `atari-800xl/ataribas.rom` under the
conventional ROM root. Both regional profiles use the same filenames.

MCP honours firmware, cartridge, disk and BASIC startup configuration. Absent
optional firmware permits cartridge-only boot or a blank MCP session; an
explicit missing image is an error. Interactive and script startup still
require an OS or cartridge. Existing permissive ROM-size handling is unchanged.

A region change cold-boots with freshly resolved firmware while retaining the
parsed cartridge type, BASIC boot policy and live D1: image, including in-memory
writes. It clears RAM, input state and mounted XEX autoload state. Failed
resolution leaves the live machine intact. Cartridge validation precedes
mutation, so an invalid insertion no longer poisons reset or ejects D1:.
Reset and replacement retain snapshot-restored firmware and cartridge mapper
metadata. Version-5 snapshots remain readable; they carry the live BASIC
mapping rather than the original launch flag, so that mapping supplies the
boot policy after restoration. Timing and chip execution are unchanged.

Catalogue adoption covers twenty-seven of 30 binaries, with Dragon, Game Boy
and NES remaining. Twenty-one expose native selectors on macOS. Automated
checks cover firmware precedence, cartridge-only and blank startup, MCP disk
configuration, failed-switch preservation, modified-disk retention, restored
mapper/firmware reset and native startup/pacing.

The local targeted suites pass (345 tests; 14 fixture-dependent tests ignored),
as do the headless-only build/tests, workspace Clippy, formatting, doc links and
registry checks. MCP adds only `set_machine` (36→37 tools), preserving every
existing tool definition. Local 300-frame captures are byte-for-byte identical
to the pre-migration build: PAL reaches `READY` at 368×288; NTSC shows a blue
screen and cursor at 374×240 with the installed ROMs. This establishes capture
parity, not NTSC boot validation. Native menu click-through and audio-device
output remain unverified.

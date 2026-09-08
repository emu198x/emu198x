# Native UI catalogue audit

Source audit dated 2026-09-08, against main `330c9269` plus the Amiga catalogue
and grouped-menu changes described here. This is a source-level coverage audit,
not a claim that every preset has booted or passed hardware validation.

## Coverage

All 30 registered machine binaries have a native `UiApp` adapter and enable the
UI feature by default. Five expose a Machine-menu selector. Seventeen other
binaries have multiple runtime profiles without an in-window selector; the
remaining eight have a single runtime profile.

The shared native menu is currently attached on **macOS only**. Windows menu
attachment remains TODO #549, and Linux uses a stub. Therefore the selector
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
| [jupiter-ace](../../crates/emu198x-jupiter-ace/src/app.rs) | 3 | `--ram-kb` | None | 3/16/48 KiB RAM presets |
| [mattel-aquarius](../../crates/emu198x-mattel-aquarius/src/app.rs) | 1 | Single profile | — | RAM expansion independent |
| [memotech-mtx](../../crates/emu198x-memotech-mtx/src/app.rs) | 2 | `--model` | None | MTX500/512 marketed models |
| [msx](../../crates/emu198x-msx/src/app.rs) | 2 | `--region` | None | MSX1 region |
| [nes](../../crates/emu198x-nes/src/app.rs) | 1 | Single profile | — | NTSC |
| [oric-atmos](../../crates/emu198x-oric-atmos/src/app.rs) | 2 | `--model` | None | Oric-1/Atmos |
| [sega-game-gear](../../crates/emu198x-sega-game-gear/src/app.rs) | 1 | `--variant` (one choice) | — | Game Gear |
| [sega-master-system](../../crates/emu198x-sega-master-system/src/app.rs) | 5 | `--variant` | None | Hardware revisions/market/region |
| [sega-sg-1000](../../crates/emu198x-sega-sg-1000/src/app.rs) | 2 | `--region` | None | Region |
| [sinclair-zx80](../../crates/emu198x-sinclair-zx80/src/app.rs) | 3 | ZX80 fixed; `--ram-bytes` override | None | ZX80/USA/RAM pack; USA profile lacks a launch selector |
| [sinclair-zx81](../../crates/emu198x-sinclair-zx81/src/app.rs) | 3 | `--model` | 3/3 | ZX81/16 KiB RAM pack/Timex TS1000 |
| [sord-m5](../../crates/emu198x-sord-m5/src/app.rs) | 2 | `--region` | None | Region |
| [spectravideo-svi-328](../../crates/emu198x-spectravideo-svi-328/src/app.rs) | 2 | `--region` | None | Region |
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
   menu but retains its own switching path. Start with ROM-based families such
   as ZX80; expose its USA profile and distinguish RAM-pack configuration.
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

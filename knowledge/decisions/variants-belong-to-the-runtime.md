# Variants belong to the runtime

**Status:** Adopted 2026-09-08 for the Spectrum, Amiga, C64, ZX81, ZX80, Jupiter Ace, MTX, CPC and Einstein families.

## The problem

A family runtime already builds any of its variants from a
`FirmwareSet`. Everything else about a variant lived in the binary: the
id a script's `set_machine`, the `--machine` flag and the window's
variant menu use, and where that variant's ROMs are on disk. Three
binaries carried three copies of that, and inside each binary the copy
was reached three ways: the launcher at boot, the UI's `switch_variant`,
and the MCP `set_machine` tool. The Spectrum's `machine.rs` was 430
lines of vocabulary and bundle table sitting beside a runtime `Model`
enum that already knew every variant's profile id and display name; the
Amiga's `ModelArg` re-enumerated nine of the runtime's eighteen models
so a `--model` flag could name them.

Two costs followed. The shell could not own the `set_machine` step,
because only a binary knew how to turn an id into a booted variant, so
`set_machine` stayed a per-binary intercept after every other step had
moved up. And the three doors into a binary could disagree: the Amiga's
MCP tool took `model` while the script step took `machine`, and the
Spectrum's UI switch and its MCP swap each resolved ROMs with their own
copy of the same table.

## The decision

**The runtime crate owns the variant. The binary keeps flag parsing.**

`FamilyRuntime` declares the catalogue:

| method | what it is |
|---|---|
| `variant_ids()` | every id, in menu order |
| `model_from_id(id)` / `variant_id(model)` | the id vocabulary, round-tripping |
| `profile_for(model)` | the profile the variant boots as |
| `rom_convention()` | the family's ROM directory: an environment variable, then directories under `~/.emu198x/roms` |
| `firmware_sources(model)` | each firmware id the variant boots and its conventional file names |

The shell resolves them once, in `emu198x_shell::variants`:
`resolve_firmware` finds every image (pins applied), `build_variant`
boots one, and `swap_variant` is the body of the shared `set_machine`
step, reached through the `MachineCore::set_machine` hook that a family
runtime implements as one call. The tool registers from the
`variant-switch` capability, as the other tiers do under
[`tools-follow-the-machine-spec.md`](tools-follow-the-machine-spec.md).

`FirmwareOverrides` carries what a flag can add: a ROM directory, and
pins by firmware id. The Spectrum's `--rom ID=PATH` semantics moved with
it and apply to every family: a pin replaces exactly one image, an id
the variant does not take is an error rather than a silent fallback, a
bare path names the sole image of a single-ROM variant and is refused on
a multi-ROM one, and a variant whose every image is pinned needs no ROM
directory.

The ids are the ones each family already published (`spectrum_128k`,
`a1200`), so no script changes. The step and tool accept `model` as a
second spelling of `machine` because the Amiga's tool used it.

## What moved and what did not

The Spectrum's `machine.rs` is gone; its bundle table is
`Model::firmware_sources`, its ids are `Model::VARIANT_IDS`, and its
override tests are the shell's. The window title and menu keep their
short labels through `Model::menu_label`. The three boot paths in the
binary each became one call to `build_variant`.

The Amiga's `model.rs` is gone the same way. `Model::VARIANT_IDS` now
exposes all eighteen regional presets: the nine original ids retain PAL
semantics and their NTSC counterparts add `-ntsc`. The native menu groups
these presets under six base machines, with RAM and accelerator
configurations labelled separately. Its Kickstart candidate names are
`Model::firmware_sources` by chip stack, and `--rom-dir` /
`--kickstart` are a `FirmwareOverrides` directory and pin. Its MCP
`set_machine` tool, which took `model` and answered with `model`, is
the shell's, which takes either spelling and answers with `machine`
like every other family; the smoke test that read `model` back now
reads `machine`.

The C64's `ModelArg`, its ROM-directory search and its per-file
candidate lists are gone the same way. `Model::firmware_sources` lists
KERNAL, BASIC and the character generator as required and the three
drive DOS ROMs as optional, which is why the shell only insists on a
ROM directory when a required image is unpinned: a launch naming the
three by hand works without one, and a `--load-snapshot` boot with no
firmware at all still restores from the snapshot. `c64c` stays an
accepted spelling of `c64c-pal`. The C64 gains `set_machine` over MCP
and in scripts, which it never had. Its window used to rebuild a
switched variant from firmware bytes stashed at launch, so `--kernal`
survived a menu switch there and nowhere else; it now resolves by
convention like the other two menus and the shell, so a switch means
one thing on every family: the variant's conventional ROMs, launch-time
pins not carried across.

What did not move is the launch-time policy the flags express: which
variant a `--machine`, `--model` or portable snapshot selects, and
which flags map onto which pins. That is the binary's, because it is
the binary's command line.

The ZX81 catalogue uses its existing profile ids (`sinclair-zx81`,
`sinclair-zx81-16k`, `timex-ts1000`) in the window menu, `--model`, and
shared `set_machine` tool and script step. A model supplies its default
RAM and frame pacing; `--ram-bytes` overrides RAM at launch. Switching
builds the selected model's default configuration, resolves firmware by
convention, and drops launch-time overrides as on the other families.
MCP retains its ability to start blank when firmware is unavailable.

The ZX80 follows the same path for its existing stock, USA and RAM-pack
profile ids. Its native labels describe region and configuration within one
base machine. `--rom-dir`, `EMU198X_ZX80_ROM_DIR` and `--rom ID=PATH` use the
shared resolver while `--rom PATH` and `EMU198X_ZX80_ROM` stay compatible.
A switch ejects the tape and replaces launch-time RAM overrides with the
selected preset's defaults; reports describe that live state. Blank MCP
startup remains available when conventional firmware is absent, but invalid
images and explicitly requested missing paths are errors.

Jupiter Ace and MTX also use the shared catalogue. Ace uses its existing
profile ids and preserves `--ram-kb` as a preset selector; MTX keeps `mtx500`
and `mtx512` as canonical ids and accepts profile ids as aliases. Their native
labels distinguish Ace expansion configurations from marketed MTX models.
Both retain existing per-file environment conventions alongside shared
`--rom-dir`, family directory variables and `--rom ID=PATH` pins. MCP loads
available firmware and permits blank startup only when conventional firmware
is absent. MTX ROM-size validation stays in its runtime, including larger
combined images containing whole paged ROMs.

`FirmwareSource::with_env_var` describes a conventional variable naming
one ROM file, preserving `EMU198X_ZX81_ROM` through every entry point.
An explicit image pin wins over the file variable, which wins over
conventional directory lookup. A missing file named by either is an
error, not a reason to silently load a different ROM.

The Spectrum profiles gained `variant-switch` and a family ROM
directory variable, `EMU198X_SPECTRUM_ROM_DIR`, that the Amiga and C64
already had in their own spelling.

## Single-model families and blank startup

CPC464 and Tatung Einstein use the same firmware catalogue with one entry.
Their existing single-model launch policy remains in the binary; they do not
advertise `variant-switch` or add a redundant native selector. Runtime frame
budgets preserve the existing values. CPC's `--tape` loads through the same
path in normal launch and MCP, and reset keeps the cassette.

`build_variant_or_blank` is an opt-in shell helper: the binary supplies the
family's blank constructor. It falls back only for missing conventional
firmware without explicit pin or directory overrides. Invalid images,
unreadable files and missing explicit paths remain errors. ZX80, Ace, MTX,
CPC and Einstein use it; strict launch continues through `build_variant`.
This keeps blank-start policy reusable without making it implicit for every
family.

## Regional computer families with cartridges

Sord M5 and SVI-328 use their existing PAL/NTSC profile ids as catalogue ids.
Their runtime owns firmware conventions and the unchanged host frame budgets;
`--region` remains a launch alias for model selection. SVI's `--bios PATH` and
both families' per-file environment variables remain compatible with shared
firmware overrides. Missing conventional firmware permits blank MCP startup;
explicit failures remain errors.

A profile switch builds a fresh target using conventional firmware, ejecting
cartridges and the SVI cassette. Reset keeps media, and a failed switch keeps the
entire current runtime. UI pacing and dimensions follow the installed region.
This policy describes these computer families; cartridge-only consoles still
need an explicit retention/reload policy before migration.

`MachineApp::startup_media` holds parsed media for scripts and the default window
constructor. Firmware-based apps opt into it for MCP via `mcp_startup_media`,
avoiding a second interpretation of `--rom` as a cartridge. The default MCP hook
retains raw media-flag discovery for unmigrated callers. New migrations should
use parsed media consistently across all three entry points.

## Acorn RAM presets and dual-ROM firmware

Atom exposes its existing base 2.5 KiB and expanded 32 KiB presets using their
profile ids. The runtime owns the legacy `--ram-kb` threshold mapping; report
fields describe installed RAM. A switch boots fresh and ejects cassette and
utility-ROM media, while reset and failed switches retain them. Electron uses
the same catalogue contract with one entry and separate required OS/BASIC images;
it does not advertise switching.

Both families retain per-file firmware variables and legacy path flags alongside
shared directory overrides and named pins. Blank-start fallback checks per-file
environment overrides as well as CLI pins and directory choices: a partially
specified dual-ROM set must fail, not appear to have launched successfully blank.

## Oric firmware names and Aquarius launch configuration

Oric retains `oric-1` and `atmos` as canonical ids, accepting `oric1` and
`oric-atmos` as aliases. Its catalogue prefers the model-specific filenames
`oric-1.rom` and `atmos.rom`, retaining `oric.rom` as a legacy fallback. This
preserves existing installs without claiming that a generic file identifies a
firmware version. Per-file environment choices and explicit pins take precedence.
Switching builds a fresh target and ejects tape; reset and failed switches retain it.

Aquarius has one existing NTSC catalogue entry with required BIOS and character
ROMs. RAM expansion remains an independent launch setting, capped by the runtime
at the existing 16 KiB limit. Both normal and MCP launch apply it; parsed cartridge
loading is shared with the window and script paths. Reports describe live state.
Neither migration changes emulated hardware timing or ROM validation policy.

## Cartridge retention across console regions

SG-1000 and ColecoVision use their existing PAL/NTSC profile ids as catalogue
ids. Their region switches cold-boot while retaining the installed cartridge's
in-memory bytes. The runtime owns this policy through `FamilyRuntime::replacement`;
the default implementation builds fresh, preserving other families' existing
media-ejection behaviour. UI switches call `build_replacement`; session swaps
call the same runtime hook after resolving firmware. Construction must leave the
source runtime unchanged, including on failure. Recording rejection occurs before
replacement so a failed switch cannot silently replace the running machine.

SG-1000's empty firmware catalogue needs no directory. Cartridge loading is media,
not firmware resolution, and normal launch still requires it. ColecoVision retains
its BIOS file convention and legacy flag alongside shared directory and named-pin
options. Its switches resolve conventional BIOS firmware, dropping launch pins,
while retaining cartridge bytes. Both launchers share parsed cartridge startup
with script, MCP and window modes, including positional paths.

## Optional Atari 5200 BIOS

Atari 5200's single NTSC model uses the shared resolver with an optional BIOS
source. Both conventional filenames (`bios.rom`, then `5200.rom`) remain valid;
explicit file pins and the legacy file environment variable take precedence.
Missing conventional firmware does not require a HOME or ROM directory, while
an explicitly missing BIOS is an error. BIOS-size handling remains the runtime's
existing policy. Parsed cartridge loading is shared by window, script and MCP,
and MCP now loads the available BIOS before a cartridge arrives. No alternate
hardware profile or switch capability is introduced.

## Catalogues over a shared Sega runtime

Master System and Game Gear supply distinct `SmsModel` implementations to the
class crate's generic `SmsRuntime<M>`. The class owns the `MachineCore`, debug,
query, snapshot and `FamilyRuntime` implementations; each per-system crate owns
its catalogue and exports a concrete alias. No foreign trait implementation is
needed in a per-system crate, so Rust's orphan rule does not prevent this shape.
The shared Z80 debug macro accepts a bounded generic runtime without duplicating
its debug implementation.

Master System switches retain cartridge SRAM and its dirty state as well as ROM
bytes. This preserves clean loaded sidecars and pending writes across a cold boot.
Snapshot ids and version remain unchanged. Game Gear's catalogue has no console
ids, and its single profile publishes no switching capability. Both launchers
retain their existing cartridge construction order; MCP reuses that parsed path
so Master System restores a sidecar before executing requests.

## BBC Micro default language is launch policy

The Model B catalogue declares required MOS and optional teletext-font and BASIC
images. Their names and file environment variables belong to the runtime.
`from_firmware` installs MOS and font; `from_firmware_with_basic` also installs
BASIC into bank 15. The shared `build_variant_with` resolver supplies either
constructor with the same firmware set.

The window selects the BASIC constructor by default. Headless modes do so only
for an explicit BASIC firmware pin, preserving their existing bare-MOS default.
Explicit sideways banks are installed afterwards in every mode and therefore
win over the default language. Firmware selection does not introduce a new
hardware profile. Missing-file and MOS-validation errors use the shared policy;
optional language/font byte validation retains existing behaviour.

## Adding a variant or migrating a family

For another variant of a migrated family, extend its runtime model and
catalogue, supply its firmware requirements and constructor, and verify
its profile, RAM or other configuration, and native frame pacing. The
existing shared script executor, MCP switch tool and firmware resolver
remain unchanged. A window menu should enumerate the runtime catalogue.

For an existing family joining this convention, implement `FamilyRuntime`
and delegate its `MachineCore::set_machine` hook to `swap_variant`.
Declare `variant-switch` in the profiles. Route launch-time firmware
pins through `build_variant` and window switches through `build_replacement`
when the family retains media; keep CLI parsing
and optional blank-start policy in the binary. Test the actual CLI and
MCP entry points, including missing firmware and a failed switch that
leaves the current model intact. The ZX81's `tests/variants.rs` files
exercise this without external firmware.

## MSX cartridge retention

MSX regional switches use `FamilyRuntime::replacement` to retain both cartridge
ROMs and explicit mapper choices while cold-booting the selected profile with
conventional BIOS. Mapper bank state resets with the machine. The launcher
installs parsed cartridges directly in every mode because generic media loading
auto-detects mappers and would overwrite explicit choices.

Snapshot restore must refresh the runtime's cached boot media from the restored
machine. Otherwise reset and replacement would silently revive launch-time ROMs
or lose snapshot-restored cartridges. Read-only machine accessors expose the
installed BIOS and cartridge/mapper pairs for that synchronization; snapshot
serialization and hardware execution remain unchanged.

## Atari console cartridge retention

Atari 2600 and 7800 regional replacement cold-boots the cartridge in the live
machine. A separately cached launch image is insufficient: after snapshot
restore it can describe a different cartridge, and a 7800's parsed A78 hardware
configuration cannot be recovered from ROM payload bytes alone.

Each machine shares its existing constructor setup with a `cold_boot` path.
Cartridge ROM and configuration survive; banks, RAM and peripheral state use
the existing power-on values. Runtime reset and `FamilyRuntime::replacement`
use that path, removing redundant ROM caches without adding fields to snapshots.
New cartridge insertion constructs successfully before replacing the live
machine, so a failed parse leaves reset and switching usable.

## Drift triggers

Stop and re-read this record if you find yourself:

- adding a `MachineKind` / `ModelArg` enum to a binary, or a
  `from_id` that maps a string onto the runtime's `Model` — the runtime
  owns that map;
- writing a path under `~/.emu198x/roms` in a binary — it belongs in
  the runtime's `firmware_sources`;
- resolving firmware differently in the UI switch, the launcher and
  the MCP swap — all three use the shared resolver;
- intercepting `set_machine` in a binary.

Related: [`tools-follow-the-machine-spec.md`](tools-follow-the-machine-spec.md),
[`amiga-machine-catalogue.md`](amiga-machine-catalogue.md) (the model
catalogue whose ids this exposes).

The [native UI catalogue audit](../../docs/status/native-ui-catalogue-audit.md)
records the remaining launcher and menu coverage gaps. `VariantInfo::in_group`
lets the shared menu present runtime-owned configuration labels under a base
machine; families without groups retain a flat menu.

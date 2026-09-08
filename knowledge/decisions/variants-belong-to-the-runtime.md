# Variants belong to the runtime

**Status:** Adopted 2026-09-08 for the Spectrum, Amiga, C64 and ZX81 families.

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

`FirmwareSource::with_env_var` describes a conventional variable naming
one ROM file, preserving `EMU198X_ZX81_ROM` through every entry point.
An explicit image pin wins over the file variable, which wins over
conventional directory lookup. A missing file named by either is an
error, not a reason to silently load a different ROM.

The Spectrum profiles gained `variant-switch` and a family ROM
directory variable, `EMU198X_SPECTRUM_ROM_DIR`, that the Amiga and C64
already had in their own spelling.

## Adding a variant or migrating a family

For another variant of a migrated family, extend its runtime model and
catalogue, supply its firmware requirements and constructor, and verify
its profile, RAM or other configuration, and native frame pacing. The
existing shared script executor, MCP switch tool and firmware resolver
remain unchanged. A window menu should enumerate the runtime catalogue.

For an existing family joining this convention, implement `FamilyRuntime`
and delegate its `MachineCore::set_machine` hook to `swap_variant`.
Declare `variant-switch` in the profiles. Route launch-time firmware
pins and the window switch through `build_variant`; keep CLI parsing
and optional blank-start policy in the binary. Test the actual CLI and
MCP entry points, including missing firmware and a failed switch that
leaves the current model intact. The ZX81's `tests/variants.rs` files
exercise this without external firmware.

## Drift triggers

Stop and re-read this record if you find yourself:

- adding a `MachineKind` / `ModelArg` enum to a binary, or a
  `from_id` that maps a string onto the runtime's `Model` — the runtime
  owns that map;
- writing a path under `~/.emu198x/roms` in a binary — it belongs in
  the runtime's `firmware_sources`;
- resolving firmware differently in the UI switch, the launcher and
  the MCP swap — all three are `build_variant`;
- intercepting `set_machine` in a binary.

Related: [`tools-follow-the-machine-spec.md`](tools-follow-the-machine-spec.md),
[`amiga-machine-catalogue.md`](amiga-machine-catalogue.md) (the model
catalogue whose ids this exposes).

The [native UI catalogue audit](../../docs/status/native-ui-catalogue-audit.md)
records the remaining launcher and menu coverage gaps. `VariantInfo::in_group`
lets the shared menu present runtime-owned configuration labels under a base
machine; families without groups retain a flat menu.

# Native installation and developer journey evidence

The selected Spectrum build, ROM-load and source-debug journey passed on
Intel macOS, Apple Silicon macOS and Windows Server 2022 on 16 October 2026.
These observations use published binaries and actual installed executable paths.
They do not establish desktop GUI, sound output or every-machine usability.

## Accepted observations

| Host and route | Selected Asm / Build / Emu releases | Accepted public run |
|---|---|---|
| Intel macOS, Homebrew | 0.0.71 / 0.2.12 / 0.35.3 | [38963566008](https://github.com/emu198x/emu198x/actions/runs/38963566008), Intel installer job |
| Apple Silicon macOS, Homebrew | 0.0.71 / 0.2.12 / 0.35.3 | [38963566008](https://github.com/emu198x/emu198x/actions/runs/38963566008), ARM installer job |
| Windows Server 2022 x64, public Asm/Build PowerShell installers and Emu ZIP | 0.0.71 / 0.2.12 / 0.35.3 | [38964506987](https://github.com/emu198x/emu198x/actions/runs/38964506987), Windows installer job only |
| Intel macOS, release archives; Spectrum and Amiga | 0.0.67 / 0.2.12 / 0.35.2 | [38965383166](https://github.com/emu198x/emu198x/actions/runs/38965383166) |

The Windows observation belongs to its successful job, not the whole run:
a separate Intel archive job exceeded its original command time limit. The
fresh Intel archive run passed with the same 15,000 Spectrum frames and a bounded
360-second cold-boot command limit. This is not a measured performance range.

The source fixture is pinned to samples revision
`51297ac3196a990286b0d21a4dedd4653c0168f0`. The Spectrum journey builds the
Meteor Storm loading-screen checkpoint into a six-block tape, loads it through
the genuine 48K ROM tape path and checks loaded bytes, title pixels and CPU
execution. A separate first-program fixture emits a debug sidecar and stops at
source line 9, address `$8004`; loaded bytes and disassembly agree. Removing
the CODE block or debug sidecar causes the corresponding check to reject.

The Intel Amiga observation builds the retained sample and ADF, cold-boots
Kickstart 1.3 and checks its completion marker and hold loop. Its executable
and disk match the pinned canonical products. A real truncated disk and a
missing-completion mutation reject. This is a separate fixture and firmware
configuration, not an extension of the Spectrum result to all Amigas.

## Installation and removal boundaries

Before installation, no family executable was present. On macOS no family
formula was installed, including unlinked formulae. Installed executable bytes
matched the selected public archives before the journey ran. Homebrew uninstall
then removed the tools and their PATH entries.

Windows uses the unmodified public Asm/Build PowerShell scripts and constructs
a fresh PATH from machine and user settings. Emu has a ZIP route; it has no
vendor installer in this observation. The selected PowerShell scripts document
no uninstaller. Explicit file/receipt removal and restoration of the exact
previous user PATH were checked; that is not vendor-uninstaller qualification.

All hosts are disposable developer images with prerequisites already present.
Homebrew itself was already installed. Consumer setup, Homebrew bootstrap,
Windows client GUI, signing/notarization/quarantine first launch, native menus,
physical keyboard interaction and audible output remain unassessed here.
Newer releases do not inherit these observations.

## Inspect or reproduce

The accepted artifact identities are SHA-256 hashes of the downloaded ZIPs:

| Artifact ID | Public run | ZIP SHA-256 |
|---|---|---|
| 12073663319, macOS Intel installer | 38963566008 | `6d209cf8f9a206a1f1027b98f9491d5c5f4f6048aa67a56f5a8d378fdd7d1cdd` |
| 12073647799, macOS ARM installer | 38963566008 | `ce2c91cb07f5202e2742ef06ad390c2dabee2a0b931dcd996a9605ff37d9dab7` |
| 12074412510, Windows installer | 38964506987 | `f159870f47a84227225b637172ee081531b051533b5b423016f9cb068c7f94e9` |
| 12074811956, Intel archives | 38965383166 | `cf6d0508163f2b7eb949c232cdb9be31c92270d2456a37b37e39efd3a362c7d0` |

Artifacts have 30-day retention, expiring on 15 November 2026. Their reports
retain executable/archive identities, commands, runtime observations, removal
results and exclusions. Firmware and downloaded native tools are excluded.

The earlier Windows artifact in run 38963566008 is not accepted: its wrapper
record overwrote an executable hash with an archive hash. The fresh Windows
record above preserves those as separate fields and agrees with the underlying
journey report. Successful job transport alone did not establish acceptance.

The [native package workflow](../../.github/workflows/test-linux-packages.yml)
provides manual `installer_journey` and `macos_developer_journey` inputs.
The checked-in [installer pins](../../scripts/installer-journey-inputs.json),
[Intel archive pins](../../scripts/macos-developer-journey-inputs.json),
[Spectrum verifier](../../scripts/windows-developer-journey.py) and
[Amiga verifier](../../scripts/macos-amiga-journey.py) define the exact selected
inputs and acceptance checks. Reproduction requires legitimately available
firmware and the workflow's existing private provisioning; no firmware is
published with this document. The underlying acceptance implementation was
merged in [PR #1805](https://github.com/emu198x/emu198x/pull/1805).

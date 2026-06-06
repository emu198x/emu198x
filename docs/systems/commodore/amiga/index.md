# Commodore Amiga

## Status: Boots Workbench 3.1; full model matrix; CPU oracles green

The most ambitious target in the project, and well past "started." A1200 +
Kickstart 3.1 boots to the Insert-Workbench prompt and Workbench 3.1 mounts to a
clean AGA desktop with no palette or geometry artefacts. The full A1000 /
A500-family / A600 / A1200 (AGA) / A2000 model matrix is reachable via `--model`.
Native `wgpu` window with keyboard/mouse, port-1 joystick/gamepad, and live
Paula audio; headless Kickstart/Workbench runner with DF0 `ADF`, screenshots,
audio capture, scripted input.

Deep per-chip and per-Kickstart notes live in [`chipset/`](chipset/) and
[`kickstart/`](kickstart/).

## What works

- **68k CPUs** — 68000 100% Tom Harte (1M tests); 68010/68020 100% against
  Musashi (via `m68k-test-gen`). Variants wired per model.
- **Workbench 3.1 (AGA)** — A1200 + KS 3.1 → clean desktop. Recent AGA fixes:
  64-bit FMODE bitplane wide-fetch (`d31e46a`), 68020 full-format EA decode for
  the WB palette path (`369d50b`), DENISEID `$00F8` for AGA Lisa (`bc0e8ec`).
- **Chipset** — Agnus (DMA, beam), Denise/Lisa (bitplanes, HAM, sprites, AGA),
  Paula (4-ch PCM audio + disk MFM), Copper, Blitter. Per-chip status in
  [`chipset/`](chipset/).
- **Kickstart** — 1.2 through 3.1 boot paths documented and exercised; see
  [`kickstart/`](kickstart/) (per-version notes + boot-flow + debugging guide).
- **Media** — DF0 `ADF`; screenshots, audio capture, scripted input.

## Not implemented / accuracy gaps

- **Game/application breadth** — Workbench boots, but broad OCS/ECS/AGA game and
  application validation is the open frontier (only a narrow set exercised).
- **Gayle (A600/A1200)** — IDE / PCMCIA paths not fleshed out.
- **No automated Workbench smoke** — WB 3.1 boot is verified by hand, not yet a
  screenshot smoke in CI.
- **Native window is primary-tier** — present (unlike the headless extended
  systems), but per-chip accuracy gaps are tracked in [`chipset/`](chipset/).

## Known unknowns / disproven hypotheses

- **This page was itself stale** — it read "Not started" while the Amiga booted
  Workbench 3.1. A standing reason the per-system status layer exists: keep the
  headline honest. (Corrected 2026-06-06 from `docs/status/current-system-usability.md`.)
- **Open: software-compatibility surface** — which real games/demos boot across
  OCS/ECS/AGA is largely unmeasured; the chipset is validated by CPU oracles and
  Workbench, not a broad title sweep.
- **Verification targets** — per-chip timing claims in [`chipset/`](chipset/)
  (inter-chip timing, DMA priority) should be confirmed against the primary
  Amiga Hardware Reference Manual + `syntheses/` deep-dives, not just code.

## Validated against

- 68000 Tom Harte (1M); 68010/020 vs Musashi (`m68k-test-gen`).
- Workbench 3.1 AGA desktop (hand-verified screenshot).
- `../../syntheses/` — 24 Amiga deep-dive docs (Paula, Kickstart, MFM, …).
- Reference emulators: vAmiga, WinUAE, fs-uae, Minimig-AGA (`emulators/amiga/`).

## Timing & cycle-accuracy

- **Master clock & dividers** — 28.37516 MHz PAL. 68000 = ÷4 (7.09 MHz); chipset
  at the colour clock (÷2) and low-res pixel clock (÷4).
- **Timing model realised** — architecturally the **strongest cycle-accuracy
  story**: the design is DMA-driven, with Agnus arbitrating CPU/copper/blitter/
  sprite/audio/disk/bitplane access cycle-by-cycle and the copper synced to the
  beam. The model *is* the master clock; per-chip timing notes live in
  [`chipset/inter-chip-timing.md`](chipset/inter-chip-timing.md).
- **CPU timing** — 68000/010/020 cycle-accurate (§62; Tom Harte 1M + Musashi
  prove the ISAs).
- **Distance to full cycle-accuracy** — broad software validation across OCS/ECS/
  AGA will surface the residual blitter/copper/DMA-priority edge cases; the
  framework is cycle-exact by design.

## Tooling & drivability

- **Script / MCP** — deep: `--script` + `--mcp` with Exec/library introspection
  (`query_exec_tasks`, `resolve_lvo`, `address_to_library`), chipset queries,
  copper-list/blitter/CIA queries, `run_until_pc`, watch-memory. One of the
  richest debug surfaces in the fleet.
- **Native window** — yes (primary tier): `wgpu` video, keyboard/mouse, joystick,
  Paula audio.
- **Disassembler** — `disasm`/`disasm_around` present (68k); the Asm198x shared
  68000 disassembler is the convergence target.

## Peripherals & connectivity

- **Emulated now** — DF0 floppy (`ADF`), mouse, port-1 joystick/gamepad, Paula audio.
- **Period peripherals (emulatable)** — extra floppies, hard disk (via Gayle —
  stub-level), serial, parallel, Zorro cards, RTG framebuffers, PCMCIA (A600/A1200).
- **Internet-capable** — **Yes** (strongly): period serial TCP/IP stacks (AmiTCP,
  Miami), A2065 Zorro Ethernet, PCMCIA Ethernet on A600/A1200 (via Gayle), and the
  modern PiStorm. The Amiga was a real internet machine — a first-class net target.

## Crates

| Crate | Role |
|-------|------|
| `cpu-m68k` (`motorola-68000` family) | 68000/010/020 cores |
| `commodore-agnus*` | DMA controller / beam |
| `commodore-denise*` / Lisa (AGA) | video |
| `commodore-paula` | audio + disk |
| `machine-commodore-amiga-ocs` / `-ecs` / `-a1200` | per-chipset machine wiring |
| `runtime-commodore-amiga` | shared-shell runtime |
| `emu198x-amiga` | native + headless runner |

(Crate names approximate — confirm against `crates/` when editing; the donor-era
names in the old page were wrong.)

## ROMs

Kickstart images at `~/.emu198x/roms/commodore-amiga/` (e.g. `kick13.rom`,
`kick20.rom`, `kick31.rom`). Workbench/app disks as `ADF`.

## Launch

```sh
cargo run --release -p emu198x-amiga --no-default-features -- \
  --model a1200 --kickstart ~/.emu198x/roms/commodore-amiga/kick31.rom \
  --disk workbench31.adf --frames 2500 --screenshot wb.png
```

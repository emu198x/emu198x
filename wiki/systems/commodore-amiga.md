# Commodore Amiga (OCS PAL)

> Status as of 2026-04-24: **OCS headless runtime with A500-family
> and real A1000 bootstrap paths.** The runtime catalogue exposes A1000,
> stock A500, A500+A501, A500+, and maxed A500 RAM profiles. A500
> Kickstart 1.3 reaches the insert-disk screen; A500+A501 with
> Workbench 1.3 reaches the Workbench desktop in the golden matrix.
> The A1000 path uses the bootstrap ROM, writable WOM, Kickstart disk,
> and scripted Workbench disk swap. `emu198x-amiga` provides a minimal
> native verifier window with keyboard, mouse input, and live Paula
> audio. Runtime audio drains Paula's live stereo mix into 48 kHz
> audio packets; host-side Paula channel mute/gain is available from
> the native verifier without changing AUDx registers. Snapshots and
> joystick input remain pending.

## Implementation status

| Component | Crate | Status |
|-----------|-------|--------|
| 68000 CPU | `motorola-68000` | Pin-level bus core imported and running under the Amiga machine |
| CIA pair | `mos-cia-8520` | Imported Amiga `8520` variant |
| Gary | `commodore-gary` | Imported and wired into the machine |
| Agnus OCS | `commodore-agnus-ocs` | Imported and driving the PAL machine timing |
| Denise OCS | `commodore-denise-ocs` | Imported and producing the machine framebuffer |
| Paula | `commodore-paula-8364` | Imported; register/audio-DMA path active, runtime drains live stereo mix and exposes host channel controls |
| ADF parser | `format-commodore-amiga-adf` | Fresh-workspace disk container/media parser |
| Floppy peripheral | `peripheral-commodore-amiga-floppy` | DF0 drive mechanics + MFM support |
| Keyboard peripheral | `peripheral-commodore-amiga-keyboard` | Raw-key queue and serial keyboard path |
| Machine wiring | `machine-commodore-amiga-ocs` | OCS PAL board loop with A1000 and A500-family RAM profiles |
| Runtime | `runtime-commodore-amiga` | Fresh `MachineCore` runtime over the machine crate |
| Native verifier | `emu198x-amiga` | Windowed OCS video, live Paula audio, A1000/A500-family firmware loading, optional DF0 media, basic keyboard and mouse input |
| Headless runner | `emu198x-script-amiga` | Kickstart/bootstrap boot, DF0 media insertion, screenshots, audio capture, scripted keys |

### What works

- OCS PAL master-clock machine loop in the fresh workspace.
- A1000 bootstrap ROM + WOM path and A500-family Kickstart ROM validation.
- RAM presets for stock A500, A500+A501 slow RAM, A500+ 1 MiB chip, and maxed A500 with Zorro-II fast RAM.
- Standard-viewport RGBA framebuffer output from Denise.
- Paula register/audio-DMA execution in the machine layer, drained through the runtime as 48 kHz stereo audio packets.
- `floppy-0` / DF0 media insertion with zipped or plain `ADF` images.
- Native `emu198x-amiga` verifier window with Kickstart/bootstrap ROM loading, optional DF0 media, hard reset, live Paula audio, host-side Paula channel controls, basic keyboard input, and port-0 mouse input.
- Shared scripted keyboard input routed through the Amiga keyboard peripheral.
- Queryable machine/runtime state including CPU PC, visible-output detection, A1000 bootstrap visibility, keyboard queue state, and DF0 insertion/motor/head state.

### Validated

- **Kickstart 1.3 insert-disk proof** — the fresh `emu198x-script-amiga` path boots a real Kickstart ROM and reaches the real insert-disk screen.
- **Workbench 1.3 desktop proof** — the golden matrix captures A500+A501 + Kickstart 1.3 + Workbench 1.3 reaching the Workbench desktop after the long boot path.
- **A1000 bootstrap proof** — the golden matrix covers the real A1000 bootstrap ROM path, Kickstart disk load into WOM, and scripted Workbench disk swap.
- **RAM/autoconfig proof** — runtime RAM-variant tests cover stock, trapdoor, A500+, custom fast RAM, and Kickstart configuration of the maxed fast-RAM board.
- **Workspace verification** — the imported Amiga crates and fresh runtime/runner pass their current unit-test slice in the active workspace.

### What doesn't work yet

- **Native verifier UI depth** — the fresh `emu198x-amiga` shell is intentionally minimal and does not yet expose joystick input.
- **Snapshots** — the fresh Amiga runtime deliberately reports snapshot import/export as unsupported.
- **Software proof beyond the current goldens** — Workbench 1.3 and the A1000 Kickstart/Workbench route are proven locally; broader game/application boot coverage is still pending.
- **Broader platform hardening** — joystick/mouse paths, stronger disk/software regressions, and frontend ergonomics are still pending.

## Architecture

The machine layer (`machine-commodore-amiga-ocs`) owns the OCS PAL board:

- `Cpu68000`
- Agnus / Denise / Paula / Gary
- two `mos-cia-8520` devices
- DF0 floppy and keyboard peripherals
- chip RAM and ROM mapping

The fresh runtime layer (`runtime-commodore-amiga`) owns:

- family/profile metadata for the A1000 and A500-family OCS PAL machines
- `MachineCore` translation over Kickstart firmware, DF0 media, shared input events, and frame/audio sinks
- the current query surface (`boot.detected`, `amiga.cpu.pc`, `amiga.display.non_black_pixels`, `amiga.disk.*`, `amiga.keyboard.*`)

The headless runner (`emu198x-script-amiga`) currently provides:

- Kickstart discovery from the ROM directory or an explicit `--kickstart`
- optional `--disk` insertion into DF0
- `--wait-for-boot`, `--frames`, `--screenshot`, `--audio-capture`, and shared script playback

The native verifier (`emu198x-amiga`) currently provides:

- A1000 and A500-family model selection
- ROM directory discovery or explicit `--kickstart` firmware loading
- optional `--disk` insertion into DF0
- windowed 768x576 RGBA video through `pixels`/`winit`
- hard reset, live Paula audio, basic A-Z / 0-9 / Space / Enter / Tab / Backspace keyboard input, and port-0 mouse movement/buttons

## Related

- [Amiga port plan](../decisions/amiga-port-plan.md) — the staged port plan that produced this fresh baseline
- [Nintendo NES](nintendo-nes.md) — another fresh headless-only baseline in the current workspace
- [Commodore 64](commodore-c64.md) — the more mature Commodore family target in the same architecture

# Commodore Amiga (A500 OCS PAL)

## Implementation status

| Component | Crate | Status |
|-----------|-------|--------|
| 68000 CPU | `motorola-68000` | Pin-level bus core imported and running under the Amiga machine |
| CIA pair | `mos-cia-8520` | Imported Amiga `8520` variant |
| Gary | `commodore-gary` | Imported and wired into the machine |
| Agnus OCS | `commodore-agnus-ocs` | Imported and driving the PAL machine timing |
| Denise OCS | `commodore-denise-ocs` | Imported and producing the machine framebuffer |
| Paula | `commodore-paula-8364` | Imported and producing stereo audio samples |
| ADF parser | `format-commodore-amiga-adf` | Fresh-workspace disk container/media parser |
| Floppy peripheral | `peripheral-commodore-amiga-floppy` | DF0 drive mechanics + MFM support |
| Keyboard peripheral | `peripheral-commodore-amiga-keyboard` | Raw-key queue and serial keyboard path |
| Machine wiring | `machine-commodore-amiga` | A500 OCS PAL board loop |
| Runtime | `runtime-commodore-amiga` | Fresh `MachineCore` runtime over the machine crate |
| Headless runner | `emu198x-script-amiga` | Kickstart boot, DF0 media insertion, screenshots, audio capture, scripted keys |

### What works

- A500 OCS PAL master-clock machine loop in the fresh workspace.
- Kickstart ROM validation and boot through the fresh `MachineCore` runtime.
- Standard-viewport RGBA framebuffer output from Denise.
- Stereo audio capture through Paula.
- `floppy-0` / DF0 media insertion with zipped or plain `ADF` images.
- Shared scripted keyboard input routed through the Amiga keyboard peripheral.
- Queryable machine/runtime state including CPU PC, visible-output detection, keyboard queue state, and DF0 insertion/motor/head state.

### Validated

- **Kickstart 1.3 insert-disk proof** — the fresh `emu198x-script-amiga` path boots a real Kickstart ROM, reaches the real insert-disk screen, and matches the old blessed screen closely enough to compare visually and by palette.
- **Workbench disk insertion smoke** — the same runner accepts a zipped Workbench 1.3 `ADF`, keeps it mounted in DF0 across a long post-boot run, and exposes the inserted-drive state through the query surface.
- **Workspace verification** — the imported Amiga crates and fresh runtime/runner pass their current unit-test slice in the active workspace.

### What doesn't work yet

- **Native verifier UI** — there is no fresh-workspace `emu198x-amiga` shell yet.
- **Snapshots** — the fresh Amiga runtime deliberately reports snapshot import/export as unsupported.
- **Software proof beyond Kickstart insert-disk** — the current honest bar is a correct no-disk KS1.3 boot plus disk insertion, not yet a proven Workbench or game boot.
- **Broader platform hardening** — joystick/mouse paths, stronger disk/software regressions, and frontend ergonomics are still pending.

## Architecture

The machine layer (`machine-commodore-amiga`) owns the A500 OCS PAL board:

- `Cpu68000`
- Agnus / Denise / Paula / Gary
- two `mos-cia-8520` devices
- DF0 floppy and keyboard peripherals
- chip RAM and ROM mapping

The fresh runtime layer (`runtime-commodore-amiga`) owns:

- family/profile metadata for the A500 OCS PAL baseline
- `MachineCore` translation over Kickstart firmware, DF0 media, shared input events, and frame/audio sinks
- the current query surface (`boot.detected`, `amiga.cpu.pc`, `amiga.display.non_black_pixels`, `amiga.disk.*`, `amiga.keyboard.*`)

The headless runner (`emu198x-script-amiga`) currently provides:

- Kickstart discovery from the ROM directory or an explicit `--kickstart`
- optional `--disk` insertion into DF0
- `--wait-for-boot`, `--frames`, `--screenshot`, `--audio-capture`, and shared script playback

## Related

- [Amiga port plan](../decisions/amiga-port-plan.md) — the staged port plan that produced this fresh baseline
- [Nintendo NES](nintendo-nes.md) — another fresh headless-only baseline in the current workspace
- [Commodore 64](commodore-c64.md) — the more mature Commodore family target in the same architecture

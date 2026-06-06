# TRS-80 Color Computer (CoCo)

## Status: Not started

The CoCo 1/2 share almost all hardware with the Dragon 32. The CoCo 3 adds significant new hardware (GIME chip). The 6809 CPU and all peripheral chips are already implemented in the Dragon crate.

## What can be reused from Dragon

- `cpu-6809` — identical CPU
- `machine-dragon` PIA, SAM, VDG — same chips with minor address map differences
- Shell infrastructure — audio, keyboard, save states, rewind

## What's different

### CoCo 1/2
- **Address map** — minor differences from Dragon (disk controller at different address)
- **BASIC ROM** — Microsoft Color BASIC (different from Dragon BASIC)
- **Keyboard matrix** — same 8×7 matrix but different key assignments in some positions
- **Cassette format** — CoCo CAS format (different header/encoding from Dragon)

### CoCo 3 (later variant)
- **GIME chip** — replaces SAM + VDG + PIA functionality with a single chip. Enhanced graphics (320×225, 640×225), MMU (512KB/2MB address space), hardware timers, new palette (64 colours)
- **Keyboard** — additional keys (ALT, CTRL, F1, F2)
- **Memory** — up to 512KB or 2MB with MMU

## Work needed

### CoCo 1/2
- **Machine crate** (`machine-tandy-coco`) — mostly configuration over Dragon. Different ROM, address map tweaks, keyboard layout adjustment.
- **Color BASIC ROM** — need CoCo BASIC ROM image
- **Shell binary** (`emu198x-coco`) — thin wrapper like Dragon shell
- **Effort:** Small — primarily configuration differences

### CoCo 3
- **GIME chip** — significant new hardware (enhanced video, MMU, timers)
- **Extended BASIC ROM** — Super Extended Color BASIC
- **Effort:** Medium-large

## ROMs needed

| File | Size | Description |
|------|------|-------------|
| `bas13.rom` | 8KB | Color BASIC 1.3 |
| `extbas11.rom` | 8KB | Extended Color BASIC 1.1 |
| `disk11.rom` | 8KB | Disk BASIC 1.1 (optional) |
| `coco3.rom` | 32KB | CoCo 3 Super Extended BASIC (for CoCo 3) |

## Known unknowns / disproven hypotheses

- **Open: the "mostly configuration over Dragon" claim.** Plausible for CoCo
  1/2 (shared 6809 + PIA/SAM/VDG) but unverified — the address-map, keyboard, and
  CAS-format differences haven't been pinned to a source.
- **Verification targets** — CoCo 1/2 vs Dragon address-map deltas, the CoCo
  keyboard matrix, and the GIME chip (CoCo 3) are from secondary knowledge.
  Confirm against the CoCo service manual / `emulators/dragon-coco/` (XRoar) and
  the GIME datasheet before implementing.

## Validated against

- (Nothing yet — not started. The shared 6809 + Dragon chips are validated under
  the Dragon page.)

## Timing & cycle-accuracy

- **Master clock & dividers** — 6809 at ≈0.89 MHz (NTSC), SAM-derived — same
  family as the Dragon. (CoCo 3 GIME replaces SAM+VDG+PIA.)
- **Timing model realised** — **not started**; CoCo 1/2 would inherit the Dragon's
  beam-updated VDG model.
- **CPU timing** — 6809 cycle-accurate (§62) via the shared core.
- **Distance to full cycle-accuracy** — everything CoCo-specific; the CoCo 3 GIME
  (MMU, timers, enhanced video) is a substantial new timing surface.

## Tooling & drivability

- **Script / MCP** — not started (no binary yet); would inherit the Dragon shell.
- **Native window** — not started.
- **Disassembler** — will use the Asm198x shared 6809 disassembler.

## Peripherals & connectivity

- **Period peripherals (emulatable)** — disk (the FD-501/RS-DOS controllers),
  cassette, the RS-232 program pak, joysticks, printer; CoCo 3 adds the higher-res
  display + 512 KB/2 MB RAM.
- **Internet-capable** — **Yes**: the CoCo is the heartland of **DriveWire**
  (serial-to-host virtual drive + networking) and CoCoNet — a thriving, documented,
  emulatable net path. Period RS-232 modems too.

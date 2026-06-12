# Commodore Amiga

## Status: Boots Workbench 1.3 (OCS) + 3.1 (AGA); deep chips, integration-limited

The most ambitious target in the project. A1200 + Kickstart 3.1 boots to a clean
Workbench desktop; A500/A1000 + KS 1.2/1.3 boot WB 1.3 (golden-tested). The
individual chips are deep and well-validated **in isolation** — the 68000 is the
best-tested CPU in the fleet, OCS Denise is pixel-exact, every Blitter minterm and
Copper instruction is implemented, Paula's audio + interrupt controller are
complete, the 8520s and AutoConfig fast-RAM are done.

What holds the Amiga back is **not the chips — it's integration and a few large
facades.** Three honest caveats the "boots Workbench 3.1" headline conceals:

1. **AGA colour and deep modes now render.** The 256-colour 24-bit palette (#93),
   HAM8 (#94), the BPLCON4 BPLAM bitplane XOR (#96), 32/64-px wide sprites — display
   *and* DMA fetch (#95/#99) — and 8-plane lowres bitplane fetch (#99) are decoded
   **and displayed**. AGA is no longer "an ECS machine that boots 3.1": real AGA
   colour software looks right. The one known render-bandwidth gap is **superhires
   (SHRES)**, whose DMA fetch is still modelled as hires (#469) — a niche mode.
2. **The Blitter is incremental but drains on observation.** The per-slot scheduler
   is wired into all three machines (#31) with BBUSY/BZERO readback (#32) and
   Copper BFD=0 sync (#33); a synchronous `run_blit_to_completion` drain remains
   only as a fallback when the CPU observes a mid-blit result. True per-slot bus
   contention (blitter-nasty, CPU stalls) is still approximate.
3. **You can't save a disk or boot a hard drive.** Floppy *read* is solid; disk
   *write-back* is built at the drive layer but unwired, and there is no hardfile/
   HDF/IDE path at all (Gayle is a stub).

This is an encouraging shape: most of the remaining work is **wiring and
integrating already-built, already-tested pieces** — not novel chip research. The
two in-flight refactors (single-bus-per-cck, unified-driver) *are* the critical
path to a cycle-exact chipset.

Deep per-chip and per-Kickstart notes live in [`chipset/`](chipset/) and
[`kickstart/`](kickstart/). The AGA render path described in
[`chipset/denise.md`](chipset/denise.md) is now largely real (24-bit palette, HAM8,
wide sprites, BPLAM XOR all landed); the one outstanding render gap is superhires
DMA bandwidth (#469) — see the gaps below.

## What works

- **68000 CPU** — real, complete, cycle-accurate (two-word prefetch, pin-level
  bus, full exception model). 124-opcode Tom Harte fixtures present; the standing
  CI net is an all-opcode disassembler conformance diff against the Asm198x ISA
  spec. The best-validated CPU core in the fleet.
- **68010/020 (+EC020)** — thin Deref-wrappers over the 68000 with their integer-
  ISA delta (68020 bitfields, MULL/DIVL Dn-source, scaled-index + the full-format
  extension word, MOVEC). The 68020 is ~70% of its ISA — enough for the EC020
  machines, which is what the A1200/CD32 (68EC020) need, and they need **no FPU or
  MMU**. (Missing: CAS/CAS2, CHK2/CMP2, TRAPcc, PACK/UNPK, Bcc.L, CALLM/RTM.)
- **OCS/ECS video (Denise)** — pixel-exact OCS: cycle-accurate shifters, BPLCON1
  barrel-scroll, HAM6, EHB, dual-playfield, sprite/playfield priority, pixel-level
  collisions. ECS adds KILLEHB, SHRES decode, DENISEID, the border flags.
- **Agnus logic** — the full Copper instruction set (MOVE/WAIT/SKIP, comparator +
  mask, CDANG, COP1LC/2LC + STROBE, mid-screen reprogramming), all 256 Blitter
  minterms across 4 channels (A/B shift, FWM/LWM, ascending/descending, inclusive/
  exclusive fill, line-mode octants), and a real per-colour-clock DMA slot table.
  AGA FMODE 16/32/64-bit bitplane wide-fetch landed and is validated against a
  live WB3.1 modulo oracle.
- **Paula** — 4-channel DMA audio with cross-channel period/volume modulation and
  the A500 DAC S-curve; the complete interrupt controller (all 14 sources, IPL
  priority encoder, INTENA/INTREQ); MFM floppy **read**; the serial + POT register
  surfaces.
- **CIA (8520 ×2)** — excellent: dual timers with the TBHI one-shot autostart
  quirk + the 8520 TOD write-halt, ICR, the keyboard serial handshake
  (WinUAE-matched), mouse/joystick ports. Memory map + OVL overlay correct.
- **AutoConfig** — real Zorro-II state machine; the "Maxed A500" autoconfigs 8 MiB
  fast RAM into AvailMem.
- **Kickstart** — 1.2/1.3 (OCS, golden-tested), 3.1 (AGA, hand-verified). 2.0x
  (ECS) boots but is unproven in CI.
- **Variants** — A1000, A500 (×4 RAM configs), A500+, A600, A1200, A2000 — all
  real and selectable at launch via `--model` and swappable mid-session via the
  `set_machine` tool (MCP) / `SetMachine` step (`--script`), which rebuilds the
  OCS/ECS/AGA variant and re-paces the session.
- **Drivability** — one of the richest debug surfaces in the fleet: ~45 MCP tools
  (chip queries, Exec/Kickstart introspection, task control, tracing, framebuffer
  dump, model swap) over a unified `HeadlessSession`. The driver replatform is
  **complete**.
- **Media (read)** — DF0 `ADF` (incl. inside `.zip`); screenshots, audio capture,
  scripted input.

## Not implemented / accuracy gaps

### AGA video: colour + deep modes done, superhires bandwidth remaining

OCS Denise is ~95% pixel-exact and ECS ~90%. AGA/Lisa has closed the colour and
deep-mode gaps: the 24-bit 256-entry banked palette (#93), HAM8 with 24-bit
chaining (#94), the BPLCON4 BPLAM bitplane XOR (#96), 32/64-px wide sprites —
display *and* DMA fetch (#95/#99) — and 8-plane lowres bitplane fetch (#99) all
render through the AGA path now, not the OCS 12-bit palette. The AGA Denise
bitplane ceiling was raised to 8 so deep modes compose, and Agnus fills the two
idle lowres fetch slots with BPL7/BPL8. The one remaining AGA-render gap is
**superhires (SHRES)**: the Denise shifter handles 4 source-pixels/output and
colour resolves correctly, but the Agnus DMA fetch models SHRES as hires, so a
superhires screen is fed half the plane bandwidth it needs (#469). Niche mode;
far narrower than the old "everything is a facade" framing — real AGA *colour*
software now looks right.

### The chipset is pre-cycle-exact (integration debt)

- **Blitter: incremental, with a synchronous drain fallback** — the per-slot
  scheduler now runs in all three machines (#31); DMACONR returns BBUSY/BZERO
  (#32) so blitter-polling and collision-via-BZERO read correctly, and the Copper
  honours BFD=0 blitter-wait (#33). A `run_blit_to_completion` drain still fires
  when the CPU observes a result mid-blit, so true per-slot bus contention
  (blitter-nasty, exact CPU stalls) is still approximate, not yet cycle-exact.
- **Two parallel DMA slot tables** — Agnus's `cck_bus_plan` and Denise's
  `dma_claim` independently re-derive the bitplane window, with an even-vs-odd
  copper-slot polarity disagreement. The single-bus-per-cck refactor unifies them.
- **Copper↔Blitter WAIT (BFD=0) sync** now implemented (#33): a Copper WAIT with
  BFD=0 blocks until the blitter goes idle.
- **68020 timing** uses the 68000's 4-clock model (no 020 3-clock/pipeline/cache),
  overstating A1200 cycle counts.
- Three machine crates each re-implement the ~1500-line per-CCK tick loop — the
  unified-driver refactor merges them so the above lands once, not three times.

### Storage: no save, no hard disk

- **Disk write-back unwired** — the MFM-encode-and-persist mechanism is built and
  unit-passes in the floppy crate (`flush_write_capture` → `save_adf`) but has zero
  callers; the write-side DMA never fires DSKBLK and there's no runtime flush
  surface. The Amiga is not yet on the `disk-save-write-back` parity list the C64
  completed 2026-06-08. **A Workbench SAVE is silently lost.**
- **No hard disk** — `commodore-gayle` is an explicit "no IDE drive" stub; no HDF/
  hardfile, RDB, FFS, SCSI, or IDE. A600/A1200 cannot boot from hard disk. (The
  full donor IDE path is harvestable from `Emu198x-Oldest/`.)
- **Formats** — Extended ADF, IPF (the `format-ipf` crate is referenced but never
  created), DMS, ADZ all absent.

### Audio + I/O fidelity

- **No audio output filter** — neither the fixed RC nor the LED-switchable
  Butterworth; and no Paula volume-PWM/aliasing model.
- **Serial is a register husk** — no baud timing, no host transport (blocks the
  AmiTCP/Miami internet path and the Rachel netplay goal).
- **POT analog ramp** is a stub (digital mouse buttons work); **mouse** uses a
  position-delta counter, not true Gray-code quadrature.

### Two small correctness bugs

- **CIA-B FLAG (disk index)** — `drive.tick()` returns the index pulse but it's
  discarded; trackdisk index-sync sees no interrupt.
- **CIA-B TOD** is never pulsed (real hardware counts the disk index there).

### Dormant modules (only matter for A3000/A4000)

- **FPU** (`motorola-68040/src/fpu.rs`, 705 LoC, f64-backed) and **MMU**
  (`motorola-68030/src/mmu.rs`, 2421 LoC) are fully written and unit-tested but
  **wired into nothing** — there is no F-line or PMMU decode dispatch, so FMOVE/
  FADD and PMOVE/PFLUSH don't execute. Needed only for 68030/040 machines.
- Missing 68020+ opcodes: CHK2/CMP2, PACK/UNPK, TRAPcc, CAS/CAS2, CALLM/RTM,
  Bcc.L, MUL/DIV memory-source.

### Missing machines + breadth

- **CDTV / CD32** — no CD subsystem at all; Akiko (CD32 chunky-to-planar + CD
  controller) has zero implementation; the models aren't constructible. **CD32
  also needs working AGA video** (see the facade above).
- **A3000 / A4000** — need 68030/68040 with FPU+MMU wiring, Zorro-III, SCSI/IDE.
- **Zorro-III** fast RAM (>8 MiB), PCMCIA, A2065/PCMCIA Ethernet, RTG (Picasso96/
  uaegfx), the RF5C01A RTC (some A1200s), no automated WB smoke in CI.

## Test coverage / validated against

- **68000** Tom Harte 124-opcode fixtures + an all-65,536-opcode disassembler
  conformance diff (standing CI net). **68010/020** — the green net is the
  inherited 68000 corpus run *through* the wrappers (with an allowlist); the
  separate 68020-semantics Musashi sweep is an `#[ignore]`d baseline, not an
  asserted pass. **68030/040** harnesses are byte-identical clones of the 020 one,
  so they only exercise the 020 subset the scaffolds inherit. No FPU/MMU/full-
  format-EA fixtures exist anywhere — that hole is how the AGA full-format `lea`
  bug survived a "100%" claim.
- WB 1.2/1.3 golden boot tests (OCS); WB 3.1 AGA desktop (hand-verified).
- `../../syntheses/` — 24 Amiga deep-dive docs (Paula, Kickstart, MFM, …).
- Reference emulators: vAmiga, WinUAE, fs-uae, Minimig-AGA (`emulators/amiga/`).

## Timing & cycle-accuracy

- **Master clock** — 28.37516 MHz PAL; 68000 = ÷4 (7.09 MHz); chipset at the
  colour clock. The CPU is genuinely cycle-accurate (prefetch + pin-level bus); the
  Amiga ticks it at 2× CCK and drives `BusStatus::Wait` when the CPU wants a
  claimed chip-RAM slot, so **DMA contention falls out of the slot grid naturally**.
- **The model is right; the assembly isn't finished.** The single-bus contention
  design is sound and the CPU honours it, but the Blitter bypasses it (synchronous)
  and two slot tables disagree. Cycle-exactness across copper/blitter/CPU is gated
  on the single-bus-per-cck + incremental-blitter integration, not new logic.

## Tooling & drivability

- **Script / MCP** — deep and unified: `--script` + `--mcp` over one
  `HeadlessSession`; Exec/library introspection, chipset/copper/blitter/CIA
  queries, `run_until_pc`, watch-memory, tracing, framebuffer dump. Replatform
  complete.
- **Native window** — `wgpu` video, keyboard/mouse, joystick, Paula audio.
- **Disassembler** — `disasm`/`disasm_around` (68k), conformance-checked against
  the Asm198x ISA spec.

## Peripherals & connectivity

- **Wired now** — DF0 floppy (`ADF` read), mouse (port 0) + buttons, digital
  joystick (port 1) + gamepad, keyboard, Paula audio.
- **Not yet wired** — disk write/save, extra floppies, hard disk (Gayle IDE stub),
  serial host transport, parallel port, Zorro cards beyond fast RAM, RTG, PCMCIA,
  CD32 controller.
- **Internet-capable** — historically yes (AmiTCP/Miami over serial; A2065 Zorro
  Ethernet; PCMCIA Ethernet on A600/A1200; modern PiStorm). **Not started** — the
  serial port has no host transport today.

## Crates

| Crate | Role |
|-------|------|
| `motorola-68000` (+`-68010/-68020/-68030/-68040`, `motorola-68k-common`) | 68000 core + per-variant Deref-wrappers |
| `commodore-agnus-{ocs,ecs,aga}` | DMA slot table, Copper, Blitter, beam |
| `commodore-denise-{ocs,ecs,aga}` (Lisa = AGA) | bitplane → pixel video |
| `commodore-paula-8364` | audio + MFM disk + interrupts + serial + POT |
| `mos-cia-8520` | the two 8520 CIAs |
| `commodore-amiga-autoconfig` / `commodore-gary` / `commodore-gayle` | Zorro-II fast RAM / chip-select / IDE+PCMCIA (stub) |
| `common-commodore-amiga` | memory map, OVL, RTC, the board render loop |
| `format-commodore-amiga-adf` | ADF read |
| `peripheral-commodore-amiga-{floppy,keyboard}` | drive (read + unwired write) + keyboard |
| `machine-commodore-amiga-{ocs,ecs,a1200}` | per-chipset machine wiring (3× tick loops) |
| `runtime-commodore-amiga` / `emu198x-amiga` | shared-shell runtime / native + headless runner + MCP |

## ROMs

Kickstart images at `~/.emu198x/roms/commodore-amiga/` (e.g. `kick13.rom`,
`kick20.rom`, `kick31.rom`). Workbench/app disks as `ADF`.

## Launch

```sh
cargo run --release -p emu198x-amiga --no-default-features -- \
  --model a1200 --kickstart ~/.emu198x/roms/commodore-amiga/kick31.rom \
  --disk workbench31.adf --frames 2500 --screenshot wb.png
```

## Road to 100%

The full tiered breakdown — the honesty tier (AGA render + disk save), the
cycle-exact chipset integration, audio/I-O fidelity, mass-storage + variants, and
the new-machine long tail — is in
[`../../plans/2026-06-08-amiga-100-percent-plan.md`](../../plans/2026-06-08-amiga-100-percent-plan.md).

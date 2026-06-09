# Outstanding Work — Index

**As of 2026-06-09, the per-machine backlogs that used to live here are tracked in
GitHub Issues** on [`emu198x/emu198x`](https://github.com/emu198x/emu198x/issues),
filterable by the `system:<machine>` label, with a code-grounded "road to 100%"
plan per machine under [`../plans/`](../plans/). This document is now the
**family-wide status rollup + an index**, not the per-machine task list.

How the old tags map to issue labels: **L** (hard blocker) → `bug`; **A**
(accuracy/correctness debt) → `accuracy` + `bug`; **S** (scope/breadth) →
`enhancement`. Cross-system chip defects (e.g. the SN76489 noise-tap bug) are
filed once and labelled with every affected system.

## Family-wide status (2026-06-09)

- **Operational parity (Capture + Script + MCP) is landed across the whole
  family** — the 6 primary systems plus every donor extraction (rollout
  `31b49271`→`5cf1a0e2`, 2026-06-02). The one surface that remains is the
  **native `wgpu` window**, which only the 6 primary systems have.
- **Borders** — the TV-visible CRT frame is now rendered across the affected
  chips (TMS9918, Sega VDP, the Atari chips, the inline VIC/VDG/Ace renderers;
  Phase 1.1–1.4). No longer active-area-only.
- **`zilog-z80-ctc` landed 2026-06-03** and is wired into the **Sord M5**, which
  boots through the CTC (BASIC-I `Ready`, Dig Dug). The crate is general
  (4-channel, daisy-chain, both modes); wiring it into Memotech MTX and Tatung
  Einstein is the remaining reuse (each is separate port work — and Einstein is
  also blocked on `western-digital-wd1770`).
- **`western-digital-wd1770` landed 2026-06-06** — a standalone, MAME-faithful
  WD1770 crate (full Type I–IV command set, read/write/multi-sector,
  read-address with real CRC, `INTRQ`/`DRQ`, 12 unit tests). **Tatung Einstein
  disk boot is now blocked only on a disk image** — no Einstein `.dsk` exists in
  the asset tree; the integration test is written and `#[ignore]`d pending one.
- **Highest-leverage unblock now:** source an Einstein disk image (CP/M / Xtal
  DOS) to verify the end-to-end Ctrl-BREAK disk boot.

## Where each system's backlog lives

The four launch systems are also grouped into `<System> 100%` **milestones**; the
others are tracked by `system:` label alone (a milestone is added when a system
becomes an active push). Every system has a road-to-100% plan under `../plans/`.

| System | `system:` label | Milestone | Plan |
|--------|-----------------|-----------|------|
| ZX Spectrum | `system:spectrum` | [Spectrum 100%](https://github.com/emu198x/emu198x/milestone/1) | [spectrum](../plans/2026-06-08-spectrum-100-percent-plan.md) |
| Commodore 64 | `system:c64` | [C64 100%](https://github.com/emu198x/emu198x/milestone/2) | [c64](../plans/2026-06-08-c64-100-percent-plan.md) |
| Nintendo NES | `system:nes` | [NES 100%](https://github.com/emu198x/emu198x/milestone/3) | [nes](../plans/2026-06-08-nes-100-percent-plan.md) |
| Commodore Amiga | `system:amiga` | [Amiga 100%](https://github.com/emu198x/emu198x/milestone/4) | [amiga](../plans/2026-06-08-amiga-100-percent-plan.md) |
| Nintendo Game Boy | `system:game-boy` | — | [game-boy](../plans/2026-06-09-game-boy-100-percent-plan.md) |
| Dragon 32 | `system:dragon` | — | [dragon](../plans/2026-06-09-dragon-100-percent-plan.md) |
| Oric-1 / Atmos | `system:oric` | — | [oric](../plans/2026-06-09-oric-100-percent-plan.md) |
| Atari 2600 | `system:atari-2600` | — | [atari-2600](../plans/2026-06-09-atari-2600-100-percent-plan.md) |
| Atari 5200 | `system:atari-5200` | — | [atari-5200](../plans/2026-06-09-atari-5200-100-percent-plan.md) |
| Atari 7800 | `system:atari-7800` | — | [atari-7800](../plans/2026-06-09-atari-7800-100-percent-plan.md) |
| Atari 800XL | `system:atari-800xl` | — | [atari-800xl](../plans/2026-06-09-atari-800xl-100-percent-plan.md) |
| Jupiter Ace | `system:jupiter-ace` | — | [jupiter-ace](../plans/2026-06-09-jupiter-ace-100-percent-plan.md) |
| Commodore PET | `system:pet` | — | [pet](../plans/2026-06-09-pet-100-percent-plan.md) |
| Commodore VIC-20 | `system:vic-20` | — | [vic-20](../plans/2026-06-09-vic-20-100-percent-plan.md) |
| Acorn Atom | `system:atom` | — | [atom](../plans/2026-06-09-atom-100-percent-plan.md) |
| Memotech MTX | `system:mtx` | — | [mtx](../plans/2026-06-09-mtx-100-percent-plan.md) |
| Sinclair ZX80 | `system:zx80` | — | [zx80](../plans/2026-06-09-zx80-100-percent-plan.md) |
| Sinclair ZX81 | `system:zx81` | — | [zx81](../plans/2026-06-09-zx81-100-percent-plan.md) |
| Acorn BBC Micro | `system:bbc-micro` | — | [bbc-micro](../plans/2026-06-09-bbc-micro-100-percent-plan.md) |
| Acorn Electron | `system:electron` | — | [electron](../plans/2026-06-09-electron-100-percent-plan.md) |
| Tatung Einstein | `system:einstein` | — | [einstein](../plans/2026-06-09-einstein-100-percent-plan.md) |
| Mattel Aquarius | `system:aquarius` | — | [aquarius](../plans/2026-06-09-aquarius-100-percent-plan.md) |
| Spectravideo SVI-328 | `system:svi-328` | — | [svi-328](../plans/2026-06-09-svi-328-100-percent-plan.md) |
| Sega Master System | `system:master-system` | — | [master-system](../plans/2026-06-09-master-system-100-percent-plan.md) |
| Sord M5 | `system:sord-m5` | — | [sord-m5](../plans/2026-06-09-sord-m5-100-percent-plan.md) |
| MSX1 | `system:msx` | — | [msx](../plans/2026-06-09-msx-100-percent-plan.md) |
| Sega SG-1000 / SC-3000 | `system:sg-1000` | — | [sg-1000](../plans/2026-06-09-sg-1000-100-percent-plan.md) |
| ColecoVision | `system:colecovision` | — | [colecovision](../plans/2026-06-09-colecovision-100-percent-plan.md) |

See [`current-system-usability.md`](current-system-usability.md) for the
per-system launch-path / capability-surface view, and the per-system plans for
the tiered road-to-100% breakdown with effort estimates.

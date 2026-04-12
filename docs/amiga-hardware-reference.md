# The Amiga Hardware Reference — A Register-Level Reference for Emulator Authors

*Synthesised from the ten Amiga reference PDFs in `/Users/stevehill/Desktop/AmigaPDFs/txt/`.*

## How to read this document

This is a single, register-level reference for the Amiga custom chips, the two 8520 CIAs, the 68000 bus environment they live in, and the system-wide memory map. It is written for an emulator author who needs every bit of every register, every DMA slot, and every edge of every interrupt line collected in one place, with inline citations back to the primary sources.

It is a companion to [`amiga-boot-process.md`](./amiga-boot-process.md). That document covers power-on through Workbench: reset state, the overlay, Kickstart, Exec, devices, and tasks. This document does **not** re-walk the boot sequence — where reset state matters (e.g. what DMACON contains on cold boot) this document cross-references the boot document and adds only the register-level facts.

Structure:

1. [System memory map](#1-system-memory-map)
2. [Bus architecture and DMA](#2-bus-architecture-and-dma)
3. [Agnus — DMA controller, beam counters, Copper, Blitter, pointers](#3-agnus)
4. [Denise — playfield, sprites, colour, collisions](#4-denise)
5. [Paula — audio, disk, serial, interrupts](#5-paula)
6. [CIA-A ($BFE001, INT2)](#6-cia-a)
7. [CIA-B ($BFD000, INT6)](#7-cia-b)
8. [68000 integration — vectors, autovectors, exceptions, bus sizing](#8-68000-integration)
9. [Interrupts — sources, levels, dispatch, SET/CLR convention](#9-interrupts)
10. [Clocks and timing — master oscillator, color clocks, line/frame structure](#10-clocks-and-timing)
11. [Reset state of every register](#11-reset-state)
12. [Address decode and aliasing](#12-address-decode-and-aliasing)
- Appendix A — [$DFF000 register summary, address order](#appendix-a--dff000-register-summary-address-order)
- Appendix B — [$DFF000 register summary, alphabetical](#appendix-b--dff000-register-summary-alphabetical)
- Appendix C — [CIA-A and CIA-B register summary](#appendix-c--cia-register-summary)
- Appendix D — [68000 exception / interrupt vector table](#appendix-d--68000-exception-and-interrupt-vectors)
- Appendix E — [Reset state table](#appendix-e--reset-state-table)
- [Gaps in corpus](#gaps-in-corpus)
- [Source map](#source-map)

Source abbreviations used inline:

- **HRM** — *Amiga Hardware Reference Manual, 3rd edition*.
- **TRM** — *Commodore Amiga A500/A2000 Technical Reference Manual, 1987*.
- **Mapping** — Thomson/Anderson, *Mapping the Amiga, 2nd edition, 1993*.
- **SPG** — *Amiga System Programmers Guide*, Abacus, 1988.
- **Abacus ML** — *Amiga Machine Language*, Abacus, 1991.
- **Exec RKM** — *ROM Kernel Reference Manual: Exec*.
- **L&D RKM** — *ROM Kernel Reference Manual: Libraries & Devices*.
- **Includes/Autodocs** — *ROM Kernel Reference Manual: Includes & Autodocs*.
- **RKM 3rd** — Beats, *Amiga ROM Kernel Ref 3rd*.

When the corpus only covers some bits of a register, the undocumented bits are flagged as such rather than filled in from outside knowledge. When behaviour depends on OCS vs ECS (Agnus 8361/8367 vs 8372, Denise 8362 vs 8373), that is called out.

A note on OCS/ECS/AGA scope: the corpus thoroughly documents OCS and the ECS extensions that were standard in the A3000 and optional in A500/A2000 (HRM Appendix C). AGA (A1200/A4000 — Lisa, AA Alice, AA Paula) is **not covered** by any of the ten source PDFs. Every register below applies to OCS; ECS extensions are marked `(E)`; AGA is out of scope.

---

## 1. System memory map

The Amiga's 68000 sees a flat 24-bit address space ($000000–$FFFFFF). Everything — chip RAM, custom chip registers, CIAs, expansion, ROM — is memory-mapped. There is no separate I/O space. This section gives the A1000/A500/A2000 map first (the 16 MB classic map), then the A3000 extension into the 32-bit space.

### 1.1 A1000, A500, A2000 memory map

Reproduced from the HRM Appendix D (HRM, "System Memory Maps"):

| Range | Size | Contents |
|-------|------|----------|
| `$00 0000 – $03 FFFF` | 256 KB | Chip RAM. First 256 KB of A500/A2000; the whole of an unexpanded A1000. |
| `$04 0000 – $07 FFFF` | 256 KB | Chip RAM. Second 256 KB (present on stock A500/A2000; optional on A1000). |
| `$08 0000 – $0F FFFF` | 512 KB | Extended chip RAM. Populated on A500/A2000 with 1 MB chip RAM (Fat Agnus). Not accessible to custom chip DMA on an original Agnus (see below). |
| `$10 0000 – $1F FFFF` | 1 MB | Reserved. Do not use. |
| `$20 0000 – $9F FFFF` | 8 MB | Primary Zorro II auto-config space (expansion RAM/I/O). |
| `$A0 0000 – $BE FFFF` | ~2 MB | Reserved. Do not use. |
| `$BF D000 – $BF DFFF` | 4 KB | CIA-B (access only at **even** byte addresses). |
| `$BF E001 – $BF EFFF` | 4 KB | CIA-A (access only at **odd** byte addresses). |
| `$C0 0000 – $D7 FFFF` | 1.5 MB | "Slow" / "Ranger" RAM. Internal expansion memory on some systems. Not chip-accessible. |
| `$D8 0000 – $DB FFFF` | 256 KB | Reserved. (On A2000 with clock, $DC0000–$DCFFFF decodes throughout $D80000–$DBFFFF due to incomplete decoding — see §12.) |
| `$DC 0000 – $DC FFFF` | 64 KB | Real-time clock (not on all systems; typically Oki MSM6242 on A500 clock cartridge, RF5C01A on A2000). |
| `$DD 0000 – $DE FFFF` | 128 KB | Reserved (A3000 uses this for SCSI/motherboard). |
| `$DF F000 – $DF FFFF` | 4 KB | Custom chip registers. **Aliased through the whole $DFF000–$DFFFFF 4K window**; only the low 9 bits of the offset matter (see §12). |
| `$E0 0000 – $E7 FFFF` | 512 KB | Reserved. |
| `$E8 0000 – $E8 FFFF` | 64 KB | Auto-config "nexus". An unconfigured Zorro II PIC with /CFGIN asserted responds here. As each board is configured, the next board in the chain replies here. See §1.3. |
| `$E9 0000 – $EF FFFF` | 448 KB | Secondary auto-config space — 64 KB Zorro II I/O board final addresses. |
| `$F0 0000 – $F7 FFFF` | 512 KB | Reserved on OCS/ECS. Used as "diagnostic ROM" slot on A3000. |
| `$F8 0000 – $FB FFFF` | 256 KB | Reserved on 256 KB Kickstart machines; Kickstart 2.0+ starts here on 512 KB ROM machines. |
| `$FC 0000 – $FF FFFF` | 256 KB | System ROM (Kickstart 1.x, and 2.x if 256 KB; on 512 KB Kickstart this is the top half). On reset, this ROM is also visible at $000000 via the overlay (OVL) — see [boot doc Phase 1](./amiga-boot-process.md#phase-1--overlay-the-reset-vector-and-the-first-cpu-fetch). |

(HRM, Appendix D "System Memory Maps"; TRM §PAL equations for A500 PALs shows literal address decode equations.)

A1000 specifics (TRM, "A1000 WCS"): the A1000 had 256 KB of **writeable control store** instead of ROM. Kickstart was loaded from a boot floppy into WCS at $F80000–$FBFFFF and then write-protected. The boot floppy is outside the scope of this document. For the A1000 the `$FC0000–$FFFFFF` region instead contains a tiny boot ROM that can load the WCS from disk.

### 1.2 A3000 memory map

(HRM Appendix D, "A3000 Memory Map")

| Range | Contents |
|-------|----------|
| `$0000 0000 – $001F FFFF` | 2 MB chip memory (Fat Agnus/ECS Agnus) |
| `$0020 0000 – $009F FFFF` | Zorro II memory expansion space (8 MB) |
| `$00A0 0000 – $00B7 FFFF` | Zorro II I/O expansion |
| `$00B8 0000 – $00BE FFFF` | Reserved |
| `$00BF 0000 – $00BF FFFF` | CIA ports and timers |
| `$00C0 0000 – $00C7 FFFF` | Ranger (slow) expansion memory |
| `$00C8 0000 – $00D7 FFFF` | Reserved |
| `$00D8 0000 – $00DB FFFF` | Reserved |
| `$00DC 0000 – $00DC FFFF` | Memory-mapped clock |
| `$00DD 0000 – $00DD FFFF` | SCSI control (A3000 WD33C93) |
| `$00DE 0000 – $00DE FFFF` | Motherboard resources (Ramsey, DMAC, etc.) |
| `$00DF 0000 – $00DF FFFF` | Amiga chip registers |
| `$00E0 0000 – $00E7 FFFF` | Reserved |
| `$00E8 0000 – $00EF FFFF` | Zorro II I/O & auto-config nexus |
| `$00F0 0000 – $00F7 FFFF` | Diagnostic ROM slot |
| `$00F8 0000 – $00FF FFFF` | 512 KB High ROM (Kickstart 2.x) |
| `$0100 0000 – $03FF FFFF` | Reserved |
| `$0400 0000 – $07FF FFFF` | Motherboard Fast RAM |
| `$0800 0000 – $0FFF FFFF` | Coprocessor slot expansion |
| `$1000 0000 – $7FFF FFFF` | Zorro III expansion |
| `$FF00 0000 – $FF00 FFFF` | Zorro III configuration unit |

The A3000 map is for reference. An OCS-accurate emulator only needs §1.1; an ECS/A3000 emulator needs to respect the high-ROM location at $F80000 and the memory-mapped clock at $DC0000.

### 1.3 Chip, fast and slow RAM — the three performance classes

The HRM and TRM distinguish three kinds of RAM based on which buses they sit on:

- **Chip RAM** (`$000000–$07FFFF`, extended to `$0FFFFF` on 1 MB Agnus, `$1FFFFF` on ECS 2 MB Agnus) — directly accessible to Agnus's DMA. Custom chip DMA can *only* target chip RAM: "The custom chips can only access Chip memory; using a non-Chip address will fail" (HRM Appendix A intro). Bitplane data, copper lists, sprite data, audio data, disk DMA buffers and blitter sources/destinations **must** live here.
- **Fast RAM** (`$200000+` auto-config, or motherboard on A3000) — not on the chip bus. The CPU can run from fast RAM without contending with Agnus DMA, at full 7.16/7.09 MHz. Custom chips cannot touch it.
- **Slow RAM / Ranger RAM** (`$C00000–$D7FFFF`) — sits on the chip bus electrically, but Agnus cannot DMA from it (on OCS; ECS Fat Agnus can use it for chip purposes in some configurations). "On the A500, memory at $000000 is 'slow' RAM (the processor is locked out by the custom chips) rather than fast RAM" (TRM §Internal RAM Expansion on the A500). The A501 512 KB trapdoor expansion in an A500 appears at $C00000, giving 512 KB chip + 512 KB slow.

Emulator note: "slow RAM" is a real thing and it is neither chip nor fast. The CPU contends for cycles against chip bus activity when accessing it, but Agnus cannot read bitplanes from it. An accurate emulator needs a third memory class.

### 1.4 Custom chip register window: `$DFF000`

All Paula/Agnus/Denise registers live at offsets `$000–$1FE` from $DFF000, word-aligned. "For example, for the 68000 to write to ADKCON (address = $09E), the address would be $DFF09E. No other access address is valid" (HRM Appendix A intro).

The 4 KB region $DFF000–$DFFFFF is **mirrored**: only offsets $000–$1FE actually decode, and the chip ignores the high address bits of the offset. In practice this means a write to $DFF200 usually hits the same register as $DFF000 + (x & $1FE). An emulator should mask the offset and flag any access outside the defined range as undefined behaviour — real software should not do it. The full list of registers is in [Appendix A](#appendix-a--dff000-register-summary-address-order).

Read/write asymmetry: each register is *either* read-only *or* write-only, never both. The address *may* be the same (e.g. DMACON at $096 is write, DMACONR at $002 is read — different addresses) or mirrored with the same offset. "If a register is marked as a read-only register, only read its contents. Do not attempt to write to a read-only register, as this will cause unpredictable results. If a register is marked as a write-only register, do not attempt to read from it, as this may trash the register and crash the system." (HRM Appendix A, warning box.)

### 1.5 CIA register window: `$BFD000` and `$BFE001`

The CIAs occupy `$BF 0000–$BF FFFF`, with partial address decoding (TRM PAL equations; HRM Appendix F "Hardware Connection Details"):

> The system hardware selects the CIAs when the upper three address bits are 101. Furthermore, CIAA is selected when A12 is low, A13 high; CIAB is selected when A12 is high, A13 low. CIAA communicates on data bits 7-0, CIAB communicates on data bits 15-8.
>
> Address bits A11, A10, A9, and A8 are used to specify which of the 16 internal registers you want to access. This is indicated by "r" in the address. All other bits are don't cares. So, CIAA is selected by the following binary address: `101x xxxx xx01 rrrr xxxx xxx0`. CIAB address: `101x xxxx xx10 rrrr xxxx xxx1`.
>
> With future expansion in mind, we have decided on the following addresses: CIAA = `$BFExx1`; CIAB = `$BFDxx0`. Software must use byte accesses to these addresses, and no other. (HRM Appendix F.)

Consequences for an emulator:

1. **CIA-A only appears on D7–D0, CIA-B only on D15–D8.** A word read of $BFD000 returns CIA-B in the high byte and floating bus in the low byte (and likewise for a word read of $BFE000 returning CIA-A in the low byte).
2. **Software must use byte accesses.** Word accesses to CIA addresses work but read or write both CIAs if the address decode happens to overlap.
3. **The register offset is in A11–A8**, so CIA-A register 0 ($00 = PRA) is at $BFE001, register 1 ($01 = PRB) is at $BFE101, register 2 ($02 = DDRA) is at $BFE201, and so on in steps of $100. Same structure for CIA-B at $BFDxx0. This gives the "underlined digit chooses which of the 16 internal registers" convention the HRM uses in its memory map.
4. CIA-A is selected with A12=0, A13=1 → register offset $001 LSB (odd byte).
5. CIA-B is selected with A12=1, A13=0 → register offset $000 LSB (even byte).

So every CIA-A address ends in `01` and every CIA-B address ends in `00`. This is how the two chips share the 16-bit data bus without a mux.

### 1.6 ROM and the overlay (OVL)

On reset the OVL line is asserted and the system ROM is mirrored at `$000000` so the 68000's reset vector fetch picks up `SSP` and `PC` from ROM (HRM §"Reset and Early Startup Operation"). `OVL` is an output of CIA-A (PA0 of $BFE001); very early Kickstart code clears it, after which chip RAM is visible at $000000 and ROM is only visible at $F80000/$FC0000. The full sequence is in the [boot process document, Phase 1](./amiga-boot-process.md#phase-1--overlay-the-reset-vector-and-the-first-cpu-fetch).

The A500 PAL equations in the TRM §7.3 make the decode explicit:

```
/ROME = /AS*A23*A22*A21*A20*A19*OVR*PRU          ; $F80000-FFFFFF
      + /AS*A23*A22*A21*/A20*/A19*OVR*PRU        ; $E00000-E7FFFF  (never populated on OCS)
      + /AS*/A23*/A22*/A21*/A20*/A19*OVR*OVL*PRW ; $000000-07FFFF  (overlay active)
      + /AS*/A23*/A22*/A21*A20*A19*OVL*OVR*PRU   ; $180000-1FFFFF  (overlay mirror)
```

Note from this that:

- The ROM natively lives at `$F80000–$FFFFFF`.
- With OVL=1, it is also decoded at `$000000–$07FFFF`.
- There is an additional overlay mirror at `$180000–$1FFFFF` during OVL. That is how a 256 KB ROM covers a 512 KB overlay window.

On 1 MB Kickstart ROMs (Kickstart 2.x/3.x), the ROM covers `$F80000–$FFFFFF` (512 KB) and the overlay scheme mirrors the whole range. On A1000 the ROM at $F80000 is actually WCS and a tiny init ROM sits higher. The HRM does not cover A1000 WCS in detail.

### 1.7 Reserved regions — unused or "guarded"

The TRM §PAL equations and HRM Appendix D both list regions as "Reserved. Do not use." An accurate emulator should:

- Return bus error (BERR) or open-bus-floating-data on reads of unmapped address ranges when `/OVR` is not asserted by any card. On real Amigas, many reserved regions return "last value on the bus" (floating bus), which is why the HRM warns so strongly not to rely on read values from write-only registers (HRM Appendix A).
- Honour the overlay for the whole `$000000–$1FFFFF` range during OVL, regardless of how much RAM is actually installed.

---

## 2. Bus architecture and DMA

### 2.1 The 68000 bus and the chip bus

The 68000 sits on the chip bus, unbuffered, alongside Agnus (which is the master DMA engine) and through Agnus the two video chips (Denise) and the audio/serial/disk/interrupt chip (Paula). CIAs hang off the chip bus via a decoder but are accessed through the E clock, not the main bus.

Key characteristics (HRM §6, "Blitter Hardware: System DMA"; TRM §Timing):

- **68000 runs at 7.15909 MHz on NTSC, 7.09379 MHz on PAL.** "The 68000's system clock speed is 7.15909 MHz on NTSC systems (USA) or 7.09379 MHz on PAL systems (Europe). These speeds can vary when using an external [genlock]" (HRM §1 overview).
- The entire board is synchronous to the master crystal (28.63636 MHz NTSC / 28.37516 MHz PAL) divided down through C1/C2/C3/C4 phase-shifted color clocks. The 68000's 7M clock is derived as `C1 XOR C3*` (TRM §Timing).
- A **memory cycle** is ~280 ns, i.e. one color clock (1/3.579545 MHz NTSC). This is the fundamental slot granularity for chip-bus arbitration.
- 68000 instructions naturally want a bus cycle every other 280 ns slot ("the 68000 uses only the even-numbered memory access cycles"; HRM §6). This is why Agnus can hand alternate slots to DMA without slowing the CPU at all — *most of the time*. The exceptions are documented below.

**Crucial nuance** (HRM §6): the 68000 and Agnus do not use the 68000 bus arbitration protocol (BR/BG/BGACK) for inter-chip bus sharing. Agnus owns the chip bus unconditionally, and **the 68000 only gets a slot when Agnus grants one** via /DTACK. Bus arbitration via BR/BG exists for external Zorro DMA devices but not for on-chip DMA. "The Amiga chips access Chip memory directly via DMA, rather than utilizing traditional bus arbitration mechanisms. Therefore, processor supplied features for multiprocessor support, such as the 68000 TAS (test and set) instruction, cannot serve their intended purpose and are not supported" (HRM §7).

**No `TAS`** — the HRM hard-warns against this (§6 and §7). The read-modify-write cycle does not fit into a DMA slot. An emulator should probably still support it, since software shouldn't use it and the hardware behaviour is "undefined".

### 2.2 DMA slot allocation within a horizontal line

This is the load-bearing diagram in the HRM (Figure 6-9, "DMA Time Slot Allocation"). Reproduced below with the HRM's numbers verbatim.

> "During a horizontal scan line (about 63 microseconds), there are 227.5 'color clocks', or memory access cycles. A memory cycle is approximately 280 ns in duration. The total of 227.5 cycles per horizontal line includes both display time and non-display time. Of this total time, 226 cycles are available to be allocated to the various devices that need memory access." (HRM §6.)

Per-line slot budget:

| Slots | Purpose |
|-------|---------|
| 4     | Memory refresh |
| 3     | Disk DMA |
| 4     | Audio DMA (1 slot × 4 channels, interleaved across line) |
| 16    | Sprite DMA (2 words × 8 sprites) |
| 80    | Bitplane DMA (max, 8 planes × 40 words at normal width; actual number depends on HIRES/LORES and BPU) |
| **107** | **Fixed overhead maximum** (not all slots used every line) |
| remainder | Available to Copper, Blitter, 68000 |

"If a device does not request one of its allocated time slots, the slot is open for other uses. These devices are given first priority because missed DMA cycles can cause lost data, noise in the sound output, or on-screen interruptions." (HRM §6.)

#### Slot layout within the line (color clocks)

From Figure 6-9:

- `$00–$07` — 4 memory refresh slots (spread through this range, interleaved).
- `$08–$0D` — 3 disk DMA slots (only if disk DMA enabled; otherwise open).
- `$0E–$15` — 4 audio DMA slots (1 per channel; only if that channel's DMA is enabled).
- `$15–$27` — 16 sprite DMA slots (2 words × 8 sprites). Sprites fetch their data during horizontal blanking *before* the display; the higher-numbered sprites are latest and can be lost if bitplane DMA starts early.
- `$18` — earliest possible bitplane data-fetch start (DDFSTRT lower limit). Wider-than-normal displays that push DDFSTRT earlier steal the highest-numbered sprites' DMA slots.
- `$28–$D6` — bitplane DMA window (actual start and length depend on DDFSTRT, DDFSTOP, HIRES, BPU). Normal 320-pixel lores display: DDFSTRT=$38, DDFSTOP=$D0, 20-word fetch.
- `$D8` — hardware-enforced bitplane-fetch stop: "A hardware data-fetch stop has been installed at count $D8 so as to prevent the bit plane data fetch from overrunning the time allotted for the memory refresh or disk DMA." (HRM Fig 6-9.)

Even/odd slot convention (HRM §6):

> "The 68000 uses only the even-numbered memory access cycles... The Copper is basically a two-cycle machine that requests the bus only during odd memory cycles (4 memory cycles per instruction). This prevents collisions with display, audio, disk, refresh, and sprites, all of which use only even cycles."

That is, there's a two-beat pattern:

- **Even color clocks** are the default 68000 slot, but are stolen first by bitplane DMA in hi-res and by audio/sprite/disk/refresh in their designated positions. "Sprite DMA (2 words/channel)", audio DMA, disk DMA and refresh all use **even** cycles. Bitplane DMA in lores uses every other even cycle; in hires it uses both even and odd cycles.
- **Odd color clocks** are the Copper's territory and the Blitter's preferred territory. When the CPU is running out of chip RAM and the Copper is idle, the CPU can take odd cycles too (that's how interleaved access to chip RAM approaches full speed when only 1–4 lores bitplanes are displayed).

### 2.3 Priority order

From highest to lowest (HRM §6, "Blitter Operations and System DMA"):

1. **Memory refresh.** Non-negotiable. 4 slots per line.
2. **Disk DMA.** 3 slots per line if `DSKEN` + `DMAEN` and a disk transfer is active.
3. **Audio DMA.** 1 slot per channel per line (channels 0–3) if that channel's `AUDxEN` + `DMAEN` is set and a sample is playing.
4. **Bitplane DMA.** Variable — 0 to 160 slots per line (see §2.4).
5. **Sprite DMA.** 2 words (4 slots counting H and L?) per sprite per line, but *only outside active bitplane fetch*. If bitplane DMA starts early, the sprites with the latest DMA slots (sprite 7 first, then 6, …) lose their fetch and won't display that line.
6. **Copper DMA.** Uses odd cycles, so normally non-interfering. Copper MOVE = 2 instruction words + 2 cycles dead = 4 cycles. Copper WAIT = 6 cycles (it takes one extra cycle to "wake up").
7. **Blitter DMA.** Takes any available chip RAM cycle not claimed above. The blitter can optionally be given full priority over the 68000 (see BLTPRI below).
8. **68000.** Whatever is left. With 0–4 lores planes displayed, effectively all even cycles are available during the display; with 5–6 lores planes, about 50% of even cycles are stolen; with 4 hires planes, *all* even cycles are stolen during active display.

Note the reordering of "display vs sprite" when the display is wider than normal:

> "Display DMA has priority over sprite DMA under certain circumstances... larger displays may block out one or more of the highest-numbered sprites, especially with scrolling." (HRM §6.)

### 2.4 Bitplane DMA cost by mode

From HRM Figures 6-11 and 6-12 and §6 narrative:

| Mode | Planes | Words/line | Even slots stolen | Odd slots stolen | 68000 effective speed |
|------|--------|------------|--------------------|------------------|-----------------------|
| Lores 1–4 planes | 1–4 | 20–80 | some even | 0 | ~100% |
| Lores 5 planes | 5 | 100 | most even | some odd | partial |
| Lores 6 planes | 6 | 120 | all even + half odd | half odd | ~50% during display |
| Hires 1–2 planes | 1–2 | 40–80 | some even + some odd | some | ~100% |
| Hires 3–4 planes | 3–4 | 120–160 | all during display | most | ~0% during display |

> "If you specify four high resolution bitplanes (640 pixels wide), bitplane DMA needs all of the available memory time slots during the display time just to fetch the 40 data words for each line of the four bitplanes (40 * 4 = 160 time slots). This effectively locks out the 68000 (as well as the blitter or Copper) from any memory access during the display, except during horizontal and vertical blanking." (HRM §6.)

The CPU can still run unimpeded in fast RAM or ROM or internal cache during this period — it only blocks on chip-bus references.

### 2.5 The blitter-nasty bit (BLTPRI)

DMACON bit 10 — `BLTPRI`, also called "blitter nasty" or `BLITHOG`:

> "If `DMAF_BLITHOG` is a 1, the blitter will keep the bus for every available Chip memory cycle. This could potentially be every cycle (ROM and Fast memory are not typically Chip memory cycles). If `DMAF_BLITHOG` is a 0, the DMA manager will monitor the 68000 cycle requests. If the 68000 is unsatisfied for three consecutive memory cycles, the blitter will release the bus for one cycle." (HRM §6.)

So:

- `BLTPRI = 1`: blitter grabs every free chip-bus slot. CPU (when running from chip RAM) waits. CPU running from fast RAM is unaffected.
- `BLTPRI = 0` (the friendly default): blitter yields one slot for every three the 68000 is starved of. The CPU trickles along.

Display, disk, and audio DMA always take precedence over the blitter regardless of BLTPRI.

### 2.6 68000 vs chip bus — why the CPU "seems" to run at full speed

> "The 68000 uses only the even-numbered memory access cycles. The 68000 spends about half of a complete processor instruction time doing internal operations and the other half accessing memory. Therefore, the allocation of alternate memory cycles to the 68000 makes it appear to the 68000 that it has the memory all of the time, and it will run at full speed."
>
> "Some 68000 instructions do not match perfectly with the allocation of even cycles and cause cycles to be missed. If cycles are missed, the 68000 must wait until its next available memory slot before continuing. However, most instructions do not cause cycles to be missed, so the 68000 runs at full speed most of the time if there is no blitter DMA interference." (HRM §6.)

**Contention rules for an emulator:**

- On every chip-bus CPU cycle, check the DMA slot map for the current beam position. If the slot is claimed by a higher-priority DMA (refresh, disk, audio, bitplane, sprite), stall the CPU until the next free slot on the opposite phase.
- On fast RAM / ROM / internal cache, no contention — run the CPU at full 7.16/7.09 MHz.
- On slow RAM ($C00000), the CPU is on the chip bus electrically and contends, but Agnus cannot DMA from there, so contention is only for refresh/CPU arbitration.

### 2.7 Copper slot usage

> "The Copper is basically a two-cycle machine that requests the bus only during odd memory cycles (4 memory cycles per instruction). This prevents collisions with display, audio, disk, refresh, and sprites, all of which use only even cycles. It therefore needs (and has) priority over only the blitter and microprocessor." (HRM Appendix A, COPINS.)

MOVE = 4 cycles, WAIT = 6 cycles, SKIP = 4 cycles. Since the Copper only takes odd cycles, a Copper program runs in parallel with bitplane/audio/sprite DMA (which take even cycles) without contention. It does contend with the blitter (also odd-preferring) and the CPU (bot can take odd cycles when idle).

### 2.8 External DMA and the Zorro bus

The Zorro expansion bus exposes 68000-style BR/BG/BGACK. An external DMA device (e.g. a hard disk controller, graphics card) asserts /BR, waits for /BG from the 68000, then asserts /BGACK and owns the bus (TRM §Expansion Bus, "Bus Grant Acknowledge"). When an external master owns the bus, it can target chip RAM, but it still contends with Agnus — Agnus is king of the chip bus slots regardless of 68000 bus mastery.

Note the A2000 has a separate coprocessor slot (86-pin) that bypasses standard BR/BG and hands full bus control to a coprocessor (TRM §A2000 Expansion Bus). An emulator probably doesn't need to model the coprocessor slot unless it targets A2000 + Bridgeboard configurations.

---

## 3. Agnus

Agnus is the master of the chip bus. It contains:

- The **beam counter** (VPOSR/VHPOSR) — the single source of truth for horizontal and vertical position.
- The **DMA controller** — DMACON, all the pointer registers for bitplane/sprite/audio/disk/copper/blitter DMA, and the slot allocation logic described in §2.
- The **Copper** — a tiny DMA-driven coprocessor that reads a list of MOVE/WAIT/SKIP instructions from chip RAM and writes to custom chip registers at precise beam positions.
- The **Blitter** — a programmable bit-blit engine with line-drawing and area-fill modes.
- The **display window logic** — DIWSTRT/DIWSTOP, DDFSTRT/DDFSTOP.
- The **refresh address generator** — REFPTR.

OCS Agnus revisions (HRM Appendix C, ECS section):

| Part | Name | Notes |
|------|------|-------|
| 8361 | NTSC Agnus | Original A1000/A500 NTSC, 512 KB chip only. Reports `$1X` in VPOSR bits 8–14. |
| 8367 | PAL Agnus | Original A500/A2000 PAL. Reports `$0X`. |
| 8370 | Fat NTSC Agnus | 1 MB chip. Reports `$1X`. |
| 8371 | Fat PAL Agnus | 1 MB chip. Reports `$0X`. |
| 8372 | ECS Fat-HR Agnus | 1 MB or 2 MB chip, programmable sync, SuperHires support. Reports `$2X` for PAL, `$3X` for NTSC (HRM Appendix C). |

Detection: read `VPOSR` ($DFF004) bits 8–14 — the values above discriminate. (HRM Appendix C, "Determining Chip Revisions".)

### 3.1 Beam counters: VPOSR, VHPOSR, VPOSW, VHPOSW

| Reg | Addr | R/W | Bits | Notes |
|-----|------|-----|------|-------|
| VPOSR | $004 | R | 15=LOF, 14–8=chip ID (ECS only), 0=V8 | High bit of vertical position + long-frame flag + chip ID on ECS |
| VHPOSR | $006 | R | 15–8=V7–V0, 7–0=H8–H1 | Low 8 vertical, high 8 horizontal (H0 always 0 — 2-pixel granularity) |
| VPOSW | $02A | W | 15=LOF, 0=V8 | Write vertical MSB and frame flop |
| VHPOSW | $02C | W | same as VHPOSR | Writable for test |

Horizontal position `H` counts color clocks within the line. Ranges:

- `$00–$E3` normal NTSC line (228 color clocks on long lines, 227 on short)
- `$00–$E3` PAL (all lines 227 color clocks on PAL)

Vertical position `V` counts scan lines within the field:

- NTSC: 262 lines (short field) or 263 lines (long field)
- PAL: 312 (short) or 313 (long)

The LOF bit in VPOSR toggles each field in interlace; in non-interlace it stays 1 (and all frames are "long"). The copper compares on `V7–V0` only (8 bits), so for positions past 255 you have to WAIT for V=255 then WAIT for a lower V value — HRM §2 gives the idiom.

Important quirk (Mapping, "VHPOSR" entry): when the horizontal counter wraps from 227 back to 0, the vertical counter increments *one color clock before* the line physically starts at H=46. So the coordinate sequence is:

```
... (226,1) (227,1) (0,2) (1,2) ... (45,2) (46,2) ← this is where the beam physically starts line 2
```

This affects Copper WAIT semantics and is a gotcha for anyone writing a "wait for exact screen position" copper.

### 3.2 DMACON / DMACONR

**DMACON** (write, `$096`) and **DMACONR** (read, `$002`). The single control register for all DMA channels and the blitter status read. Uses SET/CLR convention (bit 15 = set if 1, clear if 0; bits written as 0 are unchanged).

| Bit | Name | Description |
|-----|------|-------------|
| 15 | SET/CLR | 1=set selected bits, 0=clear selected bits |
| 14 | BBUSY | **Read-only** Blitter busy (DMACONR) |
| 13 | BZERO | **Read-only** Blitter zero flag — the last blit produced all-zero output (DMACONR) |
| 12 | — | Unused |
| 11 | — | Unused |
| 10 | BLTPRI | Blitter DMA priority (a.k.a. "blitter nasty" / BLITHOG). 1 = blitter gets every free chip slot; 0 = blitter yields every 4th cycle when CPU is starved. |
| 9 | DMAEN | Master DMA enable. If 0, all DMA channels are off regardless of per-channel bits below. |
| 8 | BPLEN | Bitplane DMA enable |
| 7 | COPEN | Copper DMA enable |
| 6 | BLTEN | Blitter DMA enable |
| 5 | SPREN | Sprite DMA enable |
| 4 | DSKEN | Disk DMA enable |
| 3 | AUD3EN | Audio channel 3 DMA enable |
| 2 | AUD2EN | Audio channel 2 DMA enable |
| 1 | AUD1EN | Audio channel 1 DMA enable |
| 0 | AUD0EN | Audio channel 0 DMA enable |

(HRM Appendix A "DMACON", §7 "DMA Control"; Mapping "$DFF096 DMACON".)

The HRM warns to always use the SET/CLR convention — never `MOVE.W #0, DMACON` to clear; that will clear bits 0–13 and set bit 15 to 0, which is fine as "clear all bits written as 1", so actually **writing zero to DMACON does nothing** (no bits selected). The idiom is `MOVE.W #$7FFF, DMACON` to clear everything, or `MOVE.W #$83E0, DMACON` to set specific bits.

Reset state (see §11 and [boot doc](./amiga-boot-process.md#phase-3--very-early-kickstart-silence-the-custom-chips-load-the-vector-table)): DMACON is cleared to 0 at cold reset. Kickstart very-early init sets `MOVE.W #$7FFF, DMACON` to explicitly disable everything before loading copper lists.

### 3.3 Display window and data fetch

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| DIWSTRT | $08E | W | Display window start (V7..V0, H7..H0) — upper-left corner |
| DIWSTOP | $090 | W | Display window stop (V7..V0, H7..H0) — lower-right corner |
| DIWHIGH | $1E4 | W | ECS only — high bits for DIWSTRT/DIWSTOP |
| DDFSTRT | $092 | W | Display data fetch start (H8..H3 only — coarse, 8-clock granularity) |
| DDFSTOP | $094 | W | Display data fetch stop |

(HRM Appendix A, entries "DIWSTRT/DIWSTOP/DDFSTRT/DDFSTOP"; §3 "Playfield Hardware".)

Display window (DIWSTRT/DIWSTOP) controls which *pixels* are painted; display data fetch (DDFSTRT/DDFSTOP) controls which *memory cycles* fetch bitplane data. The two are different because there are 5 color clocks of latency between data fetch and display.

> "Five clocks must occur before the data fetched for a particular position can appear on screen. For example, if data fetch start is $38, data will not be available for display until clock number $45." (HRM Fig 6-9.)

DDFSTRT must be a multiple of 8 color clocks in lores (4 in hires). Hardware clamps DDFSTRT to ≥ $18 (wider than this overruns disk/audio DMA) and DDFSTOP to ≤ $D8.

Standard values (HRM §3):

| Purpose | DDFSTRT | DDFSTOP |
|---------|---------|---------|
| Normal lores (320 px) | `$38` | `$D0` |
| Wide lores | `$30` | `$D8` |
| Extra-wide lores | `$28` | `$D8` |
| Normal hires (640 px) | `$3C` | `$D4` |

DIWSTRT standard for 320×200 NTSC centred: `$2C81` (V=$2C, H=$81). DIWSTOP standard NTSC: `$F4C1` (V=$F4 extended by implicit V8 bit to $1F4 = 500, H=$C1 extended by implicit H8 to $1C1 = 449). PAL DIWSTOP vertical: `$2C` → wrap logic different (HRM §3). The emulator needs to respect the "V8 is implicit 1 if V7≠V8" encoding of DIWSTOP's vertical component.

### 3.4 Bitplane DMA pointers and modulos

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| BPL1PTH/L | $0E0/$0E2 | W | Bitplane 1 pointer (H=hi 3 or 5 bits, L=lo 15 bits). On ECS Agnus the high bits are 5 (supports 2 MB chip). |
| BPL2PTH/L | $0E4/$0E6 | W | Bitplane 2 pointer |
| BPL3PTH/L | $0E8/$0EA | W | Bitplane 3 pointer |
| BPL4PTH/L | $0EC/$0EE | W | Bitplane 4 pointer |
| BPL5PTH/L | $0F0/$0F2 | W | Bitplane 5 pointer |
| BPL6PTH/L | $0F4/$0F6 | W | Bitplane 6 pointer |
| BPL1MOD | $108 | W | Modulo (odd bitplanes 1, 3, 5) — bytes added at end of each scan line |
| BPL2MOD | $10A | W | Modulo (even bitplanes 2, 4, 6) |

(HRM Appendix A "BPLxPTH/L", "BPL1MOD/BPL2MOD".)

Pointer writes must be word-aligned (low bit ignored). "This pointer must be reinitialized by the processor or copper to point to the beginning of bitplane data every vertical blank time." (HRM Appendix A.) Typical pattern: a 2-entry copper list at the start of each field reloads all 6 BPLxPT pointers; between lines Agnus auto-increments them by the word count and adds the modulo.

Odd/even modulo split supports dual-playfield mode, where the two playfields have independent sizes.

### 3.5 Sprite DMA pointers

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| SPR0PTH/L ... SPR7PTH/L | $120–$13E | W | 8 sprite DMA pointers |

Sprites fetch control words (POS, CTL) then data words (DATA, DATB) each scan line. Writing to SPRxDATA arms the sprite; writing to SPRxCTL disables the horizontal comparator (HRM Appendix A "SPRxDATA/SPRxDATB" and "SPRxCTL"):

> "Writing to the A buffer enables (arms) the sprite. Writing to the SPRxCTL register disables the sprite."

Sprite data is fetched during horizontal blank of the line *before* the line the sprite starts on. Positions and sizes are compared to the beam counter in real time.

### 3.6 Disk DMA

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| DSKPTH/L | $020/$022 | W | Disk DMA pointer (18 bits, 20 on ECS) |
| DSKLEN | $024 | W | Disk length + enable + direction |
| DSKDAT | $026 | W (DMA) | Disk DMA data write (dummy address, DMA-only) |
| DSKDATR | $008 | ER (DMA) | Disk DMA data early read (dummy) |
| DSKBYTR | $01A | R | Disk byte ready + status |
| DSKSYNC | $07E | W | Disk sync match word (typically $4489) |
| ADKCON | $09E | W | Audio/Disk control — disk portion (MFMPREC, WORDSYNC, FAST, etc.) |

**DSKLEN** bits (HRM §8; Appendix A):

| Bit | Name | Function |
|-----|------|----------|
| 15 | DMAEN | Enable this transfer |
| 14 | WRITE | 1 = write to disk, 0 = read from disk |
| 13–0 | LENGTH | Number of words to transfer |

**Critical safety sequence** (HRM §8 "Length, Direction, DMA Enable"):

> "The hardware requires a special sequence in order to start DMA to the disk. This sequence prevents accidental writes to the disk. In short, the DMAEN bit in the DSKLEN register must be turned on twice in order to actually enable the disk DMA hardware.
> 1. Enable disk DMA in the DMACON register.
> 2. Set DSKLEN to $4000, thereby forcing the DMA for the disk to be turned off.
> 3. Put the value you want into the DSKLEN register.
> 4. Write this value again into the DSKLEN register. This actually starts the DMA.
> 5. After the DMA is complete, set the DSKLEN register back to $4000."

An emulator must implement this — software written for real hardware will write DSKLEN twice and the second write is the actual trigger. A naive emulator that fires on the first write will break almost everything.

Hardware quirks noted in HRM §8:

- "There is a hardware bug that causes the last three bits of data sent to the disk to be lost."
- "The last word in a disk-read DMA operation may not come in (that is, one less word may be read than you asked for)."

These matter for accurate MFM decoding. A trackdisk device driver works around the first by padding; the second is usually worked around by asking for one extra word.

**DSKSYNC + WORDSYNC** (HRM §8; Appendix A "ADKCON"):

- If `ADKCON` WORDSYNC (bit 10) is set, disk DMA doesn't start until the input stream matches the 16-bit word in DSKSYNC. Once matched, DMA begins from the **following** word (the sync word itself is eaten). Typical value: `$4489` (standard MFM Amiga sync mark).
- While the input matches, the `WORDEQUAL` bit (DSKBYTR bit 12) reads 1. This is a 2 µs window.
- Independently, matching the sync word always sets `DSKSYN` in INTREQ (bit 12 = level 5 interrupt).

**ADKCON disk bits** (HRM Appendix A "ADKCON"):

| Bit | Name | Function |
|-----|------|----------|
| 15 | SET/CLR | SET/CLR (standard) |
| 14–13 | PRECOMP 1–0 | Write precompensation: 00=none, 01=140ns, 10=280ns, 11=560ns |
| 12 | MFMPREC | 1=MFM precomp, 0=GCR precomp |
| 11 | UARTBRK | Forces UART break (TXD=0) |
| 10 | WORDSYNC | Enable disk read sync on DSKSYNC match |
| 9 | MSBSYNC | Enable disk sync on MSB of every byte (GCR) |
| 8 | FAST | 1 = fast (2µs/bit, MFM), 0 = slow (4µs/bit, GCR) |
| 7 | USE3PN | Use audio channel 3 to modulate nothing (no effect) |
| 6 | USE2P3 | Audio ch 2 modulates ch 3 period |
| 5 | USE1P2 | Audio ch 1 modulates ch 2 period |
| 4 | USE0P1 | Audio ch 0 modulates ch 1 period |
| 3 | USE3VN | Audio ch 3 modulates nothing (no effect) |
| 2 | USE2V3 | Audio ch 2 modulates ch 3 volume |
| 1 | USE1V2 | Audio ch 1 modulates ch 2 volume |
| 0 | USE0V1 | Audio ch 0 modulates ch 1 volume |

> "NOTE: If both period and volume are modulated on the same channel, the period and volume will be alternated. First word xxxxxxxx V6-V0, Second word P15-P0 (etc)" (HRM Appendix A.)

(Audio modulation is a rarely-used Paula feature that reinterprets one channel's DMA stream as modulating another channel's period or volume. An emulator needs it for a handful of demo-scene productions.)

**MFM encoding rule** (HRM §8):

```
1 → 01
0 → 10 (if following a 0)
0 → 00 (if following a 1)
```

The raw MFM stream is twice the size of the unencoded data. The blitter can MFM-encode/decode at track-fill speed.

**Low-level floppy format**: the HRM does *not* cover the Amiga standard track format (sector preamble, MFM sync, odd/even interleaved data, checksum). That lives in L&D RKM's trackdisk.device chapter and in Mapping §Hardware. An emulator that models the FDC at the raw-MFM level needs that detail separately — this reference only covers the disk controller registers.

### 3.7 Copper registers

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| COP1LCH/L | $080/$082 | W | Copper list 1 location (18 bits, 20 on ECS) |
| COP2LCH/L | $084/$086 | W | Copper list 2 location |
| COPJMP1 | $088 | S (strobe) | Load PC from COP1LC (jumps Copper to list 1) |
| COPJMP2 | $08A | S (strobe) | Load PC from COP2LC |
| COPINS | $08C | W (DMA) | Copper instruction fetch identifier (DMA-only) |
| COPCON | $02E | W | Copper control — bit 1 `CDANG` grants Copper access to the blitter |

(HRM Appendix A "COP1LC/COP2LC/COPJMP1/COPJMP2/COPINS/COPCON".)

**Copper instructions** (HRM Appendix A "COPINS"):

| Instruction | Cycles | IR1 bit 0 | IR2 bit 0 |
|-------------|--------|-----------|-----------|
| MOVE immediate | 4 (2 bus + 2 dead) | 0 | — |
| WAIT until | 6 | 1 | 0 |
| SKIP if | 4 | 1 | 1 |

MOVE IR1: `DA8..DA1` = destination address (bits 8–1 of $DFFxxx), IR2 = RAM data.
WAIT IR1: `VP7..VP0 HP8..HP1 1`; IR2 = `BFD VE6..VE0 HE8..HE1 0`.
SKIP IR1: same as WAIT; IR2 bit 0 = 1.

`BFD` = Blitter Finished Disable. If 0, the WAIT/SKIP also waits for the blitter to finish.

Only `DA8..DA1` bits of the destination matter — addresses below $080 (the Copper itself, DMACON, etc) are **not normally writable by the Copper unless CDANG is set**:

> "This is a 1-bit register that when set true, allows the Copper to access the blitter hardware. This bit is cleared by power-on reset, so that the Copper cannot access the blitter hardware." (HRM Appendix A "COPCON".)

With CDANG=0, the Copper can write registers with offsets ≥ $080 (from BLTCON0 onwards), but not blitter registers at $040–$07E. With CDANG=1, Copper can drive the blitter. HRM Appendix B flags these with `+` for "not writable by Copper unless CDANG set".

**Copper auto-restart at vertical blank:**

> "At the start of each vertical blanking interval, COP1LC is automatically used to start the program counter. That is, no matter what the Copper is doing, when the end of vertical blanking occurs, the Copper is automatically forced to restart its operations at the address contained in COP1LC." (HRM §2.)

This is the single most important piece of Copper behaviour for an emulator. Most OS copper lists never use COPJMP2 — they just let vertical blank reload COP1LC automatically.

### 3.8 Blitter registers

The blitter is an Agnus subsystem with its own set of registers at `$040–$076`. Everything is clustered close together to make blitter setup fast.

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| BLTCON0 | $040 | W | Control 0: shift A, USE flags, LF logic function |
| BLTCON1 | $042 | W | Control 1: shift B, fill modes, line mode flags |
| BLTAFWM | $044 | W | First-word mask for source A |
| BLTALWM | $046 | W | Last-word mask for source A |
| BLTCPTH/L | $048/$04A | W | Source C pointer |
| BLTBPTH/L | $04C/$04E | W | Source B pointer |
| BLTAPTH/L | $050/$052 | W | Source A pointer |
| BLTDPTH/L | $054/$056 | W | Destination D pointer |
| BLTSIZE | $058 | W | Size — **writing this starts the blit** |
| BLTCON0L | $05A | W | ECS only — lower 8 bits of BLTCON0 for 11-bit width |
| BLTSIZV | $05C | W | ECS only — 15-bit vertical size |
| BLTSIZH | $05E | W | ECS only — 11-bit horizontal size, **writing starts the blit on ECS** |
| BLTCMOD | $060 | W | Source C modulo |
| BLTBMOD | $062 | W | Source B modulo |
| BLTAMOD | $064 | W | Source A modulo |
| BLTDMOD | $066 | W | Destination D modulo |
| BLTCDAT | $070 | W (DMA) | Source C data register (DMA-loaded; can preload manually) |
| BLTBDAT | $072 | W (DMA) | Source B data |
| BLTADAT | $074 | W (DMA) | Source A data |
| BLTDDAT | $000 | ER | Destination data (DMA, dummy read address) |

(HRM §6 "Blitter Hardware", Appendix A "BLTxxx".)

**BLTCON0** area mode:

| Bit | Name | |
|-----|------|--|
| 15–12 | ASH3–ASH0 | A source shift value (0–15, shifts right in ascending mode) |
| 11 | USEA | Use source A |
| 10 | USEB | Use source B |
| 9 | USEC | Use source C |
| 8 | USED | Use destination D |
| 7–0 | LF7–LF0 | Logic function minterm select |

**BLTCON1** area mode:

| Bit | Name | |
|-----|------|--|
| 15–12 | BSH3–BSH0 | B source shift value |
| 7 | DOFF | Destination data-off (disable writes) |
| 4 | EFE | Exclusive fill enable |
| 3 | IFE | Inclusive fill enable |
| 2 | FCI | Fill carry input |
| 1 | DESC | Descending mode |
| 0 | LINE | 0 = area mode (the area-mode interpretation) |

**BLTCON0 line mode** (BLTCON1 LINE=1):

| Bit | BLTCON0 | BLTCON1 |
|-----|---------|---------|
| 15–12 | START3–START0 | TEXTURE3–TEXTURE0 |
| 11 | 1 | 0 |
| 10 | 0 | 0 |
| 9 | 1 | 0 |
| 8 | 1 | 0 |
| 7 | LF7 | 0 |
| 6 | LF6 | SIGN |
| 5 | LF5 | 0 (reserved) |
| 4 | LF4 | SUD |
| 3 | LF3 | SUL |
| 2 | LF2 | AUL |
| 1 | LF1 | SING |
| 0 | LF0 | LINE (=1) |

LF field for line-draw must be `$4A` (selects `D = (A & ~C) | (~A & C)` — i.e. XOR with the texture bit).

Octant select from SUD/SUL/AUL (HRM Appendix A):

| Octant | SUD | SUL | AUL |
|--------|-----|-----|-----|
| 0 | 1 | 1 | 0 |
| 1 | 0 | 0 | 1 |
| 2 | 0 | 1 | 1 |
| 3 | 1 | 1 | 1 |
| 4 | 1 | 0 | 1 |
| 5 | 0 | 1 | 0 |
| 6 | 0 | 0 | 0 |
| 7 | 1 | 0 | 0 |

**BLTSIZE**:

```
BIT# 15 14 13 12 11 10 09 08 07 06 05 04 03 02 01 00
     h9 h8 h7 h6 h5 h4 h3 h2 h1 h0 w5 w4 w3 w2 w1 w0
```

h = height in pixels/lines (10 bits = 1024 max). w = width in words (6 bits = 64 words = 1024 pixels max). A height of 0 is treated as 1024; a width of 0 as 64.

Writing BLTSIZE starts the blit. This is the last write — the HRM's "key points" list leads with it (HRM §6). On ECS, BLTSIZV (vertical, 15 bits, 32 767 max) and BLTSIZH (horizontal, 11 bits) replace BLTSIZE, and writing BLTSIZH triggers the blit instead.

**Blitter behaviour notes** (HRM §6, "Blitter Key Points"):

- Modulos and pointers are in **bytes**; width is in words; height is in pixels. LSB of pointers/modulos ignored.
- Order of operations: masking, shifting, logical combination of sources, area fill, zero flag setting.
- Ascending mode: increment pointers, add modulos, shift right.
- Descending mode: decrement pointers, subtract modulos, shift left.
- **Area fill only works correctly in descending mode.**
- Always check BLTDONE (DMACONR bit 14, BBUSY) before writing blitter registers.
- BZERO (DMACONR bit 13) = 1 if the last blit produced all-zero output (useful for collision detection).

**Completion interrupt**: the blitter sets INTREQ bit 6 (BLIT, level 3) when BBUSY transitions from 1 to 0.

**Line-draw register preload** (HRM Appendix A "BLTxDAT/BLTxMOD/BLTxPTH"):

- BLTADAT = `$8000` (single-bit index register)
- BLTBDAT = `$FFFF` (or texture pattern)
- BLTAMOD = `4*(dY-dX)` (slope storage)
- BLTBMOD = `4*dY`
- BLTAPTL = `2*dY - dX` (accumulator; the Bresenham error term)
- BLTCPT / BLTDPT = starting address of the line
- BLTCMOD = BLTDMOD = byte-width of destination image
- BLTSIZE width = 2 (mandatory for line mode)
- BLTSIZE height = line length in pixels

### 3.9 Refresh

`REFPTR` at `$028` is the dynamic RAM refresh address generator. Writable only for testing:

> "This register is used as a dynamic RAM refresh address generator. It is writeable for test purposes only, and should never be written by the microprocessor." (HRM Appendix A.)

Agnus uses it to generate 4 CAS-before-RAS refresh cycles per scan line. An emulator does not need to model refresh except as a slot-taker in the DMA allocation.

### 3.10 Agnus-owned registers summary

Every register at `$DFF000` involving `A` in the HRM Appendix A/B "Agnus/Denise/Paula" column:

Agnus **only** (A): DMACON (shared with Denise/Paula), VPOSR/W, VHPOSR/W, DSKPT, DSKLEN, REFPTR, COP1LC/COP2LC/COPJMP1/COPJMP2/COPINS, COPCON, DIWSTRT/DIWSTOP/DIWHIGH, DDFSTRT/DDFSTOP, BLTCON0/1/0L, BLTAFWM/BLTALWM, BLTxPT, BLTSIZE/BLTSIZV/BLTSIZH, BLTxMOD, BLTxDAT, BLTDDAT, BPLxPT, BPL1MOD, BPL2MOD, SPRxPT, AUDxLCH/L, STREQU, STRVBL, STRHOR, STRLONG, HTOTAL, HSSTOP, HBSTRT, HBSTOP, VTOTAL, VSSTOP, VBSTRT, VBSTOP, BEAMCON0, HSSTRT, VSSTRT, HCENTER.

Agnus+Denise (AD): BPLCON0 (Agnus uses HIRES/BPU/LACE for DMA; Denise uses the same register for display), SPRxPOS, SPRxCTL, STRHOR (shared).

Agnus+Paula (AP): DMACON (DMA state, shared).

The authoritative mapping is in [Appendix A](#appendix-a--dff000-register-summary-address-order).

---

## 4. Denise

Denise is the video output chip: playfield serialiser, sprites, color palette, collision detection. It does not do DMA directly — Agnus DMA's the data into Denise's data registers (BPLxDAT, SPRxDATA/B) and Denise serialises them into the RGB output.

**Revisions:**

- `8362` — original OCS Denise.
- `8373` — ECS Denise with SuperHires, DENISEID register, new genlock features, BPLCON3.

Detection: read `DENISEID` at `$07C`. On OCS Denise this address is floating-bus garbage; on ECS Denise it returns `$00FC` (HRM Appendix C, "Determining Chip Revisions"). Because floating-bus garbage is not deterministic, software usually checks AgnusID (VPOSR bits 14–8) first.

### 4.1 BPLCON0 — main video control

`$100`, W, A+D(E).

| Bit | Name (OCS) | Name (ECS extra) | Function |
|-----|------------|------------------|----------|
| 15 | HIRES | | 1 = high-res (70 ns pixels, 640 wide), 0 = lo-res (140 ns, 320 wide) |
| 14 | BPU2 | | Bitplane-use bits 2 |
| 13 | BPU1 | | ... bit 1 |
| 12 | BPU0 | | ... bit 0 (000 = 0 planes, 110 = 6 planes; 111 = reserved on OCS) |
| 11 | HAM (HOMOD) | | 1 = Hold-and-modify mode. If BPU=6 and HAM=0, selects EHB mode; if BPU=6 and HAM=1, selects HAM mode. |
| 10 | DBLPF | | 1 = dual playfield mode (odd planes=PF1, even=PF2) |
| 9 | COLOR | | 1 = composite color enable |
| 8 | GAUD | | 1 = genlock audio enable (muxed on BKGND pin during vblank) |
| 7 | — | SHRES | ECS SuperHires enable (must have HIRES=0 and BPU ≤ 2) |
| 6 | — | BPLHWRM | ECS bitplane hardware write marker |
| 5 | — | SPRHWRM | ECS sprite hardware write marker |
| 4 | — | | — |
| 3 | LPEN | | Light pen enable (when set, VPOSR/VHPOSR latches to light pen position) |
| 2 | LACE | | 1 = interlace mode |
| 1 | ERSY | | 1 = external resync — HSYNC/VSYNC pads become inputs (genlock) |
| 0 | — | | — |

(HRM Appendix A "BPLCON0", §3 "Playfield Hardware".)

**BPU encoding** (HRM §3):

| BPU2 BPU1 BPU0 | Planes | Max colors (lores) |
|----------------|--------|---------------------|
| 000 | 0 | background only |
| 001 | 1 | 2 |
| 010 | 2 | 4 |
| 011 | 3 | 8 |
| 100 | 4 | 16 |
| 101 | 5 | 32 (or EHB, 64) |
| 110 | 6 | 64 (HAM only — 4096 HAM colors) |
| 111 | — | reserved |

**EHB** (Extra Half-Brite): BPU=6 with HAM=0, HIRES=0, DBLPF=0 → bitplane 6 acts as a brightness modifier, halving the colour of the base palette entry. Gives 32 normal colours + 32 half-bright colours = 64. (HRM §3 "Playfield Hardware".)

**HAM** (Hold-and-Modify): BPU=6 with HAM=1 → bitplanes 5–6 define an operation on bitplanes 1–4:

- `00` — use bitplanes 1–4 as an index into COLOR00–COLOR15 (a 16-colour normal palette).
- `01` — Hold R and G, modify B from bitplanes 1–4 (4-bit new B value).
- `10` — Hold G and B, modify R.
- `11` — Hold R and B, modify G.

Each pixel gets its value from the previous pixel's RGB with one component updated. This gives 4096 on-screen colours but at a per-channel-per-pixel cost.

**Dual playfield**: DBLPF=1 splits the 6 bitplanes into two 3-plane playfields (PF1 = planes 1,3,5 → COLOR00–COLOR07; PF2 = planes 2,4,6 → COLOR08–COLOR15). Priority of PF1 vs PF2 is set by PF2PRI in BPLCON2.

### 4.2 BPLCON1 — horizontal scroll

`$102`, W, D.

| Bit | Name | Function |
|-----|------|----------|
| 7 | PF2H3 | Playfield 2 horiz scroll bit 3 |
| 6 | PF2H2 | ... bit 2 |
| 5 | PF2H1 | ... bit 1 |
| 4 | PF2H0 | ... bit 0 |
| 3 | PF1H3 | Playfield 1 horiz scroll bit 3 |
| 2 | PF1H2 | ... bit 2 |
| 1 | PF1H1 | ... bit 1 |
| 0 | PF1H0 | ... bit 0 |

4-bit horizontal scroll per playfield. Value is in lores pixels (0–15). In hires it shifts by half-pixels (0–7 real hires pixels).

### 4.3 BPLCON2 — playfield and sprite priorities

`$104`, W, D(E).

| Bit | Name | Function |
|-----|------|----------|
| 6 | PF2PRI | 1 = Playfield 2 (even planes) appears in front of PF1 |
| 5 | PF2P2 | Playfield 2 priority (vs sprites) bit 2 |
| 4 | PF2P1 | ... bit 1 |
| 3 | PF2P0 | ... bit 0 |
| 2 | PF1P2 | Playfield 1 priority bit 2 |
| 1 | PF1P1 | ... bit 1 |
| 0 | PF1P0 | ... bit 0 |

PF1P and PF2P are 3-bit codes (0–7) indicating which sprite pair the playfield passes in front of:

- 0 — playfield in front of sprites 0–7
- 1 — in front of sprites 2–7 (i.e. sprite pair 0/1 in front of this playfield)
- 2 — in front of sprites 4–7
- 3 — in front of sprites 6/7
- 4 — behind all sprites
- 5–7 — reserved (behind all sprites)

(HRM §4 "Sprite Hardware".)

### 4.4 BPLCON3 (ECS only)

`$106`, W, D(E). Genlock and SuperHires extras. The HRM documents individual bits only briefly; full bit layout is in HRM Appendix C with fields for chromakey, bitplane key, border transparent, loct/high color select, and bank select for the 32 colour registers. Not fully reproduced — see HRM Appendix C "Enhanced Chip Set" for the ECS Denise details if you need them.

### 4.5 Colour palette

`COLOR00`–`COLOR31` at `$180–$1BE`, W, D.

Each register is 16 bits: `0000 R3 R2 R1 R0 G3 G2 G1 G0 B3 B2 B1 B0`. 12-bit colour (4 bits per channel, 4096 possible colours).

There are 32 registers, used as:

- Normal modes: 32 colours (or up to the active BPU max).
- EHB: 32 base + 32 half-bright.
- HAM: 16 base colours in COLOR00–COLOR15 used as HAM-mode base.
- Dual playfield: PF1 → COLOR00–COLOR07, PF2 → COLOR08–COLOR15.
- Sprites: each sprite pair uses 3 colours from a group of 4 (COLOR17–19, 21–23, 25–27, 29–31), with COLOR00 shared as transparent (sprite colour 0 = transparent).

### 4.6 Sprite data registers

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| SPRxPOS | $140, $148, ..., $178 | W | Vertical/horizontal start position |
| SPRxCTL | $142, $14A, ..., $17A | W | Vertical stop + high bits + attach flag |
| SPRxDATA | $144, $14C, ..., $17C | W | Image data A (arms the sprite) |
| SPRxDATB | $146, $14E, ..., $17E | W | Image data B |

**SPRxPOS**:

- Bits 15–8: SV7..SV0 (start vertical, low 8 bits)
- Bits 7–0: SH8..SH1 (start horizontal, 8 bits; low bit in SPRxCTL)

**SPRxCTL**:

- Bits 15–8: EV7..EV0 (end vertical)
- Bit 7: ATT — attach bit (odd sprites only). Attached pairs become a single 16-colour sprite.
- Bits 6–4: unused
- Bit 2: SV8 — start vertical high bit
- Bit 1: EV8 — end vertical high bit
- Bit 0: SH0 — start horizontal low bit

Sprites are 16 pixels wide (lores pixels) × (stop-start) lines tall. Writing SPRxDATA arms the sprite for the current line; writing SPRxCTL disarms it. DMA-driven sprites get their POS/CTL/DATA/DATB loaded from chip RAM via the sprite pointer registers each scan line.

**Attach mode**: setting ATT in an odd-numbered sprite's CTL register combines it with the even sprite (0+1, 2+3, 4+5, 6+7) into a 16-colour 16-pixel sprite using colours COLOR17–31.

### 4.7 Collision detection

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| CLXCON | $098 | W | Collision control |
| CLXDAT | $00E | R | Collision data (read and clear) |

**CLXCON** bits (HRM Appendix A):

| Bit | Name | Function |
|-----|------|----------|
| 15 | ENSP7 | Enable sprite 7 (ORed with sprite 6) |
| 14 | ENSP5 | Enable sprite 5 (ORed with sprite 4) |
| 13 | ENSP3 | Enable sprite 3 (ORed with sprite 2) |
| 12 | ENSP1 | Enable sprite 1 (ORed with sprite 0) |
| 11 | ENBP6 | Enable bitplane 6 (match required) |
| 10 | ENBP5 | ... |
| 9 | ENBP4 | ... |
| 8 | ENBP3 | ... |
| 7 | ENBP2 | ... |
| 6 | ENBP1 | ... |
| 5 | MVBP6 | Match value for bitplane 6 |
| 4 | MVBP5 | ... |
| 3 | MVBP4 | ... |
| 2 | MVBP3 | ... |
| 1 | MVBP2 | ... |
| 0 | MVBP1 | ... |

> "NOTE: Disabled bitplanes cannot prevent collisions. Therefore if all bitplanes are disabled, collisions will be continuous, regardless of the match values." (HRM Appendix A.)

**CLXDAT** bits (HRM Appendix A):

| Bit | Collision |
|-----|-----------|
| 15 | (unused) |
| 14 | Sprite 4/5 ↔ sprite 6/7 |
| 13 | Sprite 2/3 ↔ sprite 6/7 |
| 12 | Sprite 2/3 ↔ sprite 4/5 |
| 11 | Sprite 0/1 ↔ sprite 6/7 |
| 10 | Sprite 0/1 ↔ sprite 4/5 |
| 9 | Sprite 0/1 ↔ sprite 2/3 |
| 8 | Playfield 2 ↔ sprite 6/7 |
| 7 | Playfield 2 ↔ sprite 4/5 |
| 6 | Playfield 2 ↔ sprite 2/3 |
| 5 | Playfield 2 ↔ sprite 0/1 |
| 4 | Playfield 1 ↔ sprite 6/7 |
| 3 | Playfield 1 ↔ sprite 4/5 |
| 2 | Playfield 1 ↔ sprite 2/3 |
| 1 | Playfield 1 ↔ sprite 0/1 |
| 0 | Playfield 1 ↔ playfield 2 |

**CLXDAT is read-and-clear**: reading it returns the current collision state and atomically clears it. There is no interrupt — collisions must be polled (typically once per VBlank).

Sprite pairs (0+1, 2+3, 4+5, 6+7) are collision-ORed — you cannot distinguish odd and even sprites within a pair for collision purposes.

### 4.8 JOYxDAT, POTxDAT, JOYTEST — input

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| JOY0DAT | $00A | R | Joystick/mouse 0 data (left port): Y7..Y0 H | X7..X0 L |
| JOY1DAT | $00C | R | Joystick/mouse 1 data (right port) |
| JOYTEST | $036 | W | Write all four counters simultaneously (test) |
| POT0DAT | $012 | R | Pot counter pair 0 (paddles/light pen) |
| POT1DAT | $014 | R | Pot counter pair 1 |
| POTGO | $034 | W | Start pot counters + 4-bit I/O port |
| POTGOR | $016 | R | Pot port data read |

JOYxDAT returns two 8-bit mouse counters clocked by quadrature signals from the pin pair. For joystick mode, the fire buttons come through **CIA-A** PRA bits 6 and 7 (not through Denise). To detect joystick directions from JOYxDAT, XOR the low bit with the next bit (because quadrature):

```
Forward = Y1 xor Y0 (bit 9 xor bit 8)
Back    = X1 xor X0 (bit 1 xor bit 0)
Left    = Y1
Right   = X1
```

(HRM Appendix A "JOY0DAT".)

Pot counters are 8-bit counters that are reset by POTGO and count until the pot pin goes low (via the charge curve of an external capacitor+pot). This is how paddle controllers, the light pen, and the Amiga's funny extended joystick modes work. POTGO bit 0 = START, bits 15–8 = output enable + data for the 4 pot pins (used as 4-bit GPIO).

---

## 5. Paula

Paula is the I/O chip: audio, disk (low-level MFM controller), serial UART, and the interrupt controller.

The interrupt controller is the most central role. All on-chip interrupts route through Paula's INTREQ register, which in turn drives the 68000's IPL2..IPL0 lines via priority encoding. Section 8 and 9 cover this in detail.

### 5.1 Audio channels

Four identical channels. Each has five registers:

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| AUDxLCH | $0A0, $0B0, $0C0, $0D0 | W (A, DMA) | Sample location high (3 bits OCS, 5 bits ECS) |
| AUDxLCL | $0A2, $0B2, $0C2, $0D2 | W (A, DMA) | Sample location low (15 bits) |
| AUDxLEN | $0A4, $0B4, $0C4, $0D4 | W (P) | Sample length in **words** |
| AUDxPER | $0A6, $0B6, $0C6, $0D6 | W (P) | Sample period in color clocks |
| AUDxVOL | $0A8, $0B8, $0C8, $0D8 | W (P) | Volume (0–64) |
| AUDxDAT | $0AA, $0BA, $0CA, $0DA | W (DMA, P) | Sample data register (DMA-loaded; can be written directly) |

(HRM Appendix A "AUDxLCH/L/LEN/PER/VOL/DAT", §5 "Audio Hardware".)

Addresses: `x` channel is at offset `$A0 + x*$10`.

**AUDxPER** — period in color clock ticks. At PAL 3.546895 MHz color clock, period 124 → 28.6 kHz max sample rate. The HRM is explicit:

> "The minimum period is 124 color clocks. This means that the smallest number that should be placed in this register is 124 decimal. This corresponds to a maximum sample frequency of 28.86 khz." (HRM Appendix A "AUDxPER".)

**AUDxVOL** bits:

| Bit | Name |
|-----|------|
| 6 | Maximum (force all-ones output) |
| 5–0 | 6-bit linear volume (0–63) |

So 64 total levels (0–63, plus a "force max" alias at bit 6 = 64).

**Sample format**: 2's complement signed 8-bit samples, two per DMA word (MSB first). "It contains 2 bytes of data that are each 2's complement and are outputted sequentially (with digital-to-analog conversion) to the audio output pins. (LSB = 3 mV)" (HRM Appendix A "AUDxDAT".)

**DMA vs CPU modes:**

- DMA mode (DMAEN + AUDxEN set): Agnus fetches one word per audio slot per line using AUDxLC as the current pointer. Each word contains 2 samples, played at AUDxPER interval. When the word is used up, another slot is consumed.
- Manual/CPU mode: software writes AUDxDAT directly, triggered by the level-4 "audio done" interrupt.

**AUD interrupt generation**:

> "This level 4 interrupt signals 'audio block done.' When the audio DMA is operating in automatic mode, this interrupt occurs when the last word in an audio data stream has been accessed. In manual mode, it occurs when the audio data register is ready to accept another word of data." (HRM §7.)

AUDxLC is *not* a pointer — it's a latch. Paula has an internal pointer that is *loaded* from AUDxLC at the start of each sample cycle, and then auto-increments from there. You only need to write AUDxLC once, not every frame, unlike bitplane pointers. (HRM Appendix A "AUDxLCH".)

**Audio modulation** (via ADKCON USE{x}{P|V}{x+1}):

If USE0V1 is set, channel 0's DMA data is reinterpreted as volume values for channel 1 (bits 6..0 of each byte). If USE0P1 is set, channel 0's DMA data is reinterpreted as period values for channel 1. If *both*, period and volume alternate: first word is volume, second word is period, etc. (HRM Appendix A "ADKCON".)

This is how, for example, the Amiga can produce pitch envelopes by chaining channel 0 → channel 1's period and channel 2 → channel 1's volume.

### 5.2 Disk controller

Registers DSKPT/DSKLEN/DSKDAT/DSKDATR/DSKBYTR/DSKSYNC/ADKCON: already covered in §3.6 (Agnus owns DSKPT and DSKLEN — because they're DMA control; Paula owns DSKBYTR, DSKDAT, DSKDATR, DSKSYNC and the disk portion of ADKCON). The disk DMA read/write state machine is Paula's.

CIA-B owns the step, direction, side, select and motor outputs (see §7). CIA-A owns the write-protect, ready, track-0, disk-change inputs (see §6).

### 5.3 Serial UART

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| SERDAT | $030 | W | Transmit buffer + stop bit |
| SERDATR | $018 | R | Receive buffer + status |
| SERPER | $032 | W | Baud rate + LONG bit (9-bit mode) |

**SERDAT** format:

```
BIT#   15 14 13 12 11 10 09 08 07 06 05 04 03 02 01 00
USE    0  0  0  0  0  0  S  D8 D7 D6 D5 D4 D3 D2 D1 D0
```

`S` = stop bit (must be 1). `D8..D0` = up to 9 data bits. The **position of the stop bit** encodes the word length — the highest bit that is 1 when stop is set is interpreted as the stop bit; everything below that is data.

**SERDATR** bits:

| Bit | Name | Function |
|-----|------|----------|
| 15 | OVRUN | Receiver overrun (clear by resetting INTREQ bit 11) |
| 14 | RBF | Receive buffer full (mirror of INTREQ bit 11) |
| 13 | TBE | Transmit buffer empty (mirror of INTREQ bit 0) |
| 12 | TSRE | Transmit shift register empty |
| 11 | RXD | Raw RXD pin state (for software bit-banging) |
| 10 | — | unused |
| 9 | STP | Stop bit |
| 8 | STP-DB8 | Stop bit if LONG, data bit 8 if not |
| 7–0 | DB7–DB0 | Data bits |

**SERPER** format:

```
BIT#   15       14..0
USE    LONG     RATE (15-bit period)
```

Baud rate = `1 / ((N+1) × 0.2794 µs)` where N = RATE. (HRM Appendix A.) The 0.2794 µs is the PAL color-clock period (derived from the 3.546895 MHz clock).

**Serial interrupts**:

- `TBE` (INTREQ bit 0, level 1) — transmit buffer empty, ready for next word.
- `RBF` (INTREQ bit 11, level 5) — receive buffer full, new word ready.

### 5.4 INTENA / INTENAR / INTREQ / INTREQR

The interrupt mask and request registers. Both use SET/CLR convention.

| Reg | Addr | R/W | Function |
|-----|------|-----|----------|
| INTENA | $09A | W | Interrupt enable (mask) |
| INTENAR | $01C | R | Interrupt enable read |
| INTREQ | $09C | W | Interrupt request (software set/clear) |
| INTREQR | $01E | R | Interrupt request read |

Shared bit layout (HRM Appendix A "INTENA/INTREQ", §7 "Interrupts"):

| Bit | Name | 68K lvl | Source |
|-----|------|---------|--------|
| 15 | SET/CLR | — | 1=set, 0=clear |
| 14 | INTEN | — | **Master enable**. If 0, all interrupts disabled. Creates no request itself. |
| 13 | EXTER | 6 | External INT6* (from CIA-B and Zorro /INT6) |
| 12 | DSKSYN | 5 | Disk sync pattern found |
| 11 | RBF | 5 | Serial receive buffer full |
| 10 | AUD3 | 4 | Audio channel 3 block finished |
| 9 | AUD2 | 4 | Audio channel 2 block finished |
| 8 | AUD1 | 4 | Audio channel 1 block finished |
| 7 | AUD0 | 4 | Audio channel 0 block finished |
| 6 | BLIT | 3 | Blitter finished |
| 5 | VERTB | 3 | Vertical blank (start of frame, line 0) |
| 4 | COPER | 3 | Copper interrupt (Copper can write INTREQ to raise itself) |
| 3 | PORTS | 2 | External INT2* (from CIA-A and Zorro /INT2) |
| 2 | SOFT | 1 | Software interrupt (Exec's SoftInt mechanism) |
| 1 | DSKBLK | 1 | Disk block finished (DMA count = 0) |
| 0 | TBE | 1 | Serial transmit buffer empty |

**Level sharing**: level 1 has three sources (SOFT, DSKBLK, TBE), level 3 has three (BLIT, VERTB, COPER), level 4 has four (AUD0–3), level 5 has two (DSKSYN, RBF). The 68000 takes one vector for each level (autovectors); the handler reads INTREQR to determine the source(s). See §9 for the dispatch algorithm.

**Master enable (INTEN)**: "This bit is used for enable/disable only. It creates no interrupt request." (HRM §7.) Setting INTEN=0 is the primary "defer all interrupts" mechanism; Kickstart toggles this around time-critical operations.

**To set bits**: `MOVE.W #$C020,INTENA` sets INTEN (bit 14) and VERTB (bit 5). SET/CLR (bit 15) is 1, the bits to set are 1, and the rest are 0.

**To clear bits**: `MOVE.W #$7FFF,INTENA` clears all bits. SET/CLR is 0, and all 15 selector bits are 1, meaning "clear all these".

**To acknowledge an interrupt**: write to INTREQ with SET/CLR=0 and the corresponding bit = 1. e.g. `MOVE.W #$0020,INTREQ` to clear VERTB. The HRM points out the bit is **not** automatically reset when the CPU services the interrupt — software must write INTREQ explicitly. (HRM §7 "Interrupt Control Registers".)

### 5.5 Paula-owned registers summary

Full list (HRM Appendix A/B): ADKCON/ADKCONR, AUDxLCH/L/LEN/PER/VOL/DAT (x=0..3), DSKDAT/DSKDATR, DSKBYTR, DSKLEN, DSKSYNC, INTENA/INTENAR, INTREQ/INTREQR, POT0DAT/POT1DAT, POTGO/POTGOR, SERDAT/SERDATR, SERPER, STRHOR (shared with Denise).

---

## 6. CIA-A

**Base address**: `$BFE001` (PRA). All registers at offsets of `$100` → byte addresses end in `01`.
**Data bus**: D7–D0 only.
**IRQ**: routes to the system `INT2*` line → Paula INTREQ bit 3 (PORTS) → 68000 IPL level 2.
**E clock**: the 68000's E output. 1/10 of the CPU clock = 715.909 kHz NTSC / 709.379 kHz PAL.

### 6.1 Register map

(HRM Appendix F "CIAA Address Map".)

| Byte Addr | Reg | R/W | Function |
|-----------|-----|-----|----------|
| `$BFE001` | PRA | R/W | Peripheral data port A |
| `$BFE101` | PRB | R/W | Peripheral data port B (parallel data) |
| `$BFE201` | DDRA | R/W | Data direction A (1=out). Default value `$03`. |
| `$BFE301` | DDRB | R/W | Data direction B |
| `$BFE401` | TALO | R/W | Timer A low byte |
| `$BFE501` | TAHI | R/W | Timer A high byte |
| `$BFE601` | TBLO | R/W | Timer B low byte |
| `$BFE701` | TBHI | R/W | Timer B high byte |
| `$BFE801` | TODLO | R/W | TOD event counter bits 7–0 (latched) |
| `$BFE901` | TODMID | R/W | TOD event counter bits 15–8 |
| `$BFEA01` | TODHI | R/W | TOD event counter bits 23–16 |
| `$BFEB01` | — | — | unused |
| `$BFEC01` | SDR | R/W | Serial data register (keyboard) |
| `$BFED01` | ICR | R/W | Interrupt control (read: status; write: mask) |
| `$BFEE01` | CRA | R/W | Control register A (Timer A + SDR) |
| `$BFEF01` | CRB | R/W | Control register B (Timer B + TOD alarm select) |

### 6.2 PRA — port A pinout

(HRM Appendix F "Part 4 – Port Signal Assignments".)

| Bit | Name | Direction | Function |
|-----|------|-----------|----------|
| 7 | /FIR1 | in | Game port 1 fire button (active low) |
| 6 | /FIR0 | in | Game port 0 fire button |
| 5 | /RDY | in | Floppy drive ready |
| 4 | /TK0 | in | Floppy track 0 sensor |
| 3 | /WPRO | in | Floppy write-protect |
| 2 | /CHNG | in | Floppy disk change |
| 1 | /LED | out | Power LED (also: bypasses audio low-pass filter when off) |
| 0 | OVL | out | Memory overlay enable (1 = ROM at $000000, reset state) |

The default DDRA is `$03` — LED and OVL as outputs, everything else as inputs. Reading PRA gives the current state of the pins regardless of DDR (HRM Appendix F).

**Special connection**: CIA-A's **SP** (serial data) pin is connected to the keyboard's data line (KDAT), and **CNT** is connected to the keyboard's clock line (KCLK). The keyboard controller shifts serial data into the CIA's SDR via the standard CIA shift register.

### 6.3 PRB — port B pinout

| Bit | Name | Function |
|-----|------|----------|
| 7 | P7 | Centronics parallel data bit 7 |
| 6 | P6 | Centronics parallel data bit 6 |
| 5 | P5 | Centronics parallel data bit 5 |
| 4 | P4 | Centronics parallel data bit 4 |
| 3 | P3 | Centronics parallel data bit 3 |
| 2 | P2 | Centronics parallel data bit 2 |
| 1 | P1 | Centronics parallel data bit 1 |
| 0 | P0 | Centronics parallel data bit 0 |

**PC** (parallel control output, not exposed as a register bit) = DRDY (centronics data-ready). **FLAG** input = centronics /ACK.

DDRB direction is programmable — PRB is bidirectional so the parallel port can be used as input as well.

### 6.4 Timer A, Timer B

Each 16-bit down-counter with a 16-bit latch. Standard 8520 semantics (HRM Appendix F):

- Write to TALO/TBLO latches the low byte. Write to TAHI/TBHI latches the high byte **and** reloads the counter if in one-shot stopped state (or just latches).
- Read returns the current counter.
- Underflow reloads the latch and (optionally) fires an interrupt (ICR bit 0 for TA, bit 1 for TB).

**CRA** bits:

| Bit | Name | Function |
|-----|------|----------|
| 7 | — | unused |
| 6 | SPMODE | 0 = SDR input (external shift clock on CNT), 1 = SDR output (CNT is output) |
| 5 | INMODE | 0 = Timer A counts E clock pulses, 1 = counts CNT transitions |
| 4 | LOAD | Strobe: force latch into counter (always reads 0) |
| 3 | RUNMODE | 0 = continuous, 1 = one-shot |
| 2 | OUTMODE | 0 = pulse on underflow, 1 = toggle |
| 1 | PBON | 1 = Timer A output on PB6 (overrides DDRB for PB6) |
| 0 | START | 1 = run Timer A |

**CRB** bits:

| Bit | Name | Function |
|-----|------|----------|
| 7 | ALARM | 0 = writing TOD sets clock, 1 = writing TOD sets alarm |
| 6 | INMODE1 | With INMODE0: Timer B clock source |
| 5 | INMODE0 | `00`=E, `01`=CNT, `10`=Timer A underflow, `11`=Timer A underflow while CNT=high |
| 4 | LOAD | Force load (strobe) |
| 3 | RUNMODE | 0 = continuous, 1 = one-shot |
| 2 | OUTMODE | 0 = pulse, 1 = toggle |
| 1 | PBON | 1 = Timer B output on PB7 |
| 0 | START | 1 = run Timer B |

Timer tick rate: E clock = 715909 Hz NTSC / 709379 Hz PAL = 1.3968 µs per count NTSC. A full 16-bit count is 91.55 ms at NTSC.

### 6.5 TOD — 50/60 Hz event counter

On CIA-A, the TOD input is the line-frequency sync tick. From the TRM §Time of Day Clock:

> "In the A500, the Time of Day clock is tied to the VSYNC signal rather than the power line."

On A1000 and A2000 motherboards, TOD was clocked from the AC line frequency (50 Hz PAL, 60 Hz NTSC) via a dedicated 50/60Hz TICK line. On A500 and B2000, there's no easy 50/60 Hz tick from the power supply, so it's clocked from VSYNC instead, which means the rate matches the display — 50 Hz PAL, 60 Hz NTSC — instead of the AC line.

The A2000 has jumper J300 to select between the 50/60Hz TICK line and VSYNC (TRM §A2000 Motherboard Jumpers).

TOD is a 24-bit counter. Writes with ALARM=0 set the clock, writes with ALARM=1 set the alarm. Reading MSB (TODHI) latches all three bytes; reading LSB (TODLO) unlatches. Writing the LSB after a latched write restarts the clock. Alarm match raises ICR bit 2 (ALRM).

### 6.6 SDR — serial data (keyboard)

CIA-A's SDR is connected to the keyboard. The Amiga keyboard is a self-contained micro that transmits 8-bit keycodes serially into CIA-A's shift register clocked by KCLK → CNT. Every complete byte sets ICR bit 3 (SP) which raises INT2.

On receive the shifted-in byte is visible in SDR. The standard handshake is: Kickstart's keyboard handler (in `input.device`) reads SDR, flips the direction bit momentarily to send an acknowledge pulse, then flips back. Raw keycodes are in the HRM keycode table (not reproduced here — it's in HRM §E "Keycodes" and TRM §Keycode Table).

**Keyboard shift direction on the Amiga**: "SDR data is shifted out MSB first. Serial input data should appear in this same format." (HRM Appendix F.) Standard Commodore serial convention.

### 6.7 ICR — interrupt control

(HRM Appendix F "Interrupt Control Register".)

**Read** (ICR):

| Bit | Name | Function |
|-----|------|----------|
| 7 | IR | Master interrupt flag (any enabled source active) |
| 6–5 | 0 | (cleared on read) |
| 4 | FLG | Flag pin (CIA-A: not connected; CIA-B: disk INDEX*) |
| 3 | SP | Serial port full/empty |
| 2 | ALRM | TOD alarm match |
| 1 | TB | Timer B underflow |
| 0 | TA | Timer A underflow |

**Read clears all flags** and deasserts /IRQ. The handler must save the read value if polling multiple sources.

**Write** (ICR mask):

| Bit | Name | Function |
|-----|------|----------|
| 7 | S/C | 1 = set mask bits, 0 = clear mask bits |
| 4 | FLG | Mask FLG |
| 3 | SP | Mask SP |
| 2 | ALRM | Mask ALRM |
| 1 | TB | Mask TB |
| 0 | TA | Mask TA |

Uses the same SET/CLR convention as the Paula INTENA/INTREQ, independently.

### 6.8 CIA-A IRQ → 68000 INT2 path

CIA-A's `/IRQ` output is wired to the system `INT2*` line, which ORs with the Zorro bus `/INT2` to form Paula's external INT2 input, which sets INTREQ bit 3 (PORTS). When PORTS is enabled in INTENA, Paula drives the 68000's IPL lines to level 2.

From the CPU's point of view, the sequence is:

1. CIA-A underflow (or keyboard byte, or TOD alarm) sets a flag in its ICR DATA register.
2. If enabled (ICR MASK), CIA-A drives /IRQ low.
3. /IRQ pulls system INT2* low.
4. Paula latches INTREQ bit 3 (PORTS).
5. If PORTS enabled in INTENA and INTEN set, Paula encodes priority 2 on IPL2..IPL0.
6. 68000 takes a level 2 interrupt, autovectoring through $68 (vector 26).
7. Kickstart INT2 handler walks CIA-A ICR DATA, calls registered handlers (for TA, TB, ALRM, SP, FLG), then writes to INTREQ to clear PORTS.

### 6.9 A500 hard reset

> "The A500 implements a 'hard-wired' Control/Commodore/Amiga key reset rather than the 'soft' A1000/A2000 keyboard reset. 'Shut down' keyboard messages are not transmitted." (TRM §A500 Reset.)

The A1000 and A2000 have a soft reset where the keyboard transmits a shutdown message before the reset is asserted; A500 fires the reset line directly.

---

## 7. CIA-B

**Base address**: `$BFD000` (PRA). Byte addresses end in `00`.
**Data bus**: D15–D8 only.
**IRQ**: wired to the system `INT6*` line → Paula INTREQ bit 13 (EXTER) → 68000 IPL level 6.

### 7.1 Register map

(HRM Appendix F "CIAB Address Map".)

| Byte Addr | Reg | R/W | Function |
|-----------|-----|-----|----------|
| `$BFD000` | PRA | R/W | Peripheral data A (serial, parallel control) |
| `$BFD100` | PRB | R/W | Peripheral data B (floppy control) |
| `$BFD200` | DDRA | R/W | Direction A (default `$FF`) |
| `$BFD300` | DDRB | R/W | Direction B (default `$FF`) |
| `$BFD400` | TALO | R/W | Timer A low |
| `$BFD500` | TAHI | R/W | Timer A high |
| `$BFD600` | TBLO | R/W | Timer B low |
| `$BFD700` | TBHI | R/W | Timer B high |
| `$BFD800` | TODLO | R/W | TOD event counter bits 7–0 (HSYNC ticks) |
| `$BFD900` | TODMID | R/W | ... 15–8 |
| `$BFDA00` | TODHI | R/W | ... 23–16 |
| `$BFDB00` | — | — | unused |
| `$BFDC00` | SDR | R/W | Serial data register (unused in Amiga) |
| `$BFDD00` | ICR | R/W | Interrupt control |
| `$BFDE00` | CRA | R/W | Control A |
| `$BFDF00` | CRB | R/W | Control B |

### 7.2 PRA — port A pinout

| Bit | Name | Dir | Function |
|-----|------|-----|----------|
| 7 | /DTR | out | RS-232 Data Terminal Ready |
| 6 | /RTS | out | RS-232 Request To Send |
| 5 | /CD | in | RS-232 Carrier Detect |
| 4 | /CTS | in | RS-232 Clear To Send |
| 3 | /DSR | in | RS-232 Data Set Ready |
| 2 | SEL | in/out | Centronics SEL (printer select) |
| 1 | POUT | in/out | Centronics PAPER OUT |
| 0 | BUSY | in/out | Centronics BUSY |

Special connections: **SP** pin = BUSY (Commodore serial bus loopback, not used for CIA-B shift register in Amiga). **CNT** pin = POUT.

### 7.3 PRB — port B pinout (floppy control)

| Bit | Name | Dir | Function |
|-----|------|-----|----------|
| 7 | /MTR | out | Floppy motor on |
| 6 | /SEL3 | out | Select external drive 3 (DF3:) |
| 5 | /SEL2 | out | Select external drive 2 (DF2:) |
| 4 | /SEL1 | out | Select external drive 1 (DF1:) |
| 3 | /SEL0 | out | Select internal drive (DF0:) |
| 2 | /SIDE | out | Select disk side (0 = top, 1 = bottom) |
| 1 | DIR | out | Step direction (0 = out/towards track 0, 1 = in) |
| 0 | /STEP | out | Step pulse (falling edge = step) |

Default DDRB = `$FF` (all outputs).

**Disk step timing**: 3 ms minimum between steps (HRM Appendix F / Appendix E "disk drive timing"). A500 and A2000 drives need at least this gap or they'll miss steps.

**Motor on/off latching**: the "magic" is that the motor-on state is latched **when /SELx falls** — you set /MTR first, then pulse /SELx. This lets each drive remember its own motor state. A driver that wants drive 0 motor on does:

1. Set /MTR bit according to desired state (0 = on, 1 = off).
2. Assert /SEL0 (drive 0 now latches whatever /MTR was).
3. Do the operation.
4. Deassert /SEL0.

This is the single most confusing aspect of Amiga floppy control and is often stated incorrectly. The HRM and trackdisk.device source get it right.

**FLAG** pin on CIA-B = `/INDEX*` from the selected floppy drive — raises ICR bit 4 (FLG) once per revolution, wired to INT6.

### 7.4 Timer A, Timer B — CIA-B

Same semantics as CIA-A. CIA-B's timers are used by various OS services; most notably, Kickstart's serial device uses CIA-B Timer A/B for serial port baud rate on some models. Low-level disk code uses CIA-B Timer B for measuring step delays.

### 7.5 TOD — horizontal-sync event counter

CIA-B's TOD pin is wired to **HSYNC** — it increments once per scan line. At NTSC 15.734 kHz line rate, a 24-bit counter rolls over in 1065 seconds (~17 minutes). This is used by the OS for fine-grained timing of raster-related events. Note that CIA-A's TOD is line-frequency (50/60 Hz) and CIA-B's is line-scan (15 kHz) — the two clocks are independent and serve different purposes.

### 7.6 Split of disk control between Paula and CIA-B

The disk controller is split across three chips:

| Chip | Role |
|------|------|
| Agnus | DMA engine (DSKPT, DSKLEN, DMA slot allocation) |
| Paula | Low-level MFM controller (DSKBYTR, DSKDAT, DSKDATR, DSKSYNC, ADKCON disk bits); interrupt generation (DSKBLK, DSKSYN) |
| CIA-A | Status inputs: /RDY, /TK0, /WPRO, /CHNG (PRA bits 2–5) |
| CIA-B | Control outputs: /STEP, DIR, /SIDE, /SELx, /MTR (PRB); /INDEX input (FLG) |

An emulator that wants to be accurate to real software must track all four. The split makes sense historically (the CIAs provide bi-directional GPIO, Paula provides the high-speed MFM engine, Agnus provides the DMA), but it's very easy to miss one chip's contribution.

---

## 8. 68000 integration

### 8.1 68000 in the Amiga

- 7.15909 MHz NTSC / 7.09379 MHz PAL.
- 16-bit data bus, 23-bit address bus (A23..A1; byte within word selected by /UDS and /LDS).
- Directly wired to the chip bus; no cache, no MMU.
- No TAS support (HRM §6).
- Supervisor / user mode are standard 68000; Exec normally runs tasks in user mode with Supervisor() trap for OS services.

### 8.2 Exception vector table

The 68000 exception vector table lives at `$000000–$0003FF` (256 vectors × 4 bytes each). On reset, the table is in ROM (via the overlay). After Kickstart clears OVL and copies the vector table into chip RAM, the table is in chip RAM starting at $000000.

Vectors relevant to the Amiga (standard 68000 layout, see *M68000 Programmer's Reference Manual*; the HRM does not reproduce the full vector list but the Exec RKM does for interrupt purposes):

| Vector # | Offset | Name |
|----------|--------|------|
| 0 | $000 | SSP (Supervisor Stack Pointer, initial) |
| 1 | $004 | PC (initial) |
| 2 | $008 | Bus error |
| 3 | $00C | Address error |
| 4 | $010 | Illegal instruction |
| 5 | $014 | Divide by zero |
| 6 | $018 | CHK instruction |
| 7 | $01C | TRAPV |
| 8 | $020 | Privilege violation |
| 9 | $024 | Trace |
| 10 | $028 | Line 1010 emulator (A-line, used by Exec for soft traps) |
| 11 | $02C | Line 1111 emulator (F-line, used for FPU emulation) |
| 12–14 | $030–$038 | Reserved |
| 15 | $03C | Uninitialized interrupt vector |
| 16–23 | $040–$05C | Reserved |
| 24 | $060 | Spurious interrupt |
| **25** | **$064** | **Level 1 autovector** — TBE/DSKBLK/SOFT |
| **26** | **$068** | **Level 2 autovector** — PORTS (CIA-A, Zorro INT2) |
| **27** | **$06C** | **Level 3 autovector** — COPER/VERTB/BLIT |
| **28** | **$070** | **Level 4 autovector** — AUD0/AUD1/AUD2/AUD3 |
| **29** | **$074** | **Level 5 autovector** — RBF/DSKSYN |
| **30** | **$078** | **Level 6 autovector** — EXTER (CIA-B, Zorro INT6) |
| **31** | **$07C** | **Level 7 autovector** — NMI (debugger button) |
| 32–47 | $080–$0BC | TRAP #0–#15 |
| 48–63 | $0C0–$0FC | Reserved |
| 64–255 | $100–$3FC | User interrupt vectors (unused by Amiga autovector scheme) |

(68000 standard plus HRM §7 "Interrupts" level mapping.)

### 8.3 Autovectors

The Amiga **always uses 68000 autovectors for on-chip interrupts**. When Paula asserts IPL level N, the 68000 responds with an IACK cycle (FC2..0 = 111, A3..A1 = level). Paula asserts /VPA (valid peripheral address — a 6800-compatibility signal) during the IACK, and the 68000 uses the autovector 24 + level. This means levels 1–7 always dispatch through vectors $64–$7C regardless of what's on the data bus during IACK.

Zorro II also supports vectored interrupts (a real device driving data during IACK instead of VPA), but the OS uses autovectors in practice and Kickstart's interrupt dispatcher demultiplexes by reading INTREQR.

From the TRM §Expansion Bus:

> "Only the /INT 2 and /INT6 interrupt inputs are actually supported by the [Zorro II] bus... an automatic autovector timeout will occur very quickly, as any actual waiting that's required for the quick interrupt vector is potentially delaying the autovector response."

Implication: **The Amiga does not actually use the 68000's vectored-interrupt capability.** All interrupt dispatch is via autovector + software demultiplex.

### 8.4 Paula's priority encoder

Paula has a hard priority encoder that takes the highest-priority pending+enabled INTREQ bit and drives IPL2..IPL0. The mapping from INTREQ bit to level is fixed in hardware — software cannot remap it. The level mapping is:

| INTREQ bits | 68000 level |
|-------------|-------------|
| TBE (0), DSKBLK (1), SOFT (2) | 1 |
| PORTS (3) | 2 |
| COPER (4), VERTB (5), BLIT (6) | 3 |
| AUD0–AUD3 (7–10) | 4 |
| RBF (11), DSKSYN (12) | 5 |
| EXTER (13) | 6 |
| — | 7 (NMI, external only — not on OCS) |

(HRM §7 "Interrupt Priorities", figure 7-4.) An interrupt only fires if INTREQ(bit) AND INTENA(bit) AND INTENA(14=INTEN) are all 1.

### 8.5 Bus errors and address errors

- **Bus error**: asserted when the 68000 accesses a region that does not respond. On real Amigas, reserved regions of the map do not generate /BERR — they return floating-bus data. So most Amiga software never sees a bus error. However, /BERR *is* asserted by the Zorro II collision-detect logic when two expansion cards claim the same address (TRM §Slot Control Signals, "Slave"). An emulator can either return floating bus (the actual hardware behaviour) or generate bus error for accurate behaviour on out-of-range accesses — the HRM does not commit either way.

- **Address error**: standard 68000 behaviour — word or long access to an odd address. The Amiga does not special-case this; the 68000 generates the address-error exception (vector 3, $00C). Plenty of real Amiga software triggers this as a bug on the rare occasion.

### 8.6 Supervisor / user mode

Standard 68000 convention. Exec uses the supervisor stack (SSP) for interrupt handlers; tasks run in user mode on their own USP. The OS uses TRAP #0 through TRAP #15 for various software traps, and line-1010 for the library-call jump table (Exec LVO convention). None of this is hardware-enforced beyond the 68000's own privileged-instruction check, but Kickstart assumes it heavily. See the boot process document for how Exec sets up the supervisor stack and interrupt handlers.

### 8.7 /RESET line and the RESET instruction

The 68000 `RESET` instruction asserts the external /RESET line for 124 cycles, resetting all peripherals (including custom chips and CIAs) but *not* the CPU itself:

> "The 68000 RESET instruction works much like external reset or power on. All memory and AUTOCONFIG cards disappear, and the ROM image appears at location $00000000. The difference is that the CPU continues execution with the next instruction. Since RAM may not be available, special care is needed to write reboot code that will reliably reboot all Amiga models." (HRM §7 "Reset and Early Startup Operation".)

**Critical quirk for emulators**: `RESET` reasserts the overlay (OVL), so chip RAM at $000000 *disappears* and ROM reappears. The CPU is still executing, so if its next instruction isn't in ROM (or cache) it will fetch garbage. The standard ColdReboot code runs from ROM itself, in a specific longword-aligned sequence (HRM §7):

```
    move.l  $4,a6              ; ExecBase
    cmp.w   #V36_EXEC,LIB_VERSION(a6)
    blt.s   old_exec
    jmp     _LVOColdReboot(a6) ; Exec handles it on 2.0+
old_exec:
    lea.l   GoAway(pc),a5
    jsr     _LVOSupervisor(a6)
GoAway:
    lea.l   MAGIC_ROMEND,a0    ; $01000000
    sub.l   -$14(a0),a0        ; size-of-ROM
    move.l  4(a0),a0           ; cold reset vector
    sub.l   #2,a0
    reset                      ; RESET instruction
    jmp     (a0)               ; jump into ROM
```

The crucial detail is that the `RESET` + `JMP` pair must be in the 68000's prefetch queue *before* executing RESET, because RAM disappears during RESET. On a 68000 with 4-word prefetch, this is fine if the code is correctly aligned. On a 68020+ with cache, it's fine because the instructions are cached. This is the only documented way to reliably reboot; emulators must model it.

### 8.8 Halt

The 68000 halts on a double bus error (bus error during exception stacking). Outputs: HALT* asserted, address/data bus tri-state. Real Amigas: this usually manifests as a Guru Meditation followed by system lockup. Kickstart catches most bus errors via the alert manager and doesn't trigger a double-fault, but genuinely broken hardware can.

### 8.9 68000 word/long access size

Standard 68000 behaviour. Critical specifics for the Amiga:

- Word and long accesses to odd addresses → address error. No exceptions.
- Byte accesses: UDS for even byte (high), LDS for odd byte (low). Both asserted for word access.
- Long (32-bit) access: two sequential word accesses. The Amiga bus handles these as two color-clock slots; Agnus arbitration treats each word independently, so long accesses can be split across DMA contention.
- On-chip custom registers are all word-sized, so byte accesses to them are discouraged; most have well-defined word semantics only. The HRM is clear: "Software must use byte accesses to these addresses, and no other" (referring to CIA addresses). Custom chip registers are word only.

### 8.10 Prefetch queue and timing

The 68000 has a 4-word instruction prefetch queue. Most instructions fetch one word ahead of their execution. This matters for the emulator because:

- The "instruction start" address for bus-activity purposes is always `PC - 2` or `PC - 4` depending on decode state.
- Copper WAIT for a specific beam position that matches an instruction boundary can interact with the prefetch queue if the CPU is running from chip RAM — the prefetch fetches can stall under DMA contention.
- Precise cycle-exact CPU emulation requires modelling the prefetch queue, which is why real Amiga emulators (WinUAE etc.) implement "cycle-exact" and "memory-exact" modes separately.

---

## 9. Interrupts

A deeper dive into the topic introduced in §5.4 and §8.4.

### 9.1 The 14 Paula interrupt sources

From the HRM Figure 7-4 "Interrupt Priorities" and §7:

| Exec priority | Hardware level | Name | Description |
|---------------|----------------|------|-------------|
| 1 | 1 | SOFTINT | Software interrupt (Exec sets via INTREQ write) |
| 2 | 1 | DSKBLK | Disk block complete (DMA finished) |
| 3 | 1 | TBE | Transmit buffer empty |
| 4 | 2 | PORTS | External INT2 (CIA-A) |
| 5 | 3 | COPER | Graphics coprocessor (Copper) |
| 6 | 3 | VERTB | Vertical blank interval (line 0) |
| 7 | 3 | BLIT | Blitter finished |
| 8 | 4 | AUD2 | Audio channel 2 |
| 9 | 4 | AUD0 | Audio channel 0 |
| 10 | 4 | AUD3 | Audio channel 3 |
| 11 | 4 | AUD1 | Audio channel 1 |
| 12 | 5 | RBF | Receiver buffer full |
| 13 | 5 | DSKSYNC | Disk sync pattern found |
| 14 | 6 | EXTER | External INT6 (CIA-B) |
| 15 | — | INTEN | Master enable (not an interrupt source) |
| — | 7 | NMI | Non-maskable (via expansion bus only; not generated on stock hardware) |

Exec's priority assignment is a software convention for dispatch order when multiple sources share a hardware level — it does not change how the 68000 sees the interrupt.

Important: the Exec "priority" column is *higher-numbered = higher priority*, which is the opposite of the 68000's level convention. So SOFTINT is the lowest-priority Exec interrupt but has 68000 level 1 (lowest 68000 level); EXTER is the highest Exec priority and has 68000 level 6 (second-highest 68000 level — level 7 is NMI only).

### 9.2 Level-sharing and dispatch

When two or more sources share a level (e.g. VERTB, BLIT, COPER all on level 3), the 68000 autovectors through the single level 3 vector ($06C). Kickstart's level 3 handler then:

1. Reads INTREQR.
2. Checks the bits for COPER, VERTB, BLIT in Exec priority order.
3. For each set bit, calls the registered handler (from the `IntVects` table in ExecBase).
4. Writes INTREQ with the cleared bits to acknowledge.

This is why writing INTREQ to clear a bit is mandatory — the handler must acknowledge before returning, or the 68000 will immediately re-enter the interrupt. (Boot doc covers how Kickstart builds the IntVects table; see Phase 5.)

### 9.3 Level-triggered vs edge-triggered semantics

Most Paula INTREQ bits are level-triggered — they stay set until software writes INTREQ to clear them. But a few are effectively edge-triggered:

- **VERTB** — set every frame at line 0. Automatically re-set next frame even if not cleared.
- **DSKSYN** — pulses when the sync word matches.
- **DSKBLK** — pulses when DMA completes.

The master rule: **never rely on a bit being high across multiple ticks**. Always clear it in the handler. The HRM states this obliquely — "These status bits are not automatically reset when the interrupt is serviced, and must be reset when desired by writing to this address" (HRM §7).

### 9.4 SET/CLR convention

All four of these registers use it:

- DMACON
- INTENA
- INTREQ
- ADKCON

Semantics:

- `MOVE.W #$8000 | bits, REG` — set selected bits (1-bits are set, 0-bits unchanged).
- `MOVE.W #$7FFF & bits, REG` — clear selected bits.
- `MOVE.W #$0000, REG` — no effect (no bits selected).
- `MOVE.W #$7FFF, REG` — clear all bits (classic "disable all interrupts" pattern).
- `MOVE.W #$FFFF, REG` — set all bits (including INTEN).

This is why Kickstart's "disable everything" sequence is:

```
MOVE.W  #$7FFF,DMACON  ; clear all DMA
MOVE.W  #$7FFF,INTENA  ; clear all interrupts
MOVE.W  #$7FFF,INTREQ  ; clear all pending interrupts
```

You cannot use `CLR.W` because:

1. `CLR.W` writes zero, and writing zero to a SET/CLR register is a no-op.
2. On 68000, `CLR.W` performs a read-then-write. Reading a write-only register is undefined. (HRM Appendix A warning.)

Always `MOVE.W #$7FFF`.

### 9.5 Software interrupts

The SOFT bit (INTREQ bit 2, level 1) has no hardware source — software sets it by writing to INTREQ with SET/CLR=1 and bit 2 = 1. This is how Exec implements deferred processing: a handler at high interrupt priority can queue work by raising SOFT, which then fires when the CPU is ready. See Exec RKM §Interrupts for the SoftInt mechanism.

### 9.6 Interrupt handler registration

Exec manages a list of handlers per level in `ExecBase->IntVects`. `AddIntServer()` adds a handler to a level's list; the level 1–6 interrupt dispatcher walks the list in priority order. Levels 1, 2, 3 and 6 share multiple sources and need the INTREQR check step; levels 4 and 5 also do, because they have multiple sources too; only the NMI vector is a single source (and is not used on stock Amigas anyway). See Exec RKM §Interrupts for the full list of handler hooks.

---

## 10. Clocks and timing

### 10.1 Master oscillator

From the TRM §Timing "Clocks":

> "The entire computer board is run synchronously to the 3.579545 Mhz color clock (C1). This is accomplished by generating a number of sub-multiple frequencies from our master 28.63636 Mhz crystal oscillator."

| Name | NTSC | PAL | Description |
|------|------|-----|-------------|
| Master crystal | 28.63636 MHz | 28.37516 MHz | Single crystal on the motherboard |
| C1 (color clock) | 3.579545 MHz | 3.546895 MHz | Master ÷ 8 (1 color clock = 1 "CCK") |
| C2 | C1 + 45° | | phase shifted |
| C3 | C1 + 90° | | " |
| C4 | C1 + 135° | | " |
| 7M (CPU clock) | 7.15909 MHz | 7.09379 MHz | `C1 XOR C3*` = master ÷ 4 |
| DAC | 7M + 90° | | chroma DAC |
| 14M (from expansion) | 14.3 MHz | | 7M × 2, used for ECS hires |

The PAL colour-burst frequency of 4.43362 MHz is derived as `(5/4) × C1`.

**Load-bearing takeaway for an emulator author**: the master oscillator drives *everything*. The CPU clock, the DMA clock, the color clock, the display timing, the audio sample period, and the CIA E-clock are all harmonic ratios of the crystal. Every clock in the system is phase-aligned to one reference. An accurate emulator is driven by a master oscillator tick (28.63636 MHz NTSC, 28.37516 MHz PAL), not by "CPU cycles", not by "color clocks" and not by "lines". Everything else derives from that single tick.

### 10.2 Line structure

**Color clocks per line**:

- NTSC: alternating 227 / 228 (average 227.5). Every other line is the "long line" of 228.
- PAL: uniformly 227.

One horizontal scan = 227.5 color clocks = 63.56 µs NTSC / 64.00 µs PAL.

From HRM §2 "Vertical Beam Position":

> "All lines are not the same length in NTSC. Every other line is a long line (228 color clocks, 0–$E3), with the others being 227 color clocks long. In PAL, they are all 227 long. The display sees all these lines as 227 1/2 color clocks long, while the copper sees alternating long and short lines."

This "alternating long/short line" is how NTSC delivers exactly 227.5 cycles per line on average — the pair of (227, 228) lines sums to 455 cycles, which divides evenly.

**DMA slot count per line**: 227.5 color clocks ≈ 227 or 228 slot positions. The HRM says 226 are allocatable (the remainder is overhead around hsync).

### 10.3 Frame structure

| Mode | Lines per frame | Frame rate |
|------|-----------------|------------|
| NTSC non-interlace | 262 (short) or 263 (long) | 60 Hz (approx 59.94) |
| NTSC interlace | 524 (short frame) or 525 (long frame) | 30 Hz (fields at 60) |
| PAL non-interlace | 312 (short) or 313 (long) | 50 Hz |
| PAL interlace | 624 (short) or 625 (long) | 25 Hz (fields at 50) |

(HRM §2 and §3.)

**Four-field NTSC interlace pattern** (HRM §2):

```
short field ending on short line
long field ending on long line
short field ending on long line
long field ending on short line
(repeats)
```

The LOF bit in VPOSR toggles each field in interlace mode so the video monitor can distinguish even from odd fields.

### 10.4 Vertical and horizontal blanking

- NTSC vertical blank: 20 lines minimum (lines 0–20 are blanked); line 21 is the earliest a display can start (HRM §7 "Vertical Blanking Interrupt").
- PAL vertical blank: 25 lines minimum (lines 0–25).
- Horizontal blank: within each line, the period around H=$00–$13 covers hsync + back porch (roughly $E4/0 → $1A). DMA slots for refresh/disk/audio/sprites use the horizontal blank period.

The ECS BEAMCON0 register ($1DC) lets software override all of these with the VARBEAMEN bit, giving fully programmable HTOTAL/VTOTAL/HSSTRT/HSSTOP/VSSTRT/VSSTOP/HBSTRT/HBSTOP/VBSTRT/VBSTOP registers in the $1C0–$1E4 range. OCS has no such thing — timing is hardcoded for NTSC or PAL by Agnus's mask set.

### 10.5 VERTB interrupt timing

VERTB fires at **line 0** (beam at top of frame, during vertical blank). The handler has until roughly line 20 (NTSC) or line 25 (PAL) to do its work before the display starts; most OS vblank handlers finish well within that. Programs with more work to do raise a Copper interrupt (COPER) scheduled for a later line.

### 10.6 68000 cycle counting

> "One horizontal count takes one cycle of the system clock (processor is twice this)." (HRM §2.)

The "system clock" here means the color clock (C1 = 3.58 MHz). One color clock is one DMA slot. The 68000 CPU clock (7M) is twice C1. A 68000 MOVE.W instruction on an even address is:

- 4 CPU clocks = 2 color clocks if in fast RAM (no contention).
- 4 CPU clocks stretched to align with DMA slots if in chip RAM (may be delayed 0–several slots by contention).

For an average estimate: 68000 instructions take 4–16 CPU clocks = 2–8 color clocks. In practice, most real programs run at 500k–700k instructions/sec (0.5–0.7 MIPS) on a stock Amiga 500.

---

## 11. Reset state

This section captures the documented reset state for every register. When the corpus is silent it is marked "not specified in corpus" — a safer default than filling in from outside knowledge.

The [boot process document, Phase 3](./amiga-boot-process.md#phase-3--very-early-kickstart-silence-the-custom-chips-load-the-vector-table) covers what Kickstart writes to these registers *after* power-on. The table below covers the hardware reset value *before* Kickstart touches anything.

### 11.1 Custom chip registers at cold reset

| Reg | Reset value | Source |
|-----|-------------|--------|
| DMACON | 0 (all DMA off; BLTPRI off; DMAEN off) | HRM §7 says Kickstart assumes it; not explicitly stated but implied by "system software" expectations |
| INTENA | 0 (all interrupts disabled, INTEN=0) | Implied by HRM §7 — Paula reset sets IPL inactive |
| INTREQ | 0 | Implied |
| ADKCON | 0 | Not explicitly stated in corpus |
| BPLCON0 | 0 (LPEN=0, LACE=0, ERSY=0) | HRM Appendix A "BPLCON0": "reset on power up" noted for LPEN, LACE, ERSY |
| BPLCON1 | not specified in corpus | |
| BPLCON2 | not specified in corpus | |
| COPCON (CDANG) | 0 | HRM Appendix A: "This bit is cleared by power-on reset, so that the Copper cannot access the blitter hardware." |
| COLOR00–31 | not specified in corpus | In practice undefined — Kickstart writes a default palette |
| DIWSTRT, DIWSTOP, DDFSTRT, DDFSTOP | not specified in corpus | |
| BPLxPT, SPRxPT | undefined | Must be initialized before enabling DMA |
| BLTxPT, BLTSIZE, BLTCON0/1 | undefined | |
| COP1LC, COP2LC | undefined | |
| AUDxLC/LEN/PER/VOL | undefined | |
| DSKPT, DSKLEN, DSKSYNC | undefined | Kickstart sets `DSKLEN = $4000` to disable disk DMA |
| CLXCON | not specified in corpus | |
| CLXDAT | 0 on read-clear, but value undefined until first read | |

### 11.2 CIA registers at reset

From HRM Appendix F "RES - reset Input":

> "A low on the RES pin resets all internal registers. The port pins are set as inputs and port registers to zero (although a read of the ports will return all highs because of passive pull-ups). The timer control registers are set to zero and the timer latches to all ones. All other registers are reset to zero."

This gives a definitive CIA reset state:

| CIA register | Reset value |
|---|---|
| PRA, PRB | 0 (inputs; reads return 1s due to pull-ups) |
| DDRA, DDRB | 0 (all inputs) |
| TALO, TAHI, TBLO, TBHI (latches) | `$FFFF` (all ones) |
| TALO, TAHI, TBLO, TBHI (counters) | `$FFFF` |
| TODLO, TODMID, TODHI (clock) | 0 |
| TODLO, TODMID, TODHI (alarm) | 0 |
| SDR | 0 |
| ICR (mask) | 0 |
| ICR (data) | 0 |
| CRA, CRB | 0 |

**Overlay (OVL)** from CIA-A: DDRA bit 0 = 0 → PA0 is an input → reads as 1 (pull-up). But the OVL line is actually an open-drain pull-up on the board. So at power-on, OVL=1, meaning ROM is overlaid at $000000. Kickstart's very-early init sets DDRA bit 0 to output and writes PA0=0 to clear OVL.

### 11.3 68000 state at reset

Standard 68000 reset: SSP from $000000, PC from $000004 (both 32-bit long words). Status register: SR = $2700 (supervisor mode, interrupt mask = 7). See boot doc Phase 1.

### 11.4 Kickstart early init

For what Kickstart writes to quiet the chips at start of day, see [boot doc Phase 3](./amiga-boot-process.md#phase-3--very-early-kickstart-silence-the-custom-chips-load-the-vector-table). Briefly:

```
MOVE.W  #$7FFF, DMACON   ; clear all DMA
MOVE.W  #$7FFF, INTENA   ; clear all interrupt enable
MOVE.W  #$7FFF, INTREQ   ; clear all pending interrupts
MOVE.W  #$4000, DSKLEN   ; disable disk DMA (the safety value)
```

and then:

```
; PRA direction: bit 1 (LED) and bit 0 (OVL) as outputs
MOVE.B  #$03, CIAADDRA($BFE201)
; Clear OVL, turn LED on
MOVE.B  #$02, CIAAPRA($BFE001)  ; PA1=LED=1 to turn LED off in active-low semantics, or 0=on
```

The actual bit polarity depends on the electrical connection — see HRM Appendix F for /LED being active low (0 = bright).

---

## 12. Address decode and aliasing

### 12.1 $DFF000 custom chip window aliasing

The custom chips respond to a 9-bit register offset (covering $000–$1FE in word steps). The actual decode is on A8..A1; A9, A10, A11 are "don't care" within the $DFF000 window. So:

- `$DFF000` and `$DFF200` alias to the same register (BLTDDAT read / offset $000).
- `$DFF002`, `$DFF202`, `$DFF402`, ..., all map to DMACONR.
- `$DFF1FE` to $DFF0FE alias.

On an emulator, implement this by masking the access offset with `$1FE` before lookup.

More subtly, the HRM warns:

> "Further, do not write to an address or register that is not documented or defined in this appendix. Setting unused bits in a write-only register, reading unused bits from a read only register and writing to undocumented registers or addresses may cause serious future software incompatibility if those bits or addresses are implemented in the future by Commodore Amiga." (HRM Appendix A.)

Practical consequence: $DFF000 is aliased across the entire 4 KB window, but the chip also extends into the 512 KB window `$D80000–$DFFFFF` on the A1000/A500 (via incomplete decoding). Software that writes to e.g. $D80000 may hit DMACONR on real hardware. An emulator can be strict (only decode $DFF000–$DFF1FF) without breaking real software, because real software doesn't rely on the extended aliasing.

### 12.2 Read/write mirroring of write-only registers

A handful of registers have *separate addresses* for read and write — the write address is not readable and the read address is not writable. For example:

| Read addr | Write addr | Register |
|-----------|------------|----------|
| $002 (DMACONR) | $096 (DMACON) | DMA control |
| $01C (INTENAR) | $09A (INTENA) | Interrupt enable |
| $01E (INTREQR) | $09C (INTREQ) | Interrupt request |
| $010 (ADKCONR) | $09E (ADKCON) | Audio/disk control |
| $004 (VPOSR) | $02A (VPOSW) | Vertical position MSB |
| $006 (VHPOSR) | $02C (VHPOSW) | Horizontal position |

Reading a write-only register (e.g. $096 DMACON) is explicitly "may trash the register and crash the system" (HRM Appendix A).

Most registers are write-only with no paired read. The `$DFF000` register summary appendix (A and B) is the authoritative list.

### 12.3 CIA address decode

Covered in §1.5. Restating for completeness:

- CIA-A: `1010 xxxx xx01 rrrr xxxx xxx1` → $BFE001 base, offset = `rrrr * $100`, data on D7..D0.
- CIA-B: `1010 xxxx xx10 rrrr xxxx xxx0` → $BFD000 base, offset = `rrrr * $100`, data on D15..D8.

Consequences:

- CIA-A aliases through many addresses within `$BFxxx1` — only A13, A12 matter for chip select. An emulator can decode either strictly or loosely.
- Byte access is mandatory. Word access to $BFD000 touches CIA-B in high byte and some random decoder in low byte.

### 12.4 ROM mirrors

From TRM §PAL equations, the ROM space is decoded at:

- `$F80000–$FFFFFF` (native; 512 KB window, covers 256 KB or 512 KB ROMs).
- `$E00000–$E7FFFF` (on some A500 models; designated for diagnostic ROM on A3000).
- `$000000–$07FFFF` and `$180000–$1FFFFF` when OVL is asserted.

On A3000, Kickstart 2.x+ lives at $F80000. Older 256 KB Kickstarts sit at $FC0000 and the $F80000–$FBFFFF space is empty (or diagnostic).

### 12.5 Chip RAM wrap-around

OCS Agnus sees 19 address bits (512 KB of chip RAM). Fat Agnus 8370/8371 sees 20 bits (1 MB). ECS Fat Agnus 8372 sees 21 bits (2 MB, A3000). DMA pointer registers are 18-bit word pointers (AUDxLC, BPLxPT, SPRxPT, BLTxPT, DSKPT, COPxLC). On ECS, the high 2 bits are exposed in the PTH registers (`high 5 bits, was 3 bits`) — see HRM Appendix C.

Consequence: on OCS a DMA pointer written with a chip-memory address above $07FFFF will wrap modulo the Agnus's addressable space. An emulator must mask DMA pointers to the correct number of bits based on the configured Agnus revision.

---

## Appendix A — $DFF000 Register Summary (address order)

All registers from HRM Appendix B. Legend:

- **R/W**: R = read only, W = write only, ER = early read (DMA-only), S = strobe (write causes an effect).
- **Chip**: A = Agnus, D = Denise, P = Paula. `(E)` = register extended/added in ECS.
- **Access**: `&` = DMA only (CPU cannot access), `%` = DMA usually/CPU sometimes, `+` = address pair (pointer pair, must be even, chip memory), `*` = not writable by Copper (unless noted), `#` = not writable by Copper unless CDANG set.

| Offset | Name | R/W | Chip | Function |
|--------|------|-----|------|----------|
| $000 | BLTDDAT | ER | A | Blitter destination early read (dummy) |
| $002 | DMACONR | R | A, P | DMA control (and blitter status) read |
| $004 | VPOSR | R | A (E) | Vertical position MSB + frame flop + chip ID (ECS) |
| $006 | VHPOSR | R | A | Vertical low 8 / horizontal position |
| $008 | DSKDATR | ER | P | Disk DMA data early read (dummy) |
| $00A | JOY0DAT | R | D | Joystick/mouse 0 data |
| $00C | JOY1DAT | R | D | Joystick/mouse 1 data |
| $00E | CLXDAT | R | D | Collision data (read-and-clear) |
| $010 | ADKCONR | R | P | Audio/disk control read |
| $012 | POT0DAT | R | P (E) | Pot counter pair 0 (vert,horiz) |
| $014 | POT1DAT | R | P (E) | Pot counter pair 1 |
| $016 | POTGOR | R | P | Pot port data read (a.k.a. POTINP) |
| $018 | SERDATR | R | P | Serial port data + status |
| $01A | DSKBYTR | R | P | Disk data byte + status |
| $01C | INTENAR | R | P | Interrupt enable read |
| $01E | INTREQR | R | P | Interrupt request read |
| $020 | DSKPTH | W+ | A (E) | Disk pointer high (3/5 bits) |
| $022 | DSKPTL | W+ | A | Disk pointer low |
| $024 | DSKLEN | W | P | Disk length |
| $026 | DSKDAT | W& | P | Disk DMA data write |
| $028 | REFPTR | W& | A | Refresh pointer (test only) |
| $02A | VPOSW | W | A | Vertical position MSB write |
| $02C | VHPOSW | W | A | Vertical/horizontal position write |
| $02E | COPCON | W | A (E) | Copper control (CDANG) |
| $030 | SERDAT | W | P | Serial port data write |
| $032 | SERPER | W | P | Serial port period + control |
| $034 | POTGO | W | P | Pot port data write + start |
| $036 | JOYTEST | W | D | Write all four joystick counters |
| $038 | STREQU | S& | D | Strobe: horiz sync with VB and EQU |
| $03A | STRVBL | S& | D | Strobe: horiz sync with VB |
| $03C | STRHOR | S& | D, P | Strobe: horiz sync |
| $03E | STRLONG | S& | D (E) | Strobe: identify long horiz line |
| $040 | BLTCON0 | W | A | Blitter control 0 |
| $042 | BLTCON1 | W | A (E) | Blitter control 1 |
| $044 | BLTAFWM | W | A | Blitter first-word mask A |
| $046 | BLTALWM | W | A | Blitter last-word mask A |
| $048 | BLTCPTH | W+ | A | Blitter src C ptr high |
| $04A | BLTCPTL | W+ | A | Blitter src C ptr low |
| $04C | BLTBPTH | W+ | A | Blitter src B ptr high |
| $04E | BLTBPTL | W+ | A | Blitter src B ptr low |
| $050 | BLTAPTH | W+ | A (E) | Blitter src A ptr high |
| $052 | BLTAPTL | W+ | A | Blitter src A ptr low |
| $054 | BLTDPTH | W+ | A | Blitter dest D ptr high |
| $056 | BLTDPTL | W+ | A | Blitter dest D ptr low |
| $058 | BLTSIZE | W | A | Blitter start + size |
| $05A | BLTCON0L | W | A (E) | Blitter control 0 lower 8 bits |
| $05C | BLTSIZV | W | A (E) | Blitter vert size (15 bit) |
| $05E | BLTSIZH | W | A (E) | Blitter horiz size + start (11 bit) |
| $060 | BLTCMOD | W | A | Blitter modulo C |
| $062 | BLTBMOD | W | A | Blitter modulo B |
| $064 | BLTAMOD | W | A | Blitter modulo A |
| $066 | BLTDMOD | W | A | Blitter modulo D |
| $068–$06E | — | — | — | reserved |
| $070 | BLTCDAT | W% | A | Blitter src C data |
| $072 | BLTBDAT | W% | A | Blitter src B data |
| $074 | BLTADAT | W% | A | Blitter src A data |
| $076 | — | — | — | reserved |
| $078 | SPRHDAT | W | A (E) | Ext UHRES sprite ptr/data ID |
| $07A | — | — | — | reserved |
| $07C | DENISEID | R | D (E) | Denise chip revision ($FC if ECS) |
| $07E | DSKSYNC | W | P | Disk sync pattern |
| $080 | COP1LCH | W+ | A (E) | Copper list 1 ptr high |
| $082 | COP1LCL | W+ | A | Copper list 1 ptr low |
| $084 | COP2LCH | W+ | A (E) | Copper list 2 ptr high |
| $086 | COP2LCL | W+ | A | Copper list 2 ptr low |
| $088 | COPJMP1 | S | A | Copper restart from list 1 |
| $08A | COPJMP2 | S | A | Copper restart from list 2 |
| $08C | COPINS | W | A | Copper instruction fetch dummy |
| $08E | DIWSTRT | W | A | Display window start |
| $090 | DIWSTOP | W | A | Display window stop |
| $092 | DDFSTRT | W | A | Display data fetch start |
| $094 | DDFSTOP | W | A | Display data fetch stop |
| $096 | DMACON | W | A, D, P | DMA control write |
| $098 | CLXCON | W | D | Collision control |
| $09A | INTENA | W | P | Interrupt enable write |
| $09C | INTREQ | W | P | Interrupt request write |
| $09E | ADKCON | W | P | Audio/disk control |
| $0A0 | AUD0LCH | W+ | A (E) | Audio ch 0 location high |
| $0A2 | AUD0LCL | W+ | A | Audio ch 0 location low |
| $0A4 | AUD0LEN | W | P | Audio ch 0 length |
| $0A6 | AUD0PER | W | P (E) | Audio ch 0 period |
| $0A8 | AUD0VOL | W | P | Audio ch 0 volume |
| $0AA | AUD0DAT | W& | P | Audio ch 0 data |
| $0B0–$0BA | AUD1xxx | — | — | Audio channel 1 (same layout as 0) |
| $0C0–$0CA | AUD2xxx | — | — | Audio channel 2 |
| $0D0–$0DA | AUD3xxx | — | — | Audio channel 3 |
| $0E0–$0E2 | BPL1PTH/L | W+ | A | Bitplane 1 ptr |
| $0E4–$0E6 | BPL2PTH/L | W+ | A | Bitplane 2 ptr |
| $0E8–$0EA | BPL3PTH/L | W+ | A | Bitplane 3 ptr |
| $0EC–$0EE | BPL4PTH/L | W+ | A | Bitplane 4 ptr |
| $0F0–$0F2 | BPL5PTH/L | W+ | A | Bitplane 5 ptr |
| $0F4–$0F6 | BPL6PTH/L | W+ | A | Bitplane 6 ptr |
| $0F8–$0FE | — | — | — | reserved |
| $100 | BPLCON0 | W | A, D (E) | Bitplane control 0 |
| $102 | BPLCON1 | W | D | Bitplane control 1 (horiz scroll) |
| $104 | BPLCON2 | W | D (E) | Bitplane control 2 (priorities) |
| $106 | BPLCON3 | W | D (E) | Bitplane control 3 (ECS extras) |
| $108 | BPL1MOD | W | A | Bitplane modulo odd |
| $10A | BPL2MOD | W | A | Bitplane modulo even |
| $10C–$10E | — | — | — | reserved |
| $110 | BPL1DAT | W& | D | Bitplane 1 data (parallel-to-serial) |
| $112 | BPL2DAT | W& | D | Bitplane 2 data |
| $114 | BPL3DAT | W& | D | Bitplane 3 data |
| $116 | BPL4DAT | W& | D | Bitplane 4 data |
| $118 | BPL5DAT | W& | D | Bitplane 5 data |
| $11A | BPL6DAT | W& | D | Bitplane 6 data |
| $11C–$11E | — | — | — | reserved |
| $120–$13E | SPR0PTH/L..SPR7PTH/L | W+ | A | Sprite 0–7 pointers |
| $140 | SPR0POS | W% | A, D | Sprite 0 vert-horiz start |
| $142 | SPR0CTL | W% | A, D (E) | Sprite 0 vert stop + control |
| $144 | SPR0DATA | W% | D | Sprite 0 image data A (arms sprite) |
| $146 | SPR0DATB | W% | D | Sprite 0 image data B |
| $148–$17E | SPR1..SPR7 | | | (same layout, 8 bytes apart) |
| $180–$1BE | COLOR00..COLOR31 | W | D | Color table 0–31 (12-bit RGB) |
| $1C0 | HTOTAL | W | A (E) | Horiz total count |
| $1C2 | HSSTOP | W | A (E) | HSYNC stop |
| $1C4 | HBSTRT | W | A (E) | HBLANK start |
| $1C6 | HBSTOP | W | A (E) | HBLANK stop |
| $1C8 | VTOTAL | W | A (E) | Vert total count |
| $1CA | VSSTOP | W | A (E) | VSYNC stop |
| $1CC | VBSTRT | W | A (E) | VBLANK start |
| $1CE | VBSTOP | W | A (E) | VBLANK stop |
| $1D0–$1DA | — | — | — | reserved |
| $1DC | BEAMCON0 | W | A (E) | Beam counter control (SHRES, PAL, VARBEAMEN) |
| $1DE | HSSTRT | W | A (E) | HSYNC start |
| $1E0 | VSSTRT | W | A (E) | VSYNC start |
| $1E2 | HCENTER | W | A (E) | Horiz position for VSYNC on interlace |
| $1E4 | DIWHIGH | W | A, D (E) | Display window high bits |
| $1FE | NO-OP | — | — | Written by the copper as a no-op |

(HRM Appendix B "Register Summary Address Order".)

---

## Appendix B — $DFF000 Register Summary (alphabetical)

See the HRM Appendix A for the full alphabetical list with bit-level descriptions. All of that content is referenced in the body of this document. If you need a lookup table, use Appendix A above and Ctrl-F.

---

## Appendix C — CIA Register Summary

### CIA-A ($BFE001 + offset × $100; byte addresses, odd bytes, D7..D0)

| Addr | Reg | R/W | Function |
|------|-----|-----|----------|
| $BFE001 | PRA | R/W | /FIR1 /FIR0 /RDY /TK0 /WPRO /CHNG /LED OVL |
| $BFE101 | PRB | R/W | Centronics parallel data |
| $BFE201 | DDRA | R/W | Direction A (default $03) |
| $BFE301 | DDRB | R/W | Direction B |
| $BFE401 | TALO | R/W | Timer A low |
| $BFE501 | TAHI | R/W | Timer A high |
| $BFE601 | TBLO | R/W | Timer B low |
| $BFE701 | TBHI | R/W | Timer B high |
| $BFE801 | TODLO | R/W | TOD bits 7–0 (50/60 Hz or VSYNC tick) |
| $BFE901 | TODMID | R/W | TOD bits 15–8 |
| $BFEA01 | TODHI | R/W | TOD bits 23–16 |
| $BFEC01 | SDR | R/W | Serial data (keyboard) |
| $BFED01 | ICR | R (status, clear) / W (mask) | Interrupt control |
| $BFEE01 | CRA | R/W | Control register A |
| $BFEF01 | CRB | R/W | Control register B |

SP = KDAT (keyboard data), CNT = KCLK (keyboard clock), FLAG = unused.

### CIA-B ($BFD000 + offset × $100; byte addresses, even bytes, D15..D8)

| Addr | Reg | R/W | Function |
|------|-----|-----|----------|
| $BFD000 | PRA | R/W | /DTR /RTS /CD /CTS /DSR SEL POUT BUSY |
| $BFD100 | PRB | R/W | /MTR /SEL3 /SEL2 /SEL1 /SEL0 /SIDE DIR /STEP |
| $BFD200 | DDRA | R/W | Direction A (default $FF) |
| $BFD300 | DDRB | R/W | Direction B (default $FF) |
| $BFD400 | TALO | R/W | Timer A low |
| $BFD500 | TAHI | R/W | Timer A high |
| $BFD600 | TBLO | R/W | Timer B low |
| $BFD700 | TBHI | R/W | Timer B high |
| $BFD800 | TODLO | R/W | TOD bits 7–0 (HSYNC tick) |
| $BFD900 | TODMID | R/W | TOD bits 15–8 |
| $BFDA00 | TODHI | R/W | TOD bits 23–16 |
| $BFDC00 | SDR | R/W | Serial data register (unused on Amiga) |
| $BFDD00 | ICR | R/W | Interrupt control |
| $BFDE00 | CRA | R/W | Control A |
| $BFDF00 | CRB | R/W | Control B |

SP = BUSY, CNT = POUT, FLAG = /INDEX (disk index pulse).

### CRA and CRB bits (same on both CIAs)

See §6.4 for full bit layout.

### ICR bits (same on both CIAs)

See §6.7 for full bit layout.

---

## Appendix D — 68000 Exception and Interrupt Vectors

The full 68000 exception vector table is in the processor manual. This appendix lists only the interrupt autovectors and the exceptions relevant to the Amiga.

| Vector | Offset | Name | Amiga use |
|--------|--------|------|-----------|
| 0 | $000 | SSP initial | ROM reset vector |
| 1 | $004 | PC initial | ROM reset vector ($F800D2 or similar) |
| 2 | $008 | Bus error | Guru Alert #0100 or access fault |
| 3 | $00C | Address error | Guru Alert #0200 |
| 4 | $010 | Illegal instruction | Exec exception dispatch |
| 5 | $014 | Divide by zero | |
| 6 | $018 | CHK | |
| 7 | $01C | TRAPV | |
| 8 | $020 | Privilege violation | |
| 9 | $024 | Trace | |
| 10 | $028 | Line 1010 (A-line) | Exec library call jump table |
| 11 | $02C | Line 1111 (F-line) | FPU emulation / 68020 coprocessor |
| 15 | $03C | Uninitialized vector | |
| 24 | $060 | Spurious interrupt | |
| 25 | $064 | Level 1 autovector | TBE, DSKBLK, SOFT |
| 26 | $068 | Level 2 autovector | PORTS (CIA-A, Zorro INT2) |
| 27 | $06C | Level 3 autovector | COPER, VERTB, BLIT |
| 28 | $070 | Level 4 autovector | AUD0, AUD1, AUD2, AUD3 |
| 29 | $074 | Level 5 autovector | RBF, DSKSYN |
| 30 | $078 | Level 6 autovector | EXTER (CIA-B, Zorro INT6) |
| 31 | $07C | Level 7 autovector | NMI (not on stock hardware) |
| 32–47 | $080–$0BC | TRAP #0–#15 | Exec Supervisor(), etc. |

---

## Appendix E — Reset state table

See §11. The short version:

- **Custom chips**: DMACON=0, INTENA=0, INTREQ=0, ADKCON=0, COPCON=0, BPLCON0 has LPEN/LACE/ERSY cleared; everything else is undefined and must be initialized by software.
- **CIA-A/B**: ports as inputs (read as 1s via pull-ups), timer latches=$FFFF, timer counters=$FFFF, CRA/CRB/ICR/SDR/TOD=0.
- **OVL**: 1 (ROM at $000000). Cleared by Kickstart.
- **68000**: SSP and PC from vectors 0 and 1. SR = $2700.

---

## Gaps in corpus

Things that an emulator author will need but that are not fully covered by the ten source PDFs:

1. **Exact DMA slot positions within a line.** HRM Figure 6-9 gives the big picture (refresh at $00–$07, disk at $08–$0D, audio in $0E–$15, sprites in $15–$27, bitplanes in $28–$D6) but does not specify which color clock each channel uses. The emulator-accurate map (which cycle each channel takes) is not in the HRM — it's folklore from the Amiga emulation scene (WinUAE's cycle-exact mode is the reference). The corpus is silent on the exact slot positions.
2. **BPLCON3 bit-level layout.** The HRM Appendix C introduces BPLCON3 for ECS genlock but does not give the full bit list. Full details require the ECS Denise datasheet.
3. **BEAMCON0 bit-level layout.** Same — introduced in HRM Appendix C, no bit list.
4. **Reset values for BPLCONx, DIWSTRT, DIWSTOP, DDFSTRT/STOP, colour registers, DMA pointers.** The HRM only explicitly commits to LPEN/LACE/ERSY in BPLCON0 and CDANG in COPCON. Everything else is "undefined and must be initialized". An emulator should treat these as undefined at power-on.
5. **Exact floppy low-level track format (sector preamble, headers, checksums).** The HRM documents the DSKSYNC = $4489 sync mark and ADKCON encoding, but not the Amiga standard track structure (11 sectors × 544 bytes, odd/even interleaved, etc.). That is in L&D RKM §trackdisk.device (a side reference) and in the original Commodore internal tech notes.
6. **Blitter microcycle timing.** The HRM gives the concept (4 memory cycles per blitter cycle, priority over CPU depending on BLTPRI) but not exact cycle-by-cycle counts for a given BLTCON0/1 configuration. Again, folklore.
7. **RBE (overrun) vs RBF for the UART.** SERDATR documents OVRUN, RBF, TBE but does not specify the exact mechanism by which double-buffering fails on overrun.
8. **Exact CIA handshake timing on the parallel port.** HRM Appendix F says "PC will go low on the third cycle after a port B access" but doesn't clock this against the master oscillator.
9. **The exact HRM of the A1000 WCS.** The A1000 had 256 KB of writeable control store at $F80000 instead of ROM. None of the ten PDFs document the WCS loader state machine in detail.
10. **AGA.** None of the source PDFs cover the AA chipset (A1200/A4000 Lisa/Alice/AGA Paula). An emulator covering AGA needs to go beyond this corpus.
11. **Chip revision-specific behaviour other than the summary in HRM Appendix C.** Differences between the 8361 and 8370 Agnuses in how extended chip RAM is decoded, for example, are not in the corpus.
12. **Exact 68000 cycle counts for individual instructions under chip-bus contention.** The HRM says "the 68000 runs at full speed most of the time if there is no blitter DMA interference" but does not give a contention model at instruction granularity.

---

## Source map

Full titles of the corpus PDFs, and what each is primarily used for in this reference:

| Abbreviation | Full title | Primary use |
|--------------|------------|-------------|
| **HRM** | *Amiga Hardware Reference Manual, 3rd edition* | Authoritative for every register bit and DMA slot. Used for §§3, 4, 5, 7 (indirect), 9, 10, 11, 12 and both register summary appendices. |
| **TRM** | *Commodore Amiga A500/A2000 Technical Reference Manual, 1987* | Authoritative for memory map, buses, electrical, PAL decoding equations, Expansion Bus, Zorro II, board-level timing (master clock, C1-C4 derivation). Used for §§1, 2, 8, 10. |
| **Mapping** | Thomson/Anderson, *Mapping the Amiga, 2nd edition, 1993* | Secondary per-address commentary and worked examples. Used for cross-checking §3.1 beam counter semantics. |
| **SPG** | *Amiga System Programmers Guide*, Abacus, 1988 | Secondary; used as sanity check on interrupts, not extensively cited in this document. |
| **Abacus ML** | *Amiga Machine Language*, Abacus, 1991 | Not cited — superseded by HRM for register detail. |
| **Exec RKM** | *ROM Kernel Reference Manual: Exec* | Used for interrupt dispatch semantics, autovector handling, SoftInt. Cross-ref §9. |
| **L&D RKM** | *ROM Kernel Reference Manual: Libraries & Devices* | Used as source for the trackdisk device details. Not extensively quoted here. |
| **Includes/Autodocs** | *ROM Kernel Reference Manual: Includes & Autodocs* | Used as cross-ref for register names and constants. |
| **RKM 3rd** | Beats, *Amiga ROM Kernel Ref 3rd* | Not directly cited — overlap with Exec RKM. |
| *AmigaDOS Manual* | Baker et al, 3rd ed., 1991 | Not relevant to hardware. |

Inline citations through the document are in the form `(HRM §N)`, `(HRM Appendix A "REG")`, `(TRM §Topic)`, `(Mapping "REG")`, etc. If you need the full page number, search the corresponding text file in `/Users/stevehill/Desktop/AmigaPDFs/txt/` for the register name or section title.

Cross-references into [`amiga-boot-process.md`](./amiga-boot-process.md): anywhere this document says "Kickstart does X" or "at reset Y is true", see the relevant Phase N section of the boot document for the full walk-through.

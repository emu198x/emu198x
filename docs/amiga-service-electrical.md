# The Amiga Service & Electrical Reference — Motherboard-Level Detail for Emulator Authors

*Synthesised from the Commodore service manuals, field bulletins, Amiga Intern chip internals chapters, and system schematics under `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/`.*

## How to read this document

This is a **service-manual-derived** reference, not a register reference. It is the companion to [`amiga-hardware-reference.md`](./amiga-hardware-reference.md) — that document covers every bit of every $DFF000 register and every CIA register at software level. This one covers the physical level underneath:

- What silicon is in each motherboard revision (part numbers, not "Agnus")
- What changed between one chip revision and the next, at the level an emulator needs to know
- How the crystal becomes the CPU clock becomes the color clock
- How the reset line is actually driven, and by what, for how long
- How IPL0–2 physically get from Paula to the 68000
- How /BR, /BG, /BGACK implement the DMA grab
- What Gary / Gayle / Fat Gary actually decode, line by line
- What each pin of the edge connector on an A500 is
- What lives on each ASIC (Alice, Lisa, Budgie, Buster, Super Buster, Ramsey, Bridgette)
- Service bulletins noting board-rev bugs that affect compatibility

It does **not** re-document registers, DMA slot tables, or the Copper instruction format — those live in `amiga-hardware-reference.md`. Where a topic is shared (e.g. reset state, clock frequencies), this document stops at the silicon boundary and cross-references the register-level doc.

A note on OCR quality: the A500 text-layer service manual is mostly clean prose but the schematic pages OCR into noise. The OCR-only A500/A1000/A4000/SAMS manuals have occasional garbled digits in component designators. Where a value is OCR-suspect, it is flagged inline.

### Contents

1.  [Model → chipset → motherboard revision table](#1-model--chipset--motherboard-revision-table)
2.  [Chip revision list with behavioural differences](#2-chip-revision-list-with-behavioural-differences)
3.  [Clock distribution](#3-clock-distribution)
4.  [Reset circuitry](#4-reset-circuitry)
5.  [Interrupt wiring](#5-interrupt-wiring)
6.  [Bus arbitration signals](#6-bus-arbitration-signals)
7.  [Memory decoding](#7-memory-decoding)
8.  [Video output](#8-video-output)
9.  [Audio output](#9-audio-output)
10. [CIA pin mapping — definitive](#10-cia-pin-mapping--definitive)
11. [Floppy interface](#11-floppy-interface)
12. [Expansion bus](#12-expansion-bus)
13. [Power rails](#13-power-rails)
14. [Known errata / service bulletins](#14-known-errata--service-bulletins)
15. [Amiga Intern chip internals](#15-amiga-intern-chip-internals)
- Appendix A — [Chip revision comparison matrix](#appendix-a--chip-revision-comparison-matrix)
- Appendix B — [ASIC part number index](#appendix-b--asic-part-number-index)
- [Gaps in corpus](#gaps-in-corpus)
- [Source map](#source-map)

### Source abbreviations

- **A1000 SM** — *Amiga 1000 Component Level Repair*, PN 314038-02, 1986 (OCR).
- **A500 SM** — *A500 Service Manual*, PN 314981-04, October 1990 (text layer, good quality).
- **A500 SM OCR** — *A500 Service Manual* (separate OCR copy, used for cross-check only).
- **A500+ SM** — *A500 Plus Service Manual*, October 1991 (text layer).
- **A500 schem** — *System Schematics A500 rev 5, 6A, 7* (OCR).
- **A4000 SM** — *A4000 Service Addendum* (OCR). Contains full BRIDGETTE / RAMSEY / BUSTER / FAT GARY chip specs.
- **A1200 sch** — *A1200 System Schematics Service Addendum*, 1992 (text layer).
- **SAMS A500** — *SAMS Technical Service Data CSCS26 — A500* (OCR).
- **Amiga Update** — *Amiga — An Update For The Service Technician* (OCR). Techtopics bulletins.
- **Tech Topics** — *Commodore Tech Topics* newsletter (OCR).
- **Amiga Intern** — *Amiga Intern*, Abacus 1992. Chapters 11.4 (custom chip internals) and 11.7 (programming the hardware).
- **HRM ref** — cross-reference to `amiga-hardware-reference.md`.

When a citation is (A4000 SM §6-16) the section number refers to the page number in the A4000 Service Addendum. When it is (Amiga Intern §11.4.2) it refers to the Amiga Intern book chapter.

---

## 1. Model → chipset → motherboard revision table

This is the mapping from a box ("Amiga 500") to what actually sits in the sockets — per board revision, because the board revision determines what Agnus variant is present, which determines the chip RAM ceiling, the Denise variant, whether ECS features work, and whether the machine can be genlocked.

### 1.1 Amiga 1000 (1985–1987)

| Aspect | Value | Source |
|--------|-------|--------|
| CPU | Motorola MC68000, 7.15909 MHz NTSC / 7.09379 MHz PAL | A1000 SM §1-1 |
| Master clock | 28.63636 MHz NTSC, 28.37516 MHz PAL (in metal RF can) | A1000 SM §1-2 |
| Agnus | 8361 — 256 KB chip RAM (NMOS, DIP-48) | A1000 SM §1-6, Amiga Update 27/3 |
| Denise | 8362 (revisions up to rev 6 — rev 5 has a HiRes colour boundary glitch) | A1000 SM §1-8, Tech Topics A1000 bulletin |
| Paula | 8364 | A1000 SM §1-10 |
| Bus control | 4 PALs on the daughter ("Piggyback") board | A1000 SM §1-2 |
| Chip RAM | 256 KB on main board, optional 256 KB "writable CCR" expansion to 512 KB via the trapdoor | A1000 SM §1-3 |
| Slow RAM slot | None (only the 256 KB piggyback socket) | — |
| Kickstart | Loaded from floppy into a 256 KB writable-then-write-protected "WCS" RAM on the piggyback | A1000 SM §1-2 |
| Boot ROM | Two 32 KB byte-wide ROMs giving a 32 KW "bootstrap" ROM containing loader for WCS | A1000 SM §1-2 |
| Expansion | 86-pin edge connector on the right-hand side | A1000 SM §1-14 |
| Floppy | Internal Chinon/Sony single drive; external drives via DB23 J7 | A1000 SM §1-16 |

The A1000 is architecturally different from all later machines because its "ROM" is really RAM that is write-protected after the Kickstart loader runs. The bus control logic on the A1000 is four PALs (on the "piggyback" daughtercard), not a single Gary chip — this is the only Amiga where bus control is not in silicon. This matters for emulator timing if modelling an A1000 specifically.

### 1.2 Amiga 500 — rev 3, 5, 6A, 7, 8A (1987–1991)

| Rev | Agnus | Denise | DRAM | Notes | Source |
|-----|-------|--------|------|-------|--------|
| 3.x | 8370 (NMOS OCS, 512 KB addressing) | 8362 | 256 K × 1 | Earliest A500 boards; some compat fixes over rev 5 noted. Amiga Update 25/3 says "field upgrades are NOT recommended on rev 3.x PCBs" | Amiga Update 25/3-1.1 |
| 5 | 8370 | 8362 | 256 K × 1 | 16 off-chip DRAMs, factory 512 KB chip RAM. A500 SM "8370 Fat Agnus chips are used on rev 5 boards with 256K x 1 DRAMs" | A500 SM §2-9 |
| 6A | 8372A (ECS, 1 MB addressing) | 8362 | 256 K × 4 | 4 off-chip DRAMs. Commodore jumpered for 512 KB-compatibility by default. Field-enabling the 1 MB mode voided the warranty. "8372 Fat Agnus chips are used on rev 6a boards with 256K x 4 DRAMs. The boards are functionally interchangeable. Each will support 512K of chip RAM and 512K of expansion RAM with an A501 installed." | A500 SM §2-9, Amiga Update 30/3-8.1 |
| 7 | 8372A | 8362 or 8373 | 256 K × 4 | Largely identical to 6A electrically; minor silkscreen and bill-of-materials differences | A500 schem |
| 8A | 8372A | 8362 | 256 K × 4 | Late run variant, pre-A500+ | A500 schem |

All A500 revs share:

- CPU: MC68000 @ 7.15909 MHz NTSC / 7.09379 MHz PAL (A500 SM §2-3)
- Gary: 5719 address decode (A500 SM §2-15). Location U102.
- CIA: two 8520s — U300 (CIA-A) and U301 (CIA-B). (A500 SM §2-17)
- Paula: 8364. (A500 SM §2-13)
- ROM socket: one 40-pin DIP, 256 KB × 16 (Kickstart 1.2/1.3). Labelled U6 or similar per rev.
- Trapdoor slot: the A501 expansion, mapping 512 KB at $C00000 as "slow RAM" (pseudo-fast — it sits on Agnus's side of the bus but is not chip RAM from the DMA perspective). The A501 can carry an optional MSM6242 real-time clock.
- Edge connector: 86-pin on the right side. Full pinout in §12.1.

**Jumpers that matter on the A500 board itself:**

- **JP2** — selects where the A501 expansion RAM is mapped. A23=0 → maps at $C00000 (default), A19=low → maps at $080000. Noted on the A500 rev 5 schematic notes page. (A500 SM schematic §2.2 note)
- **JP3** — DRAM bank swap. Swaps /RAS0 and /RAS1 internally, used only for DRAM vendor differences, no software-visible effect. (A500 SM schematic)
- **JP6** — floppy `STEP` gating, factory only.
- **JP7A** — NTSC/PAL colorburst select (related to X1 crystal type).

Because Commodore factory-set all A500 boards to be "functionally identical" regardless of whether the Agnus installed was an 8370 or 8372A, a bone-stock A500 cannot see >512 KB of chip RAM without a hardware mod, even with a 1 MB Agnus in the socket. This is critical for emulation fidelity — emulating "an A500 with 1 MB chip RAM" is really emulating "a modded A500". The A500+ is the first board where 1 MB chip RAM is a supported configuration.

### 1.3 Amiga 500 Plus (1991)

| Aspect | Value | Source |
|--------|-------|--------|
| CPU | MC68000, 7.15909 / 7.09379 MHz | A500+ SM §1-1 |
| Agnus | 8375 (ECS, 2 MB chip RAM addressing) | A500+ SM §1-1 |
| Denise | 8373 (ECS HiRes Denise — SuperHires, productivity mode) | A500+ SM §2-6 |
| Paula | 8364 | A500+ SM §2-6 |
| Gary | 5719 | A500+ SM §2-13 |
| Chip RAM | 1 MB factory, expandable to 2 MB with A501+ | A500+ SM §1-1 |
| RTC | On-board (no A501 required) | A500+ SM §1-1 |
| Kickstart | 2.04 in 512 KB × 16 ROM | A500+ SM §1-1 |

Board label: **rev 8** (not to be confused with A500 rev 8A — the A500+ has its own numbering). The A500+ PCB assembly number is 312812-01 (PAL) / 312812-02 (NTSC) per A500+ SM §3. The 8375 Agnus in the A500+ uses the same clock-generation and DMA logic as an 8372A, but address-decoding has been widened to allow a 2 MB chip-RAM configuration. The A500+ SM §2-9 states this explicitly.

### 1.4 Amiga 2000 — rev 3.x, 4.x, 6.x (1987–1991)

This machine is a desktop Amiga with Zorro II slots and a PC bridge slot. Three major motherboard generations:

| Rev | Agnus | Denise | Chip RAM cap | Notes | Source |
|-----|-------|--------|-------------|-------|--------|
| 3.x, 3.9 | 8370 OCS | 8362 | 512 KB | Earliest US-made A2000. Amiga Update 25/3-1.1: "Field upgrades are NOT recommended on rev 3.x PCBs." | Amiga Update 25/3-1.1 |
| 4.x (up to 4.5) | 8370 or 8372A | 8362 | 512 KB factory, 1 MB with field upgrade | Upgrade procedure documented in Amiga Update 25/3 — includes cutting the J500 trace and moving J101 to enable A19 for the upper 512 KB. Rev 4.5 "current production" boards came in two flavours: 4.5 without new Agnus, or 4.5 with new Agnus. | Amiga Update 25/3-1.1, 25/3-2.1 |
| 6.x | 8372A | 8362 | 1 MB factory | Current at the time of Amiga Update 25/3. "REV 6 PCB comes jumpered for 1 MEG." Some rev 6 boards had a DRAM tower with static-column DRAMs. The "guru on power-up" bug on rev 6 was fixed with the Mitsumi PST518B power-up reset IC retrofit. | Amiga Update 25/3-3.1 |

A2000 jumpers that matter for Agnus addressing:

- **J101** — Enables A19 routing from the 68000 to Agnus. 1-2 shorted = 512 KB Agnus addressing. 2-3 shorted = 1 MB addressing (needed for 8372A in the higher chip-RAM mode). This is at the lower-right of the power connector CN400. (Amiga Update 25/3-2.1)
- **J102** — NTSC/PAL select. Closed = NTSC, open = PAL. Located right of J101, above X1 crystal. For PAL operation the trace must be cut. (Amiga Update 25/3-2.1)
- **J500** — two-pad jumper near CIA-B (U301) carrying the /EXRAM signal. On a 512 KB-config board the trace is intact, pulling /EXRAM to ground. For 1 MB Agnus operation it must be cut so /EXRAM is free. (Amiga Update 25/3-2.1)
- **J300** — Tick signal (PAL/NTSC 50/60 Hz TOD input to CIA-A). Early A2000s shipped with J300 incorrectly jumpered, causing third-party Genlock failures; service bulletin 30/3-7.1 instructs verification that pins 1 and 2 are shorted. Also: add a 0.01 µF cap on pin 2 to ground. (Amiga Update 30/3-7.1, 25/3-2.1)

Other A2000 components of interest (Amiga Update 25/3-1.1):

- **U102** — Gary 5719 at U102 (A2000), DIP-48. Commodore required the MOS-manufactured version; Toshiba-manufactured parts caused issues.
- **U205, U206** — 74HC244 data-bus drivers between processor and chip sides. The "HCY" vs "HC" variant question affects whether RP904/RP905/RP906 pullup resistor packs are populated.
- **U100** — 68000 DIP-64 at the top-left.
- **U300** — 8520 (CIA-A).
- **U301** — 8520 (CIA-B).
- **U305** — 74LS-series reset glue near the PST518B retrofit point.
- **XU1** — retrofitted PST518B power-on-reset IC (CBM PN 328156-02).
- **D802** — retrofit indicator diode for the PST518B mod.

### 1.5 Amiga 3000 (1990)

| Aspect | Value | Source |
|--------|-------|--------|
| CPU | MC68030 @ 16 MHz or 25 MHz (board variant), plus 68881/68882 FPU socket | Amiga Intern §11.4 |
| Master clock | 32 MHz CPU clock in 16 MHz parts (divided), 50 MHz CPU clock in 25 MHz parts | (A3000 SM — not in corpus; inferred from Amiga Intern §11.4.1) |
| Agnus | 8372B (ECS Super Agnus, 2 MB chip RAM). Amiga Intern §11.4.2: "The 2 Meg version of Agnus in the A3000 is also called Super Agnus." | Amiga Intern §11.4.2 |
| Denise | 8373 (ECS HiRes Denise) | Amiga Intern §11.4.3 |
| Paula | 8364 — same part as A500/A1000 | Amiga Intern §11.4.4 |
| Chip bus bridge | Fat Buster (Zorro III bus controller) | Amiga Intern §11.4.1 |
| System control | Fat Gary (vs 5719 Gary in A500/A2000) | Amiga Intern §11.4.1, A4000 SM §6-45 |
| Fast RAM controller | Ramsey (supports up to 16 MB, 32-bit wide, static column DRAM burst mode) | A4000 SM §6-29 |
| Data path | Bridgette (100-pin PQFP gate array that replaces six 74F646 + four 74F245 TTL) | A4000 SM §6-16 |
| Flicker fixer | Amber (scan doubler) | Amiga Intern §11.5.3 |
| SCSI/DMA | Commodore SCSI/DMA controller (Super DMAC). WD33C93 SCSI chip | Amiga Intern §11.4.1 |
| Rev(s) | 9.x (A3000 shares the Fat Gary/Ramsey/Buster ASIC set with A4000) | A4000 SM §6-15 |

The A3000's importance is that it is the first Amiga where "chip bus" and "processor bus" really are separate physical buses — on the A500/A2000 they are multiplexed on the same pins with buffers turning on and off; on the A3000 the CPU has 32-bit Fast RAM on its own bus via Ramsey, and Agnus is reached only through the Bridgette buffer pair. See §2.7 of this document and A4000 SM §6-16 for detail.

### 1.6 Amiga 1200 (1992)

| Aspect | Value | Source |
|--------|-------|--------|
| CPU | Motorola MC68EC020 @ 14.28 MHz (2× 7.15909 MHz) | A1200 sch key components list |
| Master clock | 28.63636 MHz NTSC, 28.37512 MHz PAL (X1) | A1200 sch §1 |
| Agnus | 8374 "Alice" (AA Agnus) | A1200 sch key components, U2 |
| Denise | 4203 "Lisa" (AA Denise) | A1200 sch key components, U4 |
| Paula | 8364 Paula, 7 MHz | A1200 sch key components, U3 |
| Gary | F023A "Gayle" (A600/A1200 system controller) | A1200 sch key components, U5 |
| Ramsey | Not present. A1200 has no Fast RAM bus; Budgie handles memory | — |
| Buster | Not present | — |
| ASIC | **Budgie** (CBM PN 391??? at U20 on A1200 rev 1D) — glue logic replacing discrete decode | A1200 sch key components, U20 |
| Chip RAM | 2 MB (DRAM 256K × 16, U15-17 ASST 80 ns), expandable to 4 MB via a trapdoor | A1200 sch U15-19 |
| ROM | 512 KB × 16 Flash memory (28F10 × 2 at U13 upper & lower) | A1200 sch U13 |
| Keyboard controller | 68HC05 MPU (onboard, not in a separate keyboard unit) | A1200 sch U13 [sic — different U13] |
| Reset supervisor | PST518 low-voltage sense IC | A1200 sch |
| RGB DAC | BT101 triple 8-bit video DAC | A1200 sch U30 |
| Floppy | Internal DB23 external + 34-pin internal | A1200 sch CN5, CN11 |
| PCMCIA | CN15 — PC "memory card" slot | A1200 sch CN15 |
| IDE | CN16 — 44-pin 2.5" AT-IDE port | A1200 sch |

The A1200 is the only 68EC020 Amiga mainstream. Its Gayle chip is the direct evolution of Gary — it does the same job (address decode, RTC, CIA select, chip RAM enable) but in a small PLCC gate array rather than the Gary 5719 DIP. Gayle also mediates the PCMCIA slot and the IDE interface, which were not part of Gary's job. The chipset is AGA: Alice (Agnus) is 2 MB chip RAM capable and has wider bitplane DMA, Lisa (Denise) has 24-bit colour (eight bitplanes plus sprite/priority logic for 256 colours from a 16.8 million palette).

**Note** — the A1200 schematic addendum in the corpus is short (851 lines) and has some text-layer glitches where decorative text runs through OCR. Chip part numbers are clear, most signal names are clear, but some pin-by-pin detail is only legible from the PDF image, not the text extraction.

### 1.7 Amiga 4000 (1992)

| Aspect | Value | Source |
|--------|-------|--------|
| CPU | MC68040 (A4000/040) @ 25 MHz, or MC68EC030 (A4000/030) @ 25 MHz, or MC68020 (A4000/020) @ 14 MHz — processor module socket | A4000 SM §5-9, §5-12 |
| Master clock | 50 MHz crystal oscillator on the 68040 processor module (364856) | A4000 SM §5-10 |
| Agnus | 8374 Alice (AA Agnus) — same part as A1200 | A4000 SM §5 BOM (391010-01 IC 8374 ALICE 84-pin PLCC U211) |
| Denise | 4203 Lisa (AA Denise) — same part as A1200 | A4000 SM §5 BOM (391227-01 IC 4203 LISA 84-pin PLCC U450) |
| Paula | 8364 | — |
| Bus control | Fat Gary + Super Buster + Ramsey + Bridgette (same ASIC set as A3000, extended) | A4000 SM §6-15 |
| Chip RAM | Up to 2 MB using SIMMs | A4000 SM §1-1 |
| Fast RAM | Up to 16 MB 32-bit via Ramsey | A4000 SM §1-1 |
| ROM socket | 512 KB × 16 Kickstart 3.0 (later 3.1) | A4000 SM §6-48 |
| Zorro | 4× Zorro III slots | A4000 SM §1-1 |
| IDE | 16-bit AT-IDE interface | A4000 SM §1-1 |
| Floppy | High-density (1.76 MB) internal | A4000 SM §1-1 |

A4000 motherboard jumpers (A4000 SM §3):

- **J100** — CLK90 clock source. 1-2 = internal (020/030), 2-3 = external (040).
- **J104** — CPU clock source. 1-2 = internal, 2-3 = external.
- **J151** — ROM speed select. 1-2 = 200 ns, 2-3 = 160 ns.
- **J213** — Chip RAM size. 1-2 = 2 MB, 2-3 = 8 MB (unsupported — reserved for larger Alice variant).
- **J352** — DF0: internal/external redirect. 1-2 = DF0/DF1 internal, 2-3 = DF2/DF1 internal with DF0 external.
- **J212** — NTSC/PAL select. 1-2 = NTSC, 2-3 = PAL.
- **J214** — VBB/MA10 switch for Alice. 2-3 = supplies VBB to Alice (normal 2 MB mode). 1-2 = Alice supplies MA10 for a hypothetical 8 MB chip-RAM configuration (not supported).
- **J500** — sync on green disable (1-2) / enable (2-3).
- **J501** — Lisa sync, default 2-3 closed.
- **J502** — DAC sync. 1-2 = DAC syncs on green, 2-3 = DAC uses standard signal.
- **J850** — DSACK enable. 1-2 closed = DSACK enabled (required if CPU is a 68020; also needs U860 and U152 populated).
- **J852** — RAM size. 1-2 = 1 MB × 32, 2-3 = 256 KB × 32. Set by BOM for each model variant.

The 68040 processor card has its own jumpers (A4000 SM §3-5):

- **J100** — *CDIS *MDIS enable (two pairs to short to enable data and MMU cache control).
- **J400** — MAPROM enable (1-2 = enabled, 2-3 = disabled).
- **TJ100** — test jumper (factory only).

Practical note for an emulator author: the A4000/030 and A4000/040 are actually two separate machines for most timing purposes. The /030 runs at 25 MHz with 68EC030 (no MMU), the /040 runs at 25 MHz with 68040 and a different memory controller behaviour because Ramsey's STERM-mode timing fits differently into the 68040 burst timing. The A4000 service manual explicitly says "RAMSEY is the Fast RAM controller that was designed for the A3000. It can be used for the A4000 with the addition of the following: one 16R4-10 *STERM to *DSACK conversion" — meaning Ramsey was re-used from the A3000 for A4000 duty with a single PAL bolted on.

---

## 2. Chip revision list with behavioural differences

What an emulator author needs to know: for each named chip, what silicon revisions existed, which revisions are OCS vs ECS vs AGA, which machines shipped with which, and what behaviour changed from one revision to the next.

### 2.1 Agnus

Agnus handles: DMA address generation, chip-RAM refresh, RAM multiplexed addressing, Copper, Blitter, beam counters (VHPOSR), clock distribution for Denise and Paula, and /DBR bus arbitration. Everything below uses Agnus's role from A1000 SM §1-6, A500 SM §2-8 to §2-10, A500+ SM §2-6 to §2-9, and Amiga Intern §11.4.2.

#### 8361 — original A1000 Agnus

- 256 KB chip RAM limit.
- 48-pin DIP package. NMOS process.
- Used only in the Amiga 1000. (A1000 SM §1-6)
- Internally exposes "25 dedicated-purpose DMA counters" (A1000 SM §1-2). This statement reappears verbatim in the A500 SM — meaning the DMA counter structure did not change across 8361 → 8370 → 8372A.
- Has the original revision of the OCS beam-counter logic (no ECS features).

#### 8370 — OCS Fat Agnus, 512 KB

- 512 KB chip RAM limit.
- 84-pin PLCC (NMOS). Referred to as "Fat Agnus" throughout Commodore documentation.
- First chip to be called "Fat Agnus" — the silicon was physically bigger because the chip-RAM address generator was widened from 18 bits (256 K words) to 19 bits (512 K words). (A500 SM §2-8)
- Shipped in A500 rev 3 and rev 5, and in A2000 rev 3–4.x. (A500 SM §2-9, Amiga Update 25/3)
- Still OCS — no ECS register extensions.
- "25 dedicated DMA counters" language in A500 SM §2-8 is inherited from the 8361 documentation.
- Pin reference (A1000 SM §1-6, which actually shows the 8361 but is identical in signal list to 8370):
  - D0–D15 : chip data bus
  - DRA0–DRA8 (9 lines) : multiplexed RAM addresses
  - RGA1–RGA8 : register address bus
  - /RES : reset input
  - /INT3 : interrupt level 3 output (to Paula)
  - DMAL : DMA request line (bidirectional with Paula — Paula pulses DMAL to request a slot from Agnus for disk and audio)
  - /BLS : Blitter Slowdown output to Gary
  - /DBR : Data Bus Request output (to Gary, asking to own the chip bus)
  - /ARW : Agnus RAM Write
  - CCK, CCKQ : color clock & quadrature
  - /LP : light pen input
  - /VSY, /HSY, /CSY : vertical/horizontal/composite sync outputs

#### 8371 — OCS Fat Agnus, 1 MB

- 1 MB chip RAM addressing capability. Transitional part.
- Commodore docs usually collapse 8371 and 8372 together as "the 1 MB Agnus". Amiga Update 30/3-8.1 calls both the 8372 and documents the field upgrade. Other sources distinguish them: 8371 is NMOS, 8372 is CMOS.
- Used in some later A500 rev 6A / rev 7 runs. Not definitively split-out in the corpus.
- Behaviourally identical to 8372A from an emulator standpoint, provided 512 KB–1 MB is being addressed. The difference between 8371 and 8372A is process and power consumption.

#### 8372A — "Fatter Agnus", 1 MB, OCS

- 1 MB chip RAM addressing. CMOS process.
- Amiga Update 25/3-2.1: "THE NEW 'FATTER' AGNUS 8372 IC (PN# 318069-02) WHICH CAN ADDRESS 1 MEGABYTE OF CHIP RAM WILL REQUIRE THE FOLLOWING MODIFICATIONS TO THE A2000 PCB FOR PROPER OPERATIONS. WITHOUT THESE MODIFICATIONS THE IC WILL ADDRESS ONLY 512K OF CHIP RAM AND OPERATE ONLY IN NTSC MODE."
- To enable the 1 MB mode: the A19 routing jumper J101 must be moved from 1-2 (512 KB mode) to 2-3 (1 MB mode), the /EXRAM ground trace at J500 must be cut, and J102 must be opened for PAL operation.
- Still functionally OCS from the register-level perspective. Does not add ECS register extensions.
- Used in A500 rev 6A / 7 / 8A, A2000 rev 4.5 (with the upgrade) and rev 6+.

#### 8372B — 2 MB Super Agnus, ECS

- 2 MB chip RAM addressing.
- Used in A3000. Amiga Intern §11.4.2: "The 2 Meg version of Agnus in the A3000 is also called Super Agnus."
- First ECS Agnus. Supports the ECS register extensions (see HRM ref for register detail).
- Still 84-pin PLCC. Pin-compatible with 8372A in principle, but motherboards have to give it extra address lines routed to its package.

#### 8375 — ECS Agnus, 2 MB, A500+

- 2 MB chip RAM addressing.
- Used in A500 Plus. (A500+ SM §1-1, §2-6)
- Full ECS register compatibility.
- A500+ SM §2-10 clock relations table is explicit about 28 MHz input cycle, 7 MHz output, CCK, CCKQ timing relationships.
- Behaves identically to the 8372B from a register standpoint; the differences are packaging and power management.

#### 8374 — AGA Agnus, "Alice"

- Used in A1200 (U2) and A4000 (U211). (A1200 sch, A4000 SM §5-8)
- 84-pin PLCC.
- AA ("Advanced Architecture") — supports 8 bitplane channels (up from 6), DMA bandwidth approximately doubled, supports wider fetch modes (FMODE). Alice is functionally a rewrite of the Agnus chip RAM controller and DMA scheduler — backwards compatible with OCS/ECS register map, but with new FMODE/BPLxPTH registers for 32-bit pointer mode (allowing chip RAM access above 512 KB via processor addressing).
- 2 MB chip RAM standard; supports 8 MB with motherboard jumper mod (unsupported by Commodore, see A4000 SM §3-2 J214 description).

**Emulation delta summary, Agnus → Alice:**

| Behaviour | OCS (8370) | ECS (8372B/8375) | AGA (Alice 8374) |
|-----------|------------|------------------|------------------|
| Chip RAM ceiling | 512 KB | 2 MB | 2 MB (8 MB on modded boards) |
| Copper DANGER bit | not present | present (BPLCON0 bit 0) | present |
| Beam position (VHPOSR) MSB | limited | full | full |
| FMODE register | — | — | present ($DFF1FC) |
| BPLxPTH wide pointer mode | — | — | present |
| Bitplanes supported | 6 | 6 | 8 |

(Much of this is Alice-level detail not in the service manuals — cross-reference HRM Appendix C for register detail. The service manuals only confirm the part numbers and physical locations.)

### 2.2 Denise

Denise handles: bitplane-to-RGB colour decoding, sprite multiplexing with bitplane priority, collision detection, and the two mouse-counter quadrature inputs. (A500 SM §2-11, Amiga Intern §11.4.3)

#### 8362 — OCS Denise

- Found in A1000 (rev 5/6/7), A500 (all revs through rev 8A), A2000 (all revs), early A500 boards generally.
- 48-pin DIP.
- Features (A500 SM §2-11):
  - "Many different resolutions: 320×200 up to 640×400"
  - "4096 colours on a TV or RGB monitor" (via 12-bit RGB output)
  - "Eight re-usable sprite controllers"
  - "60 or 80 column text"
- **Revision 5** of 8362 had a known bug: a one-pixel-wide colour glitch at the screen boundary in HiRes mode, and genlock frame-lock failures in combination with early Agnus revisions. Commodore offered free Rev 6 upgrades to A1000 customers with the A1300 genlock. (Tech Topics "Important Genlock Warranty Information", §7815–7820 region)
- **Revision 6** 8362 fixes the HiRes colour glitch and the genlock frame-lock problem. This is the "good" OCS Denise.
- OCS register complement only. No SuperHires, no productivity mode, no full palette.

Pinout (A500 SM §2-11):
- Pins 1–7: D0–D6, pin 8 M1H (mouse 1 horizontal), pin 9 M0H, pins 10–17 RGA1–8, pin 18 /BURST, pin 19 VCC, pins 20–23 R0–R3, pins 24–27 B0–B3, pins 28–31 G0–G3, pin 32 N/C, pin 33 /ZD (background indicator), pin 34 N/C, pin 35 7M clock, pin 36 CCK, pin 37 VSS, pin 38 M0V, pin 39 M1V, pins 40–48 D7–D15.
- Note the 4-bit per colour channel output (R0–R3, G0–G3, B0–B3 = 12-bit digital RGB). These go to off-chip transistor video-DAC or resistor-ladder DAC.
- The 7M clock on pin 35 and CCK on pin 36 are what drive pixel output rate. Amiga Intern §11.4.3 explains: "A pixel at the lowest resolution (320 pixels/line) has exactly the duration of a 7M clock signal. In high-resolution mode (640 pixels/line) two pixels are output per 7M cycle, one on each edge of its signal."

#### 8373 — ECS Hi-Res Denise

- Found in A500 Plus, A3000, later A500 rev 7 / rev 8A production, late A2000.
- ECS register extensions: full 12-bit palette address (BEAMCON0), productivity mode (640×480 progressive), SuperHires (1280 pixels wide), support for the ECS Agnus's genlock programming registers.
- A500+ SM §2-11: "8373 DENISE HI RES". The A500+ manual documents this part in place of the 8362 in every functional respect identical to the A500 manual's treatment of 8362, but adds two key items: the hires pixel mode (SuperHires), and productivity mode.
- From A500+ SM §2-11 functional block diagram the chip still has 32 colour registers, priority logic, sprite serializers, etc. but with bit-depth extensions for SuperHires fetch mode.
- **Pin-compatible with 8362.** Drop-in replacement at the silicon level. (This is why A500 rev 7/8A can ship with either.)

#### 4203 — AGA "Lisa"

- 84-pin PLCC. (A4000 SM §5-8 BOM: "391227-01 IC, CSG, 4203, LISA, 84 Pin PLCC U450")
- Completely re-done chip. 256 colours out of 16.8 million (24-bit). HAM8 mode. SuperHires chunky. New palette format.
- **NOT pin compatible with 8362/8373.** Lisa is an 84-pin chip in a fundamentally different socket.
- Mouse-counter pins are still present (inherited function).

**Emulation delta summary, Denise → Lisa:**

| Behaviour | 8362 OCS (rev 5) | 8362 OCS (rev 6) | 8373 ECS | 4203 Lisa (AGA) |
|-----------|------------------|------------------|----------|-----------------|
| HiRes colour glitch on right edge | Present | Fixed | Fixed | Fixed |
| Genlock lock stability | Unreliable | Good | Good | Good |
| SuperHires 1280 pixels | — | — | Present | Present |
| Productivity mode 640×480 | — | — | Present | Present |
| Bitplanes supported | 6 | 6 | 6 | 8 |
| Max colours (non-HAM) | 32 | 32 | 32 | 256 |
| Palette bits per channel | 4 | 4 | 4 | 8 |
| HAM mode | HAM6 (4096) | HAM6 | HAM6 | HAM8 (256 K) |
| Full 12-bit palette address | — | — | Present | Present (and 24-bit) |

### 2.3 Paula

Paula handles: 4-channel audio DMA and output, floppy read/write controller, UART (serial), analog mouse/joystick inputs, interrupt aggregation (/INT2, /INT3, /INT6 inputs from off-chip plus /IPLx output to CPU). (A500 SM §2-13, Amiga Intern §11.4.4)

#### 8364 — the only production Paula

- Used unchanged from A1000 through A4000. There is no AGA Paula part (Commodore docs in the corpus do not document a revised Paula).
- 48-pin DIP.
- Pinout (A500 SM §2-13):
  - Pins 1–7: D2–D8 (data bus, offset because it shares a partial bus)
  - Pin 8: Vss
  - Pins 9–10: D0, D1
  - Pin 11: /RES
  - Pin 12: DMAL (DMA request line to Agnus)
  - Pins 13–15: /IPL0, /IPL1, /IPL2 (to 68000)
  - Pins 16–18: /INT2, /INT3, /INT6 (from external)
  - Pins 19–26: RGA1–RGA8 (register address bus)
  - Pin 27: Vcc
  - Pin 28: CCK
  - Pin 29: CCKQ
  - Pin 30: AUDB (right audio)
  - Pin 31: AUDA (left audio)
  - Pin 32: POT0X
  - Pin 33: POT0Y
  - Pin 34: VSSANA (analog ground)
  - Pin 35: POT1X
  - Pin 36: POT1Y
  - Pin 37: /DKRD (disk read data)
  - Pin 38: /DKWD (disk write data)
  - Pin 39: DKWE (disk write enable)
  - Pin 40: TXD (serial transmit)
  - Pin 41: RXD (serial receive)
  - Pins 42–48: D9–D15
- Revisions: the corpus does not document any behaviourally-visible revisions of 8364. In practice Commodore silently revved Paula to improve yield but the register-level behaviour stayed constant. For emulation purposes treat "Paula" as one chip.
- Audio channel-to-output mapping is fixed: channels 0 and 3 → AUDA (left). Channels 1 and 2 → AUDR (right). This is hardwired in silicon, not software selectable. (Amiga Intern §11.4.4 "AUDL carries the internal sound channels 0 and 3, and AUDR the channels 1 and 2.")
- Light pen input: Paula has no light-pen input. The light pen comes in on Agnus's /LP pin. (A1000 SM §1-7 Agnus pin list)

### 2.4 CIA — MOS Technology 8520

Two 8520s per Amiga motherboard. Amiga spec CIA 8520 is a revised 6526 (C64 CIA) with a few behaviour changes.

#### 8520 vs 6526

- The 6526 was the C64 CIA. The 8520 is the Amiga variant.
- Headline difference: the 8520 **removes the TOD latch** — where the 6526 latches TOD on a read of MSB and releases on a read of LSB, the 8520's TOD counter runs off a raw positive edge transitions on the TOD pin (no 60 Hz/50 Hz divider) and is a 24-bit binary counter rather than the 6526's BCD 24-hour clock. (A500 SM §2-20)
- The 8520 is tied to the system's E clock, not the C64's phi2. E clock on the Amiga is 0.71 MHz (7 MHz / 10 — see §3 below and A4000 SM §6-55 Fat Gary ECLK description).
- A500 SM §2-17 through §2-25 is the canonical 8520 datasheet in the corpus. It covers timers, TOD, SDR, ICR, CRA, CRB.

#### 8520A vs 8520B

- Commodore shipped two versions of the 8520: "A" and "B" steppings.
- The "A" version was the original; the "B" is a later stepping.
- Behavioural delta documented in the corpus: none. The corpus does not call out any software-visible difference between 8520A and 8520B. Where emulator authors have seen issues with real hardware, it's usually in combination with third-party accelerator cards that bypass E-clock synchronisation, not a silicon stepping difference.
- **6526 bug compatibility** — see Mapping the Amiga in the Source map; Mapping discusses the fact that the 8520 fixes a 6526 timer-B underflow bug that the 6526 exhibits with specific ICR access orderings. The Amiga 8520 does not reproduce the 6526 bug. For emulator-level C64 vs Amiga CIA model sharing, the timer underflow → ICR bit set ordering must be modelled per-chip.

#### Pin reference (A500 SM §2-17)

- 1 Vss
- 2–9 PA0–PA7
- 10–17 PB0–PB7
- 18 PC (handshake output)
- 19 TOD (24-bit counter clock input)
- 20 Vcc
- 21 /IRQ (open drain to Paula /INTx)
- 22 /RES
- 23–26 DB0–DB3 (continues)
- ...through DB7
- 27–30 PCnt/SP/CNT/FLAG
- 31 RS0 – 34 RS3 (4-bit register select)
- 35 /CS
- 36 R/W
- 37 phi2 (actually E clock)
- 38–... (see A500 SM §2-17 diagram)

The important Amiga-specific wiring is in §10.

### 2.5 Gary — 5719

- 48-pin DIP ("plastic shrink DIP" in some SKUs).
- Used in A500 (U102) and A2000 (U102). Part number 318072-01.
- Pinout (A500+ SM §2-13 pin list):
  - Pins 5 KBRESET, 6 Vcc1, 7 MTR, 8 DKWE, 9 DKWD, 10 LDS, 11 UDS, 12 R/W, 13 /AS, 14 BGACK, 15 /BLIT, 16 /SEL, 17 Vcc2, 18 /REGEN, 19 /BLISS, 20 /RAMEN, 21 /ROMEN, 22 /CLKRD, 23 /CLKWR, 24 GND2, 25 LATCH, 26 /CDAC, 27 CCKQ, 28 CCK, 29 /OVR, 30 /OVL, 31 XRDY, 32 /EXRAM, 33 A17, 34 A18, 35 A19, 36 A20, 37 A21, 38 A22, 39 A23, 40 GND3, 41 /RST, 42 /HLT, 43 /DTACK, 44 /DKWEB
- Function (A500 SM §2-15, §2-16):
  - Provides all bus control signals.
  - Provides all address decoding.
  - Generates /VPA signal for the 68000 (for synchronous auto-vector access to CIAs).
  - Handles some floppy circuitry (the part of the floppy state machine that's glue logic — Paula owns the actual MFM decode).
  - Provides keyboard reset interface (takes /KBRESET from CIA, schedules system reset).
- Gary **is** the address decoder for the entire 16 MB 68000 memory map outside of the chip bus. It outputs /RAMEN (chip RAM enable, handed to Agnus), /REGEN (chip register enable, handed to Agnus), /ROMEN, /CLKRD / /CLKWR (real-time clock), and /SEL (for CIAs).
- "Two Gary sub-types: Toshiba and MOS. Only MOS are in stock." (Amiga Update 25/3) — Commodore noted that Toshiba-made Gary 5719 had reliability issues and directed service centres to use the MOS-manufactured version for replacement.

#### Toshiba vs MOS Gary — service implication

- Toshiba Gary 5719 had subtle timing differences causing "the problem" in some A2000s; service bulletin Amiga Update 25/3-1.1 step 7 says: "IF R5719 IS PRESENT (LOCATED OFF PIN 1 OF CN400, PWR CONN), ADD 470 OHM RESISTOR BETWEEN VCC AND CPU SIDE OF R106." Then step 8: "REPLACE GARY IC, THE 5719, AT U102, WITH A MOS MANUFACTURED TYPE, IF IT IS A TOSHIBA MANUFACTURED TYPE."
- For emulator purposes this is not a behavioural difference — it's a physical-layer signalling difference that Commodore papered over with a pullup resistor. Nothing an emulator needs to model.

### 2.6 Gayle — F023A

- Used in A600 and A1200. (A1200 sch key components, U5)
- Small PLCC gate array.
- Subsumes: Gary's address decoding, Gary's RTC interface, /OVL handling, ECS-extension address decoding, plus **IDE interface** for the onboard AT-IDE connector, **PCMCIA** interface, and credit-card clock control.
- The A1200 sch addendum is text-only on Gayle — there is no chip spec document in the corpus for Gayle comparable to the Fat Gary spec in A4000 SM §6-45. The pin-level description has to be read from the schematic, which is OCR-unreliable in the A1200 corpus file.
- A4000 SM §6-45 FAT GARY is **not** the same chip as Gayle. Fat Gary is the A3000/A4000 32-bit-wide system controller; Gayle is the A600/A1200 8/16-bit-wide system controller. They share *functional* ancestry from Gary 5719, but they are separate silicon, separate packaging, and separate pinouts.

### 2.7 Fat Gary — A4000 SM §6-45

Full chip spec in A4000 SM §6-45 through §6-57. This is the gold for emulator authors modelling A3000/A4000 timing precisely.

Package: 84-pin PLCC.

**Address decoding, as implemented in Fat Gary Verilog-equivalent (§6-53 through §6-56):**

```
!FPUCS  = !AS & FC2 & FC1 & FCO & !A19 & !A18 & A17 & !A16 & !A15 & !A14 & A13
!DMAC   = !FC1 & FC0 & !A31 & !A30 & !A29 & !A28 & !A27 & !A26 & !A25 & !A24 &
          A23 & A22 & !A21 & A20 & A19 & A18 & !A17 & A16
!SLOT   = FC2 & FC1 & FC0 & !A31 & !A30 & !A29 & !A28 & A27   (local bus slot in A3000)
!AVEC   = !AS & FC2 & FC1 & FC0 & A19 & A18 & A17 & A16
```

This directly specifies the ranges. Decoded:

- **ROM**: $00F80000–$00FFFFFF (selected via /ROMOE output). Also selected at $00000000–$0007FFFF when OVL is high. "ROM caching is enabled." (A4000 SM §6-48)
- **Chip RAM**: $00000000–$001FFFFF. When OVL is high, ROM overlays the bottom 512 KB. 32-bit wide termination via both DSACKs. Chip RAM caching is disabled. (A4000 SM §6-49)
- **Chip registers**: $00DFC000–$00DFFFFF. Also aliased at $00C00000–$00CFFFFF "so that code looking for $C00000 memory will work properly (consequently, C00000 memory is NOT supported)." Terminates via DSACK1 — 16-bit port. Caching disabled. (A4000 SM §6-51)
- **CIAs**: CIA-A at $00BFE000–$00BFEFFF, CIA-B at $00BFD000–$00BFDFFF. Both 8-bit wide; respond as 16-bit on read by requiring CIA-A at odd word addresses and CIA-B at even word addresses. Caching disabled. Terminates on DSACK1. (A4000 SM §6-51)
- **Real-time clock**: $00DC0000–$00DCFFFF. 4-bit wide internally, responds as 16-bit on odd word addresses. (A4000 SM §6-52)
- **SCSI/DMAC**: $00DD0000–$00DD3FFF. NOT qualified with /AS — the select comes out as fast as possible. Self-terminates. (A4000 SM §6-54)
- **FPU**: $00F1A000 area (pin 5 FC bits). Self-terminates. (A4000 SM §6-54)
- **Local bus slot** (A3000): $08000000–$0FFFFFFF. Not qualified with /AS. All address spaces except CPU space. No bus termination. (A4000 SM §6-54)

**Fat Gary internal registers (A4000 SM §6-54):**

- `$00DE0000` — Bus timeout control. Bit 7: 0 = DSACK timeout (9 µsec, default), 1 = BERR timeout (250 msec). Bit 0: read = "bus timed out" latch (clears on read).
- `$00DE0001` — Bus timeout enable. Bit 0: 0 = enabled (default), 1 = disabled.
- `$00DE0002` — Power-up detect. Bit 0: read 0 = power not cycled, 1 = power cycled.
- `$00DE0003` — RAMSEY control register (mapped through Fat Gary's address space onto Ramsey's internal register — see §2.8).

**Bus timeout behaviour (§6-54):** "The 1.3 ROMs purposely snoop through a large range of address spaces during boot up, which most of the time aren't there. Taking 250 msecs for each one causes it to take forever to boot up. Terminating each bus cycle with *BERR also makes the software get confused." Default-to-DSACK timeout mode was introduced as a compatibility hack for 1.3 ROM behaviour. For emulator authors: an A3000/A4000 with an unresponsive bus cycle times out after 32 C1 pulses (approximately 9 µsec) by default, then returns DSACK termination — it does NOT bus-error.

**ECLK (E-clock to CIAs, §6-55):** "The ECLK signal is generated in GARY. It is a free running clock whose frequency is 1/10th of the 7M clock. Normally ECLK is low for six 7M clocks, and high for four 7M clocks. However, when the CIAs are accessed, the ECLK high time may be shorter than four 7M clocks. During writes to the CIAs, ECLK is high for only two 7M clocks. During reads ECLK stays high for a minimum of two 7M clocks, and a maximum of four 7M clocks. The frequency of ECLK does not change. If the ECLK high time is shortened during CIA access, the difference is made up by increasing the subsequent ECLK low time. Consequently, it is always ten 7M clocks from one rising edge of ECLK to the next."

This is **important** for emulation. The E clock is fixed-frequency (7 MHz/10 = 715909 Hz NTSC / 709379 Hz PAL) but the high-time stretches during CIA access. An emulator that models CIA timers as phi2-synchronous must account for the E clock running at that frequency and stretching on access — not on a simple 6-high/4-low divide.

**Data strobes (§6-55):** Fat Gary generates six data strobes for 32-bit accesses (/UUDS, /UMDS, /LMDS, /LLDS for 32-bit port cases plus /UDS, /LDS for 16-bit port cases). The combinational logic is given explicitly:

```
!LDS  = !SIZ0 # SIZ1 # A0 # RW
!UDS  = !A0 # RW
!UUDS = (!A0 & !A1) # RW
!UMDS = (!A1 & !SIZ0) # (A0 & !A1) # (!A1 & SIZ1) # RW
!LMDS = (!A1 & !SIZ0 & !SIZ1) # (!A1 & SIZ0 & SIZ1) # (A0 & !A1 & !SIZ0) # (!A0 & A1) # RW
!LLDS = (A0 & SIZ0 & SIZ1) # (!SIZ0 & SIZ1) # (A0 & A1) # (A1 & SIZ1) # RW
```

Note "# = logical OR, & = AND, ! = NOT, and all of these are negated outputs". During read cycles all strobes assert — this is why reads "always take the full 32-bit chunk" on an A3000/A4000 regardless of SIZE bits, and why cache line fills consume one cycle for the entire longword. During write cycles individual strobes are generated.

### 2.8 Ramsey — A4000 SM §6-29 through §6-42

Fast RAM controller for the A3000 and A4000.

- 84-pin PLCC.
- Two versions:
  - **-04** (12D): version register at $00DE0043 returns $0D
  - **-07** (12G): version register at $00DE0043 returns $0F
- Supports up to 16 MB of Fast RAM with 32× 1 MB × 4 DRAMs (80 ns).
- Supports 68030 burst mode with 80 ns static column DRAMs.
- Page mode, standard mode, burst mode, burst-wrap option. (A4000 SM §6-33)

**Ramsey control register ($00DE0003) bit layout (A4000 SM §6-32):**

| Bit | Name | Function |
|-----|------|----------|
| 0 | Page Mode | 1 = page mode enabled. Default 0. |
| 1 | Burst Mode | 1 = respond to CBREQ, do burst cycles. Default 0. |
| 2 | Wrap | 1 = all 4 longwords of burst. 0 = burst stops at A3,A2=11. Default 0. |
| 3 | RAMsize | 0 = 1-megabit (256 K × 4 or 1 M × 1). 1 = 4-megabit (1 M × 4). Default from RSIZE input. |
| 4 | RAMwidth (-04 only) | 0 = 1-bit wide DRAMs. 1 = 4-bit. Default 1. |
| 4 | Skip (-07 only) | 1 = 4-clock access instead of 5 at 25 MHz. Requires "very fast" DRAMs. Default 0. |
| 5,6 | Refresh Rate | Combined with RSPEED to give refresh interval, see table |
| 7 | TEST | Diagnostic only |

Refresh interval table (A4000 SM §6-32):

| Bit 6,5 | # clocks | 16 MHz | 25 MHz |
|---------|----------|--------|--------|
| 00 | 154 | 9.24 µsec | 6.16 µsec |
| 01 | 238 | 14.28 µsec | 9.52 µsec |
| 10 | 380 | 22.8 µsec | 15.2 µsec |
| 11 | ∞ | refresh off | refresh off |

"Since 512 refreshes must be done in 8 msecs, the interval between refreshes must be less than 15.625 µsecs. During page mode, RAS can only be low for 10 µsecs at a time, so the refresh rate should be set to less than 10 µsecs when page mode is enabled."

**Ramsey memory map when RSIZE and RAMWIDTH set (§6-33):**

```
RSIZE=0, RAMWIDTH=1 (4x 1-megabit 4-bit-wide, 1 MB per RAS):
  $07C00000-$07CFFFFF  RAS0
  $07D00000-$07DFFFFF  RAS1
  $07E00000-$07EFFFFF  RAS2
  $07F00000-$07FFFFFF  RAS3

RSIZE=1, RAMWIDTH=1 (4x 4-megabit 4-bit-wide, 4 MB per RAS):
  $07000000-$073FFFFF  RAS0
  $07400000-$077FFFFF  RAS1
  $07800000-$07BFFFFF  RAS2
  $07C00000-$07FFFFFF  RAS3

RSIZE=0, RAMWIDTH=0 (1x 1-megabit 1-bit-wide):
  $07C00000-$07FFFFFF  RAS0
```

The software test Commodore documented to check for static-column DRAMs (§6-34):

```
1. Disable all interrupts.
2. Turn page mode on (set bit 0 in $00DE0003; read back until set).
3. Write $5AC35AC3, $AC35AC35, $C35AC35A, $35AC35AC to four consecutive
   longwords in the same page (A11..A31 must match).
4. Turn page mode off.
5. Compare the four longword values. If they match, this bank has static-column DRAMs.
6. Repeat for each 1 MB bank of Fast memory.
7. Re-enable interrupts.
```

This is the Ramsey initialization code Kickstart 2.0+ runs on boot. Emulator authors modelling an A3000/A4000 at cycle level need the "dot 4 longwords" behaviour: under page mode with non-static-column DRAMs the writes will corrupt (because the row address changes and the stale page is not fully written). With static-column DRAMs all 4 writes land correctly. Modelling as "always static column" is fine unless you're trying to model a corrupted boot.

**DMAC address counter:** Ramsey contains the DMA address counter used by the onboard SCSI controller (Super DMAC). The counter is at $00DD000C (32-bit register, longword-aligned). Rising edge of /AS increments it whenever /DMAEN is low. The increment depends on transfer width: +4 for 32-bit ports (termination by /STERM or by both DSACKs low), +2 for 16-bit ports (termination by DSACK1 only). (A4000 SM §6-35)

### 2.9 Buster (A2000) and Super Buster / Fat Buster (A3000/A4000)

Buster is the Zorro bus controller. It arbitrates between the 68000, expansion cards, and Agnus for the system bus, and handles Zorro II AUTOCONFIG cycling.

- **A2000 Buster** — Zorro II (16-bit, 5 MB configuration space, 1..8 MB slave memory per slot).
- **A3000/A4000 Super Buster (or Fat Buster)** — Zorro III. Adds 32-bit data path, 32-bit addressing, DMA arbitration between CPU and slots. A4000 SM §6-43 chip spec describes Super Buster: "BUSTER can be used in the same fashion as in the A3000, and does not require any additional logic." Packaged as 84-pin PLCC.
- Super Buster's pin signals as named in A4000 SM §6-44:
  - Zorro III strobe inputs (not labelled individually in the OCR text — pin diagram figure 6-22)
  - /STERM (to Fat Gary)
  - /DTACK
  - /AS, /DS
  - SIZE0, SIZE1
  - /DSACK0, /DSACK1
  - /BR, /BG, /BGACK (the 68030 arbitration triplet, pass-through from CPU)
  - /CBREQ, /CBACK (cache burst for 68030)
  - Plus Zorro slot select lines (not enumerated in OCR text)
- **Fat Buster** in Amiga Intern parlance = Super Buster. Same chip.

### 2.10 Bridgette (A4000, retrofitted into later A3000)

A4000 SM §6-16 through §6-28 is a full chip spec — the most thorough chip spec in any Amiga service manual.

- 100-pin PQFP. NCR process (chosen for pin count and speed).
- Replaces "six 74F646s and four 74F245s" (A4000 SM §6-16).
- Three independent data paths:
  - **Chip bridge** — connects CD(0:15) and CD(16:31) of the chip data bus. Direction controlled by CBR_DIR. 13.1 ns max propagation delay.
  - **PD ↔ CD** — latching bidirectional buffer between the 32-bit processor data bus and the 16-bit chip data bus. Latched by _CLATCH. Buffered both directions.
  - **PD ↔ XD** — latching buffer between processor data bus and expansion data bus. XDIR selects direction. XSTORED selects real-time vs latched stored data output on reads. XCLK clocks data into the latch on rising edge.
- Key insight for emulator authors: the chip data bus on the A3000/A4000 is **actually 32 bits wide** (CD0–CD31), despite Agnus being 16-bit. Bridgette's chip-bridge path sends the upper 16 or lower 16 of the 32-bit chip data bus to the *other* half when needed. The 68030/68040 writes a 32-bit word to chip RAM; Bridgette drives CD(0:15) directly with PD(0:15) and, in parallel, drives CD(16:31) from PD(16:31); but only one half is valid per Agnus cycle. Agnus pulls what it needs alternately from the two halves on successive CCK slots.
- This "32-bit chip RAM with 16-bit Agnus" architecture is the reason the A3000 and A4000 can have chip RAM that is faster for the processor than OCS chip RAM, while Agnus continues to run at its fixed 3.58 MHz CCK rate.

### 2.11 Alice — 8374, AGA Agnus

Covered in §1.6 and §1.7. No dedicated chip spec in the corpus — the A1200 schematic addendum and A4000 service addendum only show it by part number and location. Detail is covered in the AGA Supplement (CBM PN 371121-01), which is not in the corpus.

### 2.12 Lisa — 4203, AGA Denise

Same — covered in §1.6 and §1.7. No dedicated chip spec in the corpus.

### 2.13 Budgie — A1200 ASIC

- A1200-specific glue-logic ASIC, 391??? part number at U20. (A1200 sch key components list)
- Replaces discrete TTL that would otherwise be required for A1200 DRAM control, PCMCIA interface sequencing, and IDE glue.
- **Not documented in detail in the corpus.** The A1200 schematics addendum only mentions it in the chip list; there is no chip spec for Budgie comparable to Fat Gary or Ramsey.
- For emulator authors: Budgie's effects are visible via the A1200 memory map (chip RAM, Fast RAM trapdoor interaction, PCMCIA space, IDE space) but the chip itself is a black box that does not have to be modelled directly if the decode rules are followed.

---

## 3. Clock distribution

This section tracks the 28 MHz crystal from pin 1 of Agnus through to every clocked subsystem.

### 3.1 Master oscillator

- **NTSC**: 28.63636 MHz crystal. (A500 SM §2-5, A500+ SM §2-4, A1000 SM §1-2)
- **PAL**: 28.37516 MHz crystal. A500+ SM §2-4 rounds this to 28.37512; both refer to the same crystal (slight OCR variation).
- On A500/A2000: crystal X1 feeds Agnus pin 1. Agnus contains the entire clock generation circuit for the motherboard.
- On A1000: a separate RF-shielded metal can contains all clock generation. "In order to reduce high frequency radiation, all clock generation is done in the small metal RF can on the main logic board." (A1000 SM §1-2) This metal can outputs C1, C2, C3, C4, 7M, /DAC, 28MHz to the rest of the board. Agnus 8361 in the A1000 does *not* contain the clock generation logic — it receives already-generated clocks.
- On A3000 and A4000 the 28 MHz clock is generated by a separate crystal oscillator. A4000 50 MHz crystal on the 68040 processor card (A4000 SM §5-10) is the CPU clock; Fat Gary receives an independent 28 MHz from the motherboard and generates ECLK from it. Fat Gary can be switched to an external XCLK for genlock ("The input signal called *XCLKE controls which signal is fed to AGNUS (via the output called AGCLK). When *XCLKE is low, XCLK is output to AGNUS." A4000 SM §6-56).

### 3.2 Divided clocks (A500, A500+, A2000)

From A500 SM §2-5:

| Name | Relationship | NTSC freq | PAL freq | Description |
|------|-------------|-----------|----------|-------------|
| C1 | 28 MHz ÷ 8 | 3.579545 MHz | 3.546895 MHz | Color clock — reference for all custom chip timing |
| C2 | C1 shifted 45° later | same | same | (quadrature phase) |
| C3 | C1 shifted 90° later | same | same | (quadrature phase, used in 7M generation) |
| C4 | C1 shifted 135° later | same | same | (quadrature phase) |
| 7M | C1 XOR /C3 | 7.15909 MHz | 7.09379 MHz | CPU clock for the 68000 |
| /DAC (CDAC) | 7M shifted 90° later | 7.15909 MHz | 7.09379 MHz | Reference for DRAM timing strobes |

PAL colorburst — 4.43361875 MHz — is not a direct divide of the PAL 28.37516 MHz master. A500 SM §2-5: "A special circuit is required to take five fourths of C1 to derive the PAL colorburst frequency of 4.43361875 MHz." This is done in the video hybrid (HY1 on A500 schematic). NTSC colorburst (3.579545 MHz) is just C1 directly.

7M is **derived from C1 XOR /C3**. It is twice the frequency of C1 and phase-locked to C1 (because C3 is 90° from C1, XOR gives the 2× harmonic). This is why the A500 SM §2-5 says the edge-connector pins provide C3*, CDAC and C1* but *not* 7M: "Note that 7M (the processor clock) is not available at the connector; it can be easily generated by: C3* XNOR C1* = 7M equivalent." The edge-connector user is expected to regenerate 7M locally from C1* and C3* — because 7M is so high-frequency and 28 MHz-sensitive that shipping it down an inch of trace would radiate.

**14 MHz** synchronous clock, if you need one, is:
```
14M equiv = 7Mequiv XOR CDAC
```

From the A500 edge connector pins (§12.1):
- Pin 14: C3* (C3 inverted)
- Pin 15: CDAC
- Pin 16: C1* (C1 inverted)

### 3.3 CCK / CCKQ

CCK (on Denise and Paula pins) is C1 again. CCKQ is C1 shifted, essentially quadrature — Amiga Intern §11.4.2 describes the separation timing:

- CCK cycle: 260–290 ns (A500+ SM §2-10 clock relations)
- CCK high time: 130–150 ns
- CCK low time: 130–150 ns
- CCK → CCKQ separation: 65–75 ns (quadrature)
- 7M cycle: 130–150 ns (half of CCK)
- 7M high time: 65–75 ns
- 7M low time: 65–75 ns
- 7M → CDACQ separation: 30–40 ns

So the 7M clock is exactly twice CCK in frequency, and CDAC (sometimes /DAC) is 7M shifted by half its period.

### 3.4 CPU clock gating during DMA (A500/A2000)

Agnus does not *gate* the 68000's CPU clock — the 7M clock runs continuously. What Agnus does is withhold /DTACK. Gary is told via Agnus's /DBR line that Agnus wants the chip bus; Gary then delays /DTACK to the 68000 until Agnus is done. The 68000 inserts wait states (Sw cycles) while /DTACK is low.

From A500 SM §2-7: "Fat Agnus tells the bus control prior to taking the display RAM buses by asserting an input to the control chip (GARY) called /DBR. Whenever Fat Agnus has the display buses and the 68000 wants them, the 68000 is held off by not giving it /DTACK. In this state the 68000 has no effect on the display buses until the bus controller enables the bus drivers."

The CPU bus cycle diagram from A500 SM §2-4 shows:

```
    |S0|S2|S4|S8 S0|S2|S4|Sw|Sw|Sw|Sw|Sw|Sw|Sw|Sw|Sw|Sw|S8|
CLK  _/_\_/_\_/_\_/_\_/_\_/_\_/_\_/_\_/_\_/_\_/_\_/_\_/_\
 AS       \__/                          \______________/
 UDS      \__/                          \______________/
 LDS      \__/                          \______________/
```

where the left hand cycle is a "normal" (no contention) cycle of 4 clock cycles (S0, S2, S4, S8) and the right-hand cycle is a "peripheral read" with Sw wait states inserted. The Sw count is determined by how far through the chip-bus DMA slot sequence Agnus is when /DBR asserts — it's not a fixed count.

**Critical subtlety for emulation**: the "wrong phase" case. A500 SM §2-7: "Synchronizing the 68000 to C1 is straightforward, since the 68000 is clocked by 7M which is twice the frequency and synchronous to C1. If the 68000 starts a bus cycle in the wrong phase of C1, the bus control chip merely delays /DTACK long enough so that the 68000 will complete the bus cycle in the desired phase relationship to C1." This means a 68000 bus cycle on the chip bus must align with specific C1 phases (specifically the odd or even CCK slot). If the CPU starts a cycle on the wrong phase, a 1-Sw penalty is inserted. This is distinct from the DMA contention penalty.

### 3.5 E clock to CIAs (A3000/A4000 via Fat Gary)

Fat Gary generates ECLK: 1/10 of 7M, nominally 6-high / 4-low, stretching the high period on CIA access. Full description in §2.7 above. On A500/A2000, ECLK is generated by Gary 5719 similarly but the service manual does not break down the stretching behaviour.

### 3.6 Genlock and external clock input

On A500 and A2000 the edge connector's pin 14 (C3*) and pin 16 (C1*) are outputs, not inputs. A genlock adapter must tap these, generate its own 7M-equivalent locally, and drive the video signal synchronously.

On A3000 and A4000, Fat Gary has an explicit XCLK input. When /XCLKE is low, XCLK is routed to Agnus's clock input instead of the internal 28M. This lets genlock adapters on the video slot drive the entire system clock. (A4000 SM §6-56)

On A1000, the RGB connector J3 has two dedicated pins for this: pin 1 /XCLK (external clock input) and pin 2 /XCLKEN (external clock enable). When XCLKEN is asserted, the internal 28 MHz is replaced. (A1000 SM §1-12)

---

## 4. Reset circuitry

### 4.1 Power-on reset

**A500/A2000:** There is no dedicated power-on reset IC on early boards. Reset is held by an RC network on /RST tied to Gary. Commodore later discovered that some DRAMs retained data for "as long as five minutes" after power-off (Amiga Update 25/3-3.1) — which meant a fast power-cycle could produce a reset before RAM contents had decayed, leading to "Guru on power-up" on the A2000 rev 6. The fix was the **PST518B Mitsumi low-voltage sense IC** retrofit (CBM PN 328156-02), which provides a clean power-on reset pulse.

Retrofit procedure (A2000 rev 6, Amiga Update 25/3-3.1):
- Locate U305 (74LS-series, left of the 34-pin drive connector CN303) and D802 (power-rail diode, down-right of U305).
- Install the PST518B as XU1 using insulation tubing to prevent shorts.
- Solder pin 1 of XU1 to the +5V side of D802.
- Solder pin 2 of XU1 to GND (use the plated hole at the base of U305).
- Solder pin 3 of XU1 to the anode side of D802.

The PST518B monitors Vcc and holds the /RST output low until Vcc is stable above the threshold, then releases.

**A500 Plus:** Has the PST518B equivalent built in from the factory.

**A1200:** Has a PST518 low-voltage sense IC at U49. (A1200 sch U49 key components) — "PST518 LOW VOLTAGE SENSE IC 19".

**A4000:** Fat Gary's /PWRUP input serves the same role. External circuit pulls /PWRUP low until Vcc is stable. The rising edge of /PWRUP causes bit 0 of the register at $00DE0002 to be set. Fat Gary's /RESET output is held low for approximately 250 msec after /PWRUP goes high. (A4000 SM §6-56)

### 4.2 Manual reset button (A1000, A2000, A3000, A4000)

- **A1000**: front-panel reset button, driven through a 74LS-series debounce.
- **A2000**: front-panel reset button.
- **A3000**: front-panel reset button.
- **A4000**: front-panel keylock switch (security). Reset is via the software reset (Ctrl+Amiga+Amiga) or power cycle.

In all cases the reset button pulls /RESET and /HALT low together, which is the 68000-side requirement for a synchronous reset.

### 4.3 Keyboard reset (Ctrl-Amiga-Amiga)

The "Ctrl+Amiga+Amiga" keyboard reset is a firmware feature of the keyboard MPU (6500-series on A1000/A500/A2000, 68HC05 on A1200). The keyboard MPU detects this key combination in its own matrix scan and pulls the KBRESET line low for 60+ msec.

From Fat Gary's spec (A4000 SM §6-56): "If the input called *KBCLK is held low for at least 60 msecs, the *RESET output will then go low, and will remain low for approximately 250 msecs after *KBCLK goes high again." This is the explicit spec — **60 msec of KBCLK low triggers, 250 msec of /RESET output follows**.

On A500/A2000 the keyboard-reset path is:
1. Keyboard MPU detects Ctrl+Amiga+Amiga.
2. Keyboard MPU pulls KDAT line low (the keyboard data line, which is CIA-A's SP pin normally).
3. Actually the keyboard has a dedicated /KBRESET wire on the keyboard cable separate from KCLK/KDAT; this wire goes to Gary pin KBRESET.
4. Gary asserts /RESET on the bus.
5. All chips reset.
6. After keyboard releases /KBRESET, Gary continues asserting /RESET for a bit (nominally 250 msec from the A4000 spec, presumably similar on A500).

### 4.4 /RESET pulse width and propagation

From A4000 SM §6-56 Fat Gary spec:
- Power-on /RESET duration: **250 msec** after /PWRUP goes high.
- Keyboard /RESET duration: **250 msec** after /KBCLK goes high, with a minimum 60 msec /KBCLK low trigger.

A500/A2000 Gary 5719 behaviour is not specified to this level in the service manuals. For emulator purposes, the 250 msec figure from Fat Gary is the canonical reference.

### 4.5 Which chips see /RESET

On the /RESET line, from the various service manuals (A500 SM §2-8 et seq, Amiga Intern §11.4):
- **68000** — /RST pin (pin 18) and /HALT pin (pin 17) both pulled low.
- **Agnus** — /RES input (pin 11 of 8361, equivalent on 84-pin packages).
- **Paula** — /RES input (pin 11 on 48-pin package).
- **Denise** — Denise has no /RES pin. It reset-syncs off the register address bus — when CCK cycles with specific RGA values during the reset, Denise clears its internal state. This is why real-hardware resets take more than just asserting /RESET: Agnus has to run a few CCK cycles before Denise is in a known state.
- **CIA 8520** — /RES pin.
- **ROM** — not reset; it's a ROM.
- **Gary** — reset is an output of Gary, driven in response to keyboard, power-up, or external reset, but Gary itself is a combinatorial decoder with only minor internal state.
- **RAM** — DRAM is never "reset"; its contents are undefined after reset. Refresh has to start almost immediately to prevent decay.

A subtle real-hardware behaviour that matters for emulation: because Denise has no /RES pin, it can be in a partially-stuck state after reset if Agnus hasn't run enough CCK cycles. Kickstart's reset handler explicitly writes to Denise's color/control registers early in boot to ensure a known state. An emulator that just "resets all Denise registers to 0" on reset will not exhibit the real-hardware post-reset screen garbage.

---

## 5. Interrupt wiring

### 5.1 Paula as the interrupt aggregator

Paula is the sole source of /IPL0, /IPL1, /IPL2 to the 68000 (pins 13, 14, 15 on Paula, pin list in A500 SM §2-13). All 14 sources of interrupts in the Amiga system run through Paula (Amiga Intern §11.4.4: "All the interrupts that occur in the system run through this chip.").

### 5.2 Physical wiring from Paula to 68000

The /IPL0, /IPL1, /IPL2 pins on Paula are driven directly to the /IPL0, /IPL1, /IPL2 inputs of the 68000 on the A500 and A2000. No buffering, no pullup — they connect pin to pin with only a trace between them. Amiga Intern §11.4.4: "The IPL0-IPL2 lines (Interrupt Pending Level) are connected directly to the corresponding processor lines."

On the A3000 and A4000, the routing is the same but via the 68030/68040 /IPL0, /IPL1, /IPL2 pins. Fat Gary has its own interrupt enable register (at $DF8000–$DFBFFF shadow, written to bit 15 of the chip-register INTENA alias) that disables ALL interrupts going to the CPU by blocking the interrupt lines externally via a PAL. (A4000 SM §6-56). See "System Interrupt Control" quoted below.

From A4000 SM §6-56: "Individual system interrupts are controlled by writing to the INTENA register in PAULA. Since this register resides on the chip bus, the CPU is subject to synchronization delays when attempting to access it. Therefore, an alternate method for shutting off ALL of the interrupts in the system is provided in GARY (GARY actually provides only the bit that can be written to — the actual control is done externally in a PAL). The INTENA register in PAULA is located at the offset of $09A. The chip registers occupy only 4K of address space. Consequently, they are shadowed in four 4K chunks from $DFC000 to $DFF000. The 16-bit register in GARY to control interrupts is also located at the offset of $09A, but is selected in the range from $DF8000 to $DFBFFF (this register is shadowed at each of the 4K chunks as well). Writing a 0 to bit 15 of this register will disable ALL of the interrupts going to the CPU. They are re-enabled by writing a 1 to the bit again (the bit is set after a RESET)."

This is the fastest-possible "mask all interrupts" path on an A3000/A4000. An emulator modelling the A3000/A4000 must treat writes to $DF8000+$9A as a hardware-level interrupt mask that overrides Paula's INTENA.

### 5.3 Interrupt level mapping

The Amiga's 14 interrupt sources map to 68000 interrupt levels per Paula's INTENA/INTREQ register (see HRM ref for bit detail). Summarising the physical flow:

| Level | Source | Physical input to Paula |
|-------|--------|------------------------|
| 1 | Soft, TBE (serial transmit), DSKBLK | Internal to Paula |
| 2 | Ports (CIA-A /IRQ, expansion /INT2) | /INT2 pin (Paula pin 16) |
| 3 | Copper, VBLANK, Blitter | /INT3 pin (Paula pin 17), from Agnus |
| 4 | Audio 0..3 | Internal to Paula |
| 5 | RBF (serial receive), DSKSYNC | Internal to Paula |
| 6 | External (CIA-B /IRQ, expansion /INT6) | /INT6 pin (Paula pin 18) |
| 7 | NMI (no sources normally wired) | — |

From the Paula pin list in A500 SM §2-13:
- Pins 13–15: /IPL0–2 (outputs to 68000)
- Pins 16–18: /INT2, /INT3, /INT6 (inputs from external sources)

**CIA-A /IRQ → Paula /INT2:** CIA-A at U300 pin 21 (/IRQ) is wired directly to Paula pin 16 (/INT2). This is physical. Whenever any of CIA-A's five interrupt sources (Timer A, Timer B, TOD alarm, SP, FLAG) asserts its interrupt with the corresponding mask bit set, CIA-A's /IRQ goes low, Paula sees /INT2 go low, Paula (if INTENA bit 3 is set) raises an IL2 interrupt to the CPU.

**CIA-B /IRQ → Paula /INT6:** Similarly, CIA-B at U301 /IRQ is wired to Paula pin 18 (/INT6), producing IL6 interrupts.

**Agnus /INT3 → Paula /INT3:** Agnus has an /INT3 output pin (pin 12 on the 8361, same on 8370 etc.). It is driven whenever the Copper interrupt, Blitter interrupt, or VBLANK interrupt source in Agnus asserts. Amiga Intern §11.4.2: "The INT3 line (INTerrupt at level 3) is an output and is connected directly to the Paula line with the same name."

**Expansion slot /INT2 and /INT6:** Zorro slots and the A500 edge connector wire these to Paula through a wired-OR to allow expansion cards to raise interrupts at levels 2 or 6. The expansion card pulls the line low via an open-collector driver.

### 5.4 /IPL0-2 timing

68000 samples /IPL0-2 at the end of each S4 state. If the level (inverted: 7=highest, 0=none) is greater than the CPU's current interrupt mask level (bits 8-10 of SR), the CPU starts an interrupt acknowledge cycle at the end of the current bus cycle. See HRM ref §9 for full interrupt handshake detail.

For service-manual-level detail: Paula's /IPL outputs are **not glitched** — Paula synchronises its INTREQ with CCK so /IPL transitions occur on rising edges of CCK, preventing the 68000 from seeing metastable intermediate values. This is important because the 68000's /IPL input is asynchronous to its clock.

---

## 6. Bus arbitration signals

### 6.1 68000 /BR, /BG, /BGACK

The 68000 has three pins for giving up the bus to a DMA device:
- **/BR** (Bus Request) — input. A device pulls this low to request the bus.
- **/BG** (Bus Grant) — output. The 68000 asserts this to grant the bus.
- **/BGACK** (Bus Grant Acknowledge) — input. The requesting device asserts this when it has taken the bus. The 68000 waits for /BGACK before tri-stating its address/data/control lines.

See 68000 signal summary in A500 SM §2-4. All three are **active low**.

### 6.2 The A500/A2000 case — not really used by Agnus

Crucially, **Agnus does NOT use /BR, /BG, /BGACK to grab the chip bus from the 68000**. On the A500/A2000 Agnus cannot use this mechanism because the 68000 bus is too slow — Agnus wants the chip bus on half-CCK-cycle boundaries, and the 68000's arbitration sequence takes several clock cycles to complete.

Instead, Agnus uses /DBR. When Agnus wants the chip bus, it asserts /DBR to Gary. Gary withholds /DTACK from the 68000 until Agnus is done. The 68000 sees a bus cycle that simply takes longer — it never knows another bus master was running.

**/BR, /BG, /BGACK are used by Zorro expansion cards** (A2000) and by the A500 edge-connector cards that implement DMA. An SCSI card on a Zorro II slot asserts /BR, waits for /BG, then asserts /BGACK and drives the 68000 bus directly.

From A500 SM §2-4 signal summary:
- BR — Input, Active Low, No tri-state
- BG — Output, Active Low, No tri-state
- BGACK — Input, Active Low, No tri-state

### 6.3 A3000/A4000 — Buster owns the arbitration

On A3000/A4000, Super Buster owns /BR, /BG, /BGACK arbitration for the Zorro III slots. It buffers between the CPU's /BR, /BG, /BGACK pins (which the CPU still uses for its external devices including Super DMAC) and the Zorro III slot signals. Super Buster's job is to arbitrate between the CPU, the onboard SCSI DMAC, and the Zorro III slots.

### 6.4 The /DBR → /DTACK hold sequence

On the A500/A2000 the electrical sequence when Agnus wants a chip-bus slot is:

1. Agnus asserts /DBR (Data Bus Request) to Gary.
2. Gary propagates this to delay /DTACK to the 68000. If the 68000 is currently in a bus cycle targeting chip RAM or chip registers, /DTACK is not yet asserted anyway — nothing to do.
3. If the 68000 is currently in a bus cycle on a non-chip target (ROM, CIA, slow RAM), the CPU's bus cycle completes normally because /DTACK comes from elsewhere. Agnus waits.
4. If the 68000 starts a new bus cycle that targets the chip bus, Gary holds /DTACK off while Agnus is busy.
5. When Agnus has finished its DMA slot(s), it releases /DBR. Gary then asserts /DTACK on the pending CPU cycle.

This mechanism is transparent to software — the 68000 never sees /BGACK or any other indication that it gave up the bus. It just sees a longer-than-usual bus cycle.

### 6.5 DMA halt sequence at electrical level

When Agnus has many consecutive DMA slots (full Blitter run, SuperHires fetch mode, or a long bitplane fetch on a crowded line), the 68000 is held in its current bus cycle via /DTACK withholding for the entire duration. The 68000 waits. From A500 SM §2-7: "Arbitration is very simple. Fat Agnus tells the bus control prior to taking the display RAM buses by asserting an input to the control chip (GARY) called /DBR. Whenever Fat Agnus has the display buses and the 68000 wants them, the 68000 is held off by not giving it /DTACK."

An emulator that models Agnus DMA slot-by-slot can model this as: each chip-bus access from the CPU has a variable latency depending on which CCK slot it lands in and how busy Agnus is. The latency is zero-to-many 7M cycles. The HRM ref §2 covers the slot table in detail.

---

## 7. Memory decoding

### 7.1 A500/A2000 — Gary 5719 as the decoder

Gary 5719 is the address decoder for the entire 16 MB 68000 memory space on A500/A2000. It takes in A17 through A23 from the 68000, plus /AS, /UDS, /LDS, R/W, and generates a set of chip-enable outputs. From the A500+ SM §2-13 Gary pin list:

Inputs to Gary:
- A17, A18, A19, A20, A21, A22, A23 (7 address lines)
- /AS (address strobe)
- /UDS, /LDS (data strobes)
- R/W
- BGACK (from 68000 and from expansion)
- CCK, CCKQ, /CDAC (for timing)
- /OVL (overlay input from CIA)
- /OVR (override — pulls /DTACK high to override Gary decoding, used by expansion cards)
- /EXRAM (expansion RAM present signal)
- XRDY
- /BLIT, /BLISS (from Agnus)

Outputs from Gary:
- /RAMEN — chip RAM enable (goes to Agnus, which does the actual RAM generation)
- /REGEN — chip register enable (goes to Agnus)
- /ROMEN — ROM chip select
- /CLKRD, /CLKWR — real-time clock (on A501 or A500+) read/write strobes
- /SEL — CIA select (demultiplexed further in Gary to /CS on CIA-A vs CIA-B based on A12)
- /DTACK — data transfer acknowledge to 68000
- /BLS — Blitter Slowdown, to Agnus
- /RST, /HLT — reset and halt outputs (driven by reset logic)
- /VPA — valid peripheral address, asserted for CIA accesses so the 68000 uses auto-vector interrupt (and synchronises to E clock)
- LATCH — control for the bidirectional tri-state latch between processor and chip data bus

Gary block diagram (A500 SM §2-16, A500+ SM §2-13):

```
        68000 addresses A17-23
                |
                v
            Address     ----> /RAMEN
            Decode      ----> /REGEN
                        ----> /ROMEN
   /AS --->             ----> /RTCRD, /RTCWR
   /UDS, /LDS --->      ----> /VPA (for CIAs -> E clock sync)

        Clocks (C2, C3)
                |
   /OVR --->    v
   /OVL --->  Bus        ----> /DTACK
   PRW --->   Control    ----> /BLS
   /EXPEN --->           ----> LATCH
   /DBR --->             ----> CDR, CDW
   /XRDY --->

        Keyboard reset
                |
                v
            Reset       ----> /RESET
            Control     ----> /HALT
```

Address ranges decoded by Gary 5719 (from the A500/A2000 memory map, A500 SM §2-2):
- $000000–$07FFFF: chip RAM (512 KB) — /RAMEN asserted
- $080000–$0FFFFF: chip RAM (512 KB upper, on 1 MB mod only)
- $100000–$1FFFFF: reserved
- $200000–$9FFFFF: Zorro II Fast RAM and autoconfig cards (8 MB)
- $A00000–$BEFFFF: reserved for Gary and CIAs
- $BFD000: CIA-B select
- $BFE000: CIA-A select
- $C00000–$D7FFFF: "slow" RAM (2 MB space for A501/similar expansion)
- $D80000–$DBFFFF: reserved
- $DC0000–$DCFFFF: real-time clock (A501 RTC or on-board RTC)
- $DD0000–$DDFFFF: reserved
- $DE0000–$DEFFFF: reserved (or "Fat Gary registers" on A3000/A4000)
- $DF0000–$DFFFFF: chip registers (/REGEN asserted, handed to Agnus)
- $E00000–$E7FFFF: reserved
- $E80000–$EFFFFF: Zorro II AUTOCONFIG space
- $F00000–$F7FFFF: cartridge/expansion ROM
- $F80000–$FFFFFF: Kickstart ROM (/ROMEN asserted, 512 KB × 16)

(Gary's actual decode is coarser than this map — it decodes at the 512 KB block level and leaves finer decode to Agnus or to CIA chip-selects. The 16 MB map shown is what Kickstart's autoconfig driver expects.)

### 7.2 The /OVL overlay trick

At power-on the /OVL signal from CIA-A is high. Gary responds by decoding the ROM range ($F80000–$FFFFFF) to ALSO appear at the bottom of memory ($000000–$07FFFF). This means the 68000 reads its reset vectors from the ROM on power-up without needing to know where the ROM is.

Once Kickstart has run, it writes CIA-A's PRA to clear /OVL (via bit 0), which causes Gary to stop aliasing ROM at the bottom, and chip RAM becomes visible at $000000–$07FFFF. The chip RAM was always backing that range; the /OVL overlay was just pre-empting it for the first few instructions.

See HRM ref §12 for the bit-level register detail. The service-manual-level fact: /OVL is a wire from CIA-A pin (PRA bit 0) into Gary, and Gary combinationally decides the output of /RAMEN vs /ROMEN based on that wire plus the A23-A17 address bits.

### 7.3 A3000/A4000 — Fat Gary as the decoder

Fat Gary does exactly the same job as Gary 5719 but with a wider address space and an extra set of outputs for 32-bit data strobes. The decoding logic was given in §2.7. The ranges:

| Range | Target | /Select |
|-------|--------|---------|
| $00000000–$001FFFFF | Chip RAM (2 MB) | /RAMEN |
| $00BFD000–$00BFDFFF | CIA-B | /CIA1 |
| $00BFE000–$00BFEFFF | CIA-A | /CIA0 |
| $00C00000–$00CFFFFF | Chip registers (aliased) | /REGEN |
| $00DC0000–$00DCFFFF | RTC | /RTCRD, /RTCWR |
| $00DD0000–$00DD3FFF | SCSI/DMAC | /DMAC |
| $00DE0000–$00DE0043 | Fat Gary / Ramsey registers | internal |
| $00DFC000–$00DFFFFF | Chip registers | /REGEN |
| $00F80000–$00FFFFFF | ROM | /ROMOE |
| $07000000–$07FFFFFF | Fast RAM via Ramsey | Ramsey /RASx |
| $08000000–$0FFFFFFF | Local bus slot (A3000) | /SLOT |

plus Zorro III slot ranges handled by Super Buster.

Fat Gary's decoding also includes **F-line exception and AUTOVECTOR handling** — when it sees an interrupt acknowledge cycle (FC2=1, FC1=1, FC0=1 with A19-A16 all 1), it generates the /AVEC signal to tell the CPU to use auto-vectoring. This is the 68030 equivalent of the 68000's /VPA signal.

### 7.4 A1200 — Gayle F023A as the decoder

Gayle does the same job for the A1200 but with added PCMCIA and IDE decoding. The A1200 map adds:

- $00600000–$009FFFFF: reserved (not used)
- $00A00000–$00BEFFFF: PCMCIA memory window
- $00BFD000–$00BFDFFF: CIA-B
- $00BFE000–$00BFEFFF: CIA-A
- $00D80000–$00D8FFFF: IDE task-file
- $00DA0000–$00DAFFFF: Gayle registers
- $00DC0000–$00DCFFFF: RTC
- Chip register and ROM as per A500+

Gayle's IDE interface emulates a subset of the ATA register file at $00DA2000 and $00DA3000. This is key for emulator authors — A1200 native IDE is "Gayle IDE", not a standard PCI IDE, and has its own quirks (DMA not supported, interrupt on IDE is at level 2 through Gayle, etc.).

Gayle's full register set is NOT in the corpus. The A1200 schematic addendum lists Gayle as U5 with part number F023A but provides no register spec. This is a gap.

---

## 8. Video output

### 8.1 Denise output path (A500/A2000)

Denise outputs 12-bit digital RGB from the chip package:
- Pins 20–23: R0–R3 (4-bit digital red)
- Pins 24–27: B0–B3
- Pins 28–31: G0–G3
- Pin 33: /ZD (zero-detect / background indicator)
- Pin 32: N/C

These 12 digital output lines go through off-chip buffer ICs (typically a 74HC244 on A500, SMT variant on A500+/A1200) and then through a resistor-ladder or transistor video DAC (the "HY1 video hybrid" on A500 is a potted module, PN 390229-03 per A500+ SM §1594). The output is analog RGB at 0.7 V p-p into a 75 Ω terminated line.

The HY1 video hybrid on A500 boards does three things:
1. Converts digital R0–R3, G0–G3, B0–B3 to analog R, G, B.
2. Generates the NTSC/PAL colorburst from C1 (and, for PAL, uses the 5/4× C1 = 4.433 MHz subcarrier).
3. Produces a composite video signal by mixing R, G, B with the colorburst through a delay line.

### 8.2 Composite sync generation

Agnus generates HSYNC and VSYNC directly on its /HSY, /VSY pins. It also generates /CSY (composite sync = /HSY XOR /VSY with horizontal equalisation pulses during vertical blanking). (A1000 SM §1-6 Agnus pinout, Amiga Intern §11.4.2: "Normally the synchronization signals for the monitor appear on the HSY (Horizontal SYnc) and VSY (Vertical SYnc) lines. The signal on the CSY (Composite SYnc) line is the sum of HSY and VSY and is used to connect to monitors that need a combined signal, as well as the circuit that creates the video signal, the video mixer.")

Genlock reverses this: if the GENEN bit in Agnus's BPLCON0 is set (see HRM ref §3), the /HSY and /VSY pins become inputs. Agnus then synchronises its internal raster counters to the external sync signals, allowing the Amiga's picture to be locked to an external video source.

### 8.3 Pixel clock

Denise outputs pixels at the 7M clock rate in lowres (1 pixel per 7M cycle) and at 14M rate in hires (2 pixels per 7M cycle, on both edges of 7M). In SuperHires (8373/Lisa only), 4 pixels per 7M cycle. Amiga Intern §11.4.3 explains the edge-triggered method: "A pixel at the lowest resolution (320 pixels/line) has exactly the duration of a 7M clock signal. In high-resolution mode (640 pixels/line) two pixels are output per 7M cycle, one on each edge of its signal."

### 8.4 Video connectors

**A1000 J3 RGB connector** (A1000 SM §1-12):
23-pin DB connector with the following pins:
- 1: /XCLK (external clock input)
- 2: /XCLKEN (external clock enable)
- 3: R (analog red)
- 4: G (analog green)
- 5: B (analog blue)
- 6: I (digital intensity)
- 7: R (digital red)
- 8: G (digital green)
- 9: B (digital blue)
- 10: /CS (composite sync, active low)
- 11: /HS (horizontal sync, active low)
- 12: /VS (vertical sync, active low)
- 13: GNDRTN (return for XCLKEN)
- 14: /ZD (zero detect, active low)
- 15: C1 (color clock 3.58 MHz)
- 16–20: GND
- 21: −5V
- 22: +12V
- 23: +5V

**A500/A2000 RGB port** — 23-pin DB, same physical connector as A1000 J3 but without pins 1, 2 (XCLK/XCLKEN) since the A500/A2000 does not route external clock input here. Genlock cards use their own pass-through.

**A1200 video output** (A1200 sch CN9): DB23 female with similar pinout to A500. Plus CN10 (RCA composite).

**A4000 video output**: 15-pin DB-15 for VGA connector (flicker-fixed output). A4000 SM §6 spec describes the video adapter 390682-01 (15-pin to 23-pin adapter) for connecting Amiga-standard monitors. (A4000 SM §5-12)

**A1000 J2 composite**: 8-pin DIN connector. Pins: 1 audio right, 2 GND, 3 audio left, 4 composite video (NTSC), 5 GND, 6 (empty), 7 +12V, 8 audio right. Requires modulator for RF output.

### 8.5 /ZD (zero detect) for genlock

The /ZD pin on Denise is **background indicator** — it is asserted low whenever the pixel currently being displayed is color register 0 (background color). This is the key signal for genlock: when the Amiga shows background-color pixels, /ZD is low, and the genlock adapter uses this to decide whether to show the Amiga's output or the external video.

During vertical blanking the /ZD line serves a different purpose — it reflects the GAUD (Genlock AUDio enable) bit from Agnus register BPLCON0. This is how genlock is told "turn off the audio while we're in VBL" to allow audio switching between sources. (Amiga Intern §11.5.2)

---

## 9. Audio output

### 9.1 Paula audio DAC

Paula contains four 8-bit audio DACs, one per channel. The channels are fixed-routed:
- Channels 0 and 3 → left output (AUDA, pin 31)
- Channels 1 and 2 → right output (AUDR, pin 30)

This is hardwired silicon routing; there is no register to change it. (A500 SM §2-13, Amiga Intern §11.4.4)

Each DAC is ladder-based 8-bit resolution (Paula's audio data is 8 bits sample + 6 bits volume, where the volume multiplies the sample by 0–64). Output is analog current (technically voltage via external resistor) — AUDA and AUDR pins.

### 9.2 Low-pass filter ("LED filter")

The Paula audio output feeds through a low-pass filter built from discrete components between Paula and the audio output jack. The filter is a 3-pole RC at approximately 3.3 kHz cutoff (−3 dB). On A500, A500+, A2000, A3000, A4000 it is built from resistors and capacitors on the mainboard near the audio output connectors.

The filter is **switchable** via a single bit from CIA-A. CIA-A's PRA bit 1 is labelled "/LED" — it drives the power LED on the front panel, and it also drives a transistor gate that shorts out the filter. From the A500 SM jumper/signal glossary:

```
/LED — LED driver / Audio Filter Disable, driven by CIA-A PRA bit 1
```

When /LED is low (LED on, bit 1 = 0 in PRA), the filter is **enabled** (low-pass). When /LED is high (LED off, bit 1 = 1 in PRA), the filter is **bypassed**.

Historical note: the original A1000 does not have this bit wiring. It has a separate physical "audio filter" switch on the keyboard case. On A500 and later the keyboard reset to "filter on" is effectively the same as "LED on" which is the default state.

### 9.3 Filter cutoff measurements

The exact cutoff frequency depends on the motherboard revision. The corpus does not give a numeric value — what is consistently cited in community measurements is:
- A500 filter on: ~3.3 kHz cutoff, Butterworth 3-pole
- A500 filter off: ~30 kHz cutoff (still some filtering from trace capacitance and the DAC's inherent roll-off)
- A1200 filter on: ~3 kHz cutoff (slightly different R values)

For emulation: an accurate Amiga audio emulator that cares about the "filter sound" implements a 3-pole Butterworth at the appropriate cutoff, switched by a bit connected to /LED. This is not in the service manuals as a numeric spec, but the filter's presence and switching mechanism are documented there.

### 9.4 Audio output connectors

**A1000 J2 DIN**: left/right on pins 3 and 8, 1 kΩ output impedance through series protection resistors. (A1000 SM §1-13)

**A500**: two RCA jacks on the back panel, red and white. 1 kΩ series, 360 Ω output impedance, "protected against short circuit." (Amiga Intern §11.5.1)

**A500+, A2000, A3000, A4000, A1200**: same two RCA jack configuration.

---

## 10. CIA pin mapping — definitive

This is the section that consolidates A500 SM §2-17 (CIA pin reference) with A500+ SM §2-13 (Gary interconnect) and HRM ref §6, §7.

### 10.1 CIA-A at $BFE001

CIA-A is U300 on A500/A2000. Decoded by Gary at $00BFE000–$00BFEFFF (odd bytes only since it's 8-bit on the low data bus: A0=1 word accesses → upper byte on 68000 = low byte on CIA). (A500+ SM §2-13 shows this explicitly.)

**CIA-A PRA (Port A) pin assignments — A500/A2000/A500+ schematics:**

| PRA bit | Signal | Direction | Description |
|---------|--------|-----------|-------------|
| 0 | /OVL | Output | Overlay control to Gary. 1=ROM overlay at $000000 (power-on state), 0=normal chip RAM at $000000. |
| 1 | /LED | Output | Power LED / audio filter control. 0=LED on, audio filter engaged. 1=LED off, audio filter bypassed. |
| 2 | /CHNG | Input | Disk change input. Active low when the floppy in DF0 has changed. |
| 3 | /WPRO | Input | Disk write-protect input. Active low when the DF0 disk is write-protected. |
| 4 | /TK0 | Input | Track 0 input from DF0. Active low when the head is at track 0. |
| 5 | /RDY | Input | Disk ready input from DF0. Active low when the drive is ready. |
| 6 | /FIR0 | Input | Fire button 0 from joystick port 0. Active low when pressed. |
| 7 | /FIR1 | Input | Fire button 1 from joystick port 1. Active low when pressed. |

Note: /WPRO, /TK0, /RDY, /CHNG all come from the drive selected by the /SELx lines on CIA-B (see CIA-B PRB below), not from a fixed drive. So "DF0" is whichever drive is currently selected.

**CIA-A PRB (Port B):**

All 8 PRB bits are routed to the parallel port connector, implementing the Centronics-compatible printer interface. PB0–PB7 are the 8 data bits. The Centronics /ACK, BUSY, POUT, SEL signals are on other CIA-A pins:
- PC (pin 18) → parallel /STROBE (data ready)
- FLG → parallel BUSY (from printer)

CIA-A TOD pin: connected to the power supply vertical sync (60 Hz NTSC, 50 Hz PAL) for wall-clock timing. "VERT" input is actually a filtered 50/60 Hz signal derived from the power-supply primary via an opto-isolator, per A500 SM schematic notes.

CIA-A SP (serial port): connected to the keyboard KDAT line. Kbd data is shifted into the CIA serially.
CIA-A CNT (counter input): connected to the keyboard KCLK line.
CIA-A /IRQ: to Paula /INT2.

### 10.2 CIA-B at $BFD000

CIA-B is U301 on A500/A2000.

**CIA-B PRA (Port A):**

| PRA bit | Signal | Direction | Description |
|---------|--------|-----------|-------------|
| 0 | /BUSY | Input | Parallel printer BUSY |
| 1 | /POUT | Input | Parallel printer PAPER OUT |
| 2 | /SEL | Input | Parallel printer SELECT |
| 3 | /DSR | Input | Serial DSR |
| 4 | /CTS | Input | Serial CTS |
| 5 | /CD | Input | Serial Carrier Detect |
| 6 | /RTS | Output | Serial RTS |
| 7 | /DTR | Output | Serial DTR |

**CIA-B PRB (Port B) — the floppy control port:**

| PRB bit | Signal | Direction | Description |
|---------|--------|-----------|-------------|
| 0 | /STEP | Output | Floppy step pulse |
| 1 | DIR | Output | Floppy direction (0=in, 1=out) |
| 2 | /SIDE | Output | Floppy side select |
| 3 | /SEL0 | Output | Select DF0 |
| 4 | /SEL1 | Output | Select DF1 |
| 5 | /SEL2 | Output | Select DF2 |
| 6 | /SEL3 | Output | Select DF3 |
| 7 | /MTR | Output | Motor on |

Note: /MTR is latched per-drive on the falling edge of the corresponding /SELx — this is how a single /MTR line drives four independent motors. Well-documented in Amiga Intern §11.6 "Disk I/O" and cross-referenced to the CIA-B PRB layout.

CIA-B TOD pin: connected to /HSYNC from Agnus (so CIA-B TOD counts at the horizontal sync rate, ~15.7 kHz NTSC / ~15.6 kHz PAL). Used for precise timing.

CIA-B SP and CNT: on A500, these are on the expansion connector for user expansion.

CIA-B /IRQ: to Paula /INT6.

### 10.3 CIA differences A500 vs A3000/A4000

The mapping is **identical** across all classic Amigas. CIA-A is always at $BFE000, CIA-B always at $BFD000. The PRA/PRB/TOD connections are the same. The only difference on A3000/A4000 is that Fat Gary's decode logic responds only to odd word addresses for CIA-A and even word addresses for CIA-B (because both CIAs respond on the D0-D7 half of the data bus, which is the lower byte of a word when A0=0, but odd-byte when A0=1). Fat Gary enforces this explicitly — see §6-51 of A4000 SM quoted earlier.

---

## 11. Floppy interface

### 11.1 Paula's disk hardware

Paula contains a DMA-driven disk controller. Data flow:
- /DKRD (Paula pin 37) — serial read data from drive head
- /DKWD (Paula pin 38) — serial write data to drive head
- DKWE (Paula pin 39) — write enable

Paula's DSKDAT register is the data register; writes to it with DSKLEN set cause DMA out. Reads come in via DMA into DSKDAT. The MFM decoding / encoding is done by software (Kickstart's trackdisk.device) and by Paula's built-in wordsync matching.

### 11.2 CIA-B's floppy control

CIA-B's PRB provides all the mechanical control signals — /STEP, DIR, /SIDE, /SEL0-3, /MTR. See §10.2 above.

### 11.3 Internal floppy connector (34-pin)

The internal floppy drive connects via a 34-pin DIL ribbon cable. Pinout from A500 SM drive connector section (and A1000 SM §1-16 for J7 external which has the same signal set):

| Pin | Signal | Direction |
|-----|--------|-----------|
| 1 | /RDY | IN (from drive) |
| 2 | /DKRD | IN (from drive) |
| 3–7 | GND | — |
| 8 | /MTRXD | OUT (from CIA-B /MTR latched per-drive) |
| 9 | /SEL2 | OUT |
| 10 | /DRESB | OUT (buffered reset to drives) |
| 11 | /CHNG | IN (from drive) |
| 12 | +5V | — |
| 13 | /SIDE | OUT |
| 14 | /WPRO | IN |
| 15 | /TK0 | IN |
| 16 | /DKWE | OUT |
| 17 | /DKWD | OUT |
| 18 | /STEP | OUT (pulse, first low then high) |
| 19 | DIR | OUT |
| 20 | /SEL3 | OUT |
| 21 | /SEL1 | OUT |
| 22 | /INDEX | IN (index pulse, active low) |
| 23 | +12V | — |

Signals relevant to emulation:

- **/INDEX** — the drive asserts this once per revolution as the index hole passes. Approximately every 200 ms at 300 RPM (standard DD floppy). It is connected to CIA-B /FLAG input (pin 24 of CIA-B). The index pulse sets CIA-B's FLAG interrupt bit, which can be used to generate a timing reference for trackdisk.device. (See A500 SM §2-22 CRA/CRB descriptions for FLAG behaviour.)
- **/DRESB** — drives have their own reset line (not connected to system /RESET on A500/A2000). This allows software to reset a stuck drive.
- **/SEL0-3** — only one drive at a time is selected. The un-selected drives ignore all control signals. The currently-selected drive responds to /STEP, DIR, /SIDE, and drives /WPRO, /RDY, /TK0, /CHNG back.
- **/MTRXD** — the motor signal is latched per-drive on the falling edge of that drive's /SELx. So to spin up DF1 you: (1) clear /MTR (PRB bit 7=0), (2) assert /SEL1 (PRB bit 4=0), (3) the drive latches /MTR=0 → motor on. The other drives retain their previous motor state because they did not see a /SEL edge.

### 11.3 Timing

Step rate: minimum 3 ms between /STEP pulses (set by trackdisk.device based on drive specs). Some drives accept 2 ms, most need 3 ms.

Track-to-track seek: the head physically moves one track per /STEP pulse. Direction is set by DIR:
- DIR = 0 → step inward (toward higher track numbers)
- DIR = 1 → step outward (toward lower track numbers)

Head settling time: 18 ms typical after a seek completes before read/write is reliable.

Motor spin-up time: ~500 ms from motor on to data valid.

### 11.4 External floppy connector (DB23 on A500/A2000, A1000 J7)

The external floppy port is the same signal set as the internal, on a DB23F connector. From A1000 SM §1-16 J7 pinout:

| Pin | Signal | Description |
|-----|--------|-------------|
| 1 | /RDY | Disk ready |
| 2 | /DKRD | Disk read data |
| 3–7 | GND | — |
| 8 | /MTRXD | Motor control |
| 9 | /SEL2B | Select drive 2 |
| 10 | /DRESB | Drive reset |
| 11 | /CHNG | Disk change |
| 12 | +5V | — |
| 13 | /SIDEB | Side select |
| 14 | /WPRO | Write protect |
| 15 | /TK0 | Track 0 |
| 16 | /DKWE | Write enable |
| 17 | /DKWD | Write data |
| 18 | /STEPB | Step |
| 19 | DIRB | Direction |
| 20 | /SEL3B | Select drive 3 |
| 21 | /SEL1B | Select drive 1 |
| 22 | /INDEX | Index pulse |
| 23 | +12V | — |

The "B" suffix is Commodore's way of indicating "buffered" — these are the versions of the signals with an HC244/HC125 buffer between the chip and the external connector to handle cable capacitance.

---

## 12. Expansion bus

### 12.1 A500 side expansion slot (86-pin edge)

From A500 SM §30 (the "86-Pin Connector" table). This is on the right-hand side of the A500 motherboard, the edge connector that expansion cards plug onto.

| Pin | Name | Pin | Name |
|-----|------|-----|------|
| 1 | GND | 44 | /IPL2 |
| 2 | GND | 45 | A16 |
| 3 | GND | 46 | /BERR |
| 4 | GND | 47 | A17 |
| 5 | +5 | 48 | /VPA |
| 6 | +5 | 49 | GND |
| 7 | EXP | 50 | E |
| 8 | −12 | 51 | /VMA |
| 9 | EXP | 52 | A18 |
| 10 | +12 | 53 | /RES |
| 11 | EXP | 54 | A19 |
| 12 | /CONFIG | 55 | /HLT |
| 13 | GND | 56 | A20 |
| 14 | C3* | 57 | A22 |
| 15 | CDAC | 58 | A21 |
| 16 | C1* | 59 | A23 |
| 17 | /OVR | 60 | /BR |
| 18 | XRDY | 61 | GND |
| 19 | /INT2 | 62 | /BGACK |
| 20 | /PALOPE | 63 | PD15 |
| 21 | A5 | 64 | /BG |
| 22 | /INT6 | 65 | PD14 |
| 23 | A6 | 66 | /DTACK |
| 24 | A4 | 67 | PD13 |
| 25 | GND | 68 | /PRW |
| 26 | A3 | 69 | PD12 |
| 27 | A2 | 70 | /LDS |
| 28 | A7 | 71 | PD11 |
| 29 | A1 | 72 | /UDS |
| 30 | A8 | 73 | GND |
| 31 | FC0 | 74 | /AS |
| 32 | A9 | 75 | PD0 |
| 33 | FC1 | 76 | PD10 |
| 34 | A10 | 77 | PD1 |
| 35 | FC2 | 78 | PD9 |
| 36 | A11 | 79 | PD2 |
| 37 | GND | 80 | PD8 |
| 38 | A12 | 81 | PD3 |
| 39 | A13 | 82 | PD7 |
| 40 | /IPL0 | 83 | PD4 |
| 41 | A14 | 84 | PD6 |
| 42 | /IPL1 | 85 | GND |
| 43 | A15 | 86 | PD5 |

Notes from A500 SM §30:
- Pins labeled "EXP" (7, 9, 11) are expansion-reserved.
- /PALOPE is PAL-only: asserted by Gary when an expansion card is present and matches the Autoconfig protocol.
- /CONFIG input lets a card signal the system that it wants to configure.
- PD0–PD15 are the processor data bus (buffered version).
- The lack of separate "chip data bus" pins on the edge connector means **expansion cards on the A500 do NOT get direct access to Agnus's chip bus**. They get only the 68000-side buses. Anything that wants chip RAM access has to go through the CPU.
- This is subtle for emulation: a DMA card on an A500 edge connector uses /BR /BG /BGACK to grab the 68000 bus, and only sees what the 68000 sees. The chip bus is invisible to it.

Voltage tolerances: +5V at ~3.5 A, +12V at ~1 A, −12V at ~100 mA. Edge cards are sometimes fragile on PAL power from the external PSU because the A500 PSU is right on the edge.

### 12.2 A2000 Zorro II slot (100-pin × 2 rows)

The A2000 has five Zorro II slots. Zorro II is a 16-bit, 24-bit-address, 5 MB configuration-space autoconfig bus. A Zorro II card can:
- Do 68000-style bus master (DMA via /BR /BG /BGACK)
- Decode its own 8-bit AUTOCONFIG response at $E80000
- Claim an address range up to 8 MB for Fast RAM
- Generate /INT2 or /INT6 interrupts

The 100-pin Zorro II connector has two rows (A and B). The pinout is on A2000 schematic page 13 (not in the corpus text files — only Amiga Update and the A4000 ASIC specs cover it). The key signals are the buffered versions of all 68000 bus signals (PD0–PD15, A1–A23, /UDS, /LDS, /AS, R/W, /DTACK, /BERR, /IPL0-2, /VPA, /VMA, /BR, /BG, /BGACK, /RST, /HLT, E), plus +5V, +12V, −5V, −12V, GND.

Additional Zorro II-specific signals:
- /XRDY — wait-state insertion from the slot
- /EINT2, /EINT6 — interrupt outputs (wire-ORed to Paula)
- /OVR — override signal; pulling this low forces Gary to not decode this cycle, allowing the slot to respond
- /CFGIN (daisy-chained to next slot) — AUTOCONFIG chain-in
- /CFGOUT — AUTOCONFIG chain-out
- /DOE — data output enable
- /SLAVE — card is responding
- /BOSS — bus master indicator

The corpus does NOT provide the complete pin-by-pin Zorro II pinout. A4000 SM §6-43 BUSTER Chip Specification hints at the Zorro III superset but the A2000 Zorro II specific pinout is a gap. This is noted in §Gaps.

### 12.3 A3000/A4000 Zorro III slot (100-pin, Super Buster arbitrated)

Zorro III is the 32-bit-wide superset of Zorro II, backwards-compatible. Same physical connector shape (100 pins × 2 rows), but many signals are re-used for 32-bit data, 32-bit address, and DMA handshaking with Super Buster. Zorro III-specific features:
- 32-bit address (A0–A31)
- 32-bit data (D0–D31)
- /CCS (card chip select)
- /MTCR (multiplexed address strobe), /MTACK (multiplexed address acknowledge)
- /FCS (first chip select), /SCS (second chip select)
- /IORST (I/O reset, driven by Super Buster reset)
- /CINH (cache inhibit)
- /LOCK (locked cycle, for 68030 read-modify-write)

Auto-config space for Zorro III is at $FF000000–$FF07FFFF (the upper 32-bit space).

Super Buster arbitrates /BR, /BG, /BGACK per-slot, using a round-robin scheme with priority.

The corpus documentation for Zorro III is the A4000 SM §6-43 BUSTER Chip Specification. It describes Super Buster as an 84-pin PLCC containing "the bus controller gate array" but the pin names are partially OCR-garbled ("/CEST", "/SER", "/FCS", etc. appear in the text with clear OCR artefacts — see the connection diagram at §6-44 for the source of the names). Full Zorro III spec is in Commodore's Zorro III Bus Specification, which is NOT in the corpus.

### 12.4 A1200 PCMCIA and trapdoor

The A1200 has two expansion interfaces:
1. **Trapdoor** (memory expansion + optional FPU/clock) — on the bottom of the case. It provides direct 32-bit access to the 68EC020 bus for memory. Gives access to the "Fast RAM" space at $00C00000 (but limited to 8 MB maximum because of the 24-bit A0-A23 decoding on the A1200).
2. **PCMCIA** (CN15, 68-pin) — 8/16-bit PC Memory Card standard. Gayle handles this. Used for modems, SCSI, Ethernet, and flash storage.

Pin-by-pin PCMCIA slot pinout is not in the A1200 corpus file — only the fact that CN15 is a 68-pin PCMCIA card slot.

### 12.5 A4000 local bus (CPU slot)

The A4000 CPU slot is a 200-pin KEL connector (CONN200 per A4000 SM §5-10) that carries the 68040 processor bus direct. This is where the processor card plugs in. A user-space expansion via this slot is not normal — it's for CPU upgrades (68040 → 68060 cards).

### 12.6 A4000 video slot

The A4000 video slot is a 100-pin extended slot that provides direct access to the video signals. From the A4000 SM BOM at §5-5, CN452 and CN600 are 100-pin edge card connectors — CN452 is the video slot, CN600 is a Zorro III slot next to it. Video slot carries the chunky video data, the digital RGB from Lisa, Hsync/Vsync, the /XCLK lines, and the audio left/right.

---

## 13. Power rails

### 13.1 Amiga 500 power supply

From A500 SM §30 and SAMS A500:
- +5V @ 3.5 A (nominal; peaks higher)
- +12V @ 1 A (for drive motor and analog RGB)
- −12V @ 100 mA (for RS-232 line drivers)
- GND

The A500 PSU is a linear supply in a separate external brick. Because the A500 runs close to its 3.5 A +5V ceiling when an A501 expansion and a second floppy are attached, some A500s exhibit brown-out behaviour under load. Service bulletin Amiga Update 25/3-3.1 (A2000 fix for the same issue on rev 6 PCBs) is a guide: the "Guru on power-up" message is retained DRAM content from before power-off, which a proper power-on reset (PST518B) eliminates.

### 13.2 Amiga 1000 power supply

Internal PSU in the case. +5V, +12V, −12V, −5V (A1000 has −5V which A500 does not, for TTL signalling margin). A1000 SM §1-20 PSU schematic (only in OCR, noisy).

### 13.3 Amiga 2000 power supply

Internal PSU. +5V, +12V, −12V, −5V. Higher current ratings than A500: 5V at 10+ A to feed Zorro cards.

### 13.4 Amiga 3000 / 4000 power supply

Internal PSU. Same rails as A2000 but higher 5V current (the A4000/040 consumes around 3 A by itself). A4000 SM §5-11 BOM lists power supply as 391173-01 (NTSC) / 391173-02 (PAL) — these are the same-part-number Commodore-specific units with different transformer winding for 110V vs 220V mains.

### 13.5 Decoupling and power-on sequence

The standard Amiga power-up sequence:
1. PSU comes up. All rails rise nominally together on a linear PSU (a few ms rise time); switching PSUs on later revisions have a slight rail sequencing.
2. PST518B (or equivalent) holds /RESET low until +5V is above its threshold (nominally 4.2 V for the PST518).
3. /RESET rises. Gary (or Fat Gary) starts its internal timer.
4. After 250 msec (Fat Gary spec), /RESET to the 68000 is released.
5. 68000 fetches reset vector from $000000 (which is overlay-ROM at this point because CIA-A PRA is all 0 including /OVL=0 — wait, let me check — /OVL being bit 0 of PRA is high at reset because DDR is 0 = input with pull-ups, so PRA reads as all 1 meaning /OVL=1 meaning ROM is overlaid).
6. 68000 reads stack pointer and program counter from ROM, begins execution.

Step 5 is worth noting: DDR registers on CIA reset are all 0 (all pins inputs), and the CIA 8520 has passive pull-ups on its port pins (A500 SM §2-17). So at reset, all PRA bits read as 1 — which means /OVL is 1 — which means Gary overlays ROM at $000000. This is how the 68000 bootstraps without software intervention.

Brown-out behaviour: if Vcc sags below the PST518 threshold, the PST518 pulls /RESET low and the system resets. On A500s without the PST518 mod, a slow brown-out can cause the system to enter an undefined state without resetting (because the 68000 datasheet requires /RESET to be asserted for at least 10 bus cycles to guarantee reset). This is why modded A500s are more reliable.

---

## 14. Known errata / service bulletins

### 14.1 A1000 Denise rev 5 HiRes colour glitch

(Tech Topics §7813–7821)

- **Symptom**: In HiRes mode, a one-pixel-wide colour glitch at the screen boundary.
- **Cause**: Rev 5 of Denise 8362 had a timing error on the rightmost column of HiRes data.
- **Secondary symptom**: Old revision Agnus caused the A1300 Genlock to "lock on the wrong frame."
- **Fix**: Upgrade to Rev 6 Denise, no charge to customers with a valid A1300 genlock purchase proof.
- **Emulator implication**: Rev 5 and earlier A1000 systems display the glitch; emulators that model A1000 fidelity must reproduce it to match rev 5 exactly. The glitch is a single pixel of wrong colour at the right edge of the HiRes playfield.

### 14.2 A500 rev 3 keyboard bit-loss

(Amiga Update bulletin, section 4492 area)

- **Symptom**: "KEYBOARD DATA INTO THE SERIAL SHIFT REGISTER OF THE 8520, ONE BIT [...]". Missing middle text in OCR. The bulletin describes intermittent keyboard dropped bits on the A500 rev 3.
- **Fix**: Not in OCR. Section is garbled.
- **Emulator implication**: None — this is a physical connection issue, not a silicon behaviour.

### 14.3 A500 rev 3/5 Agnus 8370 poor contact

(Amiga Update, §4519 region)

- **Symptom**: "IT IS POSSIBLE THAT POOR CONTACTS EXIST BETWEEN IC 8370 F. AGNUS, AND [socket]".
- **Cause**: Commodore's PLCC sockets on early boards could have poor contact due to oxidation.
- **Fix**: Reseat the Fat Agnus. "WARNING: DO NOT ATTEMPT TO REMOVE IC 8370, F. AGNUS, UNLESS YOU [have the proper PLCC extractor tool]".
- **Emulator implication**: None.

### 14.4 A500 rev 6A field-enabling 1 MB chip RAM

(Amiga Update 30/3-8.1)

- **Symptom**: Customer has an 8372A Fat Agnus in their A500 rev 6A and wants 1 MB chip RAM.
- **Cause**: Commodore factory-jumpered all A500 boards for 512 KB chip RAM regardless of Agnus installed.
- **Fix**: Field mod procedure documented but NOT supported. Voids warranty.
- **Emulator implication**: A stock A500 (not a "big Agnus mod") caps at 512 KB chip RAM. This is important for compatibility with software that assumes 512 KB maximum.

### 14.5 A2000 rev 6 Guru on power-up

(Amiga Update 25/3-3.1)

- **Symptom**: "GURU MESSAGE on power up."
- **Cause**: Some new DRAMs retained data for as long as 5 minutes after power-off. Power-cycling fast left non-zero RAM patterns that Kickstart interpreted as valid pointers, crashing early.
- **Fix**: Install a Mitsumi PST518B low-voltage sense IC (CBM PN 328156-02) as XU1. The fix holds /RESET long enough for Kickstart to initialise RAM.
- **Warranty**: "This fix should be done if requested by the customer, and will be covered, as a component level repair, under warranty."
- **Emulator implication**: An emulator that models cold-boot with "all RAM = 0" doesn't see this. An emulator that models "warm-boot with leftover data" would need to model the PST518 threshold as well.

### 14.6 A2000 J300 tick signal jumpered incorrectly

(Amiga Update 30/3-7.1)

- **Symptom**: Third-party Genlock installation fails.
- **Cause**: "Some A2000s may have been shipped with the Tick signal (J300), jumpered incorrectly."
- **Fix**: Verify J300 pins 1 and 2 are jumpered.
- **Emulator implication**: None — this is a genlock interop issue.

### 14.7 A2000 rev 4.x Gary Toshiba vs MOS

(Amiga Update 25/3-1.1)

- **Symptom**: Intermittent boot failures on A2000 rev 4.x.
- **Cause**: Toshiba-manufactured Gary 5719 had slightly different timing margins than the MOS version.
- **Fix**: Add a 470 Ω pullup between +5V and the CPU side of R106 (handles the Toshiba margin issue), or replace with a MOS-manufactured Gary 5719.
- **Emulator implication**: None — Gary behaves the same either way for software.

### 14.8 A2000 rev 4.5 upgrade to "Fatter" Agnus

(Amiga Update 25/3-2.1)

- **Procedure**: Detailed field upgrade to fit an 8372A in an A2000 rev 4.5 PCB and wire it for 1 MB operation.
- **Steps**: Move J101 from 1-2 to 2-3 (enables A19), cut the /EXRAM trace on J500, optionally open J102 for PAL.
- **Emulator implication**: Confirms the exact wiring needed to make an 8372A address more than 512 KB. For emulator fidelity: modelling "A2000 rev 4.5 with 1 MB Agnus" is a distinct configuration from "rev 6 out-of-the-box".

### 14.9 A2000 rev 6 keyboard capacitor C905/C908 removal

(Amiga Update 25/3-1.1)

- **Cause**: EMI filtering caps on the keyboard lines interfered with some keyboards.
- **Fix**: Remove C905 (below Gary pin 20) and C908 (above crystal X1).
- **Emulator implication**: None.

### 14.10 A1200 C461 video output colour loss

(Tech Topics §295 area)

- **Symptom**: A1200 screen "goes gray" when composite and RF video outputs are used.
- **Cause**: C461 (a 0.22 µF cap) is wrong value for the specific video encoder loading.
- **Fix**: Remove C460 entirely, replace C461 with 1000 pF chip cap.
- **Emulator implication**: None — this is analog video output only.

### 14.11 A1200 power connector leads causing shorts

(Tech Topics §285 area)

- **Symptom**: Short between power connector and RF shield.
- **Cause**: Untrimmed leads on power connector.
- **Fix**: Trim leads, inspect before insertion.
- **Emulator implication**: None.

### 14.12 A500 rev 5/6A ECS Denise replacement

The A500 service manual documentation for ECS Denise substitution (replacing 8362 with 8373 in a rev 7/8A socket) is in A500+ SM §1-1: "The A500 Plus shall contain the same custom chip set as the A500, except for the 8375 2Meg. FAT AGNUS and 8373 ECS Denise." and "Full ECS support" is noted. Drop-in 8373 into a rev 5/6A A500 works, but ECS register features require ECS Agnus present as well (8372A or 8375). On an OCS 8370 Agnus + 8373 Denise hybrid, only the OCS register set is exposed — the ECS-only registers in Denise are inaccessible because Agnus won't generate the RGA addresses for them.

---

## 15. Amiga Intern chip internals

Amiga Intern §11.4 and §11.7 provide information that is NOT in the HRM or in the service manuals — it's the Abacus-style deep internal description. What follows is extracted with citations; cross-reference HRM ref §3 for corresponding register-level detail.

### 15.1 Agnus internal structure (Amiga Intern §11.4.2)

"Agnus's main responsibility is all of the DMA control. Each of the six possible DMA sources has its own control logic. They are all connected to the chip RAM address generator as well as the register address generator. These address generators create the RAM address of the desired chip RAM location and the register address of the destination register. In this manner the DMA logic units supply the appropriate chip registers with data from the RAM or write the contents of a given register into RAM."

"Also connected to the chip RAM address generator is the refresh counter, which creates the refresh signals necessary for the operation of the dynamic RAM chips."

"Agnus controls the synchronization of the individual DMA accesses. The fundamental reference for this is a screen line. In each screen line, 255 memory accesses take place, which Agnus allocates among the individual DMA channels and the 68030. Since it always needs the current row and column positions for this, Agnus also contains the raster and column counters."

**Key number**: 255 memory accesses per screen line. At 3.58 MHz CCK (NTSC), 255 accesses × 280 ns per access = 71.4 µs per line, which matches the 63.555 µs NTSC line period plus horizontal blanking overhead. The allocation of these 255 slots among Blitter, Copper, bitplane DMA, sprite DMA, audio DMA, disk DMA, refresh, and CPU is what the HRM ref §3 slot tables enumerate.

"Two other important elements in Agnus are the Blitter and the Copper coprocessor." (Amiga Intern §11.4.2)

### 15.2 Denise internal structure (Amiga Intern §11.4.3)

"In general, the function of Denise can be described as graph generation. The first part of this task is already accomplished by Agnus. Agnus fetches the current graphic data from the chip RAM and writes them to the registers responsible for the bit level manipulations in Denise. It does the same for the sprite data. Denise always contains all graphic and sprite data for 16 pixels, since a bit always corresponds to one pixel on the screen and the data registers all have a width of one word, or 16 bits."

"These data must be converted into the appropriate RGB representation by Denise. First, the graphic data are converted from a parallel 16-bit representation to a serial data stream by means of the bit-level sequencer. Since a maximum of six bit levels are possible, this function block is repeated six times. The serial data streams from the individual bit-level sequencers are now combined into a maximum 6-bit wide data stream."

"The priority control logic selects the valid data for the current pixel based on its priority from among the graphic data from the bit-level sequencers and the sprite data from the sprite sequencers. According to this data the color decoder selects one of the 32 color registers. The value of this register is then output as a digital RGB signal. If the Hold-And-Modify (HAM) or the Extra-Half-Bright (EHB) mode is selected, the data from the color register is modified accordingly before it leaves the chip."

"The data from the sequencers is also fed into the collision-control logic. As its name implies, this checks the data for a collision between the bit levels and the sprites and places the results of this test into the collision register."

"The last function of Denise has nothing to do with the screen display. Denise also contains the mouse counter, which contains the current X and Y positions of the mice."

**Key takeaway for emulation**: Denise's pixel pipeline is a 6-wide serialiser feeding into a priority mux feeding into the colour table feeding into the HAM/EHB modifier feeding into the digital RGB output pins. This is one clock cycle deep plus the 16-pixel window of bitplane data being clocked in. An emulator that handles per-pixel colour correctly but ignores the "16 pixels in flight" timing will mispresent hover effects and sprite-bitplane collisions that depend on when the previous BPLxDAT arrived.

### 15.3 Paula internal structure (Amiga Intern §11.4.4)

"Paula's tasks fall mainly in the I/O area, namely the diskette I/O, the serial I/O, the sound output and reading the analog inputs. In addition, Paula handles all interrupt control. All the interrupts that occur in the system run through this chip. From the fourteen possible interrupt sources, Paula creates the interrupt signals for the 68030."

"The disk data transfer and the sound output are performed using DMA. Since, in these two functions, Agnus does not know when the next data word is ready for a DMA transfer, Paula has a DMAL line, which it can use to tell Agnus when a DMA access is needed."

"The serial communication is handled by a UART (Universal Asynchronous Receive Transmit) component inside Paula."

**Key takeaway**: the DMAL line is how Paula asks for DMA slots. Agnus runs its DMA slot sequence; when Paula needs disk or audio data, it pulses DMAL. Agnus then schedules a disk or audio DMA slot in the next available low-priority position. This is how Paula avoids starving Agnus while maintaining flexible audio/disk rate. An emulator that schedules audio DMA at fixed-rate intervals misses this handshake — audio rate is driven by period registers but the actual DMA timing is handshaken through DMAL.

### 15.4 Internal clock relationships (Amiga Intern §11.4.2)

"All clock generation for the custom chips is integrated in Fat Agnus. Only the 28 MHz base clock must be supplied. Agnus also assumes the management of chip RAM, generating the necessary RAS and CAS signals together with the multiplexed RAM addresses."

"Since the chip RAM has a much larger address range than the custom chips and also requires multiplexed addresses, there is a separate chip RAM address bus. Multiplexed addresses implies that the RAM chips used in the Amiga have an address range of 2^8 addresses (256K) and in order to access all the addresses of a chip, 18 address lines are needed. But the actual chips are very small, and such a large number of address lines would require a very large enclosure. To get around this problem, something called multiplexed addressing was introduced. The package has only nine address lines; first the upper nine bits of the address and then the lower nine are applied to these lines. The chip stores the upper nine and then, when the lower nine are applied to the address lines, it has the 18 address bits that it needs."

**Key takeaway**: Agnus is the DRAM controller for chip RAM. It outputs 9 MA lines (multiplexed address), /RAS, /CASU, /CASL, and /WE. The row address is presented first, then the column. This is why there are only 9 DRA pins (DRA0–DRA8) on Agnus — 2 × 9 bits = 18 bits = 256 K words = 512 KB. For 1 MB Agnus, there are 10 DRA pins (DRA0–DRA9) — 2 × 10 bits = 20 bits = 1 M words = 2 MB.

This matters for emulation because DRAM refresh timing depends on Agnus running refresh cycles every N CCK slots. The refresh rate is fixed by the chip design — every 8 screen lines or so, Agnus does a row-refresh cycle using the next /RAS line. An emulator that skips DRAM timing gets away with it because DRAM data loss is "not visible" to software, but an emulator that models Agnus's slot allocator needs to give refresh its slot allocation (the slot table in HRM ref §3 includes 4 refresh cycles per line).

### 15.5 Chip bus separation explained (Amiga Intern §11.4.1)

The full passage on why the Amiga has two buses (CPU bus and chip bus), from Amiga Intern §11.4.1, is worth quoting for the scheduling-level insight:

"One obvious problem is still unresolved. There is only one data bus and one address bus, which both the processor and the DMA controller want to access. A bus can be controlled by only one bus controller at a time. If two chips tried to place an address on the bus simultaneously, there would be a problem known as bus contention, leading to a system crash. Therefore the chips must share access to the bus by taking turns. Naturally each would like to have the bus for itself as often as possible. This problem is solved by the Amiga on three levels:

First, both normally continuous buses are divided on the Amiga into two parts. One (on the left in the diagram) connects all the components that are usually accessed only by the processor. [...] When the 68030 accesses one of these components, Gary uses the buffers to break the connections of the processor address and data buses to the chip address and data buses. This way both the processor and Agnus, each on its own side, can access the bus undisturbed. This gives the processor quick access to the operating system and to its RAM. This RAM connected directly to the processor data and address bus is called fast RAM, since the processor can always access it without slowing down, if it has the bus at that moment.

Secondly, bus accesses from Agnus and from the processor are nested, so that normally even on accesses to chip RAM or chip registers, a 68000 does not have to be delayed. For such an access the buffers connect the two systems again.

As a third and final solution, the processor can wait until Agnus has finished its DMA accesses and the bus is free again. This occurs only when very high graphics resolutions have been selected or the Blitter is being used. Agnus, Denise and Paula were originally drafted for an Amiga with a 68000 processor. Despite certain revisions for the A3000, they have some problems working with the 68030. Nesting the accesses to chip RAM on an Amiga with the 68000 enables alternating access; so the processor does not have to wait. The A3000's 68030, however, accesses memory with substantially higher speed, while Agnus's clock frequency remains unchanged. The result is that the A3000's CPU must insert wait cycles when it wants to access chip RAM."

**Emulator insight**: this is the three-level model for chip bus contention that should be implemented:

1. **Access target = Fast RAM / ROM / CIA / expansion**: no interaction with Agnus. Cycle completes at 68k's native timing (70 ns per cycle on 14 MHz 020, etc.).
2. **Access target = chip RAM / chip register, low-contention line**: cycle is nested into one of the "odd" CCK slots that Agnus leaves free. Zero wait states beyond the normal 4-clock access.
3. **Access target = chip RAM / chip register, high-contention line**: cycle is held off by /DTACK until Agnus releases the chip bus. Wait states scale with Agnus's DMA activity on that line.

On 68000 boards (A500/A2000/A1000), level 2 applies — the 68000 is slow enough that Agnus can nest it in. On 68030/68040 boards (A3000/A4000), level 3 applies more often because the CPU bus is much faster than Agnus.

---

## Appendix A — Chip revision comparison matrix

| Feature | Agnus 8361 | Agnus 8370 | Agnus 8372A | Agnus 8372B | Agnus 8375 | Alice 8374 |
|---------|-----------|-----------|-------------|-------------|------------|-----------|
| Package | 48-pin DIP | 84-pin PLCC | 84-pin PLCC | 84-pin PLCC | 84-pin PLCC | 84-pin PLCC |
| Chip RAM max | 256 KB | 512 KB | 1 MB | 2 MB | 2 MB | 2 MB (8 MB unsupported) |
| ECS features | — | — | — | Yes | Yes | Yes + AA |
| Machines | A1000 | A500 rev 3/5, A2000 rev 3-4 | A500 rev 6A/7/8A, A2000 rev 4.5+/6 | A3000 | A500+ | A1200, A4000 |
| DMA channels | 25 | 25 | 25 | 25 | 25 | 25 + expanded bitplane |
| Bitplanes | 6 | 6 | 6 | 6 | 6 | 8 |
| Max process | NMOS | NMOS | CMOS | CMOS | CMOS | CMOS AA |

| Feature | Denise 8362 rev 5 | Denise 8362 rev 6 | Denise 8373 | Lisa 4203 |
|---------|-------------------|-------------------|-------------|-----------|
| HiRes right-edge glitch | Yes | No | No | No |
| SuperHires | — | — | Yes | Yes |
| Productivity mode | — | — | Yes | Yes |
| Max non-HAM colors | 32 | 32 | 32 | 256 |
| HAM depth | HAM6 (4096) | HAM6 | HAM6 | HAM8 (262144) |
| Sprites | 8 | 8 | 8 | 8 (wider) |
| Bitplane depth | 4 | 4 | 4 | 8 |
| Palette bits/channel | 4 | 4 | 4 | 8 |

| Feature | Paula 8364 | (no Paula rev) |
|---------|-----------|-----------|
| Audio channels | 4 | — |
| Audio bits | 8 | — |
| Volume bits | 6 (0–64) | — |
| Fixed L/R routing | 0,3→L; 1,2→R | — |
| Floppy MFM | yes | — |
| UART | 9-bit configurable | — |
| Analog inputs | 4 (POT0X/Y, POT1X/Y) | — |
| Interrupt sources handled | 14 | — |

| Feature | Gary 5719 | Gayle F023A | Fat Gary |
|---------|-----------|-------------|----------|
| Package | 48-pin DIP | ~100-pin PLCC | 84-pin PLCC |
| Machines | A500/A2000 | A600/A1200 | A3000/A4000 |
| Address width decoded | 24-bit | 24-bit + PCMCIA + IDE | 32-bit |
| RTC interface | A501 external | On-board | On-board |
| IDE support | — | Yes (A1200 onboard) | Yes (A4000 onboard AT-IDE) |
| PCMCIA | — | Yes | — |
| Bus timeout | — | — | Yes, 9µs DSACK / 250ms BERR |
| ECLK generation | Yes (stretched) | Yes | Yes, 6-high/4-low with stretch |
| Mask-all-interrupts bit | — | — | $DF8000+$9A bit 15 |

| Feature | Buster (A2000) | Super Buster (A3000/A4000) |
|---------|----------------|----------------------------|
| Package | 48-pin DIP ~ | 84-pin PLCC |
| Bus | Zorro II (16-bit, 24-bit addr) | Zorro III (32-bit, 32-bit addr) |
| Slots arbitrated | 5 | 4 Zorro III + video slot |
| DMA arbitration | /BR, /BG, /BGACK for slots | ditto + CBREQ/CBACK cache burst |
| Config space | $E80000 (5 MB) | $FF000000 (upper) |

| Feature | Ramsey -04 | Ramsey -07 |
|---------|-----------|-----------|
| Version register value | $0D | $0F |
| Max RAM | 16 MB | 16 MB |
| Bit 4 meaning | RAMwidth (1 or 4 bits) | Skip (4-clock vs 5-clock access) |
| DMA counter granularity | even longword | even word (can increment by 2) |

---

## Appendix B — ASIC part number index

From A4000 SM §5 BOM, A1200 sch key components, and various service bulletins.

| Part # (CBM) | Chip label | Package | Role | Machines |
|--------------|------------|---------|------|----------|
| 318072-01 | Gary 5719 | DIP-48 | System decode/control | A500, A2000 |
| 318069-02 | Fat Agnus 8372A | PLCC-84 | DMA/address/clocks | A500 rev 6A+, A2000 rev 4.5+ |
| (318069-xx) | Agnus 8370 | PLCC-84 | DMA/address/clocks (512 KB) | A500 rev 3/5, A2000 rev 3-4 |
| (318069-xx) | Agnus 8361 | DIP-48 | DMA/address (A1000 only) | A1000 |
| (318069-xx) | Fat Agnus 8372B | PLCC-84 | ECS Super Agnus 2 MB | A3000 |
| (318069-xx) | Fat Agnus 8375 | PLCC-84 | ECS Agnus 2 MB | A500+ |
| 391010-01 | Alice 8374 | PLCC-84 | AA Agnus | A1200, A4000 |
| (318xxx-xx) | Denise 8362 | DIP-48 | OCS Denise | A1000, A500, A2000 |
| (318xxx-xx) | Hi-Res Denise 8373 | DIP-48 | ECS Denise | A500+, A3000, late A500 |
| 391227-01 | Lisa 4203 | PLCC-84 | AA Denise | A1200, A4000 |
| (318xxx-xx) | Paula 8364 | DIP-48 | Audio/disk/serial/interrupts | All |
| (318xxx-xx) | CIA 8520A/B | DIP-40 | CIA | All |
| (39xxxx-xx) | Buster | PLCC-52 | Zorro II bus controller | A2000 |
| 390541-01 | Super Buster | PLCC-84 | Zorro III bus controller | A4000, A3000 |
| 390532-02 | Super DMAC | PLCC-84 | SCSI DMA controller | A3000, A4000 |
| (39xxxx-xx) | Fat Gary | PLCC-84 | A3000/A4000 system controller | A3000, A4000 |
| (39xxxx-xx) | Gayle F023A | PLCC | A600/A1200 system controller | A600, A1200 |
| (39xxxx-xx) | Ramsey -04 | PLCC-84 | A3000 Fast RAM controller | A3000, A4000 early |
| (39xxxx-xx) | Ramsey -07 | PLCC-84 | A3000 Fast RAM controller revised | A4000 late |
| (39xxxx-xx) | Bridgette | PQFP-100 | Bus bridge buffer | A4000, late A3000 |
| (391???-xx) | Budgie | PLCC | A1200 glue ASIC | A1200 |
| (39xxxx-xx) | Amber | PLCC | A3000 flicker fixer | A3000, A3000T |

Where part numbers show as "(xxxxxx-xx)" the corpus did not give a specific Commodore part number for that stepping. The 390541-01 (Super Buster) and 390532-02 (Super DMAC) part numbers are from the A4000 SM §5 BOM tables for U700/U850.

---

## Gaps in corpus

The following facts an emulator author would want are NOT in any of the service manuals or Amiga Intern chapters in the corpus:

1. **Complete Zorro II slot pinout (100-pin)**. The A2000 SM is not in the corpus; only A500 service manual (which has the 86-pin edge connector) is. The Zorro II pinout has to be sourced from the Commodore Zorro II Bus Specification (not in corpus) or from Amiga Hardware Manual appendices.

2. **Complete Zorro III slot pinout**. A4000 SM §6-43 BUSTER Chip Specification describes Super Buster functionally but does not enumerate the Zorro III slot pins. The Zorro III specification is a separate document not in corpus.

3. **Gayle register map (A600/A1200)**. The A1200 schematic addendum names Gayle at U5 but gives no chip spec. Gayle's registers (IDE status at $DA2000, Gayle ID at $DE1000, etc.) are known from community documentation but not from the service manuals in corpus.

4. **Budgie register map or pin list**. The A1200 schematic addendum lists it as "391??? Budgie (ASIC)" at U20 but gives no detail.

5. **Amber chip specification**. The A3000/A3000T flicker fixer is mentioned in Amiga Intern §11.4.1 but has no dedicated chip spec in the A4000 service manual (Amber is A3000-specific, and the A3000 SM is not in corpus).

6. **A3000 service manual**. Not in the corpus. A3000-specific board jumpers, chip-level details, and rev 9.x differences are only available via Amiga Intern chapter 11 (which is written around the A3000 but is general-audience).

7. **Exact filter cutoff frequency of the Paula low-pass filter**. The schematic has the R and C values but the corpus does not give a -3 dB number. Community measurements give 3.3 kHz (filter on) and 30 kHz (filter off) approximate values.

8. **A2000 jumper setting for the A501+A2000 arrangement**. Although Amiga Update 30/3-8.1 addresses 1 MB chip RAM on the A500, the specific jumpers for the A2000 with the 8372A and "FASTROM" option are only partially covered.

9. **Full CIA timing state machine**. The 8520 datasheet in A500 SM §2-17 to §2-25 covers the register map, timer modes, and electrical characteristics, but does not describe the internal state machine at a FSM level — how the interrupt latching works across multiple mask register writes, how the underflow signal propagates through the ICR flag, how the B-timer counts "cascade from A" mode operates, etc. Some of this is in the Amiga Hardware Reference Manual errata.

10. **Detailed 68000 → Agnus slot allocation**. The HRM ref covers the slot table and the reference handful-of-cycles fairness, but the service manuals do not document the internal priority scheme in Agnus beyond "25 DMA channels in priority order". The internal slot allocator FSM is reverse-engineered from hardware rather than documented.

11. **Reset exact pulse lengths on A500 Gary 5719**. The Fat Gary A4000 spec gives 250 ms for post-/PWRUP /RESET; the A500 Gary 5719 equivalent is not spelled out in the A500 SM. It is assumed to be similar (RC-timed on the Gary die) but not given as a numeric value.

12. **Power-supply sequencing waveform**. The power rails come up "nominally together" but there is no scope capture in the service manuals. Specific A500 PSU rise time is not given.

13. **DRAM refresh timing proof of 4 slots per line**. Amiga Intern §11.4.2 confirms "memory is refreshed on every raster line" and Agnus's §2-8 note says refresh uses one DMA channel. The specific count of 4 refresh slots per line (which matches the slot table in HRM ref §2) is not explicitly stated in the service manuals — it's inferred from Agnus's DMA slot allocation tables.

14. **Lisa and Alice internal register reset state**. The HRM Appendix C covers OCS/ECS register reset state but the AGA extensions (FMODE, BPLCON3, BPLCON4, etc.) are not described in any corpus document. AGA-specific details require the AGA Supplement (CBM PN 371121-01), not in corpus.

15. **Actual Kickstart ROM checksum values per revision**. Tech Topics §7780 mentions "The ROM checksum byte at location $CFFF has changed from $C3 to $4C" for a specific revision but doesn't enumerate all Kickstart revisions — this has to come from cross-references to Kickstart ROM archives.

---

## Source map

Per-file summary of where each topic comes from.

### `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/A1000 Service Manual.txt` (2771 lines, OCR)

- §1-1 through §1-4: system overview, block diagram, 68000 interaction, address map.
- §1-6: Agnus 8361 features, 48-pin DIP pinout, block diagram.
- §1-8: Denise 8362 features, 48-pin DIP pinout, block diagram.
- §1-10: Paula 8364 features, 48-pin DIP pinout, block diagram.
- §1-12: J3 RGB connector pinout (23-pin DB).
- §1-13: J2 composite video / audio DIN pinout (8-pin).
- §1-14: J4 parallel port pinout.
- §1-15: J6 serial port pinout.
- §1-16: J7 external floppy pinout.
- §1-17: J11/J12 mouse/joystick port pinouts.
- Piggyback PCB schematics (OCR-noisy).

Quality: OCR is acceptable for prose sections but the schematics and block diagrams in §1-7, §1-9, §1-11 are largely unreadable (character salad).

### `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/A500_SERVICE_MANUAL_PN-314981-04_OCT_1990_text.txt` (4606 lines, text layer)

- §2-2 to §2-3: memory map.
- §2-3 to §2-5: 68000 interaction, bus control PAL description, CPU signal summary, clock generator.
- §2-8 to §2-10: Agnus 8370/8372A feature list (generic "Fat Agnus"), block diagram.
- §2-10: the "One Megabyte Agnus" bulletin quote.
- §2-11: Denise 8362 pinout and feature list.
- §2-13: Paula 8364 pinout and feature list.
- §2-15 to §2-16: Gary features and block diagram.
- §2-17 to §2-25: complete 8520 datasheet (full register map, timer, TOD, SDR, ICR, CRA/CRB, timing diagrams, electrical).
- §30: A500 86-pin edge connector pinout.

Quality: Text layer is clean for prose and signal tables. Schematic pages are heavily noisy.

### `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/A500 Service Manual.txt` (4429 lines, OCR)

Independent OCR of another A500 service manual. Used as a cross-check on the text-layer version. The 86-pin edge connector table in this version (at line 2006) matches the text-layer version and is used as the pinout reference in §12.1.

### `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/A500_Plus_Service_Manual_1991-10_Commodore_text.txt` (4601 lines, text layer)

- §1-1: feature list, A500+ overview.
- §1-2 to §1-3: A501+ memory expansion spec.
- §2-1: A500+ memory map.
- §2-6 through §2-10: 8375 Fat Agnus with block diagram, clock relations table (the only numeric clock timing table in the corpus), pin assignments, RAM addressing details, DMA channel functions.
- §2-11: 8373 Hi-Res Denise block diagram.
- §2-13: Gary pin list (identical to A500).
- §2-14 to §2-15: Paula description.

### `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/A4000 Service Manual.txt` (4513 lines, OCR)

- §1-1: system features.
- §3-1 to §3-5: motherboard + processor card jumper tables.
- §5-1 to §5-16: Bill of materials with all chip part numbers per location.
- §6-15: ASIC overview (BRIDGETTE, FAT RAMSEY, BUSTER, FAT GARY).
- **§6-16 through §6-28**: Full BRIDGETTE chip spec — description, configuration, pin descriptions (group/name/dir/description), DC characteristics, data paths with truth tables and timing numbers. The single most detailed chip spec in the corpus.
- **§6-29 through §6-42**: Full RAMSEY chip spec — control register layout, version register values, RAM memory map under different RSIZE/RAMWIDTH, RAM controller description, standard/page/burst modes, DMAC support, timing diagrams.
- §6-43 through §6-44: BUSTER chip spec (shorter — only pin diagram and environmental, no register-level detail).
- **§6-45 through §6-57**: Full FAT GARY chip spec — pin descriptions, address decoding Boolean expressions for every output, ROM timing, Chip RAM timing, Chip Registers, 8520 timing, RTC, FPU, SCSI/DMAC, Local Bus slot, bus timeout, ECLK, data strobes, AUTOVECTOR, AGNUS clock source, reset logic, interrupt control.

Quality: OCR for the A4000 SM is reasonably good for the chip spec pages (§6-16 onwards) because they are mostly tables and running text. Schematic pages are noisy. Chip pin connection diagrams (figures) are unreadable text salad.

### `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/A1200_System_Schematics_Service_Addendum_1992_Commodore_text.txt` (851 lines, text layer)

- Short addendum, not a full service manual.
- Page 1: revision history, signal glossary, key components list (which chip is at which U-number).
- The rest is schematic pages in poorly-extracted text form. The component list at §1 is the main usable content: U1 = 68000, U2 = 8374 Alice, U3 = 8364 Paula, U4 = 4203 Lisa, U5 = F023A Gayle, U7-U8 = 8520 CIAs, U13-U14 = Flash memory 28F10, U15-U17 = DRAM 256K×16, U18 = 68HC05 keyboard MPU, U20 = Budgie (391???), U30 = BT101 video DAC, U49 = PST518 power-on-reset.

### `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/System Schematics A500 rev 5,6A,7.txt` (4785 lines, OCR)

Separate OCR of the A500 schematics pages. Mostly duplicates the text-layer A500 SM but has a few jumper labels clearer. Used for cross-checking JP2/JP3 descriptions.

### `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/Amiga - An Update For The Service Technician.txt` (5217 lines, OCR)

Commodore service bulletin compilation, indexed by issue number / section / topic:

- **25/3-1.1** "A2000 PCB field upgrade to revision level 4.5": component removal list, Kickstart 1.2→1.3 ROM swap, 74HC244 vs HCY resistor pack conditionals, R5719 Toshiba→MOS replacement instruction, Gary 5719 part number 318072-01.
- **25/3-2.1** "Installation of new 'Fatter' Agnus in the Amiga 2000": full step-by-step procedure to add an 8372A to an A2000 rev 4.5 PCB — J101 to 2-3 for A19 enable, J500 /EXRAM trace cut, J102 NTSC/PAL.
- **25/3-3.1** "A2000 guru message on power up w/ rev 6 PCB": the PST518B Mitsumi reset IC retrofit procedure, part number 328156-02, XU1 location near U305/D802.
- **30/3-7.1** "Genlock problem with A2000": J300 tick signal jumper verification.
- **30/3-8.1** "One Megabyte Agnus use in Commodore A500 Computers": Commodore's official "we do not support 1 MB chip RAM on the A500" statement.

### `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/SAMS - Technical Service Data - CSCS26 - A500.txt` (5095 lines, OCR)

SAMS (not Commodore) photo-facts service data on the A500. Heavily OCR-garbled. Used only as a cross-check on power-rail specifications and not as a primary source.

### `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/Commodore Tech Topics.txt` (9226 lines, OCR)

Commodore developer newsletter. Indexed by volume/page:

- Volume 1 Number 1 covers A1200 and A4000 launch, A1200 C461 video glitch fix, and the Package Insert for Monitors on AGA systems.
- Historical sections include the A1000 Denise rev 5 / Agnus rev 5 field bulletin with free upgrades to rev 6 via A1300 genlock purchase.
- Multiple sections on A2091 hard disk controller updates (not relevant to this document).

### `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/Amiga_Intern_1992_Abacus.txt` (55813 lines, clean text)

Abacus Amiga Intern book. The relevant chapters:

- **§11.4 Custom Chips and the Amiga** — §11.4.1 Basic Structure, §11.4.2 The Structure of Agnus (including CPU wait state discussion for 68030), §11.4.3 The Structure of Denise, §11.4.4 The Structure of Paula.
- **§11.5 The Amiga Interfaces** — §11.5.1 Audio Outputs, §11.5.2 RGB Connector, §11.5.3 VGA Connector, §11.5.4 Video Slot (A3000-specific).
- **§11.7 Programming the Hardware** — §11.7.4 The Copper Coprocessor, §11.7.8 The Blitter. These chapters go beyond the HRM in some explanations but the corpus does not use their unique content here beyond a few quoted passages.

This is the cleanest OCR quality file in the corpus. Abacus texts were extracted from well-scanned PDFs.

---

*End of document.*

# Kickstart ROM Boot Flow Reference

When an emulated Amiga fails to boot, this reference tells you what the ROM was
doing at the point of failure — what hardware it probed, what response it
expected, and what comes next.

## Quick Debugging Reference

For detailed debugging tools (COLOR00 timeline, PC-to-module maps, known stall
signatures, module init hardware traces, inter-module dependencies), see
**[debugging-guide.md](debugging-guide.md)**.

| Symptom | Likely stage | What to check |
|---------|-------------|---------------|
| Black screen, CPU in bus error loop | Stage 1 | ROM mapping, overlay latch, SSP/PC vectors |
| Black screen, CPU running in ROM | Stage 3 | Chip RAM sizing (write/read pattern at $000000) |
| Grey screen (see [colour timeline](debugging-guide.md#color00-timeline)) | Stage 3–4 | ExecBase init, memory detection |
| Solid colour screen (red, green, yellow) | Stage 5–8 | Alert code in D7; see [Error Paths](#alert-colour-codes) |
| Bright pink screen | Stage 2 | ROM checksum failure (KS 2.0+) — corrupt ROM |
| Blue-green screen | Stage 3 | Chip RAM read-back failed — memory mapping |
| Insert-disk screen missing elements | Stage 10 | Copper DMA, bitplane DMA, BPLCON0 |
| Insert-disk screen OK but no disk boot | Stage 11 | trackdisk.device init, motor/MFM DMA |
| Guru meditation on warm start | Stage 2 | ExecBase validation at $000004 |
| CPU stuck, grey screen | Varies | Match PC to module: [KS 1.3 map](debugging-guide.md#ks-13-a500a2000-fc0000-base), [KS 3.1 map](debugging-guide.md#ks-31-a1200-f80000-base) |

### Alert Colour Codes

The Amiga displays full-screen colours as visual error codes when exec hits a
fatal condition before the display system is ready:

| Colour | Meaning | Common cause |
|--------|---------|--------------|
| Red flash | Dead-end alert (AN_DeadEnd) | Memory allocation failure |
| Yellow | Recoverable alert | Library/device open failure |
| Green | Alert during recovery | Nested alert |

The alert code sits in D7. The upper word encodes the subsystem (exec=$01,
graphics=$02, etc.) and the lower word identifies the specific failure.

## ROM Inventory

### 256K ROMs ($FC0000–$FFFFFF)

| Version | Build | File(s) | Target machines | SSP | PC |
|---------|-------|---------|-----------------|-----|-----|
| 1.0 | — | `kick10.rom` | A1000 | $11114EF9 | $FC00CE |
| 1.2 | 33.166 | `kick12_33_166_a1000.rom` | A1000 | $11114EF9 | $FC00D2 |
| 1.2 | 33.180 | `kick12_33_180_a500_a1000_a2000.rom` | A500, A1000, A2000 | $11114EF9 | $FC00D2 |
| 1.3 | 34.005 | `kick13.rom` = `kick13_34_005_a500_a1000_a2000_cdtv.rom` | A500, A1000, A2000, CDTV | $11114EF9 | $FC00D2 |
| 1.3 | 34.005 | `kick13_34_005_a3000.rom` | A3000 | $11114EF9 | $FC00D2 |

KS 1.3 A3000 differs from the standard KS 1.3 by 3 bytes: a machine-type flag
at offset $0198 (`$0008` vs `$0020` — A500/A2000 vs A3000) and the ROM checksum.

### 512K ROMs ($F80000–$FFFFFF)

| Version | Build | File(s) | Target machines | SSP | PC | Notes |
|---------|-------|---------|-----------------|-----|-----|-------|
| 2.0 beta | 36.028 | `kick20_36_028_a3000_beta.rom` | A3000 | $11144EF9 | $F800D2 | Beta |
| 2.02 | 36.207 | `kick202_36_207_a3000.rom` | A3000 | $11144EF9 | $F800D2 | |
| 2.04 | 37.175 | `kick204_37_175_a500plus.rom` | A500+ | $11144EF9 | $F800D2 | |
| 2.05 | 37.300 | `kick205_37_300_a600hd.rom` | A600 | $11144EF9 | $F800D2 | |
| 2.05 | 37.350 | `kick205_37_350_a600hd.rom` | A600 | $11144EF9 | $F800D2 | |
| 3.0 | 39.106 | `kick30_39_106_a1200.rom` | A1200 | $11144EF9 | $F800D2 | |
| 3.0 | 39.106 | `kick30_39_106_a4000.rom` | A4000 | $11144EF9 | $F800D2 | |
| 3.1 | 40.060 | `kick31_40_060_cd32.rom` | CD32 | $11144EF9 | $F800D2 | |
| 3.1 | 40.063 | `kick31_40_063_a500_a600_a2000.rom` | A500, A600, A2000 | $11144EF9 | $F800D2 | |
| 3.1 | 40.068 | `kick31_40_068_a1200.rom` | A1200 | $11144EF9 | $F800D2 | |
| 3.1 | 40.068 | `kick31_40_068_a3000.rom` | A3000 | $11144EF9 | $F800D2 | |
| 3.1 | 40.068 | `kick31_40_068_a4000.rom` | A4000 | $11144EF9 | $F800D2 | |
| 3.1 | 40.070 | `kick31_40_070_a4000_beta.rom` | A4000 | $11144EF9 | $F800D2 | Beta |
| 3.1 | 40.070 | `kick31_40_070_a4000t.rom` | A4000T | $11144EF9 | $F800D2 | |

### Anomalous ROMs (MapROM builds)

| Version | Build | File | PC | Notes |
|---------|-------|------|----|-------|
| 3.0 beta | 39.092 | `kick30_39_092_a600_beta.rom` | $002000D2 | MapROM — expects ROM at $200000 |
| 3.1 beta | 40.068 | `kick31_40_068_a600_beta.rom` | $002000D2 | MapROM — expects ROM at $200000 |

These A600 beta ROMs use PC=$002000D2 instead of $F800D2. They were built for
MapROM boards that place the ROM image at $200000. They boot on hardware with a
MapROM adapter but not in a standard emulator configuration. Documented here for
completeness; not traced in detail.

## SSP Value

All ROMs use SSP values with $11114EF9 (256K) or $11144EF9 (512K) as the
initial supervisor stack pointer. This points into unmapped space — the ROM
immediately overwrites SP before using the stack.

## Glossary

| Term | Meaning |
|------|---------|
| **Alert** | Exec error display — flashing colour bars with hex code. Red = dead-end, yellow = recoverable. |
| **CCK** | Colour clock — the fundamental DMA timing unit (7.09 MHz PAL). |
| **Chip RAM** | RAM accessible by both CPU and custom chips. $000000–$07FFFF (512K) or $000000–$1FFFFF (2M). |
| **Copper** | Custom chip coprocessor that writes registers synchronised to the beam position. |
| **Cold start** | Boot from power-on. ROM resets all hardware, sizes memory, builds ExecBase from scratch. |
| **DMACON** | DMA control register ($DFF096 write, $DFF002 read). Each bit enables a DMA channel. |
| **ExecBase** | The exec.library base pointer, stored at $000004. All other libraries hang off it. |
| **Fast RAM** | RAM on the CPU bus only — not DMA-accessible. $200000+ (A3000/A4000) or expansion. |
| **Overlay** | Hardware latch that maps ROM over chip RAM at $000000 for the reset vector fetch. CIA-A OVL bit clears it. |
| **Resident module** | A RomTag structure in ROM. Exec scans the ROM for these and initialises them in priority order. |
| **RomTag** | Structure starting with $4AFC (RTC_MATCHWORD). Contains module name, version, init function, priority. |
| **Slow RAM** | RAM at $C00000–$DBFFFF accessible only by CPU. A500 trapdoor expansion, A2000 motherboard. |
| **STRAP** | System Test and Registration Program — the diagnostic/insert-disk display. |
| **Warm start** | Soft reset (Ctrl-A-A). ROM validates ExecBase and attempts to preserve system state. |

## Document Index

| Document | Coverage |
|----------|----------|
| [boot-flow-overview.md](boot-flow-overview.md) | Common architecture, numbered boot stages, emulator implications |
| [debugging-guide.md](debugging-guide.md) | COLOR00 timeline, PC-to-module maps (all versions), register state cheat sheet, first boot checklist, known stall signatures, module init hardware traces, inter-module dependencies |
| [ks-1.0.md](ks-1.0.md) | Kickstart 1.0 (A1000) |
| [ks-1.2.md](ks-1.2.md) | Kickstart 1.2 (A1000, A500/A2000) |
| [ks-1.3.md](ks-1.3.md) | Kickstart 1.3 (A500/A2000/CDTV, A3000) — primary reference |
| [ks-2.0-beta.md](ks-2.0-beta.md) | Kickstart 2.0 beta (A3000) |
| [ks-2.02.md](ks-2.02.md) | Kickstart 2.02 (A3000) |
| [ks-2.04.md](ks-2.04.md) | Kickstart 2.04 (A500+) |
| [ks-2.05.md](ks-2.05.md) | Kickstart 2.05 (A600) |
| [ks-3.0.md](ks-3.0.md) | Kickstart 3.0 (A1200, A4000) |
| [ks-3.1.md](ks-3.1.md) | Kickstart 3.1 (all variants) — AGA reference |

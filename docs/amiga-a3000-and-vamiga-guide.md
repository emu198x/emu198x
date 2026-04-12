# Part 1: A3000 Service & Schematics Reference

*Extracted from the Commodore A3000 System Schematics (PN 314677-01, March 1990), OCR'd from `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/a3000_schematics.txt` (4,681 lines). This section fills the A3000 gap in `amiga-service-electrical.md`, which covers A500, A1000, A1200, and A4000.*

*OCR quality warning: the original is a set of 16 schematic sheets. Textual pages (functional spec, parts lists) OCR'd reasonably well. Schematic pages OCR'd into spatial ASCII art with heavy corruption — pin numbers, net names, and component references are often garbled. Where a value is OCR-suspect it is flagged with `[OCR?]`.*

---

## 1. A3000 overview

The A3000 is described in its own functional specification as "a cost reduced version of the A2500" using extensive gate-array technology. It ships in two speed grades:

| Variant | CPU | FPU | Crystal |
|---------|-----|-----|---------|
| 16 MHz | MC68030RC16 (QFP) | MC68881FN16 (PLCC, optional) | 32 MHz |
| 25 MHz | MC68030RC25 (QFP) | MC68882FN25 (PLCC) | 50 MHz |

Both variants use 28.6363 MHz (NTSC) or 28.375 MHz (PAL) for the video/chipset clock, same as all other Amigas. The 32/50 MHz oscillator feeds the 68030's CPU clock through the clock distribution circuit. PCB assembly number is **313310** (four variants: PAL/NTSC x 16/25 MHz).

### Key hardware features (beyond A500/A2000)

- 32-bit chip RAM access for the 68030 (up to 2 MB chip RAM on board, socketed expansion from 1 to 2 MB).
- 1-4 MB of 32-bit fast RAM on board (using 256Kx4 or 1Mx4 DRAMs, socketed expansion).
- Kickstart in 32-bit ROM (two ROMs: high word U181, low word U180; PN 390630-01 / 390629-01 for v1.4).
- On-board SCSI controller (WD33C93 + SDMAC) with internal 50-pin ribbon and external DB25 SCSI connector.
- On-board flicker fixer (Amber gate array) with VGA output.
- Real-time clock (RP5C01, U190).
- Four expansion slots: 2 bridged Amiga/PC-AT, 1 Amiga + video, 1 Amiga-only.

### Custom chip complement

In addition to the standard OCS/ECS chipset (Fat Agnus 8372, ECS Denise 8373, Paula), the A3000 adds five custom ICs:

| Chip | Part Number | Location | Function |
|------|-------------|----------|----------|
| Fat Gary | 390540-01 (4141F008A) | U110 | System glue: address decode, chip selects, DSACK generation |
| Fat Buster (Super Buster) | 390539-01 | U700 | Expansion bus controller: Zorro II/III arbitration, 32-bit bridge |
| Ramsey | 390541-01 | U890 | DRAM controller for on-board 32-bit RAM, address generation for SDMAC |
| SDMAC | 390537-01 | U802 | SCSI/DMA controller (successor to 4701-series DMAC) |
| Amber | 390538-01 | U478 | Flicker fixer / scan doubler gate array |

---

## 2. Fat Gary — system address decoder (U110)

Fat Gary is the A3000's equivalent of the A500/A2000's Gary chip. It generates chip selects and DSACK signals for motherboard resources.

### Responsibilities

- **Chip select generation** for: CIA-A (8520, U350), CIA-B (8520, U300), custom chip registers, chip RAM, Kickstart ROM, Real-Time Clock (RP5C01), and SDMAC.
- **DSACK generation** for all motherboard resources, translating the 68030's asynchronous bus protocol into the appropriate port sizes (8/16/32-bit acknowledgement).
- **Kickstart ROM overlay** control: the overlay bit (from CIA-A OVL output) controls whether the lowest 256 KB of address space maps to ROM or chip RAM — same function as Gary on the A500.
- **Reset and bus control** signal routing.

### Signals (from schematics, partially OCR-reconstructed)

Key signals visible on the schematics connecting Fat Gary (U110) to the rest of the system:

| Signal | Direction | Purpose |
|--------|-----------|---------|
| A[31:0] | Input | Address bus from 68030 |
| _AS | Input | Address Strobe from 68030 |
| _DS | Input | Data Strobe from 68030 |
| R/W (READ) | Input | Read/Write from 68030 |
| FC[2:0] | Input | Function codes from 68030 |
| SIZ[1:0] | Input | Transfer size from 68030 |
| _DSACK0, _DSACK1 | Output | Data transfer and size acknowledge |
| _CCS | Output | Custom chip select (active when $DE0000-$DFFFFF accessed) |
| _ROMEN | Output | ROM enable |
| _OVL | Input | Overlay control from CIA-A |
| _RESET | Bidirectional | System reset |
| _HALT | Bidirectional | Halt line |
| ECLK | Output | E-clock for CIAs |
| _DTACK | Input | From slow peripherals |
| _BOSS | Input/Output | Bus ownership for DMA arbitration |
| _BERR | Input | Bus error |
| _STERM | Input | Synchronous termination (for 32-bit ROM access) |
| _FCS2, _FCS3 | Output | [OCR?] Function code select outputs |
| _FPUCS | Output | FPU chip select |
| _CBR | Output | Chipset bus request |

### Memory decode regions

Fat Gary decodes the following address ranges (from the functional specification):

| Address Range | Resource | Bus Width |
|---------------|----------|-----------|
| $000000–$1FFFFF | Chip RAM (up to 2 MB) | 32-bit |
| $200000–$9FFFFF | Autoconfig expansion | 16-bit (Zorro II) |
| $A00000–$BFFFFF | CIA space (8520s) | 8-bit |
| $C00000–$D7FFFF | "Ranger" space (A500 magic $C00000) — unused on A3000 | — |
| $D80000–$D9FFFF | Custom chip register mirror / RTC | 16-bit |
| $DA0000–$DBFFFF | Unused | — |
| $DC0000–$DDFFFF | Real-Time Clock (RP5C01) | 8-bit |
| $DE0000–$DFFFFF | Custom chip registers (Agnus/Denise/Paula) | 16-bit |
| $E00000–$E7FFFF | Unused | — |
| $E80000–$E8FFFF | Autoconfig slot (primary) | 16-bit |
| $E90000–$EFFFFF | 64K autoconfig slots | 16-bit |
| $F00000–$FFFFFF | Kickstart ROM | 32-bit |

### Extended 32-bit address space (A[31:24] != 0)

| Address Range | Resource | Notes |
|---------------|----------|-------|
| $01000000–$07FFFFFF | On-board 32-bit fast RAM | Grows **downward** from $07FFFFFF |
| $08000000–$0FFFFFFF | 32-bit RAM expansion | Grows **upward** from $08000000 |
| $10000000–$EFFFFFFF | 32-bit "other" expansion space | Zorro III boards map here |

The downward growth of on-board fast RAM is significant for emulator authors: the Ramsey controller populates memory at the top of the $01–$07 range and works downward. This differs from Zorro II/III autoconfig which grows upward.

### DSACK behaviour

Fat Gary asserts the appropriate DSACK lines based on the port size of the resource being accessed:

- 32-bit resources (chip RAM, fast RAM, ROM): _DSACK0 + _DSACK1 both asserted (32-bit port)
- 16-bit resources (custom chips, Zorro II): _DSACK1 only (16-bit port)
- 8-bit resources (CIAs, RTC): _DSACK0 only (8-bit port)

The 68030 uses these to determine how many bytes were transferred and whether dynamic bus sizing is needed.

---

## 3. Ramsey — DRAM controller (U890)

Ramsey is a dual-function chip: it controls the on-board 32-bit DRAM array and provides address generation support for the SDMAC's DMA transfers.

### DRAM configuration

The A3000 supports two DRAM organisations visible on schematic sheet 16:

- **256Kx4 DRAMs** (part 318099-01, 414256, 100ns): Used in groups of 8 for 1 MB banks. Up to 4 banks = 4 MB.
- **1Mx4 DRAMs**: Used in groups of 8 for 4 MB banks. Socketed positions U850-U881 (many "NOT INSERTED" by default).

The schematic shows Ramsey generating:

| Signal | Purpose |
|--------|---------|
| _RAS[3:0] | Row Address Strobe for up to 4 DRAM banks |
| _CAS[3:0] | Column Address Strobe for up to 4 DRAM banks |
| _WE | Write Enable |
| FASTRAMADDR[9:0] | Multiplexed row/column address to DRAMs |
| SIZE SEL | Selects between 256K and 1M DRAM configurations |
| SPEED | Speed selection |

### Ramsey's interface to the CPU bus

From the schematics, Ramsey connects to the 68030 bus through:

- Address lines A[31:2] (directly and through latches/buffers)
- Data lines D[31:0] (directly connected to the DRAM data bus via 74F646 bidirectional transceivers, U253-U256 and U703-U707)
- Bus control: _AS, R/W, SIZ[1:0], _DSACK, CPUCLK, _STERM
- DMAEN: DMA enable signal from SDMAC
- BUFEN: Bus enable / buffer control
- FAIL: [OCR?] Possibly a self-test/parity failure signal

### Ramsey register interface

Ramsey is primarily a hardware controller rather than a register-mapped device from the CPU's perspective. Its configuration (RAM size, speed, organisation) is determined by:

1. Hardware jumpers / signals (SIZE SEL, SPEED)
2. The DRAM population itself (Ramsey auto-detects installed RAM)
3. A small set of configuration bits accessible through Fat Gary's address decode

**Version detection**: Ramsey revisions can be read by software. Known revisions include Ramsey-04 and Ramsey-07. AmigaOS Kickstart reads the Ramsey version during boot to configure memory timing. The revision byte is accessible at a Ramsey control register address in the $DE0000 space [exact address varies by Ramsey revision — consult the A3000 technical reference for register map details, as the schematics do not include a register-level specification].

### DMA support for SDMAC

Ramsey provides address generation for the SDMAC's SCSI DMA transfers. When the SDMAC signals DMAEN, Ramsey allows the SDMAC to drive the DRAM address bus and controls the RAS/CAS timing for the DMA cycle. The BRIDGE and BRIDGE_DIR signals on the schematic (sheet 5) control the data path direction during DMA.

---

## 4. Super Buster (Fat Buster) — expansion bus controller (U700)

Super Buster (officially "Fat Buster" in the functional specification, part 390539-01) manages the A3000's expansion bus. It provides backward compatibility with Zorro II (A2000-style 16-bit) expansion cards while adding Zorro III 32-bit bus protocol.

### Zorro III vs Zorro II

| Feature | Zorro II (A2000) | Zorro III (A3000) |
|---------|-----------------|-------------------|
| Data width | 16 bits | 32 bits |
| Address space | 24-bit ($200000–$9FFFFF, 8 MB) | 32-bit ($10000000–$EFFFFFFF, ~3.5 GB) |
| Bus protocol | Asynchronous, _DTACK | Extended, multiplexed address/data |
| DMA | Simple _BR/_BG/_BGACK | Full 32-bit address + data DMA |
| Speed | 7.09/7.16 MHz bus clock | Up to 25 MHz burst |
| Autoconfig | 8 MB expansion space | 32-bit expansion space |

Super Buster maps 68030 32-bit memory cycles onto the 16-bit Zorro II bus when a legacy card is accessed, performing the necessary bus sizing. For Zorro III cards, the full 32-bit bus is available.

### Signals (from schematics, sheet 14)

Key signals on Super Buster (U700):

| Signal Group | Signals | Purpose |
|-------------|---------|---------|
| CPU bus | _AS, _DS, SIZ[1:0], R/W, FC[2:0] | 68030 bus control |
| Expansion bus | _SBR, _SBG, _BGACK | Bus arbitration with expansion cards |
| Chip selects | _BOSS, _CBR, _CLIN | Chipset bus request/grant |
| DMA control | _DBLT, _DBREG, _DBR16 | [OCR?] DMA bus request types |
| Addressing | A[31:0] | 32-bit address for Zorro III |
| Data | D[31:0] | 32-bit data for Zorro III |
| Acknowledgement | _DSACK0, _DSACK1 | Transfer size ack |
| Slot control | _STERN | Synchronous termination |
| Interrupts | _IPL[2:0] | Directly connects to CPU IPL pins |
| Wait states | _WAIT | Expansion card wait state request |

### Data transceivers

Super Buster controls the 74F646 bidirectional bus transceivers that bridge the local 68030 bus and the expansion bus:

- U703–U707: Data bus transceivers for D[31:0]
- U708–U709: Additional bus transceivers (address/data, 74F245)
- U702: Read/write control transceiver

The 74F646 parts (also used by Ramsey for DRAM access) are registered bidirectional transceivers that allow pipelined bus transfers.

### Expansion slot configuration

From the schematics (sheets 13-14) and the functional specification:

- **CN606**: 200-pin Amiga local slot (32-bit Zorro III)
- **CN601, CN602**: Bridged Amiga/PC-AT slots (Amiga + PC-AT 16-bit ISA in-line)
- **CN603**: Amiga slot colinear with video expansion
- **CN751, CN752, CN753**: PC-AT 8/16-bit ISA slot extensions

Each slot has a full set of decoupling capacitors (1000pF, visible in the OCR as the repeated `C7xx|1000pF` patterns on sheet 13). Pull-up resistor packs RP751-RP755 (4.7K) provide bus termination.

### Slot interrupt routing

The expansion slots' interrupt lines are active-low, active-high depending on slot type. They are combined through PAL logic:

- U701: PAL 16R4A — interrupt priority encoder for expansion slot interrupts
- U714: PAL 16L8D — additional decode logic

These PALs combine expansion card _INT2, _INT6 signals and route them to the appropriate CPU IPL levels through the Super Buster.

---

## 5. SDMAC — SCSI DMA controller (U802)

The SDMAC (Super DMAC, part 390537-01) is an improved version of the A2091/A2090's DMAC (4701 series). It provides 32-bit DMA for the SCSI controller.

### SCSI subsystem components

| Component | Part Number | Location | Function |
|-----------|-------------|----------|----------|
| SDMAC | 390537-01 | U802 | DMA controller and address generator |
| WD33C93 | 390206-01 | U800 | SCSI protocol controller (Western Digital) |

### SDMAC signals (from schematics, sheet 15)

| Signal | Direction | Purpose |
|--------|-----------|---------|
| D[31:0] | Bidirectional | 32-bit data bus |
| A[31:2] | Input | Address from CPU (directly or via Ramsey) |
| _DREQ | Input | DMA request from WD33C93 |
| _DACK | Output | DMA acknowledge to WD33C93 |
| _INT5 | Output | Interrupt to Paula/CPU (directly connects as INT5, active on SCSI events) |
| _IOR, _IOW | Output | I/O read/write to WD33C93 |
| _IORDY | Input | I/O ready from WD33C93 |
| _CSX0, _CSX1 | Output | Chip select extension outputs |
| INC_ADD | Output | Increment address (self-incrementing DMA pointer) |
| _DMAEN | Output | Enables Ramsey's DMA mode for memory access |
| R/W, SIZ[1:0] | Input | From CPU bus |
| _DSACK0, _DSACK1 | Input/Output | Port size negotiation |
| _BERR | Input | Bus error |
| CPUCLK | Input | CPU clock |
| _RESET | Input | System reset |

### SDMAC register map

The schematics show the SDMAC at location U802 with connections to address lines A2–A7 (directly or through latches), suggesting a register file mapped at a specific offset within the I/O space. The register interface is accessible through Fat Gary's decode of the $DD0000–$DDFFFF range (the A3000 uses a different base address from the A2091's $E80000 autoconfig range since it is an on-board device).

Key SDMAC registers (from published A3000 technical documentation, not fully visible in the schematics OCR):

| Register | Offset | Width | Purpose |
|----------|--------|-------|---------|
| DAWR | $00 | Long | DMA address write (set DMA start address) |
| WTC | $04 | Long | Word transfer count |
| CNTR | $08 | Word | Control register (direction, interrupt enable) |
| ISTR | $0C | Word | Interrupt status register |
| FLUSH | $10 | — | Flush FIFO (write any value) |
| SASR | $40 | Byte | SCSI auxiliary status register (WD33C93 indirect) |
| SCMD | $42 | Byte | SCSI command register (WD33C93 indirect) |

*[Note: exact register offsets are from published A3000 documentation; the schematics confirm the address line connections but do not label the internal register addresses.]*

### SCSI connector pinout

The schematics show two SCSI connectors:

- **Internal**: CN802 — 50-pin IDC ribbon cable header for internal hard drive
- **External**: CN801 — DB25 SCSI connector on the rear panel

The DB25-to-50-pin mapping uses EMI filtering (EMI800–EMI817 ferrites) and termination resistor packs (RP800, RP801, RP802, RP804 — 220/330 ohm). Signal names visible: SCSI_DATA[0:7], SCSI_CONT[0:9] covering the full SCSI bus.

---

## 6. Clock distribution

### Oscillators on the A3000 PCB

The parts list confirms four oscillator positions:

| Designator | Frequency | Part Number | Purpose |
|------------|-----------|-------------|---------|
| U111 | 28.6363 MHz (NTSC) or 28.375 MHz (PAL) | 325566-14 (PAL: 252344-01) | Video/chipset master clock |
| U104 | 32 MHz or 50 MHz | 325566-24 (32 MHz) / 325566-27 (50 MHz) | CPU clock source |
| U103 | — (unstuffed) | — | Reserved position |

### Clock derivation chain

The video/chipset clock chain is identical to other Amigas:

```
28.6363 MHz (NTSC) or 28.375 MHz (PAL) master oscillator
  |
  +---> /2 = 14.318 MHz (color carrier x4 for NTSC) — used internally by Agnus
  +---> /4 = 7.159 MHz (NTSC) or 7.093 MHz (PAL) — chipset bus clock (CCK)
  +---> /8 = 3.579 MHz (NTSC) or 3.546 MHz (PAL) — E-clock for CIAs
```

The CPU clock is derived from the separate 32/50 MHz oscillator:

```
32 MHz oscillator (16 MHz variant)
  |
  +---> /2 = 16 MHz — MC68030 CLK input

50 MHz oscillator (25 MHz variant)
  |
  +---> /2 = 25 MHz — MC68030 CLK input
```

The 74F74 flip-flop U201 (sheet 15) performs the divide-by-two for the CPU clock. The schematic shows R803 (33K) as a speed selection resistor, with jumper J800 selecting between 14 MHz / 7 MHz operation.

### Asynchronous bus crossing

Because the CPU clock (16 or 25 MHz) and the chipset clock (7.09/7.16 MHz) are **asynchronous** with respect to each other, the A3000 requires synchronisation logic when the CPU accesses chipset resources. This is handled by Fat Gary's DSACK timing — it delays the assertion of _DSACK to align with the chipset clock when accessing custom chip registers or chip RAM.

This asynchronous relationship is a key difference from the A500/A2000 (where the CPU clock is derived from the same oscillator as the chipset clock, making them synchronous) and has implications for emulator timing accuracy.

---

## 7. Memory map differences

The A3000 memory map has several important differences from the A500/A2000:

### Chip RAM access width

On the A500/A2000, chip RAM is accessed through a 16-bit data path shared with the custom chips. On the A3000, the 68030 has **32-bit access** to chip RAM through dedicated 74F646 transceivers. The custom chips (Agnus/Denise/Paula) still access chip RAM through the 16-bit Agnus bus — it is the CPU path that is widened.

This means CPU reads/writes to chip RAM complete in half the clock cycles compared to the A500/A2000 when the bus is free, but DMA contention still applies at the 16-bit Agnus level.

### Kickstart ROM

The A3000 uses two 32-bit-wide ROMs:

- U181: High word (390630-01, ROM v1.4 32-bit high)
- U180: Low word (390629-01, ROM v1.4 32-bit low)

The CPU accesses ROM in 32-bit cycles with _STERM (synchronous termination), which is faster than the asynchronous DSACK protocol used for other resources.

### "Ranger" RAM

The $C00000–$D7FFFF region is described in the memory map as "Location of A500 Magic $C00000 memory. Unused in this machine." This space was used on the A500 for the optional internal 1 MB "Ranger" / "slow" RAM expansion. On the A3000, this address range is inactive.

### Kickstart-in-RAM

The A3000 supports loading Kickstart into RAM (the "KSRST" / "soft-kick" feature), where the Kickstart image is copied from disk into fast RAM during early boot, then mapped over the ROM address space. This is used for development and for running UNIX (Amiga Unix / Amix).

---

## 8. Amber — flicker fixer (U478)

The Amber gate array (390538-01) provides an on-board scan doubler / flicker fixer:

- Converts interlaced Amiga video to progressive VGA-compatible output.
- Uses specialised video DRAMs: MSM514221RS-6 (OKI, 390530-01, positions U470-U472) for line doubling buffers, and UPD42101C-2 (390532-01, positions U473-U475, U479) for additional video buffering.
- The NES64 (390531-01, U481) provides a clock synthesiser (likely the 28 MHz VGA dot clock).
- Outputs to CN451 (15-pin VGA connector) in addition to the standard CN450 (DB23 Amiga video connector).

The Amber circuit includes:

- RGB DACs feeding the VGA connector
- HSYNC/VSYNC generation for VGA timing
- Bypassing capability (jumper/software-controlled) for applications that require native Amiga timing
- A delay line (390555-01, 25ns, U102) for clock phase adjustment

For emulator authors, the Amber's scan doubling is irrelevant (the emulator generates the output at whatever resolution it chooses), but its presence explains the video memory ICs on the A3000 board.

### Amber signal path (from schematics, sheets 9-10)

The video signal flow from Denise to Amber:

1. Denise outputs 4-bit R, G, B colour signals plus sync signals (_HSYNC, _VSYNC, BCSYNC).
2. 74HCT244 buffers (U451, U452) buffer the pixel data and sync signals.
3. The video hybrid (HY480) combines the digital colour data with sync to produce composite video for the standard DB23 connector (CN450).
4. Simultaneously, the R[3:0], G[3:0], B[3:0] signals feed into Amber (U478).
5. Amber stores incoming pixels in its line buffer DRAMs (U470-U472 for one field, U473-U475/U479 for the other).
6. Amber reads out pixels at double rate, producing progressive scan output.
7. The progressive R, G, B outputs drive the VGA connector (CN451).

The NES64 clock synthesiser (U481) generates the VGA-compatible dot clock. The 74ALS163A counter (U476) provides timing signals for the line-doubling state machine.

### Amber bypass and configuration

On sheet 10, jumper/switch connections are visible:
- SW480: Appears to control bypass/enable of the scan doubler
- N_BYPASS signal: When active, passes video through without doubling
- N_SCROBL [OCR?]: Possibly scroll blanking control
- TEST_SEL: Test mode selection

The PIXELSW signal selects between Amber's output and the direct Denise output for the VGA connector.

---

## 8b. Video output path detail

### Standard Amiga video (CN450, DB23)

The standard video output uses the video hybrid module (HY480, part 390229-01) at location U480. This module contains the DACs and sync mixing circuitry to produce composite and separate-sync video. It connects to the DB23 connector (CN450) with the standard Amiga video pinout.

The hybrid receives:
- 4-bit R, G, B from Denise (buffered through 74HCT244s)
- BCSYNC (composite blanking/sync)
- _HSYNC, _VSYNC (separate horizontal and vertical sync)
- PIXELSW (pixel clock)

### VGA video (CN451, DB15)

The VGA connector receives progressive-scan video from Amber:
- ARED-S, AGREEN-S, ABLUE-S: Analog R, G, B from Amber's output DACs
- BSYNC-S: Composite sync (active low)
- _HSYNC-S, _VSYNC-S [OCR?]: Separate sync signals
- Various grounds and ID pins

The Amber circuit includes inline filtering (47pF capacitors C457, C458) on the VGA sync lines for EMI compliance.

---

## 9. Standard chipset on the A3000

The A3000 uses ECS (Enhanced Chip Set) versions of the core Amiga custom chips:

| Chip | Part Number | Location | Notes |
|------|-------------|----------|-------|
| Fat Agnus (8372) | 318069-03 (2 MB Agnus) | U205 | ECS Agnus with 2 MB chip RAM addressing |
| ECS Denise (8373) | 390433-01 | U450 | ECS Denise with productivity modes |
| Paula | 252127-01 | U400 | Standard Paula (unchanged across all models) |
| CIA-A (8520) | 318029-02 | U350 | Standard 8520 CIA |
| CIA-B (8520) | 318029-02 | U300 | Standard 8520 CIA |

These are the same chips used in late-revision A500 and A2000 boards. Their registers, DMA timing, and behaviour are identical to what is documented in `amiga-hardware-reference.md` and `amiga-service-electrical.md`.

### PAL logic

Four PAL (Programmable Array Logic) ICs provide additional glue logic:

| Part Number | Type | Location | Purpose |
|-------------|------|----------|---------|
| 390526-01 | 16L8D | U202 | [Function not labelled in schematics] |
| 390527-01 | 16L8D | U203 | [Function not labelled in schematics] |
| 390528-01 | 16R4A | U701 | Interrupt priority / expansion bus decode |
| 390529-01 | 16L8D | U714 | Address decode / bus control |

These PALs contain Commodore-proprietary logic equations. Their exact decode functions are not published in the schematics (the schematics show the PAL as a black box with input/output labels).

---

## 10. CIA wiring on the A3000

The A3000 uses the same CIA (8520) chips as other Amigas, at the same I/O addresses ($BFE001 for CIA-A, $BFD000 for CIA-B). From schematics sheets 7-8:

### CIA-A (U350) — active at odd addresses $BFE001–$BFEF01

| Port | Bit | Signal | Direction | Function |
|------|-----|--------|-----------|----------|
| PA0 | 0 | _FIRIO | Output | Gameport 0 fire button / left mouse button |
| PA1 | 1 | _FIRIO | Output | Gameport 1 fire button |
| PA2-PA7 | 2-7 | Standard | — | As per A500 (see `amiga-service-electrical.md`) |
| PB0-PB7 | 0-7 | _PARALLEL | Bidirectional | Parallel port data (directly to CN350 DB25) |
| FLAG | — | _INDEX | Input | Floppy index pulse |
| TOD | — | _VSYNC | Input | Vertical sync from Agnus (50/60 Hz timebase) |

### CIA-B (U300) — active at even addresses $BFD000–$BFDF00

| Port | Bit | Signal | Direction | Function |
|------|-----|--------|-----------|----------|
| PA0 | 0 | _STEP | Output | Floppy step |
| PA1 | 1 | DIR | Output | Floppy direction |
| PA2 | 2 | _SIDE | Output | Floppy head select |
| PA3 | 3 | _SEL0 | Output | Floppy drive 0 select |
| PA4 | 4 | _SEL1 | Output | Floppy drive 1 select |
| PA5 | 5 | _SEL2 | Output | Floppy drive 2 select |
| PA6 | 6 | _SEL3 | Output | Floppy drive 3 select |
| PA7 | 7 | _MTR | Output | Floppy motor control |
| PB0-PB7 | — | Serial control/parallel | — | As per A500 |
| FLAG | — | _DSKINDEX [OCR?] | Input | [OCR?] May be different on A3000 |
| TOD | — | _HSYNC | Input | Horizontal sync from Agnus |

The serial port uses 1488/1489 RS-232 line drivers/receivers (U304/U305) connecting to CN300 (DB25 serial).

The floppy interface uses 74LS00 gates (U355) and 7407 open-collector drivers for the disk signals, driving the internal floppy header CN351 and external drive connector. The A3000 supports two floppy drives (DF0 internal, DF1 external via header).

---

## 11. Other I/O

### Serial port

Standard RS-232 serial through CN300 (DB25), same as A500/A2000. EMI filtering through EMI302–EMI309 ferrites.

### Parallel port

Standard Centronics parallel through CN350 (DB25), directly connected to CIA-A port B. EMI filtering through EMI350 ferrites.

### Mouse / joystick ports

Two DB9 ports (CN400, CN401) for gameports. The joystick/mouse buttons and direction signals connect to Paula's gameport registers and CIA-A as on the A500/A2000. Potentiometer lines go to U401 (74LS157 multiplexer) for analog paddle input.

### Audio output

Four-channel audio from Paula, through LF347 op-amp filters (U402), to stereo RCA outputs (CN403). The audio filter circuit is similar to the A500 but uses LF347 quad op-amps. Audio filtering: 4th-order low-pass at approximately 3.4 kHz with 470K feedback resistors (R435, R445).

### Keyboard

Keyboard connects through CN420 (6-pin DIN). The keyboard interface uses pull-up resistors R420 (1 ohm, with _KBCLK and _KBDAT signals), same protocol as A500/A2000.

---

## 12. Power distribution

The A3000 uses a switching power supply providing:

- +5V (main logic rail)
- +12V (serial port, disk drives)
- -12V (serial port RS-232 drivers)
- +5V_USER (filtered 5V for audio and user port)

Fuse protection: F300, F350, F351, F400, F450 (3A fuses, part 390280-01).

The power supply is available in 120V (313285-02) and 240V (313285-01) mounting bracket assemblies, with specific power supply units 390500-01 (240V) and 300500-02 (120V).

---

## 13. Schematic sheet index

| Sheet | Content |
|-------|---------|
| 1 | Title page, functional specification |
| 2-4 | Parts lists |
| 5 | Chip RAM data path (DRAM array, 74F373/74F646 transceivers, Ramsey interface) |
| 6 | SCSI LED, external serial port (1488/1489), CIA-B |
| 7 | Floppy interface, CIA-A, CIA-B, parallel port |
| 8 | Audio output (LF347 filters), joystick/mouse ports, keyboard |
| 9 | Denise output, video hybrid, 74HCT244 buffers, Amber input |
| 10 | Amber flicker fixer, VGA output, NES64 clock, UPD42101 video RAM |
| 11 | Fat Agnus (68030 bus interface), CPU connections |
| 12 | Fat Gary (full pinout), expansion bus interconnect |
| 13 | Expansion slots (Amiga 32-bit local slot, IBM 8/16-bit) |
| 14 | Super Buster (U700), PAL decode logic, 74F245/74F646 transceivers |
| 15 | SCSI subsystem (WD33C93, SDMAC), reset/power circuit |
| 16 | Fast RAM array (Ramsey controller, DRAM banks) |

---

## 14. Reset circuitry

### Reset generation

The A3000 uses a PST518 power supervisor (U112, part 328156-02) for power-on reset generation. This is more sophisticated than the simple RC circuit used on the A500/A2000.

The PST518 monitors the +5V rail and holds _RESET asserted for a guaranteed minimum period after power stabilises. The _RESET signal from the PST518 feeds into Fat Gary, which distributes it to:

- MC68030 _RESET and _HALT pins (directly)
- CIAs (both 8520s) _RES pin
- Custom chips (Agnus, Denise, Paula)
- Expansion bus _RESET line
- WD33C93 SCSI controller _RST
- SDMAC _RESET

From the schematics (sheets 14-15), the reset chain includes:

- `_CPURST` — CPU-specific reset (from PAL U701)
- `_FPURST` — FPU-specific reset
- `_KBRST` — Keyboard reset (active when Ctrl-Amiga-Amiga pressed)
- `_TORST` — Total reset (combination of all reset sources)
- `_ERST` — Expansion bus reset

The `_KBRST` signal from the keyboard controller feeds through a 74F32 OR gate (U712) to combine with the PST518's power-on reset. This means Ctrl-Amiga-Amiga produces a warm reset that does not affect the power supervisor timing.

### Reset sequencing (emulator relevance)

For emulator authors, the A3000's reset behaviour is significant because:

1. The 68030 requires _RESET and _HALT to be asserted simultaneously for a hardware reset (vs. just _RESET for a software reset via the RESET instruction).
2. The PST518's reset hold time is longer than the simple RC circuits on earlier models, but this timing difference is rarely visible to software.
3. The expansion bus reset can be asserted independently of the CPU reset, which matters for bus master expansion cards.

---

## 14b. Bus arbitration on the A3000

### 68030 bus protocol

The A3000's 68030 uses the standard Motorola bus arbitration signals:

- **_BR** (Bus Request): Asserted by a bus master (Agnus, expansion card) to request the bus.
- **_BG** (Bus Grant): Asserted by the 68030 to grant the bus.
- **_BGACK** (Bus Grant Acknowledge): Asserted by the new bus master when it takes ownership.

Fat Gary and Super Buster manage bus arbitration between:

1. The 68030 CPU (default bus master)
2. Agnus (for chipset DMA — bitplane, sprite, disk, audio, copper, blitter)
3. Expansion bus masters (Zorro II/III DMA cards)
4. SDMAC (for SCSI DMA transfers)

### Arbitration priority

From the schematics and functional specification:

1. Agnus chipset DMA has highest priority (it must maintain video timing)
2. SDMAC SCSI DMA has the next priority
3. Expansion bus DMA cards have the lowest DMA priority

The `_BOSS` signal between Fat Gary and Super Buster coordinates chipset bus ownership. When Agnus needs the bus for DMA, Fat Gary asserts _BOSS, which tells Super Buster to defer any expansion bus DMA requests.

The `_CBR` (Chipset Bus Request) signal from Agnus tells Fat Gary that chipset DMA is needed. Fat Gary then manages the arbitration with the 68030 and SDMAC.

---

## 14c. Power supervisor detail

The PST518 (U112) provides:

- Power-on reset with a guaranteed minimum assertion time
- Brown-out detection (if +5V drops below threshold, reset is asserted)
- Open-drain _RESET output (active low)

The reset circuit also includes:
- A 1N4148 signal diode (visible on schematic sheet 7)
- Various filter capacitors for noise immunity

---

## 15. OCR quality notes and gaps

The schematics OCR quality is poor for component-level detail. Specific gaps:

- **Pin numbers**: Largely illegible due to OCR of small text near component outlines.
- **Net names**: Partially reconstructed from context and surrounding signal names.
- **Component values**: Resistor and capacitor values in the schematic artwork are mostly garbled.
- **PAL equations**: Not included in the schematics (proprietary).
- **Ramsey register map**: Not in the schematics; only the external signal interface is visible.
- **SDMAC internal registers**: Not fully documented in the schematics.
- **Fat Gary decode equations**: Not documented; behaviour inferred from the memory map.

For authoritative register-level information on A3000 custom chips (Ramsey, SDMAC, Fat Gary, Super Buster), consult:
- The Amiga A3000 Technical Reference Manual (not in the current corpus)
- The A3000+ / A4000 technical notes (which document Ramsey-07 registers)
- Dave Haynie's technical notes on the Zorro III specification

---

## 15. A3000 parts list — key ICs

The following table consolidates the IC components from the A3000 PCB assembly #313310 parts list, which OCR'd cleanly from the schematics document.

### Custom and semi-custom ICs

| Part Number | Description | Location(s) | Notes |
|-------------|-------------|-------------|-------|
| 318069-03 | 8372 Fat Agnus (2 MB) | U205 | ECS Agnus, 2 MB chip RAM addressing |
| 390433-01 | 8373 ECS Denise | U450 | ECS Denise with productivity modes |
| 252127-01 | Paula | U400 | Standard Paula |
| 318029-02 | 8520 CIA | U300, U350 | Two CIAs |
| 390538-01 | Amber | U478 | Flicker fixer gate array |
| 390537-01 | SDMAC (DPMC) | U802 | SCSI DMA controller |
| 390539-01 | Fat Buster (Super Buster) | U700 | Expansion bus controller |
| 390540-01 | Fat Gary (4141F008A) | U110 | System glue / address decoder |
| 390541-01 | RAM Controller (Ramsey) | U890 | DRAM controller |
| 390206-01 | WD33C93 | U800 | SCSI protocol controller |
| 390525-01 | RP5C01 | U190 | Real-time clock |
| 390530-01 | MSM514221RS-6 | U470-U472 | OKI video DRAM (for Amber) |
| 390531-01 | NES64 | U481 | Clock synthesiser (for Amber) |
| 390532-01 | UPD42101C-2 | U473-U475, U479 | Video buffer RAM (for Amber) |
| 390399-03 | MC68030 16 MHz QFP | U100 | CPU (16 MHz variant) |
| 390339-04 | MC68030 25 MHz QFP | U100 | CPU (25 MHz variant) |
| 390277-03 | MC68881 16 MHz PLCC | U101 | FPU (16 MHz variant, optional) |
| 390434-02 | MC68882 25 MHz PLCC | U101 | FPU (25 MHz variant) |
| 328156-02 | PST518 | U112 | Power supervisor / reset generator |

### Glue logic ICs

| Part Number | Description | Location(s) | Notes |
|-------------|-------------|-------------|-------|
| 390526-01 | PAL 16L8D | U202 | Glue logic (equations proprietary) |
| 390527-01 | PAL 16L8D | U203 | Glue logic (equations proprietary) |
| 390528-01 | PAL 16R4A | U701 | Interrupt/expansion decode |
| 390529-01 | PAL 16L8D | U714 | Address/bus control decode |
| 390555-01 | Delay line 25 ns | U102 | Clock phase adjustment |

### Standard logic ICs

| Part Number | Type | Location(s) |
|-------------|------|-------------|
| 390108-01 | 74F373 | U251 |
| 390089-01 | 74F245 | U702, U708, U709, U715, U716, U257, U258, U891+ |
| 390281-01 | 74F646 | U253-U256, U703-U707 |
| 901521-01 | 74F86 | U252 |
| 390077-01 | 74F32 | U712 |
| 390198-01 | 74LS00 | U303, U710 |
| 901521-06 | 74LS74A | U201, U351, U480 |
| 901521-03 | 74LS08 | U355 |
| 901521-11 | 74LS157 | U401 |
| 901521-63 | 74LS174 | U195, U354 |
| 390420-01 | 74ALS163 | U476 |
| 318828-01 | 74HCT244 | U451-U452 |
| 901522-30 | 7407 | U713, U801 |
| 901882-01 | 1488 | U304 |
| 901883-01 | 1489 | U305 |

### DRAM ICs

| Part Number | Type | Location(s) | Notes |
|-------------|------|-------------|-------|
| 318099-01 | 414256 (256Kx4, 100ns) | U850-U881, U263-U266, U271-U274, U858-U881 | Many NOT INSERTED by default |

### Kickstart ROMs

| Part Number | Description | Location |
|-------------|-------------|----------|
| 390630-01 | ROM v1.4 32-bit HIGH WORD | U181 |
| 390629-01 | ROM v1.4 32-bit LOW WORD | U180 |

---

## 16. Connectors

| Designator | Type | Function |
|------------|------|----------|
| CN300 | DB25 | External serial port (RS-232) |
| CN302 | — | SCSI LED / Power LED |
| CN350 | DB25 | Centronics parallel port |
| CN351 | 34-pin IDC | Internal floppy drive header |
| CN400, CN401 | DB9 | Gameport 0 and 1 (mouse/joystick) |
| CN403 | RCA pair | Stereo audio output |
| CN420 | 6-pin DIN | Keyboard connector |
| CN450 | DB23 | Standard Amiga video output |
| CN451 | DB15 (VGA) | VGA video output (from Amber) |
| CN601 | 100-pin + ISA | Bridged Amiga/PC-AT slot 1 |
| CN602 | 100-pin + ISA | Bridged Amiga/PC-AT slot 2 |
| CN603 | 100-pin + video | Amiga slot + video expansion |
| CN606 | 200-pin | 32-bit Amiga local slot (Zorro III) |
| CN751-753 | ISA | PC-AT 8/16-bit slot extensions |
| CN754-756 | — | Slot power/ground extensions |
| CN801 | DB25 | External SCSI connector |
| CN802 | 50-pin IDC | Internal SCSI ribbon header |

---

## 17. Oscillator and crystal summary

| Part Number | Frequency | Type | Location | Used in |
|-------------|-----------|------|----------|---------|
| 252344-01 | 28.375 MHz | Crystal oscillator | U111 | PAL video/chipset clock |
| 325566-14 | 28.6363 MHz | Crystal oscillator | U111 | NTSC video/chipset clock |
| 325566-24 | 32 MHz | Crystal oscillator | U104 | 16 MHz CPU clock source |
| 325566-27 | 50 MHz | Crystal oscillator | U104 | 25 MHz CPU clock source |
| — | (unstuffed) | — | U103 | Reserved |

---

## 18. Resistor pack summary (active termination)

The A3000 uses extensive bus termination through resistor packs:

| Designator(s) | Value | Pin count | Purpose |
|---------------|-------|-----------|---------|
| RP700, RP890, RP891 | 22 ohm 4x8 | 8 | Data bus series termination |
| RP252-RP254, RP451, RP452 | 47 ohm 4x8 | 8 | Video/sprite bus termination |
| RP401 | 63 ohm 4x8 [OCR?] | 8 | Gameport pull-ups |
| RP100-RP110, RP351, RP703 | 1K ohm 9x10 | 10 | General pull-ups |
| RP400 | 4.7K ohm 9x10 | 10 | Weak pull-ups |
| RP800, RP801, RP802, RP804 | 220/330 | 6/10 | SCSI active termination |
| RP892, RP893 | 22 ohm 5x10 | 10 | Fast RAM data bus termination |
| RP201, RP251, RP255 | 47 ohm 5x10 | 10 | Expansion bus termination |
| RP350, RP800, RP801 | 3.3K ohm 5x10 | 10 | SCSI/expansion pull-ups |

---

# Part 2: vAmiga Source Guide

*vAmiga is a cycle-accurate Amiga emulator by Dirk W. Hoffmann, written in modern C++ and released under the Mozilla Public License v2. Its source code is remarkably clean and well-structured, with one C++ class per hardware component, clear separation of concerns, and extensive inline documentation.*

*All file paths below are relative to `~/Projects/Emu198x-Unclean/vAmiga/Core/Components/`. Line counts are approximate (measured April 2026).*

---

## 1. Architecture overview

vAmiga is **event-driven**: the Agnus chip hosts a central event scheduler that drives all other components. The main emulation loop calls `Agnus::execute()` once per DMA cycle (one "slot" = 2 master clock cycles for PAL/NTSC). Agnus checks for pending events and dispatches to the appropriate subsystem.

This is fundamentally different from WinUAE, which uses a more traditional run-ahead approach where the CPU executes for a chunk of cycles, then the chipset catches up. vAmiga's approach gives it cycle-exact accuracy at the cost of some overhead per cycle.

### Component hierarchy

```
Amiga (Amiga.cpp/h, ~350 lines)
  |
  +-- Agnus (DMA controller + event scheduler)
  |     +-- Sequencer (DMA timeslot allocation)
  |     +-- Copper
  |     +-- Blitter
  |     +-- DmaDebugger
  |
  +-- Denise (graphics)
  |     +-- PixelEngine (colour synthesis, texture output)
  |     +-- DeniseDebugger
  |
  +-- Paula (audio, interrupts, disk)
  |     +-- StateMachine[0..3] (audio channels)
  |     +-- DiskController
  |     +-- UART
  |
  +-- CIA-A, CIA-B
  |     +-- TOD (Time-of-Day counter)
  |
  +-- CPU (Moira 68000 core)
  |
  +-- Memory
  +-- RTC (Real-Time Clock)
  +-- ZorroManager
        +-- RamExpansion
        +-- HdController[0..3]
        +-- DiagBoard
```

### Clocking model

All components share a single master clock (`Agnus::clock`). When the CPU needs to execute, Agnus runs `executeUntil(cycle)` which dispatches pending events. The CPU keeps its own `clock` value and calls back into Agnus via `sync()` methods whenever it accesses the bus.

The event scheduler divides slots into **primary** (checked every cycle), **secondary** (checked only when a wakeup is pending in the SEC_SLOT), and **tertiary** (rare events). This three-tier system avoids checking cold slots each cycle.

---

## 2. Agnus — DMA controller and event scheduler

**Files**: `Agnus/` — 12 files in the main directory plus 4 subdirectories.

| File | Lines | Purpose |
|------|-------|---------|
| Agnus.h | 804 | Class definition: event slots, registers, beam position, bus ownership |
| Agnus.cpp | 800 | Construction, reset, configuration, main `execute()` loop |
| AgnusEvents.cpp | 828 | Event scheduler: scheduling, dispatching all event types |
| AgnusRegs.cpp | 821 | Register read/write handlers for all Agnus-accessible registers |
| AgnusDma.cpp | 239 | DMA read/write operations, bus arbitration |
| AgnusInfo.cpp | 731 | Inspector / debugger information gathering |
| Beam.cpp / Beam.h | 302/177 | Beam position tracking, PAL/NTSC line counting |
| AgnusTypes.h | 598 | Enums, config structs, revision types |
| BusTypes.h | 147 | Bus owner enum (BPL, CPU, COP, BLT, etc.) |

### Key design: the event slot array

Agnus maintains a flat array of event slots:

```cpp
Cycle trigger[SLOT_COUNT];  // When the event fires
EventID id[SLOT_COUNT];     // What event it is
i64 data[SLOT_COUNT];       // Optional data payload
Cycle nextTrigger;          // Earliest trigger across all primary slots
```

Slot categories (from `AgnusTypes.h`):
- **Primary**: SLOT_REG, SLOT_RAS, SLOT_BPL, SLOT_DAS, SLOT_COP, SLOT_BLT, SLOT_SEC
- **Secondary**: SLOT_CH0–SLOT_CH3 (audio), SLOT_DSK, SLOT_VBL, SLOT_IRQ, SLOT_IPL, SLOT_KBD, SLOT_TXD, SLOT_RXD, SLOT_POT, SLOT_TER
- **Tertiary**: SLOT_DC0–SLOT_DC3 (disk change), SLOT_MSE1, SLOT_MSE2, SLOT_KEY, SLOT_SRV, SLOT_SER, SLOT_ALA, SLOT_INS

Events are scheduled in master clock cycles. The `DMA_CYCLES(n)` macro converts DMA slot numbers to master clock cycles.

### Beam position tracking

The `Beam` struct (`Beam.h`) tracks:
- `v` — vertical position (line number)
- `h` — horizontal position (DMA cycle within line, 0 to HPOS_CNT-1)
- `lof` — Long/short frame flag (PAL long frame = 313 lines, short = 312)
- `type` — PAL or NTSC

The beam advances by one `h` position per `execute()` call. At the end of a line, `eolHandler()` fires; at end of frame, `eofHandler()` fires. The `hsyncHandler()` runs when h reaches the HSYNC area.

### Bus ownership recording

```cpp
BusOwner busOwner[HPOS_CNT];  // Who owned the bus at each cycle
u32 busAddr[HPOS_CNT];        // What address was accessed
u16 busData[HPOS_CNT];        // What data was transferred
```

This per-line recording enables the DMA debugger to visualise exactly which DMA channel used each cycle — a feature that directly mirrors the HRM's DMA allocation diagram (Fig. 6-9).

---

## 3. Sequencer — DMA timeslot allocation

**Files**: `Agnus/Sequencer/` — 7 files, ~1,600 lines total.

| File | Lines | Purpose |
|------|-------|---------|
| Sequencer.h | 404 | Class definition, event tables, signal recorder |
| Sequencer.cpp | 116 | Construction, reset |
| SequencerBpl.cpp | 582 | Bitplane DMA event table computation |
| SequencerDas.cpp | 106 | Disk/Audio/Sprite DMA event table |
| SequencerRegs.cpp | 191 | DDFSTRT, DDFSTOP, DIWSTRT, DIWSTOP handling |
| SequencerInfo.cpp | 120 | Debug inspection |
| SequencerTypes.h | 85 | Signal flags, action flags |

### Design philosophy

The Sequencer is the most architecturally distinctive part of vAmiga. The header contains a detailed 113-line comment explaining the approach.

vAmiga maintains two event tables — `bplEvent[]` and `dasEvent[]` — that define what happens at each of the ~227 DMA cycles in a rasterline. Each table has a companion **jump table** (`nextBplEvent[]`, `nextDasEvent[]`) pointing to the next non-empty slot, so the scheduler can skip directly to the next event rather than polling each cycle.

The DAS table is built from a static lookup table indexed by DMACON bits 0-5:
```cpp
static EventID dasDMA[64][HPOS_CNT];
```

The BPL table is computed dynamically because bitplane DMA depends on the current state of several interacting signals (DDFSTRT, DDFSTOP, DIWSTRT, DIWSTOP, BPLCON0 resolution, DMACON). The Sequencer records signal changes in a `sigRecorder` buffer and replays them to construct the event table — essentially a software reimplementation of the hardware flipflop logic described in the HRM.

### Action flags

Three flags control when the event tables need rebuilding:
```cpp
UPDATE_SIG_RECORDER = 0b001;  // Rebuild signal recorder for next line
UPDATE_BPL_TABLE    = 0b010;  // Rebuild bitplane event table
UPDATE_DAS_TABLE    = 0b100;  // Rebuild DAS event table
```

These are evaluated lazily in the hsync handler — the tables are only recomputed when something has actually changed (DMACON write, DDFSTRT/STOP change, resolution change, etc.).

### Contrast with WinUAE

WinUAE handles DMA allocation through a more procedural approach in `custom.cpp`, iterating through hardcoded cycle positions. vAmiga's table-driven approach is more declarative and arguably closer to how the hardware works (the Agnus chip contains actual ROM tables for DMA slot allocation).

---

## 4. Copper

**Files**: `Agnus/Copper/` — 7 files, ~1,540 lines total.

| File | Lines | Purpose |
|------|-------|---------|
| Copper.h | 351 | Class definition: state machine states, registers |
| Copper.cpp | 444 | State machine execution, WAIT/MOVE/SKIP logic |
| CopperEvents.cpp | 311 | Event handler: services SLOT_COP events |
| CopperRegs.cpp | 150 | COPCON, COP1LC/COP2LC, COPJMP register handlers |
| CopperDebugger.cpp | 275 | Copper list disassembler and inspector |
| CopperDebugger.h | 148 | Debugger class |
| CopperInfo.cpp | 74 | State inspection |
| CopperTypes.h | 37 | Copper state enum |

### State machine

The Copper runs as a state machine within Agnus's event scheduler, using the `SLOT_COP` event slot. States include waiting for a beam position match, fetching the next instruction word, and executing MOVE/WAIT/SKIP commands.

Key implementation details:
- The Copper fetches one word per DMA cycle (it uses two cycles per instruction: one for the first word, one for the second).
- Beam position comparisons use masked horizontal and vertical values as specified in the WAIT instruction.
- The "Copper danger" bit (COPCON bit 1) controls whether the Copper can write to custom chip registers below $40.

### Copper registers

| Register | Variable | Purpose |
|----------|----------|---------|
| COP1LC | `cop1lc` | Copper list 1 address (set by COPJMP1) |
| COP2LC | `cop2lc` | Copper list 2 address (set by COPJMP2) |
| COPCON | `copcon` | Control register (danger bit) |
| COPINS | `cop1ins`, `cop2ins` | Current instruction words |

### Copper event handling

The Copper uses the `SLOT_COP` event slot. When a Copper event fires:

1. If the Copper has a pending WAIT, it checks whether the beam has reached the target position.
2. If the beam has reached the target, the Copper fetches the next instruction word via `doCopperDmaRead()`.
3. For MOVE instructions, the Copper writes to the appropriate custom chip register via the Agnus register change mechanism.
4. For WAIT instructions, the Copper schedules the next event at the target beam position or the next possible comparison position.
5. For SKIP instructions, the Copper tests the beam condition and either skips or executes the next instruction.

### Copper debugger

The `CopperDebugger` (275 lines) can disassemble Copper lists from memory, showing MOVE/WAIT/SKIP instructions in human-readable form. It tracks the current Copper list execution pointer and provides a history of recently executed instructions.

---

## 5. Blitter

**Files**: `Agnus/Blitter/` — 7 files, ~3,200 lines total.

| File | Lines | Purpose |
|------|-------|---------|
| Blitter.h | 586 | State machine, minterm handling, channel state |
| Blitter.cpp | 622 | Main logic, initialization, area/line mode setup |
| SlowBlitter.cpp | 1,507 | **Cycle-accurate** blitter emulation |
| FastBlitter.cpp | 305 | Fast (non-cycle-accurate) blitter for warp mode |
| BlitterRegs.cpp | 489 | Register handlers: BLTxDAT, BLTxPT, BLTCON0/1, BLTSIZE |
| BlitterEvents.cpp | 89 | Event handler for SLOT_BLT |
| BlitterInfo.cpp | 134 | Inspector |
| BlitterTypes.h | 102 | Enums, fill mode, line mode constants |

### Three accuracy levels

The Blitter supports three accuracy levels (documented in `Blitter.h`):

| Level | Engine | Behaviour |
|-------|--------|-----------|
| 0 | FastBlitter | Moves data in a single chunk. Terminates immediately. |
| 1 | FastBlitter | Moves data in a single chunk. Occupies bus cycles correctly. |
| 2 | SlowBlitter | Moves data word-by-word. Full cycle-accurate bus usage. |

Level 0 and 1 invoke `FastBlitter.cpp`; level 2 invokes `SlowBlitter.cpp`. The level is controlled by `Opt::BLITTER_ACCURACY`.

### Micro-programmed SlowBlitter

The SlowBlitter (1,507 lines) is the largest single file in the Agnus subsystem. It uses a **micro-program** approach: each blit cycle executes a micro-instruction from a pre-built program. The micro-instructions are:

```
NOTHING   — No action
BUSIDLE   — Wait for bus to be free (don't allocate)
BUS       — Wait for bus and allocate it
WRITE_D   — Write back register D hold
FETCH_A   — Load register A new
FETCH_B   — Load register B new
FETCH_C   — Load register C hold
HOLD_A    — Load register A hold
HOLD_B    — Load register B hold
HOLD_D    — Load register D hold
FILL      — Run the fill circuitry
BLTDONE   — Mark last instruction, terminate
REPEAT    — Conditional jump to instruction 0
```

Micro-programs are stored in:
```cpp
void (Blitter::*copyBlitInstr[16][2][2][6])(void);  // [ABCD channel combo][level][fill][step]
void (Blitter::*lineBlitInstr[4][2][8])(void);       // [variant][level][step]
```

The programs are derived from Table 6.2 of the HRM, with corrections noted in the source ("The published table doesn't seem to be 100% accurate").

### Pipeline registers

The Blitter maintains a full set of pipeline registers:

```cpp
u16 anew, bnew;     // Newly fetched A and B data
u16 aold, bold;     // Previous A and B data
u16 ahold, bhold;   // A and B after shifting
u16 chold, dhold;   // C source and D result
u32 ashift, bshift; // 32-bit shift registers for barrel shift
```

The barrel shift operation combines `anew`/`aold` (or `bnew`/`bold`) into the 32-bit shift register, then extracts 16 bits at the offset specified in BLTCON0/BLTCON1.

### Fill pattern tables

Fill mode uses pre-computed lookup tables:
```cpp
u8 fillPattern[2][2][256];  // [inclusive/exclusive][carry_in][data_byte]
u8 nextCarryIn[2][256];     // [carry_in][data_byte]
```

These tables enable O(1) fill computation per byte rather than bit-by-bit processing.

### Minterm logic

The 256 possible minterms are evaluated through the function combination logic, mapping the three source channels (A, B, C) and an 8-bit minterm selection byte from BLTCON0 into the output for channel D.

---

## 6. Denise — graphics

**Files**: `Denise/` — 12 files, ~4,500 lines total.

| File | Lines | Purpose |
|------|-------|---------|
| Denise.h | 778 | Registers, sprites, shift registers, collision |
| Denise.cpp | 1,360 | **Main rendering**: pixel generation, sprite processing, playfield priority |
| DeniseRegs.cpp | 518 | BPLCON0–3, SPRxPOS/CTL/DATA, CLXCON, COLOR registers |
| PixelEngine.cpp | 495 | Colour space management, HAM mode, palette lookup |
| PixelEngine.h | 270 | Texture ring buffer, colour registers |
| Colors.cpp/h | 171/175 | Amiga 12-bit colour to RGBA conversion |
| DeniseDebugger.cpp/h | 202/144 | Sprite tracker, border inspector |
| DeniseInfo.cpp | 106 | State inspection |
| Texture.cpp/h | 52/64 | Frame buffer texture management |
| DeniseTypes.h | 143 | Resolution enum (LORES/HIRES/SHRES), viewport config |
| FrameBufferTypes.h | 41 | Texture dimensions |

### Pixel generation pipeline

`Denise.cpp` is the heart of the video output (1,360 lines). Key stages:

1. **Bitplane shift registers** (`shiftReg[6]`): Loaded from BPLDAT registers when data arrives from Agnus DMA. The shift registers produce one pixel per lores clock (two per hires). The `armedOdd` / `armedEven` flags track whether the shift registers have been loaded — they gate pixel output.

2. **Playfield composition**: Dual-playfield and single-playfield modes. BPLCON2 controls playfield priority relative to sprites.

3. **Sprite processing**: 8 sprite channels, each with position (`sprpos[8]`), control (`sprctl[8]`), and data registers (`sprdata[8]`, `sprdatb[8]`). Sprites can be paired for "attached" mode (15-colour sprites). The armed/disarmed state tracking (`armed`, `wasArmed`) models the hardware behaviour of sprite visibility within a scanline.

4. **Collision detection**: CLXDAT register updated in real-time as pixels are composed. CLXCON masks control which bitplanes/sprites participate in collision detection.

5. **Register change recorder**: A `RegChangeRecorder<128>` records all mid-line register changes and replays them at the correct pixel position. This handles the common Amiga demo trick of changing colours mid-scanline.

### Multi-buffer pixel pipeline

Denise uses five internal buffers, each HPIXELS wide, to process a rasterline. This is one of the most distinctive aspects of vAmiga's design:

| Buffer | Type | Purpose |
|--------|------|---------|
| `dBuffer` | `u8[]` | Raw bitplane data (6-bit values from shift registers) |
| `bBuffer` | `u8[]` | Border mask (0xFF = no border, else colour register index) |
| `iBuffer` | `u8[]` | Colour index (after single/dual playfield resolution) |
| `mBuffer` | `u8[]` | Multiplexed colour index (after sprite compositing) |
| `zBuffer` | `u16[]` | Depth buffer for priority + collision metadata |

### Z-buffer bit encoding

The z-buffer encodes both display priority and collision metadata in a single u16 per pixel:

```
Bit 15: Z_0   — Priority level 0 (highest playfield priority)
Bit 14: Z_SP0 — Sprite 0 is solid at this pixel
Bit 13: Z_SP1 — Sprite 1 is solid
Bit 12: Z_1   — Priority level 1
Bit 11: Z_SP2 — Sprite 2 is solid
Bit 10: Z_SP3 — Sprite 3 is solid
Bit  9: Z_2   — Priority level 2
Bit  8: Z_SP4 — Sprite 4 is solid
Bit  7: Z_SP5 — Sprite 5 is solid
Bit  6: Z_3   — Priority level 3
Bit  5: Z_SP6 — Sprite 6 is solid
Bit  4: Z_SP7 — Sprite 7 is solid
Bit  3: Z_4   — Priority level 4 (lowest playfield priority)
Bits 2-0: Dual-playfield metadata (Z_DPF, Z_DPF1, Z_DPF2, Z_DPF12, Z_DPF21)
```

This layout places sprite bits **between** the playfield priority levels, so a simple comparison `(z & Z_SPx) > (z & ~Z_SPx)` determines whether a sprite pixel should be drawn in front of or behind the playfield at that priority level. The interleaved design mirrors how the real hardware's priority comparator works.

### Sprite clipping

Sprite visibility is controlled by a clipping window (`spriteClipBegin`, `spriteClipEnd`) that is set in the hsync handler. The header comments note:

> "Enabling sprites is always possible, even at high DMA cycle numbers. Disabling sprites only has an effect until the DDFSTRT position has been reached. If sprite drawing was enabled at that position, it can't be disabled in the same rasterline any more."

This models a real hardware constraint that some games and demos exploit.

### PixelEngine

The PixelEngine converts Amiga 12-bit colours (4 bits per R/G/B channel, 4096 possible) to 32-bit RGBA via a precomputed lookup table (`colorSpace[4096]`). It manages:

- Normal palette (32 colours)
- Extra-half-brite palette (32 half-brightness copies)
- HAM mode (Hold-And-Modify)
- SHRES mode (Super High Resolution, ECS Denise only)

Textures are managed in a ring buffer (`NUM_TEXTURES = 8`) for the "run-behind" feature, allowing the GUI to display older frames for smooth animation.

### Contrast with WinUAE

WinUAE's pixel generation is spread across `drawing.cpp`, `custom.cpp`, and several other files. vAmiga's containment of all pixel-level work within `Denise.cpp` makes it significantly easier to follow.

---

## 7. Paula — audio, interrupts, disk control

**Files**: `Paula/` — 19 files across main directory + 3 subdirectories, ~4,800 lines total.

### Main Paula files

| File | Lines | Purpose |
|------|-------|---------|
| Paula.h | 295 | INTREQ, INTENA, ADKCON, potentiometer counters |
| Paula.cpp | 175 | Construction, state management |
| PaulaRegs.cpp | 187 | Register handlers: INTREQ/INTENA/ADKCON/POTGO |
| PaulaEvents.cpp | 129 | IRQ event handler, IPL pin delay emulation |
| PaulaTypes.h | 103 | Interrupt bit definitions |

### Interrupt system

Paula manages all Amiga interrupts through two registers:
- `intreq` (INTREQ, $DFF01C): Interrupt request flags
- `intena` (INTENA, $DFF09A): Interrupt enable flags

The IPL (Interrupt Priority Level) pins to the CPU are emulated with a **delay pipeline** (`iplPipe`, a u64 shift register) to model the propagation delay from Paula asserting an interrupt to the CPU seeing it on its IPL pins. This is critical for cycle-accurate behaviour — some software depends on the exact number of cycles between an interrupt trigger and the CPU's response.

Each INTREQ bit has an individual `setIntreq[16]` cycle timestamp for scheduling delayed interrupt assertions.

### Audio subsystem

**Files**: `Paula/Audio/` — 10 files, ~1,600 lines total.

| File | Lines | Purpose |
|------|-------|---------|
| StateMachine.h | 311 | Audio channel state machine template |
| StateMachine.cpp | 444 | State transitions (000→001→010→011→010→...) |
| StateMachineEvents.cpp | 44 | Event handler for SLOT_CH0–CH3 |
| StateMachineRegs.cpp | 115 | AUDxLEN, AUDxPER, AUDxVOL, AUDxDAT |
| AudioFilter.h/cpp | 240/312 | Butterworth low-pass filter emulation |
| AudioStream.h/cpp | 98/237 | Ring buffer for audio samples |
| Sampler.h/cpp | 38/88 | Sample recording interface |
| AudioFilterTypes.h | 78 | Filter config (A500/A1200/none) |
| StateMachineTypes.h | 34 | State enum |
| SamplerTypes.h | 54 | Sample format |

The audio state machine is a template class `StateMachine<nr>` parameterised by channel number (0–3). Each channel runs through the states defined in the HRM's audio state diagram:

- State 000: Idle
- State 001: Length counter loaded
- State 010: Active output (period counter running)
- State 011: About to transition

The implementation includes the sample buffer locking mechanism described in the header comments — when AUDxPER is set too low (e.g., 1), the `enablePenlo`/`enablePenhi` flags prevent flooding the output buffer with identical samples.

### Audio state transitions

The state machine follows the HRM audio state diagram precisely. The transition table:

| From | To | Condition | Action |
|------|----|-----------|--------|
| 000 | 001 | AUDxON (DMA enabled) or AUDxDAT written | Load length counter |
| 001 | 010 | Period expires | Start audio output |
| 010 | 011 | Period expires (penlo) | Output low byte, request interrupt |
| 011 | 010 | Period expires (penhi) | Output high byte |
| 010 | 000 | AUDxON cleared and length expired | Stop |

The `intreq2` flag controls whether the 011-to-010 transition triggers a Paula interrupt. The `audDR` flag signals a DMA request to Agnus for the next data word.

### Audio modulation

When ADKCON modulation bits are set, audio channels can modulate each other:
- Channel 3 modulates channel 2's period
- Channel 1 modulates channel 0's period
- Channel 3 modulates channel 2's volume
- Channel 1 modulates channel 0's volume

This is handled within the StateMachine by reading the modulating channel's output and applying it to the modulated channel's period or volume register.

### Audio filter

The audio filter (`AudioFilter.cpp`, 312 lines) implements a configurable Butterworth low-pass filter that models the A500's 3.4 kHz RC filter or the A1200's more transparent filter. Filter types:

| Type | Cutoff | Used by |
|------|--------|---------|
| A500 | ~3.4 kHz | A500, A2000 (LED filter) |
| A1200 | ~28 kHz | A1200, A4000 |
| None | — | Bypass (raw output) |

The A500 filter has a dramatic effect on audio quality — it removes most high-frequency content, giving the A500 its characteristic warm sound. The `AudioFilterTypes.h` defines the filter coefficients for each model.

### Disk controller

**Files**: `Paula/DiskController/` — 5 files, ~1,170 lines total.

| File | Lines | Purpose |
|------|-------|---------|
| DiskController.h | 391 | FIFO buffer, sync detection, MFM decoding state |
| DiskController.cpp | 587 | Read/write operations, FIFO management |
| DiskControllerRegs.cpp | 209 | DSKLEN, DSKBYTR, DSKSYNC, ADKCON (disk bits) |
| DiskControllerEvents.cpp | 74 | Event handler for SLOT_DSK |
| DiskControllerTypes.h | 109 | Enums, config |

The disk controller manages the Paula-side of floppy disk DMA. It handles:
- MFM data encoding/decoding
- Sync word detection (DSKSYNC register)
- DSKLEN double-write protection (write twice to arm DMA)
- DMA read (disk to memory) and write (memory to disk) operations
- FIFO buffering between the drive bitstream and the DMA engine

### Internal state

The DiskController maintains detailed internal state:

| Variable | Type | Purpose |
|----------|------|---------|
| `selected` | `isize` | Currently selected drive (-1 = none) |
| `state` | `DriveDmaState` | Off, read, or write |
| `incoming` | `u16` | Latest incoming byte (appears in DSKBYTR) |
| `dataReg` | `u16` | 16-bit data accumulator |
| `dataRegCount` | `u8` | Bits accumulated in data register |
| `fifo` | `u64` | 8-byte FIFO buffer between drive and DMA |
| `fifoCount` | `u8` | Bytes currently in FIFO |
| `dsklen` | `u16` | DMA block length register |
| `dsksync` | `u16` | Sync word to match |
| `syncCycle` | `Cycle` | Timestamp of last DSKSYNC match |
| `syncCounter` | `isize` | Rotation watchdog for auto-sync feature |
| `prb` | `u8` | Copy of CIA-B PRB (drive select, motor) |

### Configuration options

| Option | Purpose |
|--------|---------|
| `DC_SPEED` | Drive speed multiplier (1x = original, higher = turbo) |
| `DC_AUTO_DSKSYNC` | Force sync interrupt even without SYNC mark (recovery) |
| `DC_LOCK_DSKSYNC` | Prevent software from changing DSKSYNC (anti-protection) |

### Debug checksums

For debugging, the controller computes FNV-32 checksums (`check1`, `check2`, `checkcnt`) for each DMA operation, enabling comparison against known-good disk transfers.

### UART

**Files**: `Paula/UART/` — 4 files, ~450 lines total.

| File | Lines | Purpose |
|------|-------|---------|
| UART.h | 222 | Shift registers, baud rate, parity config |
| UART.cpp | 270 | Serial data transmission/reception |
| UARTEvents.cpp | 130 | Transmit/receive event handlers |
| UARTTypes.h | 27 | Config types |

The UART handles serial communication through Paula's SERDATR/SERDAT/SERPER registers. Each transmitted/received bit is individually event-scheduled for cycle accuracy.

---

## 8. CIA — Complex Interface Adapter

**Files**: `CIA/` — 7 files, ~3,100 lines total.

| File | Lines | Purpose |
|------|-------|---------|
| CIA.h | 729 | **Extensive** action flag system (46 flags), timer state |
| CIA.cpp | 1,121 | `executeCIA()`: the tick-by-tick state machine |
| CIARegs.cpp | 551 | All 16 CIA registers (PRA/B, DDRA/B, timers, TOD, ICR, CRA/B) |
| CIAEvents.cpp | 58 | SLOT_CIA wakeup handler |
| TOD.h | 242 | Time-of-Day counter (24-bit BCD, 1/10th second resolution) |
| TOD.cpp | 226 | TOD counting logic, alarm comparison |
| CIATypes.h | 168 | Revision enum, config |
| TODTypes.h | 27 | TOD types |

### Action flag pipeline

The CIA implementation is notable for its **delay pipeline** approach. Instead of modelling each pipeline stage explicitly, the CIA uses a 64-bit `delay` register where each bit represents a pending action, and a 64-bit `feed` register for actions that repeat:

```cpp
static constexpr u64 CIACountA0 = (1ULL << 0);  // Decrement timer A
static constexpr u64 CIACountA1 = (1ULL << 1);  // ...shifted by one cycle
static constexpr u64 CIACountA2 = (1ULL << 2);  // ...two cycles later
static constexpr u64 CIACountA3 = (1ULL << 3);  // ...three cycles later
// ... up to 46 different action flags (bit 45 is the last)
```

Each `executeCIA()` call:
1. Shifts `delay` left by one position (`delay = (delay << 1) | feed`)
2. Checks which action flags have reached their trigger position (bit 0)
3. Executes the corresponding actions

This elegantly models the multi-cycle pipeline delays inherent in the real 8520 hardware without requiring explicit state tracking for each pipeline stage.

### Complete action flag catalogue

The 46 action flags cover every delayed operation in the 8520:

| Flag group | Bits | Stages | Purpose |
|------------|------|--------|---------|
| CIACountA | 0-3 | 4 | Decrement timer A |
| CIACountB | 4-7 | 4 | Decrement timer B |
| CIALoadA | 8-10 | 3 | Load timer A from latch |
| CIALoadB | 11-13 | 3 | Load timer B from latch |
| CIAPB6Low | 14-15 | 2 | Drive PB6 low (timer A output) |
| CIAPB7Low | 16-17 | 2 | Drive PB7 low (timer B output) |
| CIASetInt | 18-19 | 2 | Assert interrupt line |
| CIAClearInt | 20 | 1 | Release interrupt line |
| CIAOneShotA | 21 | 1 | Timer A one-shot stop |
| CIAOneShotB | 22 | 1 | Timer B one-shot stop |
| CIAReadIcr | 23-24 | 2 | ICR read delay (acknowledge window) |
| CIAClearIcr | 25-27 | 3 | Clear ICR bit 8 |
| CIAAckIcr | 28-29 | 2 | Clear ICR bits 0-7 |
| CIASetIcr | 30-31 | 2 | Set ICR bit 8 |
| CIATODInt | 32 | 1 | TOD alarm interrupt |
| CIASerInt | 33-35 | 3 | Serial register interrupt |
| CIASdrToSsr | 36-37 | 2 | Move SDR to shift register |
| CIASsrToSdr | 38-41 | 4 | Move shift register to SDR |
| CIASerClk | 42-45 | 4 | Serial clock signal |

The `CIADelayMask` constant masks out all bit-0 positions to enable the bulk shift operation.

### CIA sleep optimisation

The CIA tracks a `tiredness` counter that increments when the CIA's state doesn't change during execution. When tiredness exceeds a threshold, the CIA enters sleep mode:

```cpp
bool sleeping;       // Currently idle?
Cycle sleepCycle;    // When sleep began
Cycle wakeUpCycle;   // When to next check
u8 tiredness;        // Idle counter
```

This optimisation is significant because the two CIAs are checked on every E-clock cycle (10 master cycles). If both are sleeping, the scheduler can skip to the next timer expiry rather than ticking them continuously.

### Register state

The CIA maintains the full 8520 register set:

| Register | Variable | Width | Purpose |
|----------|----------|-------|---------|
| PRA | `pra` | 8 | Port A data register |
| PRB | `prb` | 8 | Port B data register |
| DDRA | `ddra` | 8 | Port A data direction |
| DDRB | `ddrb` | 8 | Port B data direction |
| Timer A | `counterA` / `latchA` | 16 | Timer A counter and reload latch |
| Timer B | `counterB` / `latchB` | 16 | Timer B counter and reload latch |
| CRA | `cra` | 8 | Control register A |
| CRB | `crb` | 8 | Control register B |
| ICR | `icr` | 8 | Interrupt control register |
| IMR | `imr` | 8 | Interrupt mask register |
| SDR | `sdr` | 8 | Serial data register |
| SSR | `ssr` | 8 | Serial shift register (internal) |

**Reset state note**: On reset, both timers initialise to $FFFF (not 0), and the CIA source comments note: "UAE initializes CRB with 4 (which I think is wrong)" — indicating a known compatibility difference.

### TOD (Time-of-Day)

The TOD counter runs from the VSYNC signal (CIA-A) or HSYNC signal (CIA-B). It counts in BCD: 1/10 seconds, seconds, minutes, hours. The alarm feature triggers an interrupt when the TOD value matches the alarm register.

vAmiga includes a `CIA_TODBUG` option to emulate a known hardware bug in some CIA revisions where the TOD counter can skip or duplicate counts under certain conditions.

### CIA idle sleep optimisation

When `CIA_IDLE_SLEEP` is enabled, the CIA tracks whether it has any pending work (timers running, serial shift active). If both CIAs are idle, the event scheduler can skip CIA wakeup events, reducing per-cycle overhead during periods of CIA inactivity.

---

## 9. CPU — Moira 68000 core

**Files**: `CPU/` — 2 main files + `Moira/` subdirectory with 21 files, ~18,700 lines total.

### vAmiga CPU wrapper

| File | Lines | Purpose |
|------|-------|---------|
| CPU.h | 324 | Configuration, breakpoints/watchpoints, overclocking |
| CPU.cpp | 878 | Bus access callbacks, sync with Agnus, reset |
| CPUTypes.h | 253 | Config, revision enum |

### Moira core

| File | Lines | Purpose |
|------|-------|---------|
| Moira.h | 715 | Core class: registers, prefetch queue, execution table |
| Moira.cpp | 874 | Initialisation, model switching, power-up state |
| MoiraExec_cpp.h | 5,758 | **Instruction execution**: every 68000 opcode |
| MoiraInit_cpp.h | 2,041 | Jump table construction (opcode -> handler mapping) |
| MoiraDasm_cpp.h | 2,015 | Disassembler |
| MoiraALU_cpp.h | 1,407 | ALU operations (ADD, SUB, MUL, DIV, shifts, etc.) |
| StrWriter_cpp.h | 1,599 | String formatting for disassembler |
| MoiraDataflow_cpp.h | 736 | Addressing mode data flow (read/write EA) |
| MoiraDasmFPU_cpp.h | 726 | FPU instruction disassembly |
| MoiraExceptions_cpp.h | 569 | Exception processing (interrupts, traps, bus error) |
| MoiraDebugger.cpp/h | 386/298 | Breakpoint/watchpoint management |
| MoiraTypes.h | 476 | Register struct, addressing modes, model enum |
| MoiraConfig.h | 94 | Compile-time configuration |
| MoiraInit.h | 218 | Lookup table types |
| MoiraMacros.h | 119 | Common macros |
| MoiraExecMMU_cpp.h | 208 | MMU instruction execution (68030/040) |
| MoiraExecFPU_cpp.h | 190 | FPU instruction execution |
| MoiraDasmMMU_cpp.h | 256 | MMU instruction disassembly |
| MoiraDataflow.h | 141 | EA data flow declarations |
| MoiraALU.h | 77 | ALU declarations |
| MoiraExceptions.h | 40 | Exception declarations |
| MoiraDasm.h | 19 | Disassembler declarations |

### Architecture

Moira is a standalone 68000/68010/68020/68030 emulator core (published separately under MIT licence). It uses **computed goto / jump tables** for instruction dispatch:

```cpp
typedef void (Moira::*ExecPtr)(u16);
ExecPtr *exec;   // Main execution table
ExecPtr *loop;   // 68010 loop mode table
```

The `MoiraInit_cpp.h` file builds these tables at startup, mapping each of the 65,536 possible opcodes to its handler function.

### Bus access model

Moira calls back into the host (vAmiga's `CPU.cpp`) for all bus accesses:
- `read8/16(addr)` / `write8/16(addr, val)` — data bus access
- `readFC()` — function code read
- `sync(cycles)` — advance the master clock

These callbacks are where vAmiga integrates Moira with the rest of the emulator: `CPU::read16()` calls into `Memory::peek16()`, which dispatches based on the memory source table, possibly triggering Agnus to run `executeUntil()` to catch up before the access completes.

### Overclocking support

vAmiga supports CPU overclocking through the `debt` and `slowCycles` fields. When overclocked, the CPU executes multiple instruction cycles per Agnus DMA cycle, but chip RAM and custom chip accesses still run at the original speed.

### Moira file organisation

The Moira source uses an unusual pattern: most "implementation" files are `.h` files rather than `.cpp` files (e.g., `MoiraExec_cpp.h`). These are included from `Moira.cpp` to enable template instantiation and avoid multiple-translation-unit issues. This is not a mistake — it is a deliberate design choice to keep the implementation in headers while retaining the logical separation of concerns.

### Instruction execution flow

A typical instruction execution in Moira follows this flow:

1. The 16-bit opcode is fetched from the prefetch queue (`queue.ird`).
2. The opcode indexes into the `exec` jump table to find the handler.
3. The handler (in `MoiraExec_cpp.h`) decodes the addressing mode.
4. Effective address computation calls `readM<mode>(...)` from `MoiraDataflow_cpp.h`.
5. The ALU operation executes via functions in `MoiraALU_cpp.h`.
6. The result is written back via `writeM<mode>(...)`.
7. The prefetch queue is refilled.
8. The cycle count is added to `clock`.

Each step calls back into the host (vAmiga) for bus accesses, ensuring that the chipset clock stays synchronised.

### Prefetch queue

The 68000's 2-word prefetch queue is faithfully modelled:
```cpp
struct PrefetchQueue {
    u16 irc;  // Instruction Register Capture (next word)
    u16 ird;  // Instruction Register Decode (current word)
};
```

The queue is refilled after each instruction execution, and prefetch timing is included in the cycle count. This is critical for accuracy — the 68000 prefetches during execution, and the exact timing of these prefetches determines bus contention patterns with Agnus DMA.

### Exception handling

Exception processing (`MoiraExceptions_cpp.h`, 569 lines) handles:
- Hardware interrupts (IPL levels 1-7)
- Traps (TRAP #N, TRAPV, CHK)
- Bus error and address error
- Privilege violations
- Illegal instructions
- Division by zero

Each exception type has the correct stack frame format and cycle count for the specific CPU model.

### Contrast with WinUAE

WinUAE uses a heavily optimised but less readable CPU core (`newcpu.cpp`, ~50,000 lines). Moira is approximately one-third the size and far more modular, with clear separation between execution, ALU, data flow, and exceptions. However, WinUAE supports a wider range of CPU models (68040, 68060) and has more extensive cycle-count tables for obscure instructions.

---

## 10. Memory — address space management

**Files**: `Memory/` — 5 files, ~4,300 lines total.

| File | Lines | Purpose |
|------|-------|---------|
| Memory.h | 580 | Memory pointers, bank map, read/write macros |
| Memory.cpp | 2,843 | `peek`/`poke` dispatch, ROM loading, bank map construction |
| MemoryDebugger.cpp | 465 | Memory search, hex dump, ASCII viewer |
| MemoryDebugger.h | 145 | Debugger interface |
| MemoryTypes.h | 303 | Memory source enum (MemSrc), bank map config |

### Memory source tables

The core of Memory's design is the **memory source table**:

```cpp
MemSrc cpuMemSrc[256];  // 256 entries, each covering 64KB
```

This 256-entry table maps each 64KB page of the 16 MB 68000 address space to a memory source:

- `MEM_CHIP` — Chip RAM
- `MEM_SLOW` — Slow RAM ($C00000)
- `MEM_FAST` — Fast RAM (Zorro II autoconfig)
- `MEM_ROM` / `MEM_WOM` — Kickstart ROM / Write-Once Memory (A1000)
- `MEM_EXT` — Extended ROM
- `MEM_CIA` — CIA space
- `MEM_RTC` — Real-Time Clock
- `MEM_CUSTOM` — Custom chip registers ($DFF000)
- `MEM_AUTO` — Autoconfig space
- `MEM_NONE` — Unmapped (bus error or open bus)

The `peek16()` / `poke16()` functions dispatch on this table:

```cpp
u16 Memory::peek16(u32 addr) {
    switch (cpuMemSrc[(addr >> 16) & 0xFF]) {
        case MEM_CHIP: return READ_CHIP_16(addr);
        case MEM_CIA:  return peekCIA16(addr);
        // ... etc
    }
}
```

### Six memory types

Memory manages six dynamically allocated regions:

| Type | Pointer | Max Size | Purpose |
|------|---------|----------|---------|
| rom | `u8 *rom` | 512 KB | Kickstart ROM |
| wom | `u8 *wom` | 256 KB | Write-Once Memory (A1000 only) |
| ext | `u8 *ext` | 512 KB | Extended ROM (AROS support) |
| chip | `u8 *chip` | 2 MB | Chip RAM |
| slow | `u8 *slow` | 1.5 MB | Slow / Bogo RAM |
| fast | `u8 *fast` | 8 MB | Fast RAM |

Each uses a bitmask for address mirroring (e.g., `chipMask = config.chipSize - 1`).

### Dual memory source tables

Memory maintains two independent bank maps:

```cpp
MemSrc cpuMemSrc[256];    // What the CPU sees at each 64KB bank
MemSrc agnusMemSrc[256];  // What Agnus sees at each 64KB bank
```

This separation models the real hardware: Agnus can only see chip RAM and custom chip registers (its 24-bit address bus is limited to the chipset's address space), while the CPU can access the full 16 MB (24-bit) or 4 GB (32-bit on 68020+) address space including fast RAM, ROM, and expansion devices.

When Agnus performs a DMA read (e.g., fetching bitplane data), it uses `agnusMemSrc` for dispatch. When the CPU reads the same address, it uses `cpuMemSrc`. This correctly models situations where fast RAM overlaps with chip RAM in the address map but only the CPU can access the fast RAM.

### Open bus / unmapped memory behaviour

The `MEM_NONE` source type returns the last value seen on the data bus (`dataBus`), emulating the Amiga's open-bus behaviour. The `MEM_UNMAPPING_TYPE` option controls exactly what happens — options include returning the last bus value, returning zero, or triggering a bus error.

### Overlay mechanism

The A1000/A500 overlay (mapping ROM at address 0 after reset, then switching to chip RAM when CIA-A OVL goes low) is implemented by rebuilding the `cpuMemSrc` table when the overlay bit changes.

### Write-Once Memory (A1000)

For A1000 emulation, Memory supports WOM (Write-Once Memory). The A1000 ships with a small Boot ROM that loads Kickstart from disk into WOM. Once loaded, the WOM is locked (`womIsLocked = true`) and becomes read-only, effectively becoming a Kickstart ROM for the rest of the session.

### Bank map configuration

The `MEM_BANKMAP` option selects between different address decode schemes (A500, A1000, A2000). This affects how slow RAM, ROM, and CIA addresses are laid out. The `updateMemSrcTables()` method rebuilds both `cpuMemSrc` and `agnusMemSrc` whenever a configuration change occurs (RAM size, overlay bit, ROM loaded, etc.).

---

## 11. RTC — Real-Time Clock

**Files**: `RTC/` — 3 files, ~614 lines total.

| File | Lines | Purpose |
|------|-------|---------|
| RTC.h | 204 | Register array, time offset |
| RTC.cpp | 345 | Register read/write, time conversion |
| RTCTypes.h | 65 | Revision enum: RICOH, OKI, NONE |

### Supported models

vAmiga emulates two RTC chip models:

- **Ricoh RP5C01** — used in A2000 and A3000
- **OKI MSM6242B** — used in A500 and A1200

The model is selected via `Opt::RTC_MODEL`. The difference is in the initial register state after reset:
- Ricoh: `reg[0][0xD] = 0b1000`
- OKI: `reg[0][0xD] = 0b0001`, `reg[0][0xF] = 0b0100`

### Time tracking

The RTC stores time as a **difference** from the host machine's real-time clock:
```cpp
i64 timeDiff;  // Amiga RTC time = host time + timeDiff
```

This means the emulated Amiga's clock tracks the host clock by default (timeDiff = 0) and diverges only if software explicitly sets the RTC to a different time.

The register file is a 4x16 array (`reg[4][16]`) representing the RTC's four banks of 16 nybble-wide registers.

---

## 12. Zorro — expansion bus emulation

**Files**: `Zorro/` — 14 files, ~2,350 lines total.

| File | Lines | Purpose |
|------|-------|---------|
| ZorroManager.h/cpp | 119/128 | Central manager: slot allocation, peek/poke dispatch |
| ZorroBoard.h/cpp | 149/172 | Base class for autoconfig boards |
| RamExpansion.h/cpp | 97/59 | Zorro II fast RAM expansion |
| HdController.h/cpp | 238/703 | Hard drive controller (emulated Zorro II board) |
| DiagBoard.h/cpp | 138/327 | Diagnostic / boot ROM board |
| HdControllerRom.h | 196 | Embedded boot ROM data |
| DiagBoardRom.h | 41 | Diagnostic ROM data |
| ZorroBoardTypes.h | 67 | Board type enum |
| HdControllerTypes.h | 172 | HD controller config |
| DiagBoardTypes.h | 26 | Diag board config |

### Autoconfig protocol

The ZorroManager handles the Amiga autoconfig protocol ($E80000 space). It maintains a list of up to 6 boards:

```cpp
ZorroBoard *slots[slotCount + 1] = {
    &ramExpansion,
    &hd0con, &hd1con, &hd2con, &hd3con,
    &diagBoard,
    nullptr
};
```

When AmigaOS's autoconfig code reads from $E80000, the ZorroManager forwards the read to the first unconfigured board. When the OS writes a base address to the board, the board maps itself into the address space and the next board becomes active.

### Hard drive controller

The `HdController` emulates a Zorro II expansion card with its own boot ROM. It provides:
- Autoconfig identification (manufacturer ID, product ID, serial)
- Boot ROM for AmigaOS to discover the hard drive
- Register-mapped I/O for sector read/write commands

This is vAmiga's mechanism for providing hard drive support without emulating specific real-world SCSI or IDE hardware.

---

## 13. Event-driven architecture summary

The entire emulation is driven by the event loop in `Agnus::execute()`. A single DMA cycle proceeds as follows:

1. Agnus advances `clock` by one DMA cycle's worth of master cycles.
2. If `clock >= nextTrigger`, primary event slots are checked.
3. If SLOT_SEC is triggered, secondary slots are checked.
4. If SLOT_TER is triggered, tertiary slots are checked.
5. Agnus processes the current BPL and DAS events (bitplane/disk/audio/sprite DMA).
6. If the CPU has cycles available, `CPU::execute()` runs until the next Agnus cycle.
7. At end of line: `eolHandler()` → `hsyncHandler()` fires CIA-B TOD, updates sequencer.
8. At end of frame: `eofHandler()` → `vsyncHandler()` fires CIA-A TOD, signals frame complete.

This architecture means **every component** is driven by Agnus's clock. There is no independent "run the CPU for N cycles, then run the chipset for N cycles" loop. This gives vAmiga its cycle-exact accuracy.

### Complete event slot catalogue

**Primary slots** (checked every cycle):

| Slot | Name | Component | Purpose |
|------|------|-----------|---------|
| SLOT_REG | Register | Agnus | Delayed register changes |
| SLOT_RAS | Raster | Agnus | Beam position events |
| SLOT_BPL | Bitplane | Sequencer | Bitplane DMA events |
| SLOT_DAS | DAS | Sequencer | Disk/Audio/Sprite DMA events |
| SLOT_COP | Copper | Copper | Copper instruction execution |
| SLOT_BLT | Blitter | Blitter | Blitter DMA cycles |
| SLOT_SEC | Secondary | Scheduler | Wakeup for secondary slot check |

**Secondary slots** (checked only when SLOT_SEC fires):

| Slot | Name | Component | Purpose |
|------|------|-----------|---------|
| SLOT_CH0–CH3 | Audio 0–3 | StateMachine | Audio channel period events |
| SLOT_DSK | Disk | DiskController | Disk rotation / byte read |
| SLOT_VBL | VBlank | Agnus | Vertical blank interrupt |
| SLOT_IRQ | Interrupt | Paula | Interrupt request processing |
| SLOT_IPL | IPL | Paula | IPL pin update delay |
| SLOT_KBD | Keyboard | CIA | Keyboard handshake |
| SLOT_TXD | Transmit | UART | Serial transmit bit |
| SLOT_RXD | Receive | UART | Serial receive bit |
| SLOT_POT | Potentiometer | Paula | Paddle/pot counter |
| SLOT_TER | Tertiary | Scheduler | Wakeup for tertiary slot check |

**Tertiary slots** (checked only when SLOT_TER fires):

| Slot | Name | Component | Purpose |
|------|------|-----------|---------|
| SLOT_DC0–DC3 | Disk change | Drive | Disk insertion/ejection animation |
| SLOT_MSE1–MSE2 | Mouse | ControlPort | Mouse movement emulation |
| SLOT_KEY | Key | Keyboard | Keyboard event |
| SLOT_SRV | Server | RemoteManager | Remote control events |
| SLOT_SER | Service | Various | Inspector/service events |
| SLOT_ALA | Alarm | Monitor | Watchdog / alarm events |
| SLOT_INS | Inspect | Various | Component inspection |

### Scheduling methods

Events can be scheduled in five different ways:

| Method | Suffix | Trigger cycle is... |
|--------|--------|-------------------|
| Absolute | `Abs` | An absolute master clock value |
| Immediate | `Imm` | The next DMA cycle (cycle 0) |
| Incremental | `Inc` | Relative to the slot's current trigger |
| Relative | `Rel` | Relative to the current master clock |
| Positional | `Pos` | A specific beam position (v, h) |

The positional method converts beam coordinates to master cycles using `DMA_CYCLES(pos.diff(vpos, hpos))`, which calculates the number of DMA cycles between the current beam position and the target.

---

## 14. Cross-reference: documentation topics to source files

| Documentation topic | vAmiga source | WinUAE equivalent |
|--------------------|---------------|-------------------|
| DMA slot allocation (HRM Fig. 6-9) | Sequencer.h, SequencerBpl.cpp, SequencerDas.cpp | custom.cpp (cycle_diagram) |
| Copper state machine | Copper.cpp, CopperEvents.cpp | custom.cpp (do_copper) |
| Blitter cycle-accurate operation | SlowBlitter.cpp | blitter.cpp |
| Bitplane shift registers | Denise.cpp (fillShiftRegister) | drawing.cpp |
| Sprite multiplexing | Denise.cpp (sprite processing), Agnus execute sprite DMA | custom.cpp (do_sprites) |
| Audio state machine | StateMachine.cpp | audio.cpp |
| Disk DMA / MFM | DiskController.cpp | disk.cpp |
| CIA timer pipeline | CIA.cpp (action flags) | cia.cpp |
| 68000 instruction execution | MoiraExec_cpp.h | newcpu.cpp |
| Memory bank mapping | Memory.cpp (cpuMemSrc) | memory.cpp |
| Interrupt priority | PaulaEvents.cpp (iplPipe) | custom.cpp (intlev) |
| Autoconfig / Zorro II | ZorroManager.cpp, ZorroBoard.cpp | autoconf.cpp |
| Colour registers / HAM | PixelEngine.cpp, Colors.cpp | drawing.cpp |
| Register change mid-line | RegChangeRecorder (in multiple .h) | custom.cpp (record_*) |

---

## 15. Notable implementation decisions and comments

### From Sequencer.h (lines 20-113)
The 113-line comment at the top of Sequencer.h is the most detailed piece of inline documentation in the codebase. It explains:
- Why two separate event tables exist (BPL and DAS)
- How the jump tables enable O(1) skip to the next event
- How the signal recorder replays bitplane sequencer logic
- Why event table recomputation is deferred when possible

### From CIA.h (action flags)
The CIA's action flag system (46 flags spanning bits 0-45 of a u64) is documented inline. Each flag name encodes its pipeline position: e.g., `CIACountA0` through `CIACountA3` represent timer A decrement at four successive pipeline stages.

### From StateMachine.h (sample lock)
Lines 92-107 explain the audio sample buffer locking mechanism — a workaround for games (James Pond 2, Ghosts'n Goblins) that set AUDxPER to 1, which would otherwise flood the sample buffer.

### From Agnus.h (pointer drop)
The `AGNUS_PTR_DROPS` option emulates a hardware behaviour where DMA pointer registers can "drop" writes under certain timing conditions — a subtlety that some copy-protection routines depend on.

### From AgnusEvents.cpp (lines 22-86)
A comprehensive 64-line comment explaining the three-tier event scheduling architecture (primary/secondary/tertiary), how the SEC_SLOT and TER_SLOT wakeup mechanism works, and the five different ways to specify trigger cycles (Absolute, Immediate, Incremental, Relative, Positional).

### From Denise.h (lines 228-308)
A detailed explanation of the five-buffer pixel pipeline (dBuffer, bBuffer, iBuffer, mBuffer, zBuffer) and the z-buffer bit encoding format. This is one of the best-documented internal data structures in any Amiga emulator.

### From Blitter.h (lines 19-31)
Clear documentation of the three accuracy levels (0, 1, 2) and which blitter engine each level uses.

### From DiskController.h (lines 54-58)
The DSKSYNC watchdog counter explanation: the counter tracks disk rotations and auto-triggers DSKSYNC interrupts as a recovery mechanism for copy-protected disks.

---

## 16b. Using vAmiga source as a learning tool

vAmiga is arguably the most readable cycle-accurate Amiga emulator available. For someone learning Amiga hardware, the recommended reading order is:

### Phase 1: Understand the architecture
1. Read `Amiga.h` — see the component hierarchy.
2. Read the comment block at the top of `AgnusEvents.cpp` — understand the event scheduler.
3. Read `Agnus.h` lines 78-100 — the event slot array.

### Phase 2: Follow a pixel from DMA to screen
1. `SequencerBpl.cpp` — how bitplane DMA slots are scheduled.
2. `AgnusDma.cpp` — `doBitplaneDmaRead()` — how a DMA read happens.
3. `DeniseRegs.cpp` — `pokeBPLxDAT()` — how the fetched word arrives at Denise.
4. `Denise.cpp` — `fillShiftRegister()` — how data enters the shift registers.
5. `Denise.cpp` — pixel composition and sprite multiplexing.
6. `PixelEngine.cpp` — colour lookup and texture output.

### Phase 3: Follow an interrupt
1. `PaulaRegs.cpp` — `pokeINTREQ()` — how an interrupt request is set.
2. `PaulaEvents.cpp` — `serviceIRQEvent()` — how the request propagates to IPL pins.
3. `CIA.cpp` — how the CIA's interrupt output connects to Paula's INTREQ.
4. `MoiraExceptions_cpp.h` — how the 68000 processes the interrupt.

### Phase 4: Understand DMA contention
1. `Agnus.h` — `busOwner[]` array — per-cycle bus ownership.
2. `AgnusDma.cpp` — `busIsFree()` and `allocateBus()` — bus arbitration.
3. `CPU.cpp` — how the CPU waits for bus access via `executeUntilBusIsFree()`.
4. `SlowBlitter.cpp` — how the blitter contends with the CPU for bus cycles.

### Phase 5: Deep dive into specific topics
- **Copper tricks**: `Copper.cpp` — the WAIT/SKIP state machine.
- **Audio accuracy**: `StateMachine.cpp` — the four-state audio DMA engine.
- **Disk protection**: `DiskController.cpp` — MFM sync detection.
- **CIA keyboard**: `CIARegs.cpp` — the keyboard handshake protocol.
- **Memory overlay**: `Memory.cpp` — the `updateMemSrcTables()` function.

---

---

## 16. Line count summary

### vAmiga component size (all .cpp and .h files)

| Component | Directory | Files | Total lines | Largest file |
|-----------|-----------|-------|-------------|-------------|
| Agnus (core) | `Agnus/` | 11 | 5,506 | AgnusEvents.cpp (828) |
| Sequencer | `Agnus/Sequencer/` | 7 | 1,604 | SequencerBpl.cpp (582) |
| Copper | `Agnus/Copper/` | 7 | 1,542 | Copper.cpp (444) |
| Blitter | `Agnus/Blitter/` | 7 | 3,232 | SlowBlitter.cpp (1,507) |
| DMA Debugger | `Agnus/DmaDebugger/` | 5 | 887 | DmaDebugger.cpp (464) |
| **Agnus total** | | **37** | **12,771** | |
| Denise | `Denise/` | 12 | 4,519 | Denise.cpp (1,360) |
| Paula (core) | `Paula/` | 5 | 889 | Paula.h (295) |
| Audio | `Paula/Audio/` | 10 | 1,594 | StateMachine.cpp (444) |
| Disk Controller | `Paula/DiskController/` | 5 | 1,170 | DiskController.cpp (587) |
| UART | `Paula/UART/` | 4 | 449 | UART.cpp (270) |
| **Paula total** | | **24** | **4,102** | |
| CIA | `CIA/` | 7 | 3,122 | CIA.cpp (1,121) |
| CPU (wrapper) | `CPU/` | 3 | 1,455 | CPU.cpp (878) |
| Moira (68k core) | `CPU/Moira/` | 21 | 17,246 | MoiraExec_cpp.h (5,758) |
| **CPU total** | | **24** | **18,701** | |
| Memory | `Memory/` | 5 | 4,336 | Memory.cpp (2,843) |
| RTC | `RTC/` | 3 | 614 | RTC.cpp (345) |
| Zorro | `Zorro/` | 14 | 2,350 | HdController.cpp (703) |
| Amiga (top-level) | `.` | 3 | ~600 | Amiga.cpp (~350) |
| **Grand total** | | **~130** | **~51,100** | |

### Comparison with WinUAE

| Area | vAmiga (lines) | WinUAE (approx.) | Ratio |
|------|---------------|------------------|-------|
| CPU core | 18,700 | ~50,000 (newcpu.cpp + cpuemu*.cpp) | 0.37x |
| Custom chipset | ~25,000 | ~30,000 (custom.cpp + drawing.cpp + blitter.cpp) | 0.83x |
| Memory | 4,300 | ~8,000 (memory.cpp + expansion.cpp) | 0.54x |
| **Total core** | **~51,000** | **~120,000** | **0.42x** |

vAmiga achieves comparable accuracy (for OCS/ECS) in roughly 40% of the code, primarily through:
1. Cleaner C++ (modern idioms, templates, no C legacy)
2. Narrower scope (no 68040/68060, no AGA, no JIT)
3. The event-driven architecture eliminating CPU/chipset sync complexity

---

## 17. Configuration options reference

Every component exposes its configuration through a unified `Opt` enum system. Key options for emulator authors:

### Chip revisions

| Option | Values | Default | Effect |
|--------|--------|---------|--------|
| `AGNUS_REVISION` | OCS_OLD, OCS, ECS_1MB, ECS_2MB | ECS_2MB | DMA address width, ECS features |
| `DENISE_REVISION` | OCS, ECS | ECS | SHRES, EHB, productivity modes |
| `CIA_REVISION` | MOS_8520, MOS_8520_DIP | MOS_8520 | Minor timing differences |
| `CPU_REVISION` | M68000, M68010, M68020, M68030 | M68000 | Instruction set, address width |
| `RTC_MODEL` | NONE, OKI, RICOH | NONE | RTC chip type |

### Accuracy tuning

| Option | Values | Effect |
|--------|--------|--------|
| `BLITTER_ACCURACY` | 0, 1, 2 | Fast blit (0) vs cycle-accurate (2) |
| `CIA_ECLOCK_SYNCING` | bool | Sync CIA execution to E-clock edges |
| `CIA_IDLE_SLEEP` | bool | Allow CIAs to sleep when idle |
| `CIA_TODBUG` | bool | Emulate TOD counting bug |
| `AGNUS_PTR_DROPS` | bool | Emulate DMA pointer write drops |
| `CPU_OVERCLOCKING` | 0-128 | CPU overclocking factor (0 = off) |

### Display options

| Option | Values | Effect |
|--------|--------|--------|
| `DENISE_VIEWPORT_TRACKING` | bool | Auto-detect visible area |
| `DENISE_FRAME_SKIPPING` | 0-N | Skip N frames in warp mode |
| `DENISE_HIDDEN_BITPLANES` | bitmask | Hide specific bitplanes (debug) |
| `DENISE_HIDDEN_SPRITES` | bitmask | Hide specific sprites (debug) |
| `DENISE_CLX_SPR_SPR` | bool | Enable sprite-sprite collision |
| `DENISE_CLX_SPR_PLF` | bool | Enable sprite-playfield collision |
| `DENISE_CLX_PLF_PLF` | bool | Enable playfield-playfield collision |

### Memory configuration

| Option | Values | Effect |
|--------|--------|--------|
| `MEM_CHIP_RAM` | 256K-2MB | Chip RAM size |
| `MEM_SLOW_RAM` | 0-1.5MB | Slow (bogo) RAM size |
| `MEM_FAST_RAM` | 0-8MB | Fast RAM size |
| `MEM_BANKMAP` | A500, A1000, A2000 | Address decode scheme |
| `MEM_SLOW_RAM_DELAY` | bool | Add wait states for slow RAM |
| `MEM_SLOW_RAM_MIRROR` | bool | Mirror slow RAM in chip RAM space |
| `MEM_UNMAPPING_TYPE` | various | Behaviour for unmapped addresses |
| `MEM_RAM_INIT_PATTERN` | various | RAM contents at power-on |

---

## 18. Serialisation and state save architecture

Every vAmiga component uses a template-based serialisation system. Each class implements:

```cpp
template <class T>
void serialize(T& worker) {
    worker
    << field1
    << field2
    << field3;
}
```

The `worker` type determines the operation:
- `SerChecker` — validates state consistency
- `SerCounter` — counts serialised bytes (for buffer allocation)
- `SerReader` — deserialises from a byte stream
- `SerWriter` — serialises to a byte stream
- `SerResetter` — resets state (soft or hard)

The `CLONE` and `CLONE_ARRAY` macros handle the copy-assignment operator, which is used for the "run-behind" feature (cloning the emulator state for background analysis).

This approach avoids the error-prone pattern of maintaining a separate "save state" function — the serialisation code is the single source of truth for which fields constitute the component's state.

---

# Gaps and source map

## Part 1 (A3000) gaps

| Topic | Status | Notes |
|-------|--------|-------|
| Fat Gary register-level decode equations | Missing | Proprietary; not in schematics |
| Ramsey register map | Missing | Not in schematics; need A3000 Technical Reference Manual |
| SDMAC full register set | Partial | Some registers documented from published sources |
| Super Buster Zorro III protocol timing | Missing | Need Zorro III specification (Dave Haynie) |
| PAL equations (U202, U203, U701, U714) | Missing | Proprietary |
| Amber register control | Missing | Not in schematics |
| A3000 errata / service bulletins | Missing | Not in current corpus |

## Part 2 (vAmiga) gaps

| Topic | Status | Notes |
|-------|--------|-------|
| Floppy drive emulation | Not covered | Lives in separate `Peripherals/` directory, not `Components/` |
| Hard drive emulation (raw) | Not covered | `Peripherals/` directory |
| GUI / host integration | Not covered | Outside `Core/Components/` |
| Chipset revision differences (OCS vs ECS vs AGA) | Mentioned | Config options exist but AGA is not fully supported |
| Cycle-exact timing verification | Not covered | Would need test ROM comparison data |

## Source map

| File | Path | Purpose |
|------|------|---------|
| A3000 schematics OCR | `/Users/stevehill/Desktop/AmigaPDFs/rkm/txt/a3000_schematics.txt` | Source for Part 1 |
| Existing service reference | `/Users/stevehill/Desktop/AmigaPDFs/amiga-service-electrical.md` | A500/A1000/A1200/A4000 reference |
| vAmiga source | `~/Projects/Emu198x-Unclean/vAmiga/Core/Components/` | Source for Part 2 |
| This document | `/Users/stevehill/Desktop/AmigaPDFs/amiga-a3000-and-vamiga-guide.md` | Output |

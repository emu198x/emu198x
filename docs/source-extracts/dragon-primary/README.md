# Dragon Primary Source Extracts

This directory contains layout-preserving text extracted from local PDFs in
`~/Downloads` on 2026-04-30. These are working extracts for Dragon 32 accuracy
audits; quote or cite the original PDF when finalising implementation notes.

## Extracts

| Extract | Source PDF | Purpose |
|---------|------------|---------|
| `mc6809-mc6809e-programming-manual-1981.txt` | `MC6809-MC6809E 8-Bit Microprocessor Programming Manual (Motorola Inc.) 1981.pdf` | Instruction semantics, programmer-visible state, addressing modes |
| `mc6809e-hmos-microprocessor-1984.txt` | `Motorola_MC6809E_HMOS_8_Bit_Microprocessor_1984_Motorola_text.pdf` | MC6809E electrical/timing details, bus signals, cycle timing |
| `mc6821-pia-1985.txt` | `Motorola_MC6821_NMOS_Peripheral_Interface_Adapter_1985_Motorola_text.pdf` | PIA register behavior, control lines, interrupt behavior |
| `mc6847-video-display-generator-1984.txt` | `Motorola_MC6847_MOS_Video_Display_Generator_1984_Motorola_text.pdf` | VDG display modes, pin behavior, timing, colour generation |
| `mc6883-sam-advance-sheet.txt` | `Motorola_MC6883_Synchronous_Address_Multiplexer_Advance_Sheet_19xx_Motorola_text.pdf` | SAM programming model, clocking, memory mapping, VDG/MPU arbitration |
| `sam-programming-guide.txt` | `Synchronous Address Multiplexer (SAM) Programming Guide.pdf` | Practical SAM programming guide; OCR quality is noisy but readable |
| `motorola-microprocessors-data-manual.txt` | `MOTOROLA MICROPROCESSORS DATA MANUAL_text.pdf` | Large Motorola data manual; use for cross-checking 6809/6809E/PIA timing and electrical specs |

## Immediate Accuracy Questions To Resolve

1. Confirm Dragon/SAM master-clock constants, line timing, frame timing, and
   MC6809E E/Q relationship from source documents.
2. Replace any VDG/SAM timing comments that cite emulator behavior with source
   citations or measured-hardware caveats.
3. Validate SAM MPU-rate behavior, especially address-dependent fast mode and
   transitions between fast and slow cycles.
4. Validate MC6847 VRAM fetch timing, CSS/control-line latching, border size,
   and PAL visible/overscan placement.
5. Validate MC6821 interrupt flag semantics and CA2/CB2 behavior against the
   datasheet.


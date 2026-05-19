# Reference Emulators

Source code for studying architecture and timing. Located at `~/Projects/Emu198x-Unclean/`.

## Spectrum references

| Emulator | Language | Notes |
|----------|----------|-------|
| **SpecIde** | C++/SFML | Cycle-accurate ULA/Z80 interleaving on alternating half-cycles. Closest to our architecture. Study for ULA subclass structure and contention gating. |
| **FUSE** | C | Gold standard test suite and contention tables. Event-driven/tstate-counting architecture (not our approach), but timing data is authoritative. |
| **zxsp** | C++/Qt | Broadest model range including Timex, Jupiter Ace. Per-model ULA subclasses useful for variant differences. |
| **z80cpp** | C++ | Clean standalone Z80 core. Used by ESPectrum. |
| **zxian** | C/SDL | Small readable codebase with explicit contention callbacks. |
| **ESPectrum** | C++/ESP32 | 100% cycle accuracy claim on embedded hardware. |
| **specemu** | Binary only | Most accurate by reputation, useful for comparison only. |

Also: EightyOne (ZX81-focused), Zen (C#), ZXSpeculator (C#/Avalonia), Spud (binary only).

## Other system references

| Emulator | System | Location |
|----------|--------|----------|
| **WinUAE** | Amiga | `~/Projects/WinUAE` — Toni Wilen's Amiga emulator |
| **Minimig MiSTer** | Amiga | `~/Projects/Minimig-AGA_MiSTer` — FPGA Amiga core |
| **VICE** | C64 | `~/Projects/vice-3.10` — true drive emulation reference |
| **xroar** | Dragon/CoCo | `~/Projects/Emu198x-References/xroar` |

## Tools

- **m68k disassembler**: `/opt/homebrew/bin/m68k-elf-objdump` (brew `m68k-elf-binutils`)

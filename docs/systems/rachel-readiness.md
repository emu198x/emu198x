# Rachel-readiness across the fleet

Emu198x's view of **[Rachel](https://github.com/rachel-multiverse)** — Steve's
cross-platform turn-based card game. Vintage machines run a Rachel **client**;
multiplayer uses **RUBP** (Rachel Unified Binary Protocol — 64-byte fixed
messages) to a host (iOS/macOS, or `rachel-server` (Go) / `rachel-phoenix`).

> Targets/RAM bars from Rachel's `.github/PLATFORMS.md`. Most vintage clients are
> **not built yet** ("In development"), so verdicts are *capability*, not shipped.

## Two independent axes — do not conflate

**Rachel-readiness is tracked entirely separately from internet-capability.**

1. **Runs Rachel** — can the machine run the Rachel client *at all*, vs AI,
   **fully offline, with no network whatsoever**. Gated only by the run-bar (RAM,
   display, input) + a built client binary. This is the primary axis here, and it
   has **nothing to do with connectivity** (the Atari 5200 client is complete and
   plays locally with zero net hardware).
2. **Netplay** — a *separate* capability: Runs Rachel **and** has a network path.
   The network path itself lives in each system page's **Internet-capable**
   verdict (period modem / native LAN / modern device), which is its own
   standalone axis tracked for its own sake. Netplay is simply where the two
   intersect; neither axis is derived from the other.

So a machine can be: Runs-Rachel + no-internet (offline/AI only), Runs-Rachel +
netplay, internet-capable + no-Rachel-client, or neither.

## Runs Rachel (the primary axis — offline/AI, no network)

| System | Rachel repo | Emu198x core today | Runs Rachel (local/AI) | Host? |
|--------|-------------|--------------------|------------------------|-------|
| C64 | `rachel-c64` | full (primary) | ✅ once client built | ✅ (turn-based) |
| ZX Spectrum | `rachel-zx-spectrum` | full (primary) | ✅ | ✅ |
| Amiga | `rachel-amiga` | WB3.1 (primary) | ✅ | ✅ strong |
| Dragon 32 | `rachel-dragon` | boots (primary-ish) | ✅ | ✅ |
| Atari 800XL | `rachel-atari8` | boots BASIC | ✅ | ✅ |
| VIC-20 | `rachel-vic20` | boots; PRG autoload | ✅ (`rachel.prg` tested) | maybe (5K RAM) |
| BBC Micro | `rachel-bbc-micro` | early boot | ✅ once boot completes | ✅ |
| Acorn Electron | `rachel-electron` | boots + types | ✅ | maybe |
| MSX1 | `rachel-msx` | boots BASIC | ✅ | ✅ |
| Oric Atmos | `rachel-oric` | boots + types | ✅ | maybe |
| NES | `rachel-nintendo-nes` | full (primary) | ✅ | no (console) |
| Game Boy | `rachel-nintendo-gameboy` | full (primary) | ✅ | no |
| Master System | `rachel-sega-mastersystem` | cart title | ✅ | no |
| ColecoVision | `rachel-coleco-colecovision` | BIOS title | ✅ (1K RAM — tight) | no |
| Atari 2600 | Rachel #008 (complete) | Combat renders | ✅ (2K — tight) | no |
| Atari 5200 | Rachel #009 (complete) | Pac-Man menu | ✅ | no |

(Run-bar is low — a card game + 64-byte protocol — so the only real *below-bar*
machines are the truly minimal ones: ZX80 1K, and arguably Jupiter Ace 3K.)

## Netplay (secondary — Runs Rachel + a net path)

Netplay needs both axes. The net path per machine is in its page's
**Internet-capable** line; for emulator testing it's bridged via an **emulated
WiFi modem / RS232→TCP bridge** (as VICE's `make test-net` does), to the host on
**port 19840**. Machines that **Run Rachel but cannot netplay** (no net path) —
play offline/AI only: **Atari 5200, Atari 2600 sans GameLine, and the bare
consoles**. Machines well-placed for netplay once the bridge lands: C64, VIC-20,
Spectrum, Amiga, Dragon, 800XL, BBC, Electron, MSX, Oric.

### The one thing Emu198x needs for netplay testing

An **emulated WiFi modem / RS232→TCP bridge** on the serial/user-port machines.
That turns Emu198x into Rachel's test harness: run a client in the matching core,
bridge its serial port to the RUBP host, drive via MCP. Because RUBP is 64-byte
fixed messages, an MCP tool could itself be a Rachel client or host for automated
cross-platform tests. (This is a *netplay-testing* enabler — it does not affect
the offline "Runs Rachel" axis above.)

## Rachel target, Emu198x core not started

`rachel-atari-st`, `rachel-coco`, `rachel-sega-genesis` (sega/mega-drive). Rachel
also targets Apple II, DOS, SAM Coupé, CPC, C128, Enterprise, QL, TI-99, TRS-80,
Lynx, PC Engine, Game Gear, Mac Classic — outside the current Emu198x fleet.

## Emu198x core exists, not (yet) a Rachel target

Sord M5, Memotech MTX, Tatung Einstein, Spectravideo SVI-328, Commodore PET,
Atari 7800, Sega SG-1000, Mattel Aquarius, ZX80, ZX81, Acorn Atom.

See `project_rachel_cross_platform_netplay.md` (memory) for project context, and
each system's **Internet-capable** line for the network axis (tracked
independently of Rachel).

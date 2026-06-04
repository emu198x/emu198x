# Decision: joystick input-port numbering follows the documented hardware ports

**Status:** Landed 2026-06-04 for the C64 and Amiga. The Spectrum was already
conformant (see [spectrum-joystick-architecture.md](spectrum-joystick-architecture.md)).

**Drift trigger:** if you map a joystick onto input **port 0** "because that's
the default" and silently drop the hardware-labelled number, or you use a
chip-register index (e.g. `JOY1DAT` → port "1") as the user-facing port number,
**stop and re-read this entry first.** The number a learner types must be the
number printed on the machine.

## The principle

`InputEvent::Button { port, … }` port numbers are **the documented control-port
labels for that machine** — the number on the case, in the manuals, and in the
Code198x curriculum that teaches the same hardware. They are *not* the runtime's
internal register index, and *not* an arbitrary 0-based scheme.

Why this matters here specifically: Code198x drives Emu198x for captures (and,
later, as a learner runtime). A unit that teaches "read the joystick in port 2 /
`$DC00`" must be drivable with `port: 2`. An input API that disagrees with the
taught hardware is a cross-project coherence bug, and the worst failure mode —
the one we hit — is to **silently drop** the hardware-true value while a
different magic number works.

One cross-system concession: **input port 0 is the "primary stick" alias**
(mirrors the Spectrum's port-0-is-default), so host gamepad code can target the
main controller uniformly across machines. It resolves to whichever port is each
machine's primary joystick. This *adds* a convenience; it never *replaces* the
hardware numbers.

## Per-machine mapping

| Machine | Primary joystick | `Button` port mapping | Primary source |
|---|---|---|---|
| **C64** | Control Port 2 = CIA1 PA = `$DC00` | `2` → gameport 2; `1` → gameport 1 (`$DC01`); `0` → gameport 2 (alias); else drop | Case silkscreen "CONTROL PORT 1/2"; the port `LDA $DC00` reads |
| **Amiga** | Control Port 2 = `JOY1DAT` | `2` → joystick (`JOY1DAT`); `0` → joystick (alias); `1` = mouse port (`JOY0DAT`) and higher = drop | *Mapping the Amiga* (Thomson & Anderson, 1993) p.460: "JOY0DAT handles port 1 and JOY1DAT handles port 2" |
| **Spectrum** | (none native) | `0` → Kempston (portless standard); `1` → Sinclair IF2 port 1; `2` → Sinclair IF2 port 2; else drop | Grussu, *Spectrumpedia* Vol.1 p.140; see sibling decision |

Notes:
- **C64** has two gameports, both joystick-capable, so `1` and `2` are both real
  joystick destinations and `0` aliases the main one (port 2).
- **Amiga** port 1 is the *mouse* port (`JOY0DAT`, routed via pointer events); a
  joystick on `JOY0DAT` isn't modelled, so `Button` port 1 drops. The mouse and
  joystick can physically swap ports on real hardware, but the runtime only
  models the conventional layout.
- The runtime translation lives in each crate's `input.rs`
  (`machine_port` for the C64, `joystick_machine_port` for the Amiga); the
  machine layer keeps its register-faithful internal numbering.

## What changed, and why it was wrong before

- **C64**: previously `0` → gameport 2, `1` → gameport 1, and **`2` was dropped**
  — a deliberately-tested choice justified as "mirror the Spectrum's
  port-0-is-default." That misapplied the Spectrum: the Spectrum puts the
  *portless* Kempston on 0 and keeps its hardware ports on 1/2, so the faithful
  mirror is to honour 1/2, not to collapse gameport 2 onto 0 and discard the
  silkscreened number.
- **Amiga**: the joystick was reachable only as register-index `port: 1`
  (`JOY1DAT`), the GUI emitted `port: 1`, and the documented `port: 2` was
  dropped. Now `port: 2` is the hardware-faithful joystick and the GUI emits it.

## Why unmapped events drop silently — and why that's a choice, not a gap

Dropping an `InputEvent` whose port or control the machine doesn't model is
**correct, routine behaviour**, not an error. Extra gamepad buttons, multitap
ports a machine lacks, paddle axes, a mouse event on a mouseless machine — all
legitimately arrive and must be ignored. So the drop stays silent and
non-fatal, deliberately:

- **Not a hard failure.** Input is an external-data boundary (the MCP `input`
  tool, authored `--script` files, host gamepad SDKs). A panic would let a
  controller with one extra button crash the emulator or abort a capture.
  Robustness at the boundary wins over fail-fast here.
- **Not a runtime-side log.** The runtime / machine crates are deliberately
  pure — no I/O; every `eprintln!` lives in the binaries. A `log::debug!`
  inside `apply_input_event` would add a dependency *and* breach that boundary.
- **Not a threaded "unhandled" signal.** Surfacing drops through the shared
  `RunResult` would touch ~30 `run_until` impls — disproportionate to a footgun
  whose actual bug (C64/Amiga control port 2) is already fixed.

The proportionate mitigation, and the one in place: the per-machine port
mappers (`machine_port`, `joystick_machine_port`) are the single source of
truth, with tests that enumerate exactly what maps and what drops, plus this
record. The original cost was **not knowing the rule**, not the silence itself
— and the rule is now written down, cited, and tested, one grep away.

If a louder signal is ever wanted, scope it to the **authored paths** (script /
MCP), where a human explicitly wrote an event that hit nothing — and surface it
*there*, at the boundary that has the context and already does diagnostics,
never as a panic in the pure runtime.

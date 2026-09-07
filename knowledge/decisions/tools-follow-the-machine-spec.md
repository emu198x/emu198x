# Tools follow the machine spec

**Status:** Adopted 2026-09-07. `emu198x_shell::mcp_tools::register_tools_for`
is the default `MachineApp::register_mcp_tools`, every binary's tool list
was diffed against the previous release's, and the only differences are
the two this record intends.

## The problem

Every binary chose its own MCP tools by listing registrars in its
`register_mcp_tools`: `register_base_tools`, then the keyboard tier if the
author remembered the machine had a keyboard, then the AY watch tier if
they remembered the chip. Fourteen binaries carried a list that was a
copy of a neighbour's; the Spectrum registered `port_read`, `port_write`,
`query_ay` and `clear_audio_capture` privately although three of them
were shared `ScriptStep` arms the shell refused to execute. Two
kinds of drift followed. A tool a client could see failed at call time
with "capability missing" (the 48K Spectrum advertised the AY watches).
A tool the machine could honour was never offered (the MSX publishes the
AY register file and had no `query_ay`).

The user-facing consequence is the script dialect Code198x sees: an
action such as `port_read` that works on one machine and is refused on
another with the same chip, for no reason the machine itself expresses.

## The decision

**A machine declares what it has; the shell registers the tools for it.**
Three things enforce that:

1. **The shell executes every shared step.** No `ScriptStep` arm is
   binary-owned. An arm that needs a capability asks the machine for the
   target (`debug_target`, `watch_target`, `keyboard_target`,
   `port_io_target`) or a query path (`ay.registers`), and fails with the
   capability-missing error when it is absent. `port_read`, `port_write`
   and `query_ay` moved into the shell under this rule. A step whose
   body needs the whole session, not just the machine, goes through a
   hook on `MachineCore` that takes the session: `load_basic_program`
   and `autoload_tape` call `M::load_basic_program` / `M::autoload_tape`,
   which the Spectrum family and the C64 implement over their own
   tokeniser, memory map and prompt handling; the default refuses with
   the same error the shell always gave. `set_machine` is the one arm
   still intercepted per binary and follows under the same rule.
2. **Registration reads the profile, not the binary.** `register_tools_for`
   registers the base set, then each optional tier when the machine
   profile declares the capability behind it, using the ids in
   `emu198x_shell::capability::ids`:

   | tier | profile capability |
   |---|---|
   | keyboard verbs | `keyboard-input` or `keyboard-matrix` |
   | `watch_memory_*` | `memory-watch` |
   | `watch_ay_*` | `ay-audio` |
   | `port_read` / `port_write` | `port-io` |
   | `load_basic_program` | `basic-program-load` |
   | `autoload_tape` | `tape-autoload` |
   | `query_ay` | the query surface lists `ay.registers` |

   The profile, not the live target, because a machine that starts blank
   grows its targets when firmware loads and the client needs the tools
   before then. A family that swaps variants live registers over its
   whole catalogue (`register_tools_for_profiles`), so the AY tier a 128K
   needs is there while the session is still a 48K. The executor checks
   the live target on every call, so a declared capability that is not
   loaded yet fails with the capability error rather than silently.
3. **A binary registers only machine-bound tools.** An override calls
   `register_tools_for` first and adds tools that need chip or OS
   knowledge the shell cannot have: the Amiga's Exec and copper walks,
   the NES's PPU dumps, the C64's IEC drive selection. A verb any machine
   with the same trait could support goes into the shell behind that
   trait instead.

## What the diff showed

Second pass (loader hooks): every binary's tool list is unchanged
except the C64, which gains `autoload_tape` — it has had the helper
since its `--autoload-tape` flag existed and only its MCP surface was
missing it. Two MCP/script differences the Spectrum and C64 carried
between their own tool and the shared step are gone: `max_boot_frames`
of zero now means the machine's default in both modes (the Spectrum's
MCP tool did that, its script path did not), and `run` defaults to true
in both (the C64's MCP tool defaulted to false, its script step to
true). Code198x's C64 and foundations captures are `--script` runs, so
they already had the shell's default.

First pass (capability-driven registration):

Fleet-wide, the registered tool lists after the change equal the lists
before it, plus `clear_audio_capture` on every machine and `query_ay` on
the MSX. Two profiles were wrong and the registrar found them: the Game
Boy declared a keyboard it does not have, and the 48K Spectrum's profile
declared no port space or memory watch although the runtime implements
both. Those are fixed in the same change; the profile is the spec, so a
wrong profile is a bug in the profile.

## Drift triggers

Stop and re-read this record if you find yourself:

- listing `register_base_tools` / `register_keyboard_tools` /
  `register_ay_watch_tools` in a binary — the profile decides;
- adding a `ScriptStep` arm that returns `SystemSpecificStep`
  unconditionally, or intercepting a shared arm in a binary;
- registering a tool in a binary that reads through `DebugTarget`,
  `WatchTarget`, `KeyboardTarget`, `PortIoTarget` or the query surface
  alone — that is a shell tool waiting to be moved up;
- writing a per-binary `execute_*` for a step that needs the session:
  that is a `MachineCore` hook taking the session, with the profile
  capability that registers its tool;
- fixing "tool X is missing on machine Y" in the binary rather than in
  Y's profile.

Related: [`machine-binaries-share-a-launcher.md`](machine-binaries-share-a-launcher.md)
(the `register_mcp_tools` hook), [`debug-surface-tiers.md`](debug-surface-tiers.md)
(the shared `DebugTarget` tier this extends), and RULES.md rule 30.

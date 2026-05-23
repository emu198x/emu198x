# Decision: Debugger implementation design — MCP transport + disasm API surface

**Date:** 2026-05-23
**Status:** Locked. Implementation-layer details flowing from the
spine in [`debugger-architecture.md`](debugger-architecture.md).
Wave 1 (MCP transport + `pause`/`resume`) and Wave 2 (per-CPU
disasm crates) of the four-wave delivery plan in that record. This
document covers the two design choices that gate those waves; the
spine record stays the higher authority.

## What this is

Two implementation choices that need locking down before debugger
work starts:

1. **MCP transport for multi-client mode** — TCP vs Unix socket vs
   both. The spine decision said "add `--mcp-listen <addr>` mode
   alongside the existing stdio `--mcp`"; this picks the transport.
2. **Disasm crate API surface** — the public API of the four
   per-CPU disassembler crates the spine record committed to
   writing.

Combined into one record because they're tightly related (both
Wave-blockers for the same product), and a single document is
easier to find later than two.

---

# Part 1 — MCP transport: TCP

## The decision

**Listen mode uses TCP, no Unix-socket variant in MVP.**

- Flag: `--mcp-listen <addr>` where `<addr>` is either `port`
  (binds `127.0.0.1:port`) or `host:port` (binds the specified
  interface explicitly)
- Default binding is loopback (`127.0.0.1`) for safety; binding to
  `0.0.0.0` or another interface requires the explicit `addr:port`
  form
- Coexists with the existing `--mcp` (stdio) mode; stdio stays for
  the Claude Code subprocess case
- Multi-client safety: shared mutable emulator session protected
  by a `Mutex`; one thread per accepted connection with a
  reasonable connection cap

## Why TCP-only for MVP

1. **Uniform OS support.** Windows is a first-class target per the
   cargo-dist build matrix. Windows has AF_UNIX support since
   Windows 10 build 17063 but it has edge cases; TCP is uniform
   across macOS / Linux / Windows.
2. **Remote debugging falls out for free.** "Debugger on my
   laptop, emulator on a Pi" is a real near-term scenario.
   `--mcp-listen 0.0.0.0:9001` (plus an SSH tunnel or trusted
   network) makes it work without extra plumbing.
3. **Implementation simpler.** `std::net::TcpListener` + per-thread
   accept loop is ~150–200 LOC in `emu198x-shell/src/mcp.rs` as a
   `serve_tcp_listen(addr)` sibling to `serve_stdio`. Both call
   into the same transport-agnostic `serve(server, context, reader,
   writer)` core.
4. **Default-loopback binds the security boundary loudly.**
   `--mcp-listen 9001` is safe by construction; only the explicit
   `0.0.0.0:9001` form opens to the network.

## Why not Unix socket

- Filesystem-permission access control is nice but unnecessary for
  the loopback-by-default scenario — process-level access is already
  implied by being on the same machine and within the same user
  account.
- Windows partial support means we'd need conditional compilation
  to support both transports cleanly, and the matrix gets fiddly.
- Stale `.sock` cleanup adds an edge case we don't need.
- Nothing about the debugger UX requires it.

## Why both is over-engineering for MVP

A "both, syntax-detected" approach (`--mcp-listen <addr>` accepts
either) sounds flexible but introduces flag-parsing ambiguity (is
`/tmp/foo:9001` a Unix path or a TCP address with a weird host?)
and doubles the test surface. Two separate flags
(`--mcp-listen-tcp` + `--mcp-listen-unix`) is honest but pays for
flexibility we don't need yet. Either can be added later if real
demand surfaces.

## What we are NOT doing in MVP

- **Unix socket transport.** Deferred.
- **TLS / authentication.** The loopback-default and explicit
  remote-binding design covers the common case; TLS is a future
  feature if/when remote debugging becomes a primary scenario.
- **Async (tokio).** The existing MCP server is sync (stdio). Per-
  connection threads with a blocking `TcpListener` is sufficient
  for the debugger's load (a handful of clients at most). Async
  would force a runtime dependency for no benefit.
- **Connection upgrades or websockets.** Plain TCP carrying
  JSON-RPC 2.0 line-delimited frames, same as stdio but over
  socket bytes.

## Sizing

- New transport in `emu198x-shell/src/mcp.rs`: ~150–200 LOC
- `--mcp-listen` flag wiring in each per-system binary's main: ~5
  LOC per binary × 6 = ~30 LOC
- Multi-client safety: `Arc<Mutex<Session>>` wrapping the existing
  context, ~50–100 LOC
- New `pause` + `resume` MCP verbs: ~50 LOC each (verb registration
  + runtime-level pause flag the tick loop checks)
- Tests: ~200 LOC of integration tests (connect, dispatch a query,
  pause, resume, disconnect; multi-client scenario)

Total Wave 1: ~500–600 LOC across `emu198x-shell` + per-binary
mains.

---

# Part 2 — Disasm crate API surface

## The decision

**One shared `disasm-common` crate + four per-CPU disassembler
crates that duplicate opcode tables internally (no shared tables
with the execution crates).**

Crate structure:

```
disasm-common              (shared types only)
  DisassembledInstruction
  Operand
  DisasmError

disasm-zilog-z80           (Z80 + Z80 prefixes)
disasm-mos-6502            (6502 / 6510 / 2A03)
disasm-motorola-6809       (6809 + 6309 variant later)
disasm-motorola-68000      (68000 + future 68010/20/30/40)
```

Function signatures (per per-CPU crate):

```rust
pub fn disassemble(bytes: &[u8]) -> Result<DisassembledInstruction, DisasmError>;
pub fn disassemble_at(memory: &[u8], pc: usize) -> Result<DisassembledInstruction, DisasmError>;
pub fn disassemble_range(memory: &[u8], start_pc: usize, count: usize) -> Vec<Result<DisassembledInstruction, DisasmError>>;
```

Shared types (in `disasm-common`):

```rust
pub struct DisassembledInstruction {
    pub mnemonic: &'static str,
    pub operands: Vec<Operand>,
    pub byte_length: u8,
    pub description: &'static str,
}

pub enum Operand {
    Register(&'static str),
    Immediate(u32),
    Address(u32),
    Indirect(Box<Operand>),
    Displacement(i32),
    Condition(&'static str),
}

pub enum DisasmError {
    InsufficientBytes { needed: usize },
    UndefinedOpcode { bytes: Vec<u8> },
}
```

## Why duplicate opcode tables instead of sharing with execution crates

The execution crates (`zilog-z80`, `mos-6502`, `motorola-6809`,
`motorola-68000`) have opcode dispatch tables internally. Three
options were on the table:

- **(a) Depend on execution crates.** DRY but creates a public-API
  contract on internal table format. Every execution-crate
  refactor becomes a disasm-crate API change.
- **(b) Duplicate tables in disasm crates.** Decoupled. ~200–500
  LOC of static data per CPU. Tables don't change after they're
  written.
- **(c) Extract a third shared opcode-tables crate.** Cleanest
  separation but multiplies crate count by four.

Chosen: **(b) duplicate**. Opcode tables are static data; the
duplication cost is paid once. Coupling the disasm public API to
the execution crate's internal table format is an architectural
mistake we'd regret. The duplication enables independent evolution
of each crate.

## Why structured operands instead of pre-formatted strings

Two output shapes were on the table:

- **String-only:** `text: String = "LD A, (HL)"` — simple, one
  field.
- **Structured:** `mnemonic + Vec<Operand>` — UI can re-format
  operands per mode (hex vs decimal vs symbolic vs learner-tooltip).

Chosen: structured. The learner-mode debugger wants to render
`LD A, (HL)` as `Load A from address held in HL` on hover; the
developer mode wants the compact form. Without structured
operands, only the compact form is possible. The cost is small
(one allocation per disassembled instruction; trivial at debugger
load).

## Why the three call patterns

- **`disassemble(bytes)`** is the primitive. Caller is responsible
  for windowing the right bytes; function disassembles starting at
  byte 0.
- **`disassemble_at(memory, pc)`** is the common-case convenience.
  Takes a full memory image and a PC, returns one instruction.
- **`disassemble_range(memory, start_pc, count)`** is what the
  debugger UI's disassembly panel needs — N instructions starting
  at PC, each potentially failing independently. Returns
  `Vec<Result<...>>` rather than `Result<Vec<...>>` so partial
  failures don't lose the successful disassemblies before them.

All three share the same core implementation; the convenience and
range forms are thin wrappers over the primitive.

## Why tooltip descriptions as `&'static str`

The hover-tooltip text per opcode lives next to the mnemonic in
the disasm crate's opcode table:

```rust
// in disasm-zilog-z80
const LDIR_DESC: &str = "Block copy: BC bytes from (HL) to (DE), \
                         incrementing both; repeats until BC=0";
```

`&'static str` because the descriptions are written once, baked
into the binary, never need to be allocated or freed. Same
lifetime as the opcode table itself.

This gates the learner-mode tooltip UX directly on the disasm
crate — the descriptions ship with the disassembler, not in a
separate lookup table. Means we own the description quality
end-to-end per the spine decision (no dependency on a third-party
crate for the tooltips).

## What we are NOT doing in MVP

- **No `no_std` / no-alloc story.** The disasm crates allocate
  freely (`Vec<Operand>`, `Box<Operand>` for indirect operand
  recursion). If anyone embeds these in a no-alloc environment
  later, a `no_std` feature flag is a non-breaking addition.
- **No flag-effect metadata in MVP.** The decision record's earlier
  sketch had an `effect_flags: EffectFlags` field. Dropped from
  MVP because nothing in the debugger needs it yet; can be added
  later as a non-breaking enum extension.
- **No source-map / symbol-table support.** The debugger doesn't
  have source for the loaded ROMs; there's nothing to map to.
- **No conditional-jump target resolution.** A `JR Z, +0x12`
  instruction's `Operand::Displacement(0x12)` is what gets
  returned; the UI is responsible for computing the absolute
  target as `pc + 2 + 0x12` if it wants to display that.
- **No formatting helpers in `disasm-common`.** No `to_string()`
  on `DisassembledInstruction` that produces compact-form text.
  The debugger UI owns formatting; the disasm crates own decoding.
  A separate `disasm-format` crate could be added later if
  multiple UIs converge on the same formatting needs.

## Implementation order

1. **`disasm-common`** first. Shared types, no logic. ~50 LOC.
2. **`disasm-zilog-z80`** second. Z80 is the lead-audience target
   (Spectrum) and has the most complex prefix structure (DD / FD /
   ED / CB across 4 planes × 256 opcodes). Locking the API shape
   against the most complex of the four CPUs validates that the
   shape generalises. ~1200–1500 LOC including tables.
3. **`disasm-mos-6502`** third. Simplest of the four; quickly
   validates the API works for a flat 256-opcode set with simple
   addressing modes. ~500–700 LOC.
4. **`disasm-motorola-6809`** and **`disasm-motorola-68000`**
   follow. 6809 has post-byte addressing modes; 68000 has the
   effective-addressing-mode field with its many variants. ~500
   and ~1500 LOC respectively.

Total estimated LOC across all five crates: ~3500–5000.

## Sizing

- `disasm-common`: ~50 LOC
- `disasm-zilog-z80`: ~1200–1500 LOC (4 prefix planes)
- `disasm-mos-6502`: ~500–700 LOC
- `disasm-motorola-6809`: ~500–700 LOC
- `disasm-motorola-68000`: ~1500 LOC (effective-addressing modes
  are the bulk)
- Per-crate tests: ~200–400 LOC each (full-opcode-table coverage
  is non-negotiable)

Total Wave 2: ~5500–7500 LOC including tests.

---

# Cross-cutting drift triggers

If I'm about to suggest any of these, stop and re-read this record.

- **"Let's add Unix socket support to MVP for completeness"** —
  TCP-only ships sooner. Unix socket is a deferred feature, not a
  pre-MVP requirement.
- **"Let's depend on the execution crates' opcode tables instead
  of duplicating"** — re-read § Why duplicate. The coupling cost
  is worse than the duplication cost.
- **"Let's use a third-party Rust disasm crate (z80-asm,
  6502-asm, ...)"** — superseded by [`debugger-architecture.md`](debugger-architecture.md);
  hand-written tables own the tooltip story.
- **"The disasm crate should also do formatting"** — separation of
  concerns. Decoding is the disasm crate's job; formatting is the
  UI's. A separate format crate can be added later if needed.
- **"Let's use async for the TCP listener"** — sync is fine for
  the debugger's load. Async forces a tokio dependency on every
  per-system binary for no real benefit.
- **"Let's allow binding to 0.0.0.0 by default"** — security
  boundary. Default loopback; explicit host required to open
  remote access.
- **"The disasm crate should be no_std"** — out of scope for MVP.
  Can be added as a feature flag later without breaking changes.

# Log

### 2026-05-23 — Decision locked

Brainstormed in-session with Steve as the two Wave-blocker design
choices flowing from [`debugger-architecture.md`](debugger-architecture.md).
Both confirmed:

- MCP transport → TCP only, loopback default, explicit host for
  remote binding. ~500–600 LOC for Wave 1.
- Disasm API → shared `disasm-common` types + four per-CPU crates
  with duplicated tables. Structured operands (not pre-formatted
  strings) to enable learner-mode rendering. Three call patterns
  (primitive / convenience / range). ~5500–7500 LOC across all
  five crates for Wave 2.

Combined into one record (option B from the brainstorm) rather
than split because both are tightly related implementation details
of the same product. The spine record stays the higher authority;
this one elaborates two specific design choices.

No code yet. First implementation work is the `disasm-common`
shared types (smallest, clears the path for the four per-CPU
crates) and the `serve_tcp_listen` sibling in `emu198x-shell/src/mcp.rs`
(Wave 1 enabler).

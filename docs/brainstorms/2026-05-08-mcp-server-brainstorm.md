---
date: 2026-05-08
topic: mcp-server
---

# MCP server (Spectrum binary, cross-system protocol layer)

## What We're Building

A working MCP server reachable as `emu198x-spectrum --mcp`. Hand-rolled
JSON-RPC 2.0 over stdio, with one tool per `ScriptStep` (≈18 tools)
plus a `query` / `query_paths` pair for the read-side. The transport
layer + JSON-RPC envelope + a `Tool` trait + a registry live in
`emu198x-shell`; the Spectrum binary registers concrete tools that
delegate to its existing `execute_step` interceptor and the shell's
script executor.

Closes the only NOT-STARTED Spectrum SOLID criterion (5) for the
October launch. Acceptance per the criterion: at least one Code198x
skill exercises the server end-to-end.

## Why This Approach

**Why hand-roll JSON-RPC over stdio, not the `rmcp` SDK** — the
protocol surface for "tools only" is small (`initialize`, `tools/list`,
`tools/call`, `notifications/initialized`). We already have `serde`
and `serde_json`. Owning ~300 lines of envelope code is cheaper than
fighting an 0.x SDK's macro ergonomics against our existing trait-
object dispatch. SDK-style ergonomics become attractive if we later
expose resources / prompts / sampling / completions; for SOLID we
need only tools.

**Why shell owns transport, binary owns tools** — mirrors how the
script vocabulary is structured today. `emu198x-shell` already houses
the cross-system pieces (session, capture, video, BASIC-loader-as-
ScriptStep, headless script runner). MCP is the natural next layer
atop that. The boundary is clear: shell owns "JSON-RPC envelope +
`Tool` trait + tool registry"; the binary owns "concrete tools that
call into its `execute_step` (with `SetMachine` / `AutoloadTape` /
`LoadBasicProgram` interceptors) and otherwise delegate to the
shell's `ScriptStep::execute_collect`". When C64 / NES / Amiga need
MCP, they reuse the shell's framework and register their own tools.

**Why one tool per `ScriptStep`, not a single `script` mega-tool** —
MCP clients work better with discoverable, narrowly-typed tools.
The criterion description specifies "one tool per script verb" and
that aligns with how MCP-aware clients render UIs. Plus the 1:1
mapping means the JSON Schema for each tool's `inputSchema` is
derivable from the existing serde shape — same JSON the script
mode already accepts.

**Why block-now, stream-later** — the SOLID bar is a working skill,
not a streaming-aware one. Code198x's existing scripts are sequences
of short blocking calls (boot → autoload → wait → save_screenshot,
each ~1–2 s wall). Streaming is real engineering — backpressure,
cancellation, interleaved tool-call semantics — and deferring it
keeps this commit narrow. When the first long-running call's
silence becomes user-visible, layer in `notifications/log` or a
custom progress notification.

## Key Decisions

- **Transport**: stdio. Server reads JSON-RPC requests from stdin,
  writes responses + notifications to stdout. Logging goes to stderr
  so it doesn't pollute the wire. Newline-delimited per the MCP stdio
  transport spec; no Content-Length framing.
- **Protocol**: JSON-RPC 2.0 with the MCP envelope. Implemented
  methods: `initialize`, `tools/list`, `tools/call`. Implemented
  notifications received: `notifications/initialized`. Implemented
  notifications sent: none in this commit (parked with streaming).
- **Capabilities** advertised in `initialize` response: `tools` only.
  No `resources`, `prompts`, `sampling`, `logging`. Server name:
  `emu198x-spectrum`; version: cargo `pkg_version!`. Protocol version:
  `2025-06-18` (the current MCP spec at time of writing).
- **Tool surface** (Spectrum binary): every `ScriptStep` variant as a
  separate tool with the snake-case JSON tag as the tool name. Plus
  `query` (read one path) and `query_paths` (list paths, optional
  prefix filter). Tools dispatch into the same `execute_step` the
  script runner uses, so `SetMachine` / `AutoloadTape` /
  `LoadBasicProgram` interception is shared with `--script`.
- **Tool input schemas**: derived per-tool by hand for this commit
  (small, stable, ~5 fields each at most). A `schemars`-based auto-
  derive would be cleaner but adds a dep and macro pass that we
  haven't validated against the workspace's lints; revisit if the
  hand-written schemas drift.
- **Tool output**: each tool returns the corresponding
  `ScriptObservation` serialized as JSON (or `null` for steps that
  emit no observation). MCP `tools/call` wraps this in a `content`
  array of `{type: "text", text: "<json>"}` per the spec.
- **Session lifecycle**: one persistent session per server
  invocation. Server boots eagerly to 48K (same as `--script` today)
  on `initialize`. Tool calls dispatch against the live session,
  serialized — no concurrency, since the emulator is single-threaded
  by nature.
- **Long-running calls**: block. The response goes out when the tool
  completes. No timeout; clients that want one impose it
  themselves.
- **Errors**: tool failures return `tools/call` with `isError: true`
  and the error message in the content array. JSON-RPC protocol
  errors (parse, method-not-found, invalid-params) use the JSON-RPC
  2.0 error envelope.
- **`SetMachine` in MCP mode**: errors with the same "not yet
  supported" message as `--script`. Mid-session runtime swaps need
  the enum-of-sessions wrapper that's still deferred from Track 1B.
  Eager 48K covers Code198x's existing pipeline.
- **Logging**: server logs to stderr via the existing `log` crate
  (or simple `eprintln!`). Clients that capture stderr see
  diagnostics; stdout stays pure JSON-RPC.

## Open / parked items (not in this commit)

- **Streaming observations** as `notifications/log` or a custom
  progress notification. Add when a real call's silence on the wire
  causes UX problems.
- **HTTP transport** alongside stdio. MCP spec supports both;
  stdio is enough for Code198x's local-pipe usage.
- **Resources / prompts / sampling / completions.** Spec-allowed
  optional capabilities. Not needed for the tool-only surface SOLID
  asks for.
- **Multiple concurrent sessions.** Stdio is one-client-per-process
  by definition; not relevant.
- **Auth**. Stdio inherits process trust; no auth needed.
- **`schemars` for derived tool schemas.** Hand-written for now,
  swap if the schemas drift relative to the serde shape.
- **`SetMachine` mid-session.** Still deferred behind the enum-of-
  sessions wrapper; same status as `--script`.
- **Cross-system MCP tools.** C64 / NES / Amiga binaries register
  their own tools when each system reaches the SOLID bar; the shell
  framework supports it.

## Next Steps

→ Implementation. Phase shape:
  1. Shell-side `mcp` module: JSON-RPC envelope types (`Request`,
     `Response`, `Error`, notification shape), the `Tool` trait
     (`fn name`, `fn description`, `fn input_schema`, `fn call`),
     a `ToolRegistry`, and a stdio loop. Unit-test the envelope
     parse / serialise paths and the registry lookup.
  2. Shell-side handshake handler: `initialize` (capability
     advertise + return), `notifications/initialized` (no-op),
     `tools/list` (registry → tool descriptors), `tools/call`
     (registry lookup + dispatch + result wrapping). Tests run a
     full handshake against an in-memory transport.
  3. Spectrum binary's `src/mcp/mod.rs`: replace the
     `McpNotImplemented` stub with the real server. Tool
     registration: every `ScriptStep` variant (with hand-written
     JSON Schema), plus `query` and `query_paths`. Each tool's
     `call` method translates the JSON args into the corresponding
     `ScriptStep`, invokes the script runner's `execute_step`, and
     serialises the resulting observation.
  4. Smoke: feed a hand-written JSON-RPC stream into the binary's
     stdin, capture stdout, verify `initialize` → `tools/list` →
     `tools/call name=load_basic_program`, screenshot output.
  5. Code198x skill: pick one (probably the screenshot skill, as
     the simplest end-to-end exercise) and rewrite to drive the
     MCP server. Acceptance for SOLID criterion 5 flips to DONE.

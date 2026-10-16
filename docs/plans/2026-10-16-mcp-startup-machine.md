# Honor the requested Spectrum model at MCP startup

Lane: Spectrum launch-hardening. Scope: correct startup selection, without changing tools, firmware policy or machine timing.

1. Reproduce released 0.35.0 with `--mcp --machine spectrum_plus3` and four explicit +3 ROM overrides. It rejects ROM 0 against spectrum_48k because `Spectrum::run_mcp` discards the requested model and `mcp::run` hardcodes 48K.
2. In `crates/emu198x-spectrum/src/app.rs`, validate the model through the existing boot selector and pass it to `src/mcp/mod.rs`. Resolve firmware and build the same model there. Keep default 48K and the existing family-wide tool registration.
3. Add `crates/emu198x-spectrum/tests/mcp_startup.rs`: launch the actual CLI and exchange JSON-RPC using synthetic ROMs. Assert +3 machine identity and ROM byte identities, default48K behavior, unknown model refusal, invalid ROM identity refusal and missing required ROM refusal. No proprietary firmware in fixtures.
4. Run the new tests against the old code to observe failure, then run affected tests, headless/default builds, Clippy and hooks after the fix. Exercise the installed local +3 firmware separately. Push a focused PR, review CI and merge rebase when ready; follow the existing patch release workflow.

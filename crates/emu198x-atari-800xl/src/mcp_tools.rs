//! Atari 800XL MCP surface tests.
//!
//! The 800XL has no bespoke MCP tools: CPU / memory / poke / disasm /
//! stepping come from [`register_base_tools`](emu198x_shell::mcp_tools::register_base_tools),
//! the ANTIC / GTIA / POKEY / PIA chip state through the generic `query`
//! tool, and `press_key` / `type_string` from the shared keyboard tier
//! ([`register_keyboard_tools`](emu198x_shell::mcp_tools::register_keyboard_tools))
//! over the 800XL's `KeyboardTarget`. This module houses the integration
//! tests that drive that surface end-to-end.

#[cfg(test)]
mod tests {
    use emu198x_shell::mcp::{ToolContent, ToolRegistry, ToolResponse};
    use emu198x_shell::mcp_tools::{register_base_tools, register_keyboard_tools};
    use emu198x_shell::{HeadlessSession, MediaSet};
    use runtime_atari_800xl::{Atari800xlRuntime, Atari800xlSessionQueryProvider, Model};
    use serde_json::{Value, json};
    use std::path::PathBuf;

    type A800xlSession = HeadlessSession<Atari800xlRuntime, Atari800xlSessionQueryProvider>;

    const FRAME_TICKS_NTSC: u64 = 262 * 228;

    const MISSING_ROMS: &str = "needs atarixl.rom + ataribas.rom in ~/.emu198x/roms/atari-800xl/; \
         these tests are `#[ignore]`d, so asking for them by name means you want them run";

    fn rom(name: &str) -> Option<Vec<u8>> {
        let home = std::env::var("HOME").ok()?;
        std::fs::read(
            PathBuf::from(home)
                .join(".emu198x/roms/atari-800xl")
                .join(name),
        )
        .ok()
    }

    /// Both ROMs, or a loud failure. These used to `eprintln!` and `return`,
    /// which is how they stayed broken: a `--ignored` sweep on a machine
    /// without the ROMs reported two passes and ran no assertions. Panicking
    /// makes an absent ROM look like what it is — see
    /// `knowledge/decisions/a-gate-nobody-runs-is-a-silent-gate.md`.
    fn roms() -> (Option<Vec<u8>>, Option<Vec<u8>>) {
        (rom("atarixl.rom"), rom("ataribas.rom"))
    }

    fn body(resp: &ToolResponse) -> Value {
        assert!(!resp.is_error, "tool returned error");
        let ToolContent::Text { text } = &resp.content[0];
        serde_json::from_str(text).expect("json body")
    }

    #[test]
    #[ignore = "FIXTURE: requires local OS + BASIC ROMs at ~/.emu198x/roms/atari-800xl/"]
    fn inspection_tools_report_booted_state() {
        let (Some(os), Some(basic)) = roms() else {
            panic!("{MISSING_ROMS}");
        };
        let runtime = Atari800xlRuntime::new(Model::A800xlNtsc, Some(os), Some(basic), None, true)
            .expect("runtime");
        let mut session: A800xlSession = HeadlessSession::new_with_query_provider(
            runtime,
            FRAME_TICKS_NTSC,
            Atari800xlSessionQueryProvider,
        );
        session.prepare(&MediaSet::new(), &[]).expect("prepare");
        session.run_frames(600).expect("boot");

        let mut reg: ToolRegistry<A800xlSession> = ToolRegistry::new();
        register_base_tools(&mut reg);
        register_keyboard_tools(&mut reg);

        let call = |s: &mut A800xlSession, name: &str, args: Value| -> Value {
            body(&reg.get(name).expect("tool").call(args, s).expect("ok"))
        };

        // Chip state is read via query paths now. ANTIC programmed for
        // display: DMACTL with DL DMA on.
        let chip = |s: &mut A800xlSession, path: &str| -> Value {
            call(s, "query", json!({ "path": path }))["result"]["value"].clone()
        };
        let antic = chip(&mut session, "antic");
        let dmactl_hex = antic["dmactl"].as_str().expect("dmactl string");
        let dmactl =
            u8::from_str_radix(dmactl_hex.trim_start_matches('$'), 16).expect("dmactl hex");
        assert_ne!(dmactl & 0x20, 0, "DMACTL DL DMA: {antic}");

        // The other chip snapshots all surface their registers.
        assert!(chip(&mut session, "gtia")["colpf"].is_array());
        assert!(chip(&mut session, "pokey")["irqen"].is_string());
        assert!(chip(&mut session, "pia")["portb"].is_string());

        // Shared memory_read: addresses are integers, the observation reports
        // `bytes` as an array rather than a `hex` string, and `len` defaults to
        // 16. This test passed `"$0200"` and read `hex`, and was never run, so
        // nothing caught either — see #905.
        let mem = call(&mut session, "memory_read", json!({"addr": 0x0200}));
        let bytes = mem["bytes"].as_array().expect("bytes array");
        assert_eq!(bytes.len(), 16);

        // poke/read round-trip via the shared tools.
        call(
            &mut session,
            "poke_byte",
            json!({"addr": 0x0600, "value": 0x5A}),
        );
        let back = call(
            &mut session,
            "memory_read",
            json!({"addr": 0x0600, "len": 1}),
        );
        assert_eq!(
            back["bytes"].as_array().expect("bytes array"),
            &[json!(0x5A)]
        );

        // poke_word is little-endian.
        call(
            &mut session,
            "poke_word",
            json!({"addr": 0x0610, "value": 0xABCD}),
        );
        let w = call(
            &mut session,
            "memory_read",
            json!({"addr": 0x0610, "len": 2}),
        );
        assert_eq!(
            w["bytes"].as_array().expect("bytes array"),
            &[json!(0xCD), json!(0xAB)]
        );
    }

    fn screen_addr(ram: &[u8]) -> usize {
        let dlist = u16::from(ram[0x0230]) | (u16::from(ram[0x0231]) << 8);
        let mut p = dlist as usize;
        for _ in 0..64 {
            let b = ram[p];
            if b & 0x40 != 0 && (b & 0x0F) >= 2 {
                return usize::from(ram[p + 1]) | (usize::from(ram[p + 2]) << 8);
            }
            if b & 0x0F == 0x01 {
                break;
            }
            p += if b & 0x40 != 0 { 3 } else { 1 };
        }
        0
    }

    #[test]
    #[ignore = "FIXTURE: requires local OS + BASIC ROMs at ~/.emu198x/roms/atari-800xl/"]
    fn run_and_input_tools_drive_basic() {
        let (Some(os), Some(basic)) = roms() else {
            panic!("{MISSING_ROMS}");
        };
        let runtime = Atari800xlRuntime::new(Model::A800xlNtsc, Some(os), Some(basic), None, true)
            .expect("runtime");
        let mut session: A800xlSession = HeadlessSession::new_with_query_provider(
            runtime,
            FRAME_TICKS_NTSC,
            Atari800xlSessionQueryProvider,
        );
        session.prepare(&MediaSet::new(), &[]).expect("prepare");
        session.run_frames(600).expect("boot");

        let mut reg: ToolRegistry<A800xlSession> = ToolRegistry::new();
        register_base_tools(&mut reg);
        register_keyboard_tools(&mut reg);
        let call = |s: &mut A800xlSession, name: &str, args: Value| -> Value {
            body(&reg.get(name).expect("tool").call(args, s).expect("ok"))
        };

        // Shared run_until_pc: the idle BASIC prompt sits at its current PC,
        // so running to that PC is reached immediately.
        // `query_cpu` nests machine-specific fields under `registers` (its own
        // tool description says so), and the 6502 reports `pc` as a `$XXXX`
        // string. `run_until_pc` takes `addr`, as an integer. This test read
        // `pc` from the top level and passed it back under the wrong key, in
        // the wrong type — three mistakes that all needed the test to actually
        // run before anything would say so.
        let registers = call(&mut session, "query_cpu", json!({}))["registers"].clone();
        let pc_text = registers["pc"].as_str().expect("pc is a $XXXX string");
        let pc = u32::from_str_radix(pc_text.trim_start_matches('$'), 16).expect("pc parses");
        let ran = call(&mut session, "run_until_pc", json!({"addr": pc}));
        assert_eq!(ran["reached"], true, "run_until_pc revisits idle PC: {ran}");

        // type_string drives BASIC: `PRINT 6*7` then RETURN evaluates to 42.
        let typed = call(&mut session, "type_string", json!({"text": "PRINT 6*7\n"}));
        assert_eq!(typed["chars_typed"], 10);
        session.run_frames(30).expect("evaluate");

        let m = session.machine().machine().expect("machine");
        let ram = m.ram();
        let scr = screen_addr(ram);
        let found = (0..40 * 24 - 1).any(|j| ram[scr + j] == 0x14 && ram[scr + j + 1] == 0x12);
        assert!(
            found,
            "type_string `PRINT 6*7` did not yield `42` on screen"
        );
    }
}

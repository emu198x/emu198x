//! The Dragon 64 ROM's own RS-232 code driving the R6551 ACIA (#330).
//!
//! Each test boots the real Dragon 64 firmware and lets its serial routines do
//! the work: BASIC's printer output with `PRNSEL` set, the `SEROUT`/`SERIN`
//! direct-jump-table entries at `$802D`/`$802A` talking to a host loopback,
//! and the 64K-mode IRQ wedge at `$FE18` servicing a receive interrupt. The
//! ROM is the oracle; the host side only moves bytes.

use std::path::PathBuf;

use emu198x_shell::{
    FirmwareImage, FirmwareSet, HeadlessSession, InputEvent, MachineCore, read_firmware_asset,
};
use runtime_dragon::{DragonRuntime, DragonSessionQueryProvider, Model};

type Session = HeadlessSession<DragonRuntime, DragonSessionQueryProvider>;

const DRAGON_FRAME_CYCLES: u64 = 894_886 / 50;
const BOOT_FRAME_BUDGET: u32 = 100;
const KEY_EDGE_FRAMES: u32 = 8;
const SEROUT: u16 = 0x802D;
const SERIN: u16 = 0x802A;

#[test]
fn dragon64_basic_prints_to_the_serial_port_when_prnsel_is_set() {
    let Some(mut session) = booted_dragon64_session() else {
        emu198x_test_skip::skip!("Dragon 64 ROMs not staged (EMU198X_DRAGON64_COMPAT_ROM)");
    };

    // PRNSEL ($03FF) non-zero routes printer output to the RS-232 port.
    type_line(&mut session, "POKE1023,1");
    wait_for_ok(&mut session, 2);
    assert!(
        drain(&mut session).is_empty(),
        "nothing should reach the port yet"
    );

    type_line(&mut session, "PRINT#-2,123");
    let mut sent = Vec::new();
    for _ in 0..50 {
        session.run_frames(1).expect("Dragon 64 should run");
        sent.extend(drain(&mut session));
    }

    assert_eq!(
        sent,
        b" 123 \r".to_vec(),
        "serial printer output; screen:\n{}",
        screen_text_lines(&session).join("\n")
    );
    assert_eq!(query(&session, "acia.control"), serde_json::json!(0x98));
    assert_eq!(query(&session, "acia.baud"), serde_json::json!(1200.0));
}

#[test]
fn dragon64_serout_and_serin_round_trip_through_a_host_loopback() {
    let Some(mut session) = booted_dragon64_session() else {
        emu198x_test_skip::skip!("Dragon 64 ROMs not staged (EMU198X_DRAGON64_COMPAT_ROM)");
    };

    // For each byte of "HELLO": JSR SEROUT, JSR SERIN, store the reply.
    let [serout_hi, serout_lo] = SEROUT.to_be_bytes();
    let [serin_hi, serin_lo] = SERIN.to_be_bytes();
    let program = [
        0x8E, 0x61, 0x00, // LDX #$6100
        0xCE, 0x60, 0x20, // LDU #$6020
        0xA6, 0xC0, // LDA ,U+
        0x27, 0x0A, // BEQ $6014
        0xBD, serout_hi, serout_lo, // JSR SEROUT
        0xBD, serin_hi, serin_lo, // JSR SERIN
        0xA7, 0x80, // STA ,X+
        0x20, 0xF2, // BRA $6006
        0x6F, 0x84, // CLR ,X
        0x39, // RTS
    ];
    poke(&mut session, 0x6000, &program);
    poke(&mut session, 0x6020, b"HELLO\0");
    poke(&mut session, 0x6100, &[0xFF; 6]);

    type_line(&mut session, "EXEC24576");
    let mut wire = Vec::new();
    for _ in 0..100 {
        session.run_frames(1).expect("Dragon 64 should run");
        let sent = drain(&mut session);
        // The loopback: everything the Dragon sends comes straight back.
        session.machine_mut().queue_serial_input(&sent);
        wire.extend(sent);
    }

    assert_eq!(wire, b"HELLO".to_vec(), "bytes the ROM transmitted");
    let echoed = peek(&session, 0x6100, 6);
    assert_eq!(
        echoed,
        b"HELLO\0".to_vec(),
        "bytes SERIN returned; screen:\n{}",
        screen_text_lines(&session).join("\n")
    );
    // SERIN raises DTR only while it waits and drops it before returning.
    assert_eq!(query(&session, "acia.command"), serde_json::json!(0x0A));
    wait_for_ok(&mut session, 2);
}

#[test]
fn dragon64_mode_irq_wedge_services_a_receive_interrupt() {
    let Some(mut session) = booted_dragon64_session() else {
        emu198x_test_skip::skip!("Dragon 64 ROMs not staged (EMU198X_DRAGON64_ROM)");
    };

    type_line(&mut session, "EXEC");
    session
        .run_frames(200)
        .expect("Dragon 64 mode transition should advance");
    assert_eq!(
        query(&session, "hardware.model"),
        serde_json::json!("dragon64-mode")
    );

    // DTR on with the receiver interrupt enabled.
    type_line(&mut session, "POKE65286,9");
    wait_for_ok(&mut session, 2);
    assert_eq!(query(&session, "acia.command"), serde_json::json!(0x09));

    assert!(session.machine_mut().queue_serial_input(b"Z"));
    session.run_frames(5).expect("Dragon 64 should run");

    // The wedge at $FE18 saw IRQ and RDRF in the status register and
    // dropped DTR, leaving the character for software to collect.
    assert_eq!(query(&session, "acia.command"), serde_json::json!(0x08));
    let status = query(&session, "acia.status")
        .as_u64()
        .expect("Dragon 64 has an ACIA");
    assert_eq!(status & 0x88, 0x08, "RDRF still set, IRQ already read");
    assert_eq!(query(&session, "acia.irq"), serde_json::json!(false));

    type_line(&mut session, "PRINTPEEK(65284)");
    session.run_frames(30).expect("Dragon 64 should run");
    let lines = screen_text_lines(&session);
    assert!(
        lines.iter().any(|line| line.trim() == "90"),
        "PEEK of the receive register should be 90 ('Z'):\n{}",
        lines.join("\n")
    );
}

fn booted_dragon64_session() -> Option<Session> {
    let compat_rom_path = rom_path("EMU198X_DRAGON64_COMPAT_ROM", "dragon64-compat.rom")?;
    let mode_rom_path = rom_path("EMU198X_DRAGON64_ROM", "dragon64.rom")?;
    let compat = read_firmware_asset(&compat_rom_path).unwrap_or_else(|err| {
        panic!(
            "read Dragon 64 compatible ROM at {}: {err}",
            compat_rom_path.display()
        )
    });
    let mode = read_firmware_asset(&mode_rom_path).unwrap_or_else(|err| {
        panic!(
            "read Dragon 64 mode ROM at {}: {err}",
            mode_rom_path.display()
        )
    });
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("dragon64-compatible-rom", &compat.bytes));
    firmware.push(FirmwareImage::new("dragon64-basic-rom", &mode.bytes));
    let runtime = DragonRuntime::from_firmware(Model::Dragon64Pal, &firmware)
        .expect("real Dragon 64 ROMs should create a runtime");
    let mut session = HeadlessSession::new_with_query_provider(
        runtime,
        DRAGON_FRAME_CYCLES,
        DragonSessionQueryProvider,
    );
    let boot = session
        .wait_for_boot(BOOT_FRAME_BUDGET)
        .expect("Dragon 64 ROM should reach the BASIC prompt");
    assert_eq!(boot.reason, "basic-ok-prompt");
    session
        .run_frames(30)
        .expect("Dragon 64 should idle at the prompt");
    Some(session)
}

fn rom_path(var: &str, file: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(var).map(PathBuf::from)
        && path.exists()
    {
        return Some(path);
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let path = home.join(".emu198x/roms/dragon").join(file);
    if path.exists() {
        return Some(path);
    }
    emu198x_test_skip::record(&format!("Dragon 64 serial: set {var}"));
    None
}

fn type_line(session: &mut Session, line: &str) {
    for ch in line.chars() {
        let (shift, key) = match ch {
            '#' => (true, "3"),
            '"' => (true, "2"),
            '(' => (true, "8"),
            ')' => (true, "9"),
            ' ' => (false, "space"),
            _ => (false, ""),
        };
        let owned;
        let key = if key.is_empty() {
            owned = ch.to_string();
            owned.as_str()
        } else {
            key
        };
        tap(session, shift, key);
    }
    tap(session, false, "enter");
}

fn tap(session: &mut Session, shift: bool, key: &str) {
    let names: Vec<&str> = if shift { vec!["shift", key] } else { vec![key] };
    for name in &names {
        session.queue_input(InputEvent::Key {
            name: (*name).to_owned().into(),
            pressed: true,
        });
    }
    session
        .run_frames(KEY_EDGE_FRAMES)
        .expect("key press should advance the Dragon");
    for name in names.iter().rev() {
        session.queue_input(InputEvent::Key {
            name: (*name).to_owned().into(),
            pressed: false,
        });
    }
    session
        .run_frames(KEY_EDGE_FRAMES)
        .expect("key release should advance the Dragon");
}

fn wait_for_ok(session: &mut Session, count: usize) {
    for _ in 0..120 {
        let lines = screen_text_lines(session);
        assert!(
            !lines.iter().any(|line| line.contains("ERROR")),
            "BASIC reported an error:\n{}",
            lines.join("\n")
        );
        if lines.iter().filter(|line| line.trim_end() == "OK").count() >= count {
            return;
        }
        session.run_frames(1).expect("Dragon 64 should run");
    }
    panic!(
        "BASIC did not return to OK:\n{}",
        screen_text_lines(session).join("\n")
    );
}

fn drain(session: &mut Session) -> Vec<u8> {
    session.machine_mut().drain_serial_output()
}

fn poke(session: &mut Session, addr: u16, bytes: &[u8]) {
    let debug = session
        .machine_mut()
        .debug_target_mut()
        .expect("Dragon runtime has a debug target");
    for (addr, byte) in (u32::from(addr)..).zip(bytes) {
        debug.poke(addr, *byte);
    }
}

fn peek(session: &Session, addr: u16, len: usize) -> Vec<u8> {
    let debug = session
        .machine()
        .debug_target()
        .expect("Dragon runtime has a debug target");
    (u32::from(addr)..)
        .take(len)
        .map(|addr| debug.peek(addr))
        .collect()
}

fn query(session: &Session, path: &str) -> serde_json::Value {
    session
        .query(path)
        .unwrap_or_else(|err| panic!("{path} query should work: {err}"))
        .value
}

fn screen_text_lines(session: &Session) -> Vec<String> {
    query(session, "screen.text.lines")
        .as_array()
        .expect("screen.text.lines should be an array")
        .iter()
        .map(|value| value.as_str().unwrap_or_default().to_owned())
        .collect()
}

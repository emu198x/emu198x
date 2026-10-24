/// Tom Harte's single-step tests for the CMOS core.
///
/// The 65C12 in the BBC Master runs the 65SC02 instruction set, which is
/// Tom Harte's Synertek 65C02 set: the CMOS additions without the Rockwell
/// bit instructions or WDC's `WAI` and `STP`. Every cycle's bus activity is
/// checked, as for the NMOS core.
///
/// `EMU198X_65C02_TOM_HARTE_DIR` names the `synertek65c02/v1` directory.
///
/// One opcode:
/// `EMU198X_6502_OPCODE=72 cargo test -p emu198x-mos-6502 --test single_step_tests_65c12 run_named_opcode -- --ignored --nocapture`
///
/// Every opcode:
/// `cargo test --release -p emu198x-mos-6502 --test single_step_tests_65c12 run_all -- --ignored --nocapture`
mod support;

use emu198x_mos_6502::M6502;
use support::find_tom_harte_65c02_dir;
use support::tom_harte::{opcode_path, run_opcode_tests};

const OPCODE_ENV: &str = "EMU198X_6502_OPCODE";

/// Where the 65C12 is not the part this suite was generated for.
///
/// - The `x7` and `xF` columns are the Rockwell bit instructions here, `$CB`
///   is `WAI` and `$DB` is `STP`. The 65C12 has none of them: MAME's
///   `g65sc12` (on the W65C02 table) and b-em both run all of these as
///   one-cycle `NOP`s.
/// - `$5C` takes four cycles here and eight in MAME and b-em.
fn not_on_the_65c12(opcode: u8) -> bool {
    matches!(opcode & 0x0F, 0x07 | 0x0F) || matches!(opcode, 0xCB | 0xDB | 0x5C)
}

/// The extra cycle of a decimal-mode `ADC #`/`SBC #` reads a fixed `$0056` or
/// `$0000` in this suite, which no other source has; MAME reads the program
/// counter. Only that cycle's address is waived: every 2-cycle binary-mode
/// case has no third cycle to compare.
fn filter(opcode: u8, errors: Vec<String>) -> Vec<String> {
    errors
        .into_iter()
        .filter(|error| !(matches!(opcode, 0x69 | 0xE9) && error.starts_with("cycle2.addr")))
        .collect()
}

/// Run one opcode's cases. The caller skips the opcodes the 65C12 lacks.
fn run(opcode: u8) -> (usize, usize, Vec<String>) {
    let root = find_tom_harte_65c02_dir().unwrap_or_else(|message| panic!("{message}"));
    run_opcode_tests(&opcode_path(&root, opcode), M6502::new_65c12, filter)
}

#[test]
#[ignore = "FIXTURE: requires the local Tom Harte Synertek 65C02 corpus"]
fn run_named_opcode() {
    let opcode = std::env::var(OPCODE_ENV)
        .ok()
        .and_then(|value| u8::from_str_radix(value.trim(), 16).ok())
        .unwrap_or_else(|| panic!("set {OPCODE_ENV} to one hex opcode, for example '72'"));
    assert!(
        !not_on_the_65c12(opcode),
        "{opcode:02X} is not a 65C12 instruction in this suite"
    );
    let (pass, fail, failures) = run(opcode);
    println!("opcode {opcode:02X}: {pass} passed, {fail} failed");
    for failure in &failures {
        println!("{failure}");
    }
    assert_eq!(fail, 0, "opcode {opcode:02X} reported {fail} failures");
}

#[test]
#[ignore = "FIXTURE: requires the local Tom Harte Synertek 65C02 corpus and runs for minutes"]
fn run_all() {
    let (mut pass, mut fail, mut waived) = (0, 0, 0);
    let mut report = Vec::new();
    for opcode in 0..=0xFFu8 {
        if not_on_the_65c12(opcode) {
            waived += 1;
            continue;
        }
        let (opcode_pass, opcode_fail, failures) = run(opcode);
        pass += opcode_pass;
        fail += opcode_fail;
        if opcode_fail != 0 {
            report.push(format!(
                "FAIL {opcode:02X}: {opcode_fail} failures, first: {}",
                failures.first().map_or("unknown", String::as_str)
            ));
        }
    }
    println!(
        "Tom Harte Synertek 65C02: {pass} passed, {fail} failed, {waived} opcodes not on the 65C12"
    );
    for line in &report {
        println!("{line}");
    }
    assert_eq!(fail, 0, "the 65C02 corpus reported {fail} failures");
}

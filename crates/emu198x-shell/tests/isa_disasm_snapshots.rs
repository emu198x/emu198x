//! Pin the rendered 6502 and 6809 opcode matrices supplied by Isa198x.
//!
//! These CPUs have no independent disassembler in Emu198x: the debugger uses
//! `isa198x-disasm` directly. A dependency bump must therefore regenerate
//! these fixtures explicitly, turning output changes into a diff that can be
//! reviewed. Run with `UPDATE_ISA_DISASM_SNAPSHOTS=1` to update both files.

use std::fmt::Write as _;
use std::path::Path;

fn render_6502() -> String {
    let mut out = String::new();
    for opcode in 0u16..=0xFF {
        let bytes = [opcode as u8, 0, 0];
        let (text, len) = isa_disasm::decode_one_6502(0, |addr| bytes[usize::from(addr)])
            .expect("every byte renders as an instruction or data");
        writeln!(out, "{opcode:02X} | {len} | {text}").expect("writing to a String cannot fail");
    }
    out
}

fn render_6809() -> String {
    let mut out = String::new();
    for prefix in [None, Some(0x10), Some(0x11)] {
        for opcode in 0u16..=0xFF {
            let bytes = match prefix {
                None => [opcode as u8, 0, 0, 0, 0],
                Some(prefix) => [prefix, opcode as u8, 0, 0, 0],
            };
            let (text, len) = isa_disasm::decode_one_6809(0, |addr| bytes[usize::from(addr)])
                .expect("every byte renders as an instruction or data");
            match prefix {
                None => writeln!(out, "{opcode:02X} | {len} | {text}"),
                Some(prefix) => writeln!(out, "{prefix:02X} {opcode:02X} | {len} | {text}"),
            }
            .expect("writing to a String cannot fail");
        }
    }
    out
}

fn check_snapshot(path: &Path, expected: &str, actual: &str) {
    if std::env::var_os("UPDATE_ISA_DISASM_SNAPSHOTS").is_some() {
        std::fs::write(path, actual).expect("snapshot path is writable");
        return;
    }

    if expected == actual {
        return;
    }

    let difference = expected
        .lines()
        .zip(actual.lines())
        .enumerate()
        .find(|(_, (left, right))| left != right)
        .map_or_else(
            || "the files have different lengths".to_string(),
            |(index, (left, right))| {
                format!(
                    "first difference at line {}:\n  expected: {left}\n  actual:   {right}",
                    index + 1
                )
            },
        );
    panic!(
        "{} changed: {difference}\nreview the Isa198x dependency, then regenerate with \
         UPDATE_ISA_DISASM_SNAPSHOTS=1 cargo test -p emu198x-shell --test isa_disasm_snapshots",
        path.display()
    );
}

#[test]
fn rendered_opcode_matrices_match_the_reviewed_snapshots() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    check_snapshot(
        &directory.join("isa-disasm-6502.txt"),
        include_str!("snapshots/isa-disasm-6502.txt"),
        &render_6502(),
    );
    check_snapshot(
        &directory.join("isa-disasm-6809.txt"),
        include_str!("snapshots/isa-disasm-6809.txt"),
        &render_6809(),
    );
}

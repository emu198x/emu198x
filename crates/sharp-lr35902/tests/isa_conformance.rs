//! Cross-check the independent SM83 disassemblers in Emu198x and Isa198x.
//!
//! The unprefixed and CB-prefixed opcode spaces are small enough to exhaust.
//! Zero operand bytes make the comparison deterministic while still checking
//! instruction length and rendered operands.

use sharp_lr35902::disassemble;

fn emu_one(buf: &[u8]) -> (String, usize) {
    let (text, len) = disassemble(0, |addr| buf.get(addr as usize).copied().unwrap_or(0));
    (text, usize::from(len))
}

fn isa_one(buf: &[u8]) -> (String, usize) {
    let line = isa_disasm::disassemble_sm83(buf, 0)
        .into_iter()
        .next()
        .expect("the non-empty opcode buffer produces one line");
    (line.text, line.bytes.len())
}

fn normalize(text: &str) -> String {
    let mut normalized = text
        .to_ascii_lowercase()
        .replace(' ', "")
        .replace('[', "(")
        .replace(']', ")");

    // rgbasm spells the accumulator explicitly for these ALU forms; Emu's
    // debugger follows the common abbreviated spelling.
    for explicit in ["suba,", "anda,", "xora,", "ora,", "cpa,"] {
        normalized = normalized.replace(explicit, &explicit[..explicit.len() - 2]);
    }

    normalized = normalized
        // rgbasm exposes the high-page base in LDH operands.
        .replace("$ff00+", "")
        .replace("ldh(c),", "ld(c),")
        .replace("ldha,(c)", "lda,(c)")
        // Equivalent signed-zero and indirect-HL spellings.
        .replace("sp,+$00", "sp,$00")
        .replace("jp(hl)", "jphl");
    normalized
}

#[test]
fn emu_sm83_disasm_matches_isa_spec() {
    const FILLER: [u8; 3] = [0; 3];
    let mut mismatches = Vec::new();

    for opcode in 0u16..=0xFF {
        let mut buf = vec![opcode as u8];
        buf.extend_from_slice(&FILLER);
        let emu = emu_one(&buf);
        let isa = isa_one(&buf);
        if normalize(&emu.0) != normalize(&isa.0) || emu.1 != isa.1 {
            mismatches.push((format!("{opcode:02X}"), emu, isa));
        }
    }

    for opcode in 0u16..=0xFF {
        let mut buf = vec![0xCB, opcode as u8];
        buf.extend_from_slice(&FILLER);
        let emu = emu_one(&buf);
        let isa = isa_one(&buf);
        if normalize(&emu.0) != normalize(&isa.0) || emu.1 != isa.1 {
            mismatches.push((format!("CB {opcode:02X}"), emu, isa));
        }
    }

    if !mismatches.is_empty() {
        for (opcode, emu, isa) in mismatches.iter().take(80) {
            eprintln!("{opcode}: emu={emu:?}, isa={isa:?}");
        }
        panic!(
            "Emu's SM83 decoder diverges from isa-disasm on {} opcodes",
            mismatches.len()
        );
    }
}

//! Cross-check Emu's hand-written Z80 disassembler against the Asm198x ISA-spec
//! disassembler (`isa-disasm`).
//!
//! Rung-1 phase 2 of `198x/decisions/rung1-wiring.md`, the Z80 counterpart to
//! the 68000 cross-check. Two independent decoders of the same encoding table;
//! diffing them over the opcode matrix (unprefixed, CB, ED, DD/FD, DD CB/FD CB)
//! is a standing regression net.
//!
//! Unlike the 68000, isa-disasm's Z80 table is complete and the two dialects are
//! close to identical (`LD A,(IX+$05)` both sides, uppercase, `$` hex, resolved
//! relative-jump targets), so this compares the **whole rendered line**, not just
//! the mnemonic — a stronger check than the 68000's. A light normaliser
//! (lower-case, collapse whitespace) absorbs the cosmetic gap. Still scoped to
//! the shared surface: where isa renders a data byte (`dc.b`/`defb`) we can't
//! adjudicate, so we skip and count it.

use zilog_z80::disassemble;

fn emu_one(buf: &[u8]) -> (String, usize) {
    let data = buf.to_vec();
    let (text, len) = disassemble(0, move |addr| data.get(addr as usize).copied().unwrap_or(0));
    (text, len as usize)
}

fn isa_one(buf: &[u8]) -> Option<(String, usize)> {
    // z80n = false: base Z80, no Spectrum Next extensions (Emu's core is base Z80).
    let lines = isa_disasm::disassemble_z80(buf, 0, false);
    lines.first().map(|l| (l.text.clone(), l.bytes.len()))
}

/// True if a rendered line is a data directive — the decoder rejected the
/// encoding rather than naming an instruction.
fn is_data(text: &str) -> bool {
    let t = text.trim_start().to_ascii_lowercase();
    t.starts_with("dc.")
        || t.starts_with("defb")
        || t.starts_with("db ")
        || t.starts_with("illegal")
}

/// Lower-case and collapse whitespace so the only-cosmetic gap between the two
/// dialects doesn't register as a divergence.
fn normalize(text: &str) -> String {
    text.to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every encoding in the Z80 opcode matrix: unprefixed, the CB/ED/DD/FD prefix
/// groups, and the DD CB / FD CB displacement groups. Each gets zero extension
/// filler so operands render simply.
fn opcode_buffers() -> Vec<Vec<u8>> {
    const FILLER: [u8; 6] = [0; 6];
    let mut bufs = Vec::new();
    let mut push = |bytes: &[u8]| {
        let mut b = bytes.to_vec();
        b.extend_from_slice(&FILLER);
        bufs.push(b);
    };

    for b in 0u16..=0xFF {
        push(&[b as u8]);
    }
    for pfx in [0xCBu8, 0xED, 0xDD, 0xFD] {
        for b in 0u16..=0xFF {
            push(&[pfx, b as u8]);
        }
    }
    // DD CB / FD CB: the opcode byte follows the displacement byte.
    for pfx in [0xDDu8, 0xFD] {
        for b in 0u16..=0xFF {
            push(&[pfx, 0xCB, 0x00, b as u8]);
        }
    }
    bufs
}

#[test]
fn emu_z80_disasm_matches_isa_spec() {
    let mut compared = 0usize;
    let mut skipped_isa_data = 0usize;
    let mut mismatches: Vec<(Vec<u8>, String, usize, String, usize)> = Vec::new();

    for buf in opcode_buffers() {
        let (emu_text, emu_len) = emu_one(&buf);
        let Some((isa_text, isa_len)) = isa_one(&buf) else {
            continue;
        };

        if is_data(&isa_text) {
            if !is_data(&emu_text) {
                skipped_isa_data += 1;
            }
            continue;
        }

        compared += 1;
        if normalize(&emu_text) != normalize(&isa_text) || emu_len != isa_len {
            mismatches.push((buf.clone(), emu_text, emu_len, isa_text, isa_len));
        }
    }

    eprintln!(
        "Z80 isa-disasm conformance: {compared} opcodes compared on the shared surface, \
         {skipped_isa_data} skipped (isa renders data)"
    );

    if !mismatches.is_empty() {
        eprintln!("=== {} divergences ===", mismatches.len());
        for (buf, et, el, it, il) in mismatches.iter().take(120) {
            let op: String = buf
                .iter()
                .take(buf.len() - 6)
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!("  [{op}]  emu={et:?} (len {el})   isa={it:?} (len {il})");
        }
        panic!(
            "Emu's Z80 decoder diverges from isa-disasm on {} opcodes",
            mismatches.len()
        );
    }

    assert!(compared > 0, "shared surface was empty?");
}

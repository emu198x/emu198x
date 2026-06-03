//! Cross-check Emu's hand-written 68000 disassembler against the Asm198x
//! ISA-spec disassembler (`isa-disasm`).
//!
//! Rung-1 phase 2 of `198x/decisions/rung1-wiring.md`: the two decoders are
//! independent implementations of the same encoding table, so diffing them is a
//! standing regression net for the class of bug we fixed three times by hand
//! (DBcc decoded as Scc/ADDQ; the group-C and group-8 overlaps). Emu keeps its
//! own decoder; this test makes the spec crate watch it.
//!
//! ## What it compares, and what it deliberately doesn't
//!
//! The two render in different dialects (Emu: `MOVEQ #42,D3`; isa-disasm:
//! `moveq.l #0,d0`), and their *operand* syntax diverges wholesale — hex vs
//! decimal immediates (`#$00000000` vs `#0`), effective-address style
//! (`(0,A0,D0.w)` vs `0(a0,d0.w)`). Normalising operand text to identity across
//! two dialects this different is brittle and beside the point. This cross-check
//! targets the *decode table* — the mnemonic and the instruction length — and
//! ignores operand rendering. Operand *semantics* are already pinned by the Tom
//! Harte single-step suite (1,000,058 tests at 100%); this is the complementary
//! decode-shape net. A [`mnemonic`] normaliser folds the documented dialect
//! diffs (implicit sizes like `lea`/`moveq`; address-register variants like
//! `suba`↔`sub`) so a surviving mismatch is a genuine opcode-table disagreement.
//!
//! ## Scoped to the surface isa-disasm implements
//!
//! As of the pinned rev (`f6569ee`), isa-disasm's 68000 spec (`isa/src/m68k.rs`)
//! is a *partial* table — it omits whole families (`muls`/`divs`, every shift
//! and rotate bar `lsl`/`lsr`, `bchg`/`bclr`, most `Scc`, `movep`, `addx`/`subx`,
//! `jmp`/`jsr`, `link`/`unlk`, …), rendering them as `dc.w`. So when isa rejects
//! an encoding we *cannot* tell "isa hasn't implemented it yet" from "Emu is
//! over-decoding an illegal EA" without per-opcode hardware truth — and the
//! former dominates. This test therefore asserts only over the **shared surface**
//! (encodings both decoders name an instruction for), and reports how many
//! opcodes it skipped for that reason so the coverage gap is never silent. As
//! isa-disasm's table fills in, the checked surface grows automatically.
//!
//! Two backlogs fall out of this and are tracked separately, not here:
//!   - isa-disasm 68000 completeness (Asm198x repo);
//!   - Emu's group-0 / `lea` / byte-`move` illegal-EA strictness (phase 2.1).

use motorola_68000::disasm::disassemble;

/// Disassemble one instruction with Emu's decoder at origin 0.
fn emu_one(buf: &[u8]) -> (String, usize) {
    let data = buf.to_vec();
    let (text, len) = disassemble(0, move |addr| data.get(addr as usize).copied().unwrap_or(0));
    (text, len as usize)
}

/// Disassemble the first instruction with the isa-disasm spec decoder.
fn isa_one(buf: &[u8]) -> Option<(String, usize)> {
    let lines = isa_disasm::disassemble_68000(buf, 0);
    lines.first().map(|l| (l.text.clone(), l.bytes.len()))
}

/// True if a rendered line is a data directive — i.e. the decoder rejected the
/// encoding rather than naming an instruction.
fn is_data(text: &str) -> bool {
    let t = text.trim_start().to_ascii_lowercase();
    t.starts_with("dc.") || t.starts_with("illegal")
}

/// Mnemonics whose operand size is fixed by the opcode, so the size suffix is
/// pure dialect: one side spells it, the other omits it (`lea`↔`lea.l`,
/// `mulu.w`↔`mulu`, `seq`↔`seq.b`). Folded to the bare base for comparison.
const IMPLICIT_SIZE: &[&str] = &[
    "moveq", "lea", "pea", "mulu", "muls", "divu", "divs", "nbcd", "tas", "swap", "st", "sf",
    "shi", "sls", "scc", "scs", "sne", "seq", "svc", "svs", "spl", "smi", "sge", "slt", "sgt",
    "sle",
];

/// The instruction's normalised mnemonic: lower-cased, dialect diffs muted,
/// operands dropped (see the module comment).
fn mnemonic(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mnem = lower
        .split_once(char::is_whitespace)
        .map_or(lower.as_str(), |(m, _)| m);
    let (base, size) = mnem
        .split_once('.')
        .map_or((mnem, None), |(b, s)| (b, Some(s)));

    // Address-register variant -> base data mnemonic. The An-destination form of
    // an arithmetic op has its own mnemonic, but isa renders it under the base
    // name: same opcode family, same length, a naming choice not a decode diff.
    let base = match base {
        "movea" => "move",
        "suba" => "sub",
        "adda" => "add",
        "cmpa" => "cmp",
        other => other,
    };

    // Drop the size suffix when it is fixed by the opcode (dialect-only).
    match size {
        Some(s) if !IMPLICIT_SIZE.contains(&base) => format!("{base}.{s}"),
        _ => base.to_string(),
    }
}

/// A Bcc/BRA/BSR encoding whose 8-bit displacement field is `$FF`. On the
/// 68020+ that is the escape to a 32-bit displacement (`Bcc.l`); on the base
/// 68000 it is just a short branch of −1 (`Bcc.s`). isa-disasm targets the
/// 68000; this crate hosts the 68020 decode arms, so the two legitimately
/// disagree here. Documented model difference, not a bug — excluded from the
/// shared-surface comparison.
fn is_68020_long_branch_escape(op: u16) -> bool {
    (0x6000..=0x6FFF).contains(&op) && (op & 0x00FF) == 0x00FF
}

#[test]
fn emu_68000_disasm_matches_isa_spec() {
    // Opcode word + zero extension filler. Zero filler keeps operand rendering
    // simple so the signal is in the mnemonic + length, not number formatting.
    const FILLER: usize = 12;

    let mut compared = 0usize;
    let mut skipped_isa_unimpl = 0usize;
    let mut mismatches: Vec<(u16, String, usize, String, usize)> = Vec::new();

    for op in 0u16..=0xFFFF {
        if is_68020_long_branch_escape(op) {
            continue;
        }

        let mut buf = vec![0u8; 2 + FILLER];
        buf[0] = (op >> 8) as u8;
        buf[1] = (op & 0xFF) as u8;

        let (emu_text, emu_len) = emu_one(&buf);
        let Some((isa_text, isa_len)) = isa_one(&buf) else {
            continue;
        };

        // Both reject -> agreement. isa alone rejects -> outside the shared
        // surface (isa hasn't implemented it, or Emu is over-decoding an illegal
        // EA — indistinguishable here); count it and move on.
        if is_data(&isa_text) {
            if !is_data(&emu_text) {
                skipped_isa_unimpl += 1;
            }
            continue;
        }
        if is_data(&emu_text) {
            // isa names an instruction Emu rejects: a real Emu decode gap.
            mismatches.push((op, emu_text, emu_len, isa_text, isa_len));
            continue;
        }

        compared += 1;
        if mnemonic(&emu_text) != mnemonic(&isa_text) || emu_len != isa_len {
            mismatches.push((op, emu_text, emu_len, isa_text, isa_len));
        }
    }

    eprintln!(
        "68000 isa-disasm conformance: {compared} opcodes compared on the shared surface, \
         {skipped_isa_unimpl} skipped (isa-disasm renders dc.w — unimplemented or illegal-EA)"
    );

    if !mismatches.is_empty() {
        eprintln!("=== {} genuine divergences ===", mismatches.len());
        for (op, et, el, it, il) in mismatches.iter().take(100) {
            eprintln!("  {op:04X}  emu={et:?} (len {el})   isa={it:?} (len {il})");
        }
        panic!(
            "Emu's 68000 decoder diverges from isa-disasm on {} shared-surface opcodes",
            mismatches.len()
        );
    }

    assert!(
        compared > 0,
        "shared surface was empty — isa_one always rejected?"
    );
}

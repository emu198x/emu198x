//! Cross-check Emu's hand-written 68000 disassembler against the Asm198x
//! ISA-spec disassembler (`isa-disasm`).
//!
//! Rung-1 phase 2 of `198x/decisions/rung1-wiring.md`: the two decoders are
//! independent implementations of the same encoding table, so diffing them is a
//! standing regression net for the class of bug we fixed three times by hand
//! (DBcc decoded as Scc/ADDQ; the group-C and group-8 overlaps). Emu keeps its
//! own decoder; this test makes the spec crate watch it.
//!
//! ## Full conformance over the whole opcode space
//!
//! The spec's 68000 table is complete as of the pinned rev (it covers the full
//! base-68000 ISA, validated byte-identical against vasm). So this test is now
//! **strict in both directions** over all 65,536 opcode words:
//!
//!   - if the spec names an instruction Emu renders `dc.w` → Emu under-decodes
//!     (a missing instruction);
//!   - if Emu names an instruction the spec rejects → Emu over-decodes (an
//!     illegal effective address it should refuse — the old "phase 2.1"
//!     strictness gap);
//!   - if both name an instruction but disagree on mnemonic or length → a real
//!     opcode-table divergence (the original bug class).
//!
//! All three are failures. Operand *rendering* is deliberately not compared:
//! the two dialects diverge wholesale (Emu `MOVEQ #42,D3`; isa `moveq.l #0,d0`;
//! hex vs decimal immediates; `(0,A0,D0.w)` vs `0(a0,d0.w)`), and operand
//! semantics are already pinned by the Tom Harte single-step suite (1,000,058
//! tests at 100%). A [`mnemonic`] normaliser folds the documented dialect diffs
//! (implicit sizes like `lea`/`moveq`/`abcd`; address-register variants like
//! `suba`↔`sub`) so a surviving mismatch is a genuine opcode-table disagreement.
//!
//! ## Documented model differences (excluded)
//!
//! Three small sets are legitimately not shared and are excluded by
//! [`documented_model_diff`], not silently skipped:
//!   - **68020 extensions** Emu decodes but the base-68000 spec does not: the
//!     `Bcc.l` `$FF`-displacement escape and `EXTB.L`. This crate hosts the
//!     68020 decode arms (for the A1200's 68020); isa targets the 68000.
//!   - **dynamic `BTST Dn,#imm`** — the immediate addressing mode is legal for
//!     the dynamic `BTST` form on the 68000 (Musashi's `btst_r` EA mask `0xbff`
//!     includes the immediate bit; the MC68000 PRM agrees). Emu follows the
//!     hardware; the spec renders `dc.w`. A spec-side note. The *static*
//!     `BTST #bit,#imm` form is genuinely illegal (`btst_s` mask `0xbfb`) and
//!     Emu rejects it — so it is not excluded.
//!
//! The effective-address legality tables were cross-validated against Musashi's
//! `m68kdasm.c` per-instruction EA masks, not just the spec.

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
/// `mulu.w`↔`mulu`, `abcd`↔`abcd.b`). Folded to the bare base for comparison.
const IMPLICIT_SIZE: &[&str] = &[
    "moveq", "lea", "pea", "mulu", "muls", "divu", "divs", "nbcd", "tas", "swap", "st", "sf",
    "shi", "sls", "scc", "scs", "sne", "seq", "svc", "svs", "spl", "smi", "sge", "slt", "sgt",
    "sle", "abcd", "sbcd", "chk",
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

/// Opcodes where Emu and the base-68000 spec legitimately differ — 68020
/// extensions Emu decodes, plus the `BTST #imm` case where the spec is stricter
/// than the 68000 hardware. Excluded from the strict comparison; see the module
/// comment. Returns a short reason for the coverage report.
fn documented_model_diff(op: u16) -> Option<&'static str> {
    // Bcc/BRA/BSR with an $FF 8-bit displacement: the 68020 32-bit-displacement
    // escape, just a −1 short branch on the 68000.
    if (0x6000..=0x6FFF).contains(&op) && (op & 0x00FF) == 0xFF {
        return Some("68020 long-branch escape");
    }
    // EXTB.L Dn — 68020 byte→long sign extension; no base-68000 encoding.
    if (op & 0xFFF8) == 0x49C0 {
        return Some("68020 EXTB.L");
    }
    // BTST Dn,#imm — the *dynamic* form's EA may be immediate on the 68000
    // (Musashi's `btst_r` EA mask 0xbff includes the immediate bit; the 68000
    // PRM agrees), but the spec renders it dc.w. The *static* form (`#bit,#imm`,
    // $083C) is genuinely illegal — Musashi's `btst_s` mask 0xbfb clears that
    // bit — and Emu now rejects it too, so it is *not* excluded here.
    if (op & 0xF1FF) == 0x013C {
        return Some("dynamic BTST Dn,#imm (68000-legal, spec-strict)");
    }
    None
}

#[test]
fn emu_68000_disasm_matches_isa_spec() {
    // Opcode word + zero extension filler. Zero filler keeps operand rendering
    // simple so the signal is in the mnemonic + length, not number formatting.
    const FILLER: usize = 12;

    let mut compared = 0usize;
    let mut excluded = 0usize;
    let mut mismatches: Vec<(u16, String, usize, String, usize)> = Vec::new();

    for op in 0u16..=0xFFFF {
        if documented_model_diff(op).is_some() {
            excluded += 1;
            continue;
        }

        let mut buf = vec![0u8; 2 + FILLER];
        buf[0] = (op >> 8) as u8;
        buf[1] = (op & 0xFF) as u8;

        let (emu_text, emu_len) = emu_one(&buf);
        let Some((isa_text, isa_len)) = isa_one(&buf) else {
            continue;
        };

        let emu_data = is_data(&emu_text);
        let isa_data = is_data(&isa_text);

        // Both reject -> agreement.
        if emu_data && isa_data {
            compared += 1;
            continue;
        }
        // Exactly one rejects -> under- or over-decode, both failures now that
        // the spec table is complete.
        if emu_data != isa_data {
            mismatches.push((op, emu_text, emu_len, isa_text, isa_len));
            continue;
        }

        compared += 1;
        if mnemonic(&emu_text) != mnemonic(&isa_text) || emu_len != isa_len {
            mismatches.push((op, emu_text, emu_len, isa_text, isa_len));
        }
    }

    eprintln!(
        "68000 isa-disasm conformance: {compared} opcodes agree, \
         {excluded} excluded as documented model differences"
    );

    if !mismatches.is_empty() {
        eprintln!("=== {} divergences ===", mismatches.len());
        for (op, et, el, it, il) in mismatches.iter().take(100) {
            eprintln!("  {op:04X}  emu={et:?} (len {el})   isa={it:?} (len {il})");
        }
        panic!(
            "Emu's 68000 decoder diverges from isa-disasm on {} opcodes",
            mismatches.len()
        );
    }

    assert!(compared > 60_000, "comparison surface unexpectedly small");
}

//! Rust port of blargg `blargg_nes_cpu_test5/01-implied`'s CRC
//! framework, used to isolate which of the 22 implied-mode opcodes
//! diverges from the silicon-validated expected CRC.
//!
//! The original `cpu.nes` / `official.nes` ROMs both fail uniquely
//! on test 01-implied after the 2026-06-01 LXA fix, but the multi
//! build doesn't print per-opcode failure info — the test reports
//! only the aggregate verdict at `$00FF`. This harness:
//!
//! 1. Mirrors the iteration order of `test_normal` ×
//!    `test_flags` × `test_instr` from `instr_test_end.a` —
//!    `in_p` in `[$00, $FF]`, then `outer_y` in `7..=0`, then
//!    `inner_y` in `7..=0`.
//! 2. Computes the same CRC-32 (poly `0xEDB88320`, init
//!    `0xFFFFFFFF`, no final XOR) over the same six bytes per
//!    iteration that `check_paxyso` accumulates: A, P (PHP'd, so
//!    bits 4-5 always = `$30`), X, Y, S, operand memory byte.
//! 3. Compares the final running CRC against the byte-reversed
//!    expected `.dword` from `01-implied.a`.
//!
//! ## Current status (2026-06-01)
//!
//! 2 of 20 OFFICIAL_ONLY opcodes match the expected CRC:
//!
//! - ✓ `$8A` TXA
//! - ✓ `$98` TYA
//!
//! The other 18 don't match. Side-by-side against Mesen2 our
//! per-opcode implementations look equivalent (see
//! `apply_implied` in `crates/mos-6502/src/tick.rs`), so the
//! discrepancy is almost certainly in some subtle detail of the
//! framework port — possibly the framework's CPU state on the
//! first iteration (Y register, S register, P bits 4-5 at the
//! moment of `set_paxyso`'s PLP), or in what blargg's `init_crc_fast`
//! initialisation produces vs what the bit-by-bit `update_crc`
//! produces, or in the operand byte timing.
//!
//! The harness is committed as a foundation: TXA/TYA matching
//! confirms the CRC algorithm, .dword interpretation, and outer
//! iteration order are all correct. The remaining work is
//! identifying what makes the other 18 diverge.
//!
//! Run with:
//!
//! ```sh
//! cargo test --release -p mos-6502 --test blargg_01_implied_crc \
//!     -- --ignored --nocapture
//! ```

use mos_6502::M6502;

/// blargg's `values` array — repeated twice so `values[y+1]` /
/// `values[y+2]` indexing wraps without an explicit modulo.
const VALUES: [u8; 16] = [
    0x00, 0x01, 0x02, 0x40, 0x7F, 0x80, 0x81, 0xFF, 0x00, 0x01, 0x02, 0x40, 0x7F, 0x80, 0x81, 0xFF,
];

/// CRC-32 step: reflected polynomial `0xEDB88320` — matches blargg's
/// `update_crc` constants (`$20, $83, $B8, $ED` summed into the
/// running checksum).
fn update_crc(crc: u32, byte: u8) -> u32 {
    let mut c = crc ^ u32::from(byte);
    for _ in 0..8 {
        if c & 1 != 0 {
            c = (c >> 1) ^ 0xEDB88320;
        } else {
            c >>= 1;
        }
    }
    c
}

/// Set up the CPU at $8000 with the implied opcode + a couple of
/// trailing NOPs, then tick until the implied operation has
/// completed (PC advanced past the opcode byte). Implied opcodes
/// are 2 cycles; we run a generous 4 ticks so any 2-3-cycle path
/// finishes.
fn run_one_implied(opcode: u8, in_a: u8, in_x: u8, in_y: u8, in_p: u8, in_s: u8) -> Post {
    let mut cpu = M6502::new_2a03();
    cpu.regs.pc = 0x8000;
    cpu.regs.sp = in_s;
    cpu.regs.a = in_a;
    cpu.regs.x = in_x;
    cpu.regs.y = in_y;
    // PLP doesn't change bits 4-5 in live P; on real hardware those
    // bits stay at $30 across PHP/PLP cycles. blargg's `set_paxyso`
    // loads in_p via PLP, so the actual P at instruction start is
    // `(in_p & 0xCF) | 0x30`, not `in_p` directly.
    cpu.regs.p = (in_p & 0xCF) | 0x30;
    cpu.total_cycles = 0;
    cpu.addr = 0x8000;
    cpu.rw = true;
    cpu.sync = true;
    cpu.data_in = opcode;
    cpu.irq = false;
    cpu.nmi = false;
    cpu.rdy = true;
    cpu.halted = false;

    // Provide a small ROM image so the CPU can fetch operand bytes
    // for the few RMW opcodes that might end up here. Trailing NOPs
    // ensure any speculative fetch returns $EA.
    let mut mem = [0u8; 65536];
    mem[0x8000] = opcode;
    mem[0x8001] = 0xEA;
    mem[0x8002] = 0xEA;

    // Tick 2 CPU cycles (the implied instruction completes in 2).
    // The bus model: each tick, present the byte at `addr` as
    // `data_in` if `rw` is true.
    for _ in 0..2 {
        cpu.data_in = mem[cpu.addr as usize];
        cpu.tick();
    }

    Post {
        a: cpu.regs.a,
        x: cpu.regs.x,
        y: cpu.regs.y,
        sp: cpu.regs.sp,
        p: cpu.regs.p,
    }
}

struct Post {
    a: u8,
    x: u8,
    y: u8,
    sp: u8,
    p: u8,
}

/// Run blargg's exact iteration order for one implied opcode and
/// return the final running CRC.
fn opcode_crc(opcode: u8) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for in_p in [0x00u8, 0xFFu8] {
        for outer_y in (0..=7u8).rev() {
            let in_a = VALUES[outer_y as usize];
            let in_x = VALUES[(outer_y + 1) as usize];
            let in_y = VALUES[(outer_y + 2) as usize];
            for inner_y in (0..=7u8).rev() {
                // operand memory byte = VALUES[inner_y]. set_paxyso
                // overwrites it with `lda values,y; sta operand`
                // where CPU Y register at that moment IS inner_y
                // (set_paxyso runs after test_normal's `sty a_idx`
                // and inherits the same y). The implied opcode
                // doesn't touch memory, so check_paxyso reads
                // VALUES[inner_y].
                let operand_for_crc = VALUES[inner_y as usize];

                let post = run_one_implied(opcode, in_a, in_x, in_y, in_p, 0x90);
                // PHP pushes P with bits 4-5 always set; check_paxyso
                // CLDs after PHP but before pulling, then CRCs the
                // pulled value. So the CRC byte for P is
                // `(post.p & 0xCF) | 0x30` after also clearing D
                // (clearing the D bit via cld is post-PHP but pre-PLA;
                // however the pushed value on the stack still has the
                // D bit as it was at PHP time). Wait — re-read
                // `check_paxyso`:
                //
                //     php
                //     cld
                //     jsr update_crc_fast   ; A
                //     pla
                //     jsr update_crc_fast   ; the pulled P
                //
                // PHP pushes (live_P | $30). CLD clears D in live P,
                // but the stack copy keeps post-instruction D.
                // PLA pulls that stack byte into A. So the CRC byte
                // for P is `(post.p | 0x30)` — D not masked.
                let p_crc = post.p | 0x30;
                crc = update_crc(crc, post.a);
                crc = update_crc(crc, p_crc);
                crc = update_crc(crc, post.x);
                crc = update_crc(crc, post.y);
                crc = update_crc(crc, post.sp);
                crc = update_crc(crc, operand_for_crc);
            }
        }
    }
    crc
}

/// One row of the cpu_test5/01-implied expected CRC table.
struct ExpectedRow {
    opcode: u8,
    name: &'static str,
    /// The `.dword` value as written in the source.
    dword: u32,
}

const EXPECTED: &[ExpectedRow] = &[
    ExpectedRow {
        opcode: 0x2A,
        name: "ROL A",
        dword: 0x013A2933,
    },
    ExpectedRow {
        opcode: 0x0A,
        name: "ASL A",
        dword: 0xA38733B0,
    },
    ExpectedRow {
        opcode: 0x6A,
        name: "ROR A",
        dword: 0x6EC2BCA6,
    },
    ExpectedRow {
        opcode: 0x4A,
        name: "LSR A",
        dword: 0x763FEBC5,
    },
    ExpectedRow {
        opcode: 0x8A,
        name: "TXA",
        dword: 0x0FF1C1E6,
    },
    ExpectedRow {
        opcode: 0x98,
        name: "TYA",
        dword: 0x5B2EB5B7,
    },
    ExpectedRow {
        opcode: 0xAA,
        name: "TAX",
        dword: 0x1D8ACEF5,
    },
    ExpectedRow {
        opcode: 0xA8,
        name: "TAY",
        dword: 0x83DC03F9,
    },
    ExpectedRow {
        opcode: 0xE8,
        name: "INX",
        dword: 0x8EBDF63B,
    },
    ExpectedRow {
        opcode: 0xC8,
        name: "INY",
        dword: 0xF34CAA18,
    },
    ExpectedRow {
        opcode: 0xCA,
        name: "DEX",
        dword: 0x9123FF08,
    },
    ExpectedRow {
        opcode: 0x88,
        name: "DEY",
        dword: 0x48897445,
    },
    ExpectedRow {
        opcode: 0x38,
        name: "SEC",
        dword: 0x4BE14840,
    },
    ExpectedRow {
        opcode: 0x18,
        name: "CLC",
        dword: 0xE7C7ECC0,
    },
    ExpectedRow {
        opcode: 0xF8,
        name: "SED",
        dword: 0x408EF097,
    },
    ExpectedRow {
        opcode: 0xD8,
        name: "CLD",
        dword: 0xA6AEF749,
    },
    ExpectedRow {
        opcode: 0x78,
        name: "SEI",
        dword: 0x8F06AD7B,
    },
    ExpectedRow {
        opcode: 0x58,
        name: "CLI",
        dword: 0xFC96AE14,
    },
    ExpectedRow {
        opcode: 0xB8,
        name: "CLV",
        dword: 0x28F10ADA,
    },
    ExpectedRow {
        opcode: 0xEA,
        name: "NOP",
        dword: 0xCA7E6620,
    },
];

#[test]
#[ignore = "diagnostic; needs the blargg cpu_test5 expected-CRC table baked in"]
fn print_crc_per_opcode() {
    println!("Opcode  Name    Got CRC    Expected (.dword byte-rev)  Match?");
    for row in EXPECTED {
        let got = opcode_crc(row.opcode);
        let expected = row.dword.swap_bytes();
        let m = if got == expected { "✓" } else { "✗" };
        println!(
            "  ${:02X}    {:<5}   ${:08X}   ${:08X}                 {}",
            row.opcode, row.name, got, expected, m
        );
    }
}

#[test]
#[ignore = "diagnostic; dumps the first few CRC byte tuples for NOP"]
fn dump_nop_byte_stream() {
    let mut crc: u32 = 0xFFFFFFFF;
    let mut count = 0;
    println!("Dumping first 8 iterations of NOP's CRC byte stream:");
    println!("  in_p outer inner | A_post P_crc X_post Y_post S_post operand | running_crc");
    for in_p in [0x00u8, 0xFFu8] {
        for outer_y in (0..=7u8).rev() {
            let in_a = VALUES[outer_y as usize];
            let in_x = VALUES[(outer_y + 1) as usize];
            let in_y = VALUES[(outer_y + 2) as usize];
            for inner_y in (0..=7u8).rev() {
                let operand_for_crc = VALUES[inner_y as usize];
                let post = run_one_implied(0xEA, in_a, in_x, in_y, in_p, 0x90);
                let p_crc = post.p | 0x30;
                for b in [post.a, p_crc, post.x, post.y, post.sp, operand_for_crc] {
                    crc = update_crc(crc, b);
                }
                if count < 8 {
                    println!(
                        "  ${:02X}  {:>4}  {:>4}  | ${:02X}    ${:02X}   ${:02X}    ${:02X}    ${:02X}    ${:02X}      | ${:08X}",
                        in_p,
                        outer_y,
                        inner_y,
                        post.a,
                        p_crc,
                        post.x,
                        post.y,
                        post.sp,
                        operand_for_crc,
                        crc
                    );
                }
                count += 1;
            }
        }
    }
    println!("\nFinal NOP CRC: ${:08X}", crc);
    println!("Expected:      ${:08X}", 0xCA7E6620u32.swap_bytes());
}

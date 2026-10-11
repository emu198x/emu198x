//! Exercise the real ROM, I/O and IM2 path. MEMU's interrupt setup uses
//! CTC channel 0 with $C5 (falling edge, counter mode), count 1.
//! MAME's MTX driver inverts its asserted VDP callback before CTC TRG0.
use common_z80_machine::Z80Machine;
use machine_memotech_mtx::{Mtx, MtxModel};

fn guest(vector: u8, rising_edge: bool) -> Vec<u8> {
    let mut rom = vec![0; 0x4000];
    let setup = [
        0xF3, // DI
        0x31,
        0x00,
        0xF0, // LD SP,$F000
        0x3E,
        0xC0,
        0xED,
        0x47, // LD A,$C0; LD I,A
        0x21,
        0x00,
        0x01, // LD HL,$0100 (ISR)
        0x22,
        vector,
        0xC0, // LD ($C000+vector),HL
        0x3E,
        vector,
        0xD3,
        0x08, // CTC vector
        0x3E,
        if rising_edge { 0xD5 } else { 0xC5 },
        0xD3,
        0x08,
        0x3E,
        0x01,
        0xD3,
        0x08, // Count 1
        0x3E,
        0x20,
        0xD3,
        0x02, // VDP R1: enable IRQ
        0x3E,
        0x81,
        0xD3,
        0x02,
        0xED,
        0x5E, // IM 2
        0xFB, // EI
        0x76, // HALT until frame IRQ
        0x18,
        0xFD, // JR back to HALT
    ];
    rom[..setup.len()].copy_from_slice(&setup);
    let isr = [
        0xDB, 0x02, // IN A,($02): clear VDP /INT
        0x21, 0x00, 0xC1, // LD HL,$C100
        0x34, // INC (HL): observable ISR count
        0xFB, 0xED, 0x4D, // EI; RETI
    ];
    rom[0x100..0x100 + isr.len()].copy_from_slice(&isr);
    rom
}

#[test]
fn falling_vdp_assertions_reach_im2_and_reti_rearms_the_next_frame() {
    for model in [MtxModel::Mtx500, MtxModel::Mtx512] {
        for vector in [0x00, 0x20, 0x78, 0xF8] {
            let mut mtx = Mtx::new(guest(vector, false), model).expect("guest ROM");
            let mut assertions = 0;
            let mut was_asserted = false;
            for _ in 0..180_000 {
                mtx.advance_tstates(1);
                let asserted = mtx.vdp().interrupt;
                if asserted && !was_asserted {
                    assertions += 1;
                }
                was_asserted = asserted;
            }
            let services = mtx.peek_memory(0xC100);
            eprintln!(
                "model={model:?} vector={vector:02x} assertions={assertions} services={services}"
            );
            assert!(
                assertions >= 2,
                "the guest must receive successive VDP assertions"
            );
            assert_eq!(
                u32::from(services),
                assertions,
                "every frame must reach the IM2 handler"
            );
            assert!(!mtx.ctc().interrupt(), "acknowledge must clear the request");
            // A lower-priority request can pass only after the real CPU's
            // RETI has released channel 0. Repeated channel-0 service alone
            // does not prove this because a same-channel request can recur.
            let saved = postcard::to_allocvec(mtx.ctc()).expect("serialize live CTC");
            let mut ctc: zilog_z80_ctc::Ctc =
                postcard::from_bytes(&saved).expect("restore live CTC");
            ctc.write(1, 0xD5);
            ctc.write(1, 1);
            ctc.set_trg(1, true);
            ctc.tick();
            assert!(ctc.interrupt(), "RETI must release lower-priority channels");
            assert_eq!(ctc.acknowledge(), vector | 2);
        }
    }
}

#[test]
fn rising_edge_counter_does_not_interrupt_on_vdp_assertion() {
    let mut mtx = Mtx::new(guest(0x20, true), MtxModel::Mtx512).expect("guest ROM");
    for _ in 0..80_000 {
        mtx.advance_tstates(1);
    }
    assert!(
        mtx.vdp().interrupt,
        "VDP must assert and stay asserted while CPU waits"
    );
    assert_eq!(
        mtx.peek_memory(0xC100),
        0,
        "the wrong edge must not enter the ISR"
    );
    assert!(!mtx.ctc().interrupt());
}

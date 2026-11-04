//! Regression: the +3 must keep clocking refresh cycles in all-RAM mode.
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::timing::TIMING_PLUS2A;
use common_sinclair_zx_spectrum_amstrad_class::{Plus3Marker, SpectrumAmstradClassCore};
use std::collections::BTreeMap;

#[test]
fn uncontended_code_is_not_delayed_by_the_refresh_bank() {
    for refresh_i in [0xc0_u8, 0x00] {
        let mut m = SpectrumAmstradClassCore::<Plus3Marker>::new();
        // All-RAM mode 2: physical banks [4,5,6,3]. Code lives in
        // uncontended bank 3; only the refresh address changes bank.
        m.memory.write_1ffd(0x05);
        m.memory.ram_bank_mut(3)[0x3f00..0x3f04].copy_from_slice(&[0x00, 0xc3, 0x00, 0xff]);
        m.z80.regs.pc = 0xff00;
        m.z80.regs.i = refresh_i;
        let mut previous = None;
        let mut was_fetch = false;
        let mut intervals = BTreeMap::<u32, usize>::new();
        for tick in 0..TIMING_PLUS2A.halfcycles_per_frame {
            m.tick_one_halfcycle();
            let fetch = m.z80.m1 && m.z80.mreq && m.z80.addr == 0xff00;
            if fetch && !was_fetch {
                if let Some(last) = previous {
                    *intervals.entry(tick - last).or_default() += 1;
                }
                previous = Some(tick);
            }
            was_fetch = fetch;
        }
        eprintln!("refresh I={refresh_i:02x} master-clock intervals={intervals:?}");
        assert!(intervals.values().sum::<usize>() > 100);
        // NOP 4T + JP nn 10T; every instruction/operand fetch is bank 3.
        // The Z80 does not sample WAIT during its M1 refresh pulse.
        assert_eq!(
            intervals.keys().copied().collect::<Vec<_>>(),
            vec![14 * TIMING_PLUS2A.cpu_divisor]
        );
    }
}

fn memory_program(arrival: u32, target: u16) -> (u32, usize, usize) {
    use emu198x_zilog_z80::{
        Z80,
        z80::{M1Phase, MemPhase, Phase},
    };
    let mut m = SpectrumAmstradClassCore::<Plus3Marker>::new();
    // Reach the active display with the ULA and master clock still aligned.
    for _ in 0..80 + arrival * TIMING_PLUS2A.cpu_divisor {
        m.tick_one_halfcycle();
    }
    m.z80 = Z80::new();
    m.memory.write_1ffd(5);
    m.memory.ram_bank_mut(3)[0x3f00..0x3f07].copy_from_slice(&[
        0x21,
        target as u8,
        (target >> 8) as u8,
        0x7e,
        0x77,
        0x76,
        0x00,
    ]);
    m.z80.regs.pc = 0xff00;
    m.z80.regs.i = 0xc0; // Refresh and all instruction bytes are uncontended.
    let mut edges = 0;
    let mut waits = 0;
    for elapsed in 1..=2000 {
        let edge = matches!(m.hc() % 5, 0 | 2);
        let phase = m.z80.phase;
        m.tick_one_halfcycle();
        if edge {
            edges += 1;
            if m.z80.wait
                && matches!(
                    phase,
                    Phase::M1(M1Phase::T2Fall)
                        | Phase::MemRead(MemPhase::T2Fall)
                        | Phase::MemWrite(MemPhase::T2Fall)
                )
            {
                waits += 1;
            }
        }
        if m.z80.halt {
            return (elapsed, edges, waits);
        }
    }
    panic!("bounded memory program did not halt");
}

#[test]
fn physical_wait_adds_complete_tstates_on_the_machine_clock() {
    let mut exercised = 0;
    for arrival in 0..8 {
        let (base_ticks, base_edges, base_waits) = memory_program(arrival, 0xf000);
        let (ticks, edges, waits) = memory_program(arrival, 0x4000);
        assert_eq!(base_waits, 0);
        assert_eq!(
            edges,
            base_edges + 2 * waits,
            "arrival {arrival}: full TW pairs"
        );
        assert_eq!(
            ticks,
            base_ticks + 5 * waits as u32,
            "arrival {arrival}: native master clock"
        );
        exercised += waits;
    }
    assert!(
        exercised > 8,
        "exercise both contended data cycles across phases"
    );
}

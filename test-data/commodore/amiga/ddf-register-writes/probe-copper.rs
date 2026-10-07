//! Connected Copper writes: retain the real admission, memory and service stages.
use common_commodore_amiga::driver::AmigaDriver;
use machine_commodore_amiga_ecs::AmigaEcs;
use motorola_68000::cpu::State;
use std::error::Error;

#[cfg(not(test))]
fn main() -> Result<(), Box<dyn Error>> {
    verify()
}

pub fn verify() -> Result<(), Box<dyn Error>> {
    let mut failures = 0;
    for (name, register, start, stop, replacement) in [
        ("start_same", 0x92, 64, 208, 64),
        ("stop_extend", 0x94, 56, 64, 128),
        ("stop_same", 0x94, 56, 64, 64),
    ] {
        let mut rom = vec![0; 512 * 1024];
        rom[..12].copy_from_slice(&[0, 4, 0, 0, 0, 0xf8, 0, 8, 0x4e, 0x72, 0x27, 0]);
        let mut machine = AmigaEcs::new(rom);
        machine.agnus_mut().vpos = 32;
        machine.agnus_mut().bplcon0 = 0x1000;
        machine.poke_word(0xdff08e, 0x2010);
        machine.poke_word(0xdff090, 0xa020);
        machine.poke_word(0xdff092, start);
        machine.poke_word(0xdff094, stop);
        machine.poke_word(0xdff0e0, 0);
        machine.poke_word(0xdff0e2, 0x2000);
        for instruction in 0..15 {
            machine.poke_word(0x1000 + instruction * 4, 0x0180);
            machine.poke_word(0x1002 + instruction * 4, 0);
        }
        machine.poke_word(0x103c, register);
        machine.poke_word(0x103e, replacement);
        machine.poke_word(0x1040, 0xffff);
        machine.poke_word(0x1042, 0xfffe);
        machine.copper_mut().cop1lc = 0x1000;
        machine.copper_mut().jump1();
        machine.poke_word(0xdff096, 0x8380);
        let mut previous_h = None;
        let mut previous_pointer = machine.agnus().bpl_pt[0];
        let mut requests = Vec::new();
        let mut services = Vec::new();
        for _ in 0..400 {
            machine.tick();
            let agnus = machine.agnus();
            if agnus.hpos >= 64 && !matches!(machine.cpu().state, State::Stopped) {
                return Err("CPU did not remain stopped during the measured write".into());
            }
            if previous_h != Some(agnus.hpos) {
                if agnus.dma_pipeline().reservation().is_some() {
                    requests.push(agnus.hpos);
                }
                if agnus.bpl_pt[0] != previous_pointer {
                    services.push((agnus.hpos, agnus.bpl_pt[0]));
                }
                previous_pointer = agnus.bpl_pt[0];
                previous_h = Some(agnus.hpos);
            }
        }
        let writes: Vec<_> = machine
            .debug_copper_move_log
            .iter()
            .filter(|row| row.3 == register)
            .collect();
        println!("{name}: writes={writes:?} requests={requests:?} services={services:?}");
        if writes.len() != 1 || writes[0].2 != 64 || writes[0].4 != replacement {
            return Err("the targeted Copper write did not retire at h=64".into());
        }
        match name {
            "start_same" if !requests.is_empty() => {
                failures += 1;
                eprintln!(
                    "FAIL: same-value DDFSTRT write at its comparator must suppress this line's start"
                );
            }
            "stop_extend" | "stop_same"
                if requests != [65, 73] || services != [(67, 0x2002), (75, 0x2004)] =>
            {
                failures += 1;
                eprintln!("FAIL: old DDFSTOP match must retain the terminal request at h=73");
            }
            _ => {}
        }
    }
    if failures > 0 {
        return Err(format!("{failures} connected DDF write cases failed").into());
    }
    Ok(())
}

//! Original ROM-slot guests exercise the manufacturer's I/O wiring through Z80 pins.
//!
//! Sord BASIC-G Appendix H lists the canonical ports. The M5C-01 schematic's
//! GA015 has no A0-A3 inputs; CTC CS1/CS0 use A1/A0 and VDP MODE uses A0.
//! MAME src/mame/sord/m5.cpp m5_io independently applies the corresponding
//! 0x0c/0x0e/0x0f mirrors. Primary scan provenance:
//! https://groups.google.com/g/rc2014-z80/c/rhZaulz_lzo

use common_sord_m5::M5Region;
use machine_sord_m5_ntsc::SordM5;

fn output(program: &mut Vec<u8>, port: u8, value: u8) {
    program.extend([0x3E, value, 0xD3, port]); // LD A,n; OUT (n),A
}

fn input(program: &mut Vec<u8>, port: u8, destination: u16) {
    let [lo, hi] = destination.to_le_bytes();
    program.extend([0xDB, port, 0x32, lo, hi]); // IN A,(n); LD (nn),A
}

fn guest(mut program: Vec<u8>) -> SordM5 {
    assert!(program.len() < 0x2000);
    program.push(0x76); // HALT after the measured program
    program.resize(0x2000, 0);
    SordM5::new(program, Vec::new())
}

fn run(program: Vec<u8>) -> SordM5 {
    let done = u16::try_from(program.len()).expect("guest fits monitor ROM");
    let mut machine = guest(program);
    assert!(machine.run_until_pc(done, 200_000).1, "guest must finish");
    machine
}

#[test]
fn ctc_counter_writes_and_reads_reach_all_sixteen_ports() {
    let mut failures = Vec::new();
    let mut cases = 0;
    {
        let region = M5Region::Ntsc;
        for write_port in 0..16 {
            let channel = write_port & 3;
            let constant = 0x41 + channel;
            let mut code = vec![0xF3]; // DI
            // Counter mode, constant follows, reset. No trigger edges occur
            // before these reads, so this checks channel selection, not time.
            output(&mut code, write_port, 0x47);
            output(&mut code, write_port, constant);
            for mirror in 0..4 {
                input(&mut code, channel + mirror * 4, 0x7100 + u16::from(mirror));
            }
            let machine = run(code);
            for mirror in 0..4 {
                let actual = machine.peek(0x7100 + mirror);
                cases += 1;
                if actual != constant {
                    failures.push((region, write_port, mirror, actual, constant));
                }
            }
        }
    }
    assert_eq!(cases, 16 * 4);
    assert!(failures.is_empty(), "CTC alias failures: {failures:?}");
}

#[test]
fn vdp_data_and_control_aliases_share_vram_and_read_buffer() {
    let mut failures = Vec::new();
    let mut cases = 0;
    {
        let region = M5Region::Ntsc;
        for data_port in (0x10..0x20).step_by(2) {
            for control_port in (0x11..0x20).step_by(2) {
                let mut code = vec![0xF3];
                output(&mut code, control_port, 0x23);
                output(&mut code, control_port, 0x41); // write VRAM $0123
                output(&mut code, data_port, 0x5A);
                output(&mut code, data_port, 0xA5);
                output(&mut code, 0x11, 0x23);
                output(&mut code, 0x11, 0x01); // read buffer prefetch $0123
                input(&mut code, data_port, 0x7100);
                input(&mut code, 0x10, 0x7101); // same auto-incrementing port
                let machine = run(code);
                let actual = [machine.peek(0x7100), machine.peek(0x7101)];
                cases += 1;
                if actual != [0x5A, 0xA5] {
                    failures.push((region, data_port, control_port, actual));
                }
            }
        }
    }
    assert_eq!(cases, 8 * 8);
    assert!(failures.is_empty(), "VDP alias failures: {failures:?}");
}

#[test]
fn vdp_status_aliases_observe_and_clear_vblank() {
    {
        let region = M5Region::Ntsc;
        let mut canonical_tstates = None;
        for port in (0x11..0x20).step_by(2) {
            let mut code = vec![0xF3];
            let poll = u16::try_from(code.len()).expect("poll address");
            input(&mut code, port, 0x7100);
            code.extend([0xE6, 0x80]); // AND $80
            let [lo, hi] = poll.to_le_bytes();
            code.extend([0xCA, lo, hi]); // JP Z,poll
            input(&mut code, 0x11, 0x7101);
            let machine = run(code);
            let expected_tstates = *canonical_tstates.get_or_insert(machine.cpu_tstates());
            assert!(
                expected_tstates > 1_000,
                "canonical poll must wait for VBlank"
            );
            assert_eq!(
                machine.cpu_tstates(),
                expected_tstates,
                "{region:?} {port:02x}: an unmapped $FF must not pass as VBlank"
            );
            assert_ne!(machine.peek(0x7100) & 0x80, 0, "{region:?} {port:02x}");
            assert_eq!(
                machine.peek(0x7101) & 0x80,
                0,
                "status must clear VBlank: {region:?} {port:02x}"
            );
        }
    }
}

fn tone(port: u8) -> Vec<f32> {
    let mut code = vec![0xF3];
    for byte in [0x84, 0x12, 0x90] {
        // tone 0 period $124, volume 0
        output(&mut code, port, byte);
    }
    let mut machine = guest(code);
    machine.run_frame();
    machine.take_audio_buffer()
}

#[test]
fn sound_aliases_produce_the_same_non_silent_waveform() {
    let mut failures = Vec::new();
    {
        let region = M5Region::Ntsc;
        let canonical = tone(0x20);
        assert!(canonical.len() > 100);
        assert!(canonical.iter().any(|sample| sample.abs() > 0.01));
        assert!(canonical.windows(2).any(|pair| pair[0] != pair[1]));
        for port in 0x20..0x30 {
            if tone(port) != canonical {
                failures.push((region, port));
            }
        }
    }
    assert!(failures.is_empty(), "PSG alias failures: {failures:?}");
}

#[test]
fn keyboard_joystick_and_unmapped_reads_keep_their_own_decode() {
    let rows = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40];
    {
        let region = M5Region::Ntsc;
        let mut code = vec![0xF3];
        for port in 0x20u8..=0xFF {
            input(&mut code, port, 0x7100 + u16::from(port));
        }
        let done = u16::try_from(code.len()).expect("guest address");
        let mut machine = guest(code);
        *machine.key_matrix_mut() = rows;
        machine.set_joystick(1, true, false, false, true); // up/right = $03
        machine.set_joystick(2, false, true, true, false); // down/left = $C0
        assert!(machine.run_until_pc(done, 200_000).1);
        for port in 0x20u8..=0xFF {
            let expected = match port {
                0x30..=0x36 => rows[usize::from(port - 0x30)],
                0x38..=0x3E => rows[usize::from(port - 0x38)],
                0x37 | 0x3F => 0xC3,
                _ => 0xFF,
            };
            assert_eq!(
                machine.peek(0x7100 + u16::from(port)),
                expected,
                "{region:?} {port:02x}"
            );
        }
    }
}

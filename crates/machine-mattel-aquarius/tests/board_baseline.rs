//! Complete original-engine state/output fixtures; no manufacturer firmware.
use emu198x_z80_cadence::Z80Machine;
use machine_mattel_aquarius::{Aquarius, AquariusRegion};
use std::path::PathBuf;

fn specimen(region: AquariusRegion) -> Aquarius {
    let mut rom = vec![0; 8192];
    // Paint a cell, scan keyboard row zero, toggle the speaker continuously.
    let code = [
        0x3e, 0x66, 0x32, 0x00, 0x34, 0x3e, 0x20, 0x32, 0x00, 0x30, 0x01, 0xff, 0xfe, 0xed, 0x78,
        0x32, 0x00, 0x38, 0x3e, 0x00, 0xee, 0x01, 0xd3, 0xfc, 0xc3, 0x14, 0x00,
    ];
    rom[..code.len()].copy_from_slice(&code);
    let mut m = Aquarius::new(rom, 16, region);
    m.set_char_rom(vec![0xff; 2048]);
    m.insert_cart((0..8192).map(|i| (i * 17) as u8).collect());
    m.set_key(0, 2, true);
    m.set_joystick(1, true, false, false, false, true);
    m.poke(0x4000, 0x5a);
    m.poke(0x7fff, 0xa5);
    m.run_native_frame();
    m.advance_tstates(137);
    m.take_audio_buffer();
    m
}
fn output(m: &mut Aquarius) -> Vec<u8> {
    let mut frames = Vec::new();
    for _ in 0..3 {
        let (ticks, complete) = m.run_native_frame();
        assert!(complete);
        let audio: Vec<_> = m
            .take_audio_buffer()
            .into_iter()
            .map(f32::to_bits)
            .collect();
        frames.push((
            ticks,
            m.cpu_tstates(),
            m.frame_count(),
            m.framebuffer().to_vec(),
            audio,
        ));
    }
    postcard::to_allocvec(&(frames, postcard::to_allocvec(m).expect("state"))).expect("output")
}
#[test]
fn original_boards_keep_complete_state_and_continued_output() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/mattel-aquarius/board-split-v4");
    for (name, region) in [("ntsc", AquariusRegion::Ntsc), ("pal", AquariusRegion::Pal)] {
        let mut m = specimen(region);
        let state = postcard::to_allocvec(&m).expect("state");
        let out = output(&mut m);
        if let Some(dir) = std::env::var_os("EMU198X_AQUARIUS_CAPTURE") {
            let dir = PathBuf::from(dir);
            std::fs::create_dir_all(&dir).expect("capture directory");
            for (suffix, bytes) in [("state", &state), ("output", &out)] {
                let p = dir.join(format!("{name}.{suffix}"));
                use std::io::Write;
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(p)
                    .expect("new capture only")
                    .write_all(bytes)
                    .expect("capture");
            }
            continue;
        }
        let saved = std::fs::read(root.join(format!("{name}.state"))).expect("frozen state");
        assert_eq!(state, saved, "{name} complete original state");
        let expected = std::fs::read(root.join(format!("{name}.output"))).expect("frozen output");
        assert!(out == expected, "{name} original pixels/clocks/audio/state");
        let mut restored: Aquarius = postcard::from_bytes(&saved).expect("original state decodes");
        restored.cpu_mut().rehydrate_walker_sequence();
        assert_eq!(
            output(&mut restored),
            expected,
            "{name} restored continuation"
        );
    }
}

//! PAL timing and boot coverage. Reference: MAME's common M5 clock and
//! European monitor `sordint.ic21`, SHA-1 ac042c4ae8272ad6abe09ae83492ef9a0026d0b2.
use common_z80_machine::Z80Machine;
use machine_sord_m5::{M5Region, SordM5};
use std::{fs, path::PathBuf};

fn fixture_root() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("set HOME for local firmware")).join(".emu198x")
}

fn pal_boot(cart_name: &str, expected_text: &[u8]) -> SordM5 {
    let root = fixture_root();
    let bios_path = std::env::var_os("EMU198X_SORD_M5_PAL_BIOS")
        .map_or_else(|| root.join("roms/sord-m5/sord-m5.rom"), PathBuf::from);
    let bios = fs::read(bios_path).expect("unpatched European M5 monitor ROM");
    assert_eq!(bios.len(), 8192);
    // FNV fingerprint is a fixture guard; the full SHA-1 is pinned above.
    let fingerprint = bios.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100_0000_01b3)
    });
    assert_eq!(
        fingerprint, BIOS_FINGERPRINT,
        "expected unpatched European BIOS"
    );
    let cart = fs::read(root.join(format!("media/sord-m5/{cart_name}.bin")))
        .expect("install the named cartridge fixture");
    let mut machine = SordM5::new(bios, cart, M5Region::Pal);
    for _ in 0..600 {
        machine.run_frame();
        machine.take_audio_buffer();
    }
    let before = machine.irq_acks_by_channel()[3];
    let clock = machine.cpu_tstates();
    for _ in 0..100 {
        assert_eq!(machine.run_frame(), 71_364);
        machine.take_audio_buffer();
    }
    assert_eq!(machine.cpu_tstates() - clock, 7_136_400);
    assert_eq!(machine.irq_acks_by_channel()[3] - before, 100);
    assert_eq!(machine.vdp().scanline(), 0);
    assert_eq!(
        (machine.framebuffer_width(), machine.framebuffer_height()),
        (278, 288)
    );
    let base = usize::from(machine.vdp().registers()[2] & 0x0f) * 1024;
    let names = &machine.vdp().vram()[base..base + 768];
    assert!(
        names
            .windows(expected_text.len())
            .any(|s| s == expected_text),
        "PAL cartridge must reach its expected prompt/title"
    );
    assert!(machine.framebuffer().iter().any(|p| p & 0x00ff_ffff != 0));
    println!(
        "PAL cart={cart_name} frames=700 measured_ticks=7136400 ch3_acks=100 prompt_verified=true"
    );
    machine
}

const BIOS_FINGERPRINT: u64 = 0x43db218c3eb58913;

#[test]
#[ignore = "FIXTURE: needs European M5 BIOS and BASIC-I cartridge"]
fn european_basic_i_reaches_ready_with_pal_frame_interrupts() {
    pal_boot("basic-i", b"Ready");
}

#[test]
#[ignore = "FIXTURE: needs European M5 BIOS and Dig Dug cartridge"]
fn european_dig_dug_reaches_player_selection_with_pal_frame_interrupts() {
    // Tile zero is the cartridge's blank character, not ASCII space.
    let mut machine = pal_boot("dig-dug", b"1\0PLAYER");
    machine.press_key(1, 0); // "1" starts a one-player game.
    for _ in 0..6 {
        machine.run_frame();
        machine.take_audio_buffer();
    }
    machine.release_key(1, 0);
    for _ in 0..200 {
        machine.run_frame();
        machine.take_audio_buffer();
    }
    let before = machine.irq_acks_by_channel()[3];
    let mut positions = std::collections::BTreeSet::new();
    // The observed round introduction keeps sprites stationary beyond 300
    // frames; sample across it and the following active play, with a finite cap.
    for _ in 0..1000 {
        machine.run_frame();
        machine.take_audio_buffer();
        let base = machine.vdp().sprite_attr_table_addr();
        positions.insert(machine.vdp().vram()[base..base + 16].to_vec());
    }
    assert_eq!(machine.irq_acks_by_channel()[3] - before, 1000);
    assert!(
        positions.len() > 1,
        "PAL game sprites must advance after starting"
    );
    println!(
        "PAL Dig Dug gameplay: 1000 frames, 1000 channel-3 interrupts, {} distinct sprite tables",
        positions.len()
    );
}

#[test]
fn each_runtime_second_produces_48000_audio_samples() {
    for region in [M5Region::Ntsc, M5Region::Pal] {
        let mut rom = vec![0; 8192];
        rom[..2].copy_from_slice(&[0xf3, 0x76]); // DI; HALT
        let mut machine = SordM5::new(rom, vec![], region);
        // Both runtime profiles and MAME's common CPU/PSG configuration use
        // 3,579,545 Hz. A region-dependent PSG denominator overproduced PAL
        // audio, stretching a runtime second into 48,441 host samples.
        machine.advance_tstates(3_579_545);
        assert_eq!(machine.take_audio_buffer().len(), 48_000, "{region:?}");
    }
}

#[test]
fn pal_audio_continues_identically_after_save_restore() {
    let mut rom = vec![0; 8192];
    // DI; LD A,$85; OUT ($20),A; LD A,$12; OUT ($20),A;
    // LD A,$90; OUT ($20),A; HALT. Program an audible tone via CPU pins.
    rom[..14].copy_from_slice(&[
        0xf3, 0x3e, 0x85, 0xd3, 0x20, 0x3e, 0x12, 0xd3, 0x20, 0x3e, 0x90, 0xd3, 0x20, 0x76,
    ]);
    let mut original = SordM5::new(rom, vec![], M5Region::Pal);
    original.advance_tstates(12_347); // Inside a frame and audio sample interval.
    original.take_audio_buffer();
    let saved = postcard::to_allocvec(&original).expect("save PAL machine");
    let mut restored: SordM5 = postcard::from_bytes(&saved).expect("restore PAL machine");
    restored.cpu_mut().rehydrate_walker_sequence();
    original.advance_tstates(3_579_545);
    restored.advance_tstates(3_579_545);
    let expected = original.take_audio_buffer();
    let actual = restored.take_audio_buffer();
    assert_eq!(expected.len(), 48_000);
    assert!(
        expected.iter().any(|sample| *sample != 0.0),
        "tone must be audible"
    );
    assert_eq!(actual, expected);
    assert_eq!(restored.cpu_tstates(), original.cpu_tstates());
}

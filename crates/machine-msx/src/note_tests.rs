//! Notes played through the whole path: registers written as software
//! writes them, the chips stepped by the machine, and the pitch read back
//! from the mixed audio the host receives.

use super::*;

/// A BIOS that loops at `$0008` with interrupts off, leaving the bus quiet.
fn bios() -> Vec<u8> {
    let mut rom = vec![0u8; 32768];
    rom[0x0008] = 0x18;
    rom[0x0009] = 0xFE;
    rom
}

/// An NTSC MSX1 with its PPI set as the BIOS sets it and page 2 in
/// cartridge slot 1.
fn machine() -> Msx {
    let mut sys = Msx::new(bios(), MsxRegion::Ntsc);
    sys.ppi.write(3, 0x82);
    sys.ppi.write(0, 0b0001_0100); // pages 1 and 2 -> slot 1
    sys
}

/// Let the chips run, as a program's delay loop would.
fn wait(sys: &mut Msx, tstates: u32) {
    for _ in 0..tstates {
        sys.tick_tstate();
    }
}

/// Write a YM2413 register through ports `$7C`/`$7D`, leaving the waits the
/// chip needs after each write.
fn opll(sys: &mut Msx, register: u8, value: u8) {
    sys.io_write(0x7C, register);
    wait(sys, 12);
    sys.io_write(0x7D, value);
    wait(sys, 84);
}

/// A sine-like user instrument and a 440 Hz note on channel 0: F-number
/// 290 in block 4, `290 × (3,579,545 / 72) / 2^15` Hz.
fn play_a440(sys: &mut Msx) {
    // Modulator silenced (TL 63), so the carrier sounds alone.
    for (register, value) in [
        (0x00, 0x21),
        (0x01, 0x21),
        (0x02, 0x3F),
        (0x03, 0x00),
        (0x04, 0xF0),
        (0x05, 0xF0),
        (0x06, 0x0F),
        (0x07, 0x0F),
        (0x30, 0x00), // user instrument, loudest
        (0x10, 0x22),
        (0x20, 0x19), // key on, block 4, F-number bit 8
    ] {
        opll(sys, register, value);
    }
}

/// Run `frames` frames, collecting the audio the host would receive.
fn record(sys: &mut Msx, frames: u32) -> Vec<f32> {
    let mut audio = Vec::new();
    let mut out = vec![0.0; 1024];
    for _ in 0..frames {
        sys.run_frame();
        let count = sys.end_audio_frame(&mut out);
        audio.extend_from_slice(&out[..count]);
    }
    audio
}

/// The pitch, from rising crossings of the mean with hysteresis of half
/// the RMS level, which ignores the odd spike resampling leaves.
fn pitch_hz(audio: &[f32]) -> f64 {
    let mean = audio.iter().sum::<f32>() / audio.len() as f32;
    let rms = (audio.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / audio.len() as f32).sqrt();
    let threshold = rms * 0.5;
    let mut crossings = 0u32;
    let mut low = false;
    for &sample in audio {
        let centred = sample - mean;
        if centred < -threshold {
            low = true;
        } else if centred > threshold && low {
            crossings += 1;
            low = false;
        }
    }
    f64::from(crossings) * 48_000.0 / audio.len() as f64
}

/// The audio's swing, peak to peak.
fn swing(audio: &[f32]) -> f32 {
    let high = audio.iter().copied().fold(f32::MIN, f32::max);
    let low = audio.iter().copied().fold(f32::MAX, f32::min);
    high - low
}

#[test]
fn a_frame_of_audio_is_the_psg_cadence_long() {
    let mut sys = machine();
    let mut out = vec![0.0; 1024];
    sys.run_frame();
    // 29,868 PSG ticks at 48 kHz: 801 whole samples and a partial one.
    assert_eq!(sys.end_audio_frame(&mut out), 802);
}

#[test]
fn msx_music_plays_a_440_hz_note() {
    let mut sys = machine();
    sys.install_msx_music(SlotAddress::primary(0), vec![0; 0x4000]);
    play_a440(&mut sys);
    record(&mut sys, 5); // let the attack settle
    let audio = record(&mut sys, 60);
    let hz = pitch_hz(&audio);
    assert!((hz - 440.0).abs() < 3.0, "MSX-MUSIC played {hz:.1} Hz");
    // One FM channel swings as far as one PSG channel at full volume.
    let level = swing(&audio);
    assert!((0.25..0.45).contains(&level), "a channel swings {level}");
    assert!(sys.msx_music().is_some());
}

#[test]
fn msx_music_maps_its_rom_in_its_own_slot() {
    let mut sys = machine();
    let mut rom = vec![0u8; 0x4000];
    rom[0x18..0x20].copy_from_slice(b"APRLOPLL");
    sys.install_msx_music(SlotAddress::primary(2), rom);
    sys.ppi.write(0, 0b0000_1000); // page 1 -> slot 2
    assert_eq!(sys.peek(0x401C), b'O');
    sys.ppi.write(0, 0b0000_0100); // page 1 -> slot 1, empty
    assert_eq!(sys.peek(0x401C), 0xFF);
}

#[test]
fn the_fm_pac_plays_through_its_ports_once_enabled() {
    let mut sys = machine();
    let mut rom = vec![0u8; 0x10000];
    rom[0x18..0x20].copy_from_slice(b"PAC2OPLL");
    assert_eq!(detect_mapper(&rom), MapperType::FmPac);
    sys.insert_cart1(rom, MapperType::FmPac);

    // The ports do nothing until bit 0 of $7FF6 is set. (The first frames
    // carry the DC block settling on the idle chip's offset.)
    play_a440(&mut sys);
    record(&mut sys, 5);
    let silent = record(&mut sys, 10);

    sys.poke(0x7FF6, 0x01);
    play_a440(&mut sys);
    record(&mut sys, 5);
    let audio = record(&mut sys, 60);
    let hz = pitch_hz(&audio);
    assert!((hz - 440.0).abs() < 3.0, "the FM-PAC played {hz:.1} Hz");
    assert!(
        swing(&silent) < 0.01 && swing(&audio) > 0.2,
        "the ports were closed until enabled: {} then {}",
        swing(&silent),
        swing(&audio)
    );
    assert!(sys.cartridge_ym2413(1).is_some());
}

#[test]
fn the_fm_pac_plays_through_its_memory_mapped_registers() {
    let mut sys = machine();
    sys.insert_cart1(vec![0; 0x10000], MapperType::FmPac);
    // $7FF4/$7FF5 work whatever the enable bit says.
    for (register, value) in [(0x30, 0x10), (0x10, 0x22), (0x20, 0x19)] {
        sys.poke(0x7FF4, register);
        wait(&mut sys, 12);
        sys.poke(0x7FF5, value);
        wait(&mut sys, 84);
    }
    record(&mut sys, 5);
    let audio = record(&mut sys, 30);
    assert!(swing(&audio) > 0.01, "instrument 1 sounds");
}

#[test]
fn the_fm_pac_keeps_pac_data_in_sram() {
    let mut sys = machine();
    sys.insert_cart1(vec![0; 0x10000], MapperType::FmPac);
    sys.poke(0x5FFE, 0x4D);
    sys.poke(0x5FFF, 0x69);
    sys.poke(0x4000, b'P');
    assert_eq!(sys.fm_pac_sram(1).map(|sram| sram[0]), Some(b'P'));
    assert_eq!(sys.fm_pac_sram(1).map(<[u8]>::len), Some(FM_PAC_SRAM_SIZE));
    if let Some(sram) = sys.fm_pac_sram_mut(1) {
        sram[1] = b'A';
    }
    assert_eq!(sys.peek(0x4001), b'A');
    assert!(sys.fm_pac_sram(2).is_none() && sys.fm_pac_sram_mut(3).is_none());
}

/// A square wave: 16 samples high, 16 low.
fn load_square(sys: &mut Msx, base: u16, channel: u16) {
    for i in 0..32 {
        sys.poke(base + channel * 32 + i, if i < 16 { 0x7F } else { 0x80 });
    }
}

#[test]
fn the_scc_plays_a_440_hz_note() {
    let mut sys = machine();
    sys.insert_cart1(vec![0; 128 * 1024], MapperType::KonamiScc);
    sys.poke(0x9000, 0x3F); // open the SCC at $9800
    load_square(&mut sys, 0x9800, 0);
    // Period 253: 3,579,545 / (32 × 254) = 440.4 Hz.
    sys.poke(0x9880, 253);
    sys.poke(0x9881, 0);
    sys.poke(0x988A, 15);
    sys.poke(0x988F, 0x01);
    let audio = record(&mut sys, 60);
    let hz = pitch_hz(&audio);
    assert!((hz - 440.4).abs() < 3.0, "the SCC played {hz:.1} Hz");
    let scc = sys.cartridge_scc(1).expect("an SCC cartridge");
    assert!((scc.frequency_hz(0, 3_579_545.0) - 440.4).abs() < 0.1);
    assert!(sys.cartridge_scc(2).is_none());
}

#[test]
fn scc_volume_scales_the_note() {
    let mut sys = machine();
    sys.insert_cart1(vec![0; 128 * 1024], MapperType::KonamiScc);
    sys.poke(0x9000, 0x3F);
    load_square(&mut sys, 0x9800, 1);
    sys.poke(0x9882, 200);
    sys.poke(0x988B, 15);
    sys.poke(0x988F, 0x02);
    record(&mut sys, 2);
    let loud = swing(&record(&mut sys, 10));
    sys.poke(0x988B, 4);
    record(&mut sys, 2);
    let quiet = swing(&record(&mut sys, 10));
    let ratio = quiet / loud;
    assert!(
        (ratio - 4.0 / 15.0).abs() < 0.05,
        "volume 4 against 15 gave {ratio:.3}"
    );
}

#[test]
fn the_scc_window_opens_only_with_3f() {
    let mut sys = machine();
    let mut rom = vec![0u8; 128 * 1024];
    rom[0x4 * 0x2000 + 0x1800] = 0x99; // bank 4 at $9800
    sys.insert_cart1(rom, MapperType::KonamiScc);
    sys.poke(0x9000, 0x04);
    assert_eq!(sys.peek(0x9800), 0x99, "ROM while the SCC is closed");
    sys.poke(0x9800, 0x11);
    sys.poke(0x9000, 0x3F);
    assert_eq!(sys.peek(0x9800), 0xFF, "the SCC's power-on waveform");
    sys.poke(0x9800, 0x11);
    assert_eq!(sys.peek(0x9800), 0x11);
    assert_eq!(sys.peek(0x9F00), 0x11, "mirrored every 256 bytes");
}

#[test]
fn a_cpu_read_of_the_deformation_register_writes_ff() {
    let mut sys = machine();
    sys.insert_cart1(vec![0; 128 * 1024], MapperType::KonamiScc);
    sys.poke(0x9000, 0x3F);
    sys.mem_read_side_effects(0x98E0);
    assert_eq!(sys.cartridge_scc(1).map(Scc::deform), Some(0xFF));
    // A peek has no side effect.
    sys.poke(0x98E0, 0x00);
    let _ = sys.peek(0x98E0);
    assert_eq!(sys.cartridge_scc(1).map(Scc::deform), Some(0x00));
}

#[test]
fn the_scc_megarom_mirrors_pages_0_and_3() {
    let mut sys = machine();
    let mut rom = vec![0u8; 128 * 1024];
    rom[0x2000 * 2] = 0x22; // bank 2, at $8000
    rom[0] = 0x11; // bank 0, at $4000
    sys.insert_cart1(rom, MapperType::KonamiScc);
    sys.ppi.write(0, 0b0101_0101); // every page -> slot 1
    assert_eq!(sys.peek(0x0000), 0x22);
    assert_eq!(sys.peek(0xC000), 0x11);
}

#[test]
fn scc_and_psg_mix() {
    let mut sys = machine();
    sys.insert_cart1(vec![0; 128 * 1024], MapperType::KonamiScc);
    sys.poke(0x9000, 0x3F);
    load_square(&mut sys, 0x9800, 0);
    sys.poke(0x9880, 253);
    sys.poke(0x988A, 15);
    sys.poke(0x988F, 0x01);
    let scc_only = swing(&record(&mut sys, 10));
    // A PSG tone on channel A at full volume as well.
    for (register, value) in [(0, 0xFE), (1, 0), (7, 0x3E), (8, 15)] {
        sys.io_write(0xA0, register);
        sys.io_write(0xA1, value);
    }
    let both = swing(&record(&mut sys, 10));
    assert!(
        both > scc_only * 1.5,
        "the PSG adds to the SCC: {scc_only:.3} then {both:.3}"
    );
}

#[test]
fn the_sound_cartridge_plays_channel_5_in_scc_plus_mode() {
    let mut sys = machine();
    sys.insert_cart1(Vec::new(), MapperType::SoundCartridge);
    sys.poke(0xBFFE, 0x20); // SCC+ mode
    sys.poke(0xB000, 0x80); // open the SCC at $B800
    load_square(&mut sys, 0xB800, 4);
    sys.poke(0xB8A8, 253);
    sys.poke(0xB8AE, 15);
    sys.poke(0xB8AF, 0x10);
    let audio = record(&mut sys, 30);
    let hz = pitch_hz(&audio);
    assert!((hz - 440.4).abs() < 3.0, "channel 5 played {hz:.1} Hz");
    assert_eq!(
        sys.cartridge_scc(1).map(Scc::mode),
        Some(emu198x_konami_scc::Mode::Plus)
    );
}

#[test]
fn a_snapshot_carries_the_sound_chips() {
    let mut sys = machine();
    sys.install_msx_music(SlotAddress::primary(0), vec![0; 0x4000]);
    sys.insert_cart1(vec![0; 0x10000], MapperType::FmPac);
    sys.insert_cart2(vec![0; 128 * 1024], MapperType::KonamiScc);
    play_a440(&mut sys);
    sys.run_frame();
    let bytes = postcard::to_allocvec(&sys).expect("encode");
    let restored: Msx = postcard::from_bytes(&bytes).expect("decode");
    assert_eq!(postcard::to_allocvec(&restored).expect("re-encode"), bytes);
    // The YM2413s' own sample logs are dropped every frame, so the state
    // does not grow as the machine runs.
    for _ in 0..10 {
        sys.run_frame();
    }
    let later = postcard::to_allocvec(&sys).expect("encode later");
    assert!(
        later.len() < bytes.len() + 1024,
        "{} bytes grew to {}",
        bytes.len(),
        later.len()
    );
}

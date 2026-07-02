//! Shared helpers for the C64 runtime integration-test suite.
//!
//! Mirrors the helpers that used to live inside the `runtime.rs`
//! `#[cfg(test)] mod tests` block, exposed `pub` so each per-topic
//! integration-test file can pull them in via `mod common;`.

#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use emu198x_shell::{
    AudioPacket, AudioSink, FirmwareImage, FirmwareSet, FramePacket, FrameSink, HeadlessSession,
    InputEvent, MachineError, MachineTime, PixelFormat,
};
use runtime_commodore_c64::{C64Runtime, C64SessionQueryProvider};

pub const KERNAL_ROM_SIZE: usize = 0x2000;
pub const BASIC_ROM_SIZE: usize = 0x2000;
pub const CHARACTER_ROM_SIZE: usize = 0x1000;
pub const DOS1541_ROM_SIZE: usize = 0x4000;
pub const SCREEN_TEXT_HEIGHT: usize = 25;

#[derive(Default)]
pub struct FrameCollector {
    pub count: usize,
    pub last_timestamp: MachineTime,
    pub last_width: u32,
    pub last_height: u32,
    pub last_format: Option<PixelFormat>,
}

#[derive(Default)]
pub struct AudioCollector {
    pub count: usize,
    pub last_timestamp: MachineTime,
    pub last_sample_rate: u32,
    pub last_channels: u8,
    pub last_samples_len: usize,
}

impl FrameSink for FrameCollector {
    fn push_frame(&mut self, frame: FramePacket<'_>) -> Result<(), MachineError> {
        self.count += 1;
        self.last_timestamp = frame.timestamp;
        self.last_width = frame.width;
        self.last_height = frame.height;
        self.last_format = Some(frame.format);
        Ok(())
    }
}

impl AudioSink for AudioCollector {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        self.count += 1;
        self.last_timestamp = packet.timestamp;
        self.last_sample_rate = packet.sample_rate;
        self.last_channels = packet.channels;
        self.last_samples_len = packet.samples.len();
        Ok(())
    }
}

pub fn blank_firmware() -> FirmwareSet<'static> {
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(
        "commodore-c64-kernal-rom",
        &[0; KERNAL_ROM_SIZE],
    ));
    firmware.push(FirmwareImage::new(
        "commodore-c64-basic-rom",
        &[0; BASIC_ROM_SIZE],
    ));
    firmware.push(FirmwareImage::new(
        "commodore-c64-character-rom",
        &[0; CHARACTER_ROM_SIZE],
    ));
    firmware
}

pub fn stub_drive_rom_bytes() -> &'static [u8] {
    let mut rom = vec![0xEA; DOS1541_ROM_SIZE];
    let vector = DOS1541_ROM_SIZE - 4;
    rom[vector] = 0x00;
    rom[vector + 1] = 0xC0;
    Box::leak(rom.into_boxed_slice())
}

pub fn blank_firmware_with_drive() -> FirmwareSet<'static> {
    let mut firmware = blank_firmware();
    firmware.push(FirmwareImage::new(
        "commodore-1541-dos-rom",
        stub_drive_rom_bytes(),
    ));
    firmware
}

pub fn make_tap(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 20];
    bytes[..12].copy_from_slice(b"C64-TAPE-RAW");
    bytes[12] = 1;
    bytes[13] = 0;
    bytes[14] = 0;
    bytes[16..20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

pub fn d64_linear_sector_index(track: u8, sector_num: u8) -> usize {
    const TRACK_SECTOR_COUNTS: [u8; 35] = [
        21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 19, 19, 19, 19, 19, 19,
        19, 18, 18, 18, 18, 18, 18, 17, 17, 17, 17, 17,
    ];
    TRACK_SECTOR_COUNTS[..usize::from(track - 1)]
        .iter()
        .map(|&count| usize::from(count))
        .sum::<usize>()
        + usize::from(sector_num)
}

pub fn write_d64_sector(bytes: &mut [u8], track: u8, sector_num: u8, sector: &[u8; 256]) {
    let offset = d64_linear_sector_index(track, sector_num) * 256;
    bytes[offset..offset + 256].copy_from_slice(sector);
}

pub fn make_d64() -> Vec<u8> {
    let mut bytes = vec![0u8; 174_848];

    let mut bam = [0u8; 256];
    bam[0] = 18;
    bam[1] = 1;
    bam[0x90..0x98].copy_from_slice(b"DEMO DIS");
    bam[0x98] = b'K';
    bam[0xA2..0xA4].copy_from_slice(b"42");
    write_d64_sector(&mut bytes, 18, 0, &bam);

    let mut directory = [0u8; 256];
    directory[2] = 0x82;
    directory[3] = 1;
    directory[4] = 0;
    directory[5..10].copy_from_slice(b"HELLO");
    directory[30..32].copy_from_slice(&(1u16).to_le_bytes());
    write_d64_sector(&mut bytes, 18, 1, &directory);

    let mut file_sector = [0u8; 256];
    file_sector[0] = 0;
    file_sector[1] = 6;
    file_sector[2..7].copy_from_slice(&[0x01, 0x08, 0x11, 0x22, 0x33]);
    write_d64_sector(&mut bytes, 1, 0, &file_sector);

    bytes
}

pub fn local_rom_firmware() -> FirmwareSet<'static> {
    let rom_dir = PathBuf::from(
        std::env::var("HOME").expect("HOME should be available for local C64 ROM tests"),
    )
    .join(".emu198x/roms/commodore-c64");

    let kernal = Box::leak(
        fs::read(rom_dir.join("kernal.rom"))
            .expect("local C64 KERNAL ROM should exist")
            .into_boxed_slice(),
    );
    let basic = Box::leak(
        fs::read(rom_dir.join("basic.rom"))
            .expect("local C64 BASIC ROM should exist")
            .into_boxed_slice(),
    );
    let chargen = Box::leak(
        fs::read(rom_dir.join("chargen.rom"))
            .expect("local C64 chargen ROM should exist")
            .into_boxed_slice(),
    );

    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new("commodore-c64-kernal-rom", kernal));
    firmware.push(FirmwareImage::new("commodore-c64-basic-rom", basic));
    firmware.push(FirmwareImage::new("commodore-c64-character-rom", chargen));
    firmware
}

pub fn local_rom_firmware_with_drive() -> FirmwareSet<'static> {
    let rom_dir = PathBuf::from(
        std::env::var("HOME").expect("HOME should be available for local C64 ROM tests"),
    )
    .join(".emu198x/roms/commodore-c64");
    let drive = Box::leak(
        fs::read(rom_dir.join("1541.rom"))
            .expect("local 1541 DOS ROM should exist")
            .into_boxed_slice(),
    );

    let mut firmware = local_rom_firmware();
    firmware.push(FirmwareImage::new("commodore-1541-dos-rom", drive));
    firmware
}

/// Root the C64 game-media paths resolve against. Honours
/// `EMU198X_CATALOGUE_MEDIA_ROOT` (the same env var the catalogue CLI uses, so
/// a mounted TOSEC tree works for both), falling back to the legacy
/// `~/Projects/Emu198x-Unclean/Reference` layout.
pub fn media_root() -> PathBuf {
    if let Some(root) = std::env::var_os("EMU198X_CATALOGUE_MEDIA_ROOT") {
        return PathBuf::from(root);
    }
    PathBuf::from(
        std::env::var("HOME").expect("HOME should be available for local C64 media tests"),
    )
    .join("Projects/Emu198x-Unclean/Reference")
}

pub fn local_thinker_tap_zip() -> PathBuf {
    media_root().join("commodore/c64/Educational/[TAP]/Thinker, The (1984)(Atlantis).zip")
}

pub fn local_thomas_tap_zip() -> PathBuf {
    media_root().join(
        "commodore/c64/Educational/[TAP]/Thomas the Tank Engine (1990)(Alternative Software).zip",
    )
}

pub fn local_thing_on_a_spring_tap_zip() -> PathBuf {
    media_root().join("commodore/c64/Games/Arcade/[TAP]/Thing on a Spring (1985)(Gremlin).zip")
}

pub fn local_ghostbusters_tap_zip() -> PathBuf {
    media_root().join("commodore/c64/Games/Arcade/[TAP]/Ghostbusters (1984)(Activision).zip")
}

pub fn local_bruce_lee_d64_zip() -> PathBuf {
    media_root().join("commodore/c64/Games/Arcade/[D64]/Bruce Lee (1984)(Datasoft).zip")
}

pub fn local_aztec_challenge_d64_zip() -> PathBuf {
    media_root().join("commodore/c64/Games/Arcade/[D64]/Aztec Challenge (1983)(Cosmi).zip")
}

pub fn local_bomb_jack_d64_zip() -> PathBuf {
    media_root().join("commodore/c64/Games/Arcade/[D64]/Bomb Jack (1986)(Elite).zip")
}

pub fn screen_text_lines(
    session: &HeadlessSession<C64Runtime, C64SessionQueryProvider>,
) -> Vec<String> {
    let result = session
        .query("screen.text.lines")
        .expect("screen.text.lines query should succeed");
    let lines = result
        .value
        .as_array()
        .expect("screen.text.lines should be an array");
    lines
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("screen.text.lines entries should be strings")
                .to_owned()
        })
        .collect()
}

pub fn wait_for_screen_line_contains(
    session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    row: usize,
    needle: &str,
    max_frames: u32,
) {
    for _ in 0..max_frames {
        if screen_text_lines(session)
            .get(row)
            .is_some_and(|line| line.contains(needle))
        {
            return;
        }
        session
            .run_frames(1)
            .expect("screen-line wait should be able to run one frame");
    }

    panic!("screen row {row} did not contain {needle:?} within {max_frames} frames");
}

pub fn press_key(
    session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    key: &str,
    held_frames: u32,
) {
    session.queue_input(InputEvent::Key {
        name: key.to_ascii_lowercase().into(),
        pressed: true,
    });
    session
        .run_frames(held_frames)
        .expect("key press should advance the runtime");
    session.queue_input(InputEvent::Key {
        name: key.to_ascii_lowercase().into(),
        pressed: false,
    });
}

pub fn press_button(
    session: &mut HeadlessSession<C64Runtime, C64SessionQueryProvider>,
    port: u8,
    name: &str,
    held_frames: u32,
) {
    session.queue_input(InputEvent::Button {
        port,
        name: name.to_ascii_lowercase().into(),
        pressed: true,
    });
    session
        .run_frames(held_frames)
        .expect("button press should advance the runtime");
    session.queue_input(InputEvent::Button {
        port,
        name: name.to_ascii_lowercase().into(),
        pressed: false,
    });
}

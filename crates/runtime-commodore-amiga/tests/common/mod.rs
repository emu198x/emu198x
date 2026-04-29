//! Shared helpers for the Amiga runtime integration-test suite.
//!
//! Mirrors the helpers that used to live inside the `runtime.rs`
//! `#[cfg(test)] mod tests` block, exposed `pub` so each per-topic
//! integration-test file can pull them in via `mod common;`.

#![allow(dead_code)]

use emu198x_shell::{
    AudioPacket, AudioSink, FirmwareImage, FirmwareSet, MachineError, MachineTime, NullAudioSink,
    NullFrameSink, NullTraceSink,
};
use runtime_commodore_amiga::A500_PAL_CCK_HZ;

/// Firmware identifier the A500 family uses. `pub(crate)` in the
/// runtime crate, so tests crossing the public-API boundary hardcode
/// it here. (See the C64 split log for the same rationale.)
pub const KICKSTART_ROM_ID: &str = "commodore-amiga-kickstart-rom";
/// Firmware identifier the A1000 uses for its bootstrap ROM.
pub const A1000_BOOTSTRAP_ROM_ID: &str = "commodore-amiga-a1000-bootstrap-rom";
/// Audio sample rate driven by `tick_and_sample_audio` — `pub(crate)`
/// in the runtime, hardcoded here for assertion purposes.
pub const AUDIO_SAMPLE_RATE_HZ: u32 = 48_000;
/// Stereo Paula output: left + right interleaved.
pub const AUDIO_CHANNELS: u8 = 2;
/// Master tick rate (`A500_PAL_CCK_HZ * 2`). Mirrors the
/// `pub(crate) const A500_PAL_TICK_HZ` in `runtime.rs`.
pub const A500_PAL_TICK_HZ: u64 = A500_PAL_CCK_HZ * 2;

/// How many audio frames (left + right pairs are 1 frame) the
/// resampler emits for a given runtime tick budget. Same formula as
/// the runtime's `audio_sample_frames_for_ticks` helper.
#[must_use]
pub fn audio_sample_frames_for_ticks(ticks: u64) -> usize {
    usize::try_from((ticks.saturating_mul(u64::from(AUDIO_SAMPLE_RATE_HZ))) / A500_PAL_TICK_HZ)
        .unwrap_or(usize::MAX)
}

/// Build a 256 KiB Kickstart-shaped image with a minimal reset vector
/// (supervisor stack at $00080000, PC at $F80008) and a `BRA.S *` (loop
/// forever) at the entry point. Keeps the CPU stable while the chipset
/// ticks around it.
pub fn dummy_kickstart() -> Vec<u8> {
    let mut kickstart = vec![0u8; 256 * 1024];
    kickstart[0] = 0x00;
    kickstart[1] = 0x08;
    kickstart[2] = 0x00;
    kickstart[3] = 0x00;
    kickstart[4] = 0x00;
    kickstart[5] = 0xF8;
    kickstart[6] = 0x00;
    kickstart[7] = 0x08;
    kickstart[8] = 0x60;
    kickstart[9] = 0xFE;
    kickstart
}

/// Build a 64 KiB A1000 bootstrap ROM with a JMP-to-Kickstart-area
/// header and a `BRA.S *` at the landing site. Same hermetic role as
/// `dummy_kickstart` but for the A1000 model variant.
pub fn dummy_a1000_bootstrap_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 64 * 1024];
    rom[0] = 0x11;
    rom[1] = 0x11;
    rom[2] = 0x4E;
    rom[3] = 0xF9;
    rom[4] = 0x00;
    rom[5] = 0xF8;
    rom[6] = 0x00;
    rom[7] = 0x08;
    rom[8] = 0x60;
    rom[9] = 0xFE;
    rom
}

/// Wrap a freshly-built `dummy_kickstart` image in a `FirmwareSet`.
/// Leaks the bytes to `'static` so the set can hand them out by
/// reference.
pub fn dummy_firmware() -> FirmwareSet<'static> {
    let kickstart = dummy_kickstart().into_boxed_slice();
    let bytes: &'static [u8] = Box::leak(kickstart);
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(KICKSTART_ROM_ID, bytes));
    firmware
}

/// Wrap a freshly-built `dummy_a1000_bootstrap_rom` image in a
/// `FirmwareSet`. Same leaking pattern as `dummy_firmware`.
pub fn dummy_a1000_firmware() -> FirmwareSet<'static> {
    let bootstrap = dummy_a1000_bootstrap_rom().into_boxed_slice();
    let bytes: &'static [u8] = Box::leak(bootstrap);
    let mut firmware = FirmwareSet::new();
    firmware.push(FirmwareImage::new(A1000_BOOTSTRAP_ROM_ID, bytes));
    firmware
}

/// Returns the trio of null sinks used by tests that don't care about
/// frame / audio / trace output.
#[must_use]
pub fn null_host_buffers() -> (NullFrameSink, NullAudioSink, NullTraceSink) {
    (NullFrameSink, NullAudioSink, NullTraceSink)
}

/// Audio sink that captures the most recent `push_audio` packet so a
/// test can assert on the resampled frame.
#[derive(Default)]
pub struct AudioCollector {
    pub packets: usize,
    pub last_timestamp: MachineTime,
    pub last_sample_rate: u32,
    pub last_channels: u8,
    pub last_samples: Vec<f32>,
}

impl AudioSink for AudioCollector {
    fn push_audio(&mut self, packet: AudioPacket<'_>) -> Result<(), MachineError> {
        self.packets += 1;
        self.last_timestamp = packet.timestamp;
        self.last_sample_rate = packet.sample_rate;
        self.last_channels = packet.channels;
        self.last_samples.clear();
        self.last_samples.extend_from_slice(packet.samples);
        Ok(())
    }
}

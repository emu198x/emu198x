//! Scorpion ZS-256 machine.
//!
//! Source references:
//! - `knowledge/systems/spectrum/variants.md`
//! - Adapted from `../Emu198x-Older/crates/machine-scorpion-zs256/src/lib.rs`
//!
//! Hardware:
//! - Z80 @ 3.5 MHz (master / 4) — same crystal as the 48K
//! - Scorpion ULA — no contention, 48K-style geometry
//! - 256 KB RAM in 16 × 16 KB banks (paged via `$7FFD` + `$1FFD`)
//! - 4 × 16 KB ROMs (128 editor / 48 BASIC / Service monitor / TR-DOS overlay)
//! - General Instrument AY-3-8912 PSG
//! - Beta 128 disk interface

pub mod memory;

use beta_disk_interface::BetaDisk;
use common_sinclair_zx_spectrum::SpectrumTapePlayer;
use common_sinclair_zx_spectrum::audio::{BeeperAudio, SpeakerMixer};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::io_trace::{IoEvent, IoTrace};
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use common_sinclair_zx_spectrum::snapshot::{
    Snapshot, apply_128k_bank_pages, apply_ay_registers, apply_z80_registers,
};
use common_sinclair_zx_spectrum::tape::{StopRelease, TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::tape_recorder::TapeRecorder;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_SCORPION};
use common_sinclair_zx_spectrum::ula::Ula;
use emu198x_zilog_z80::{BusOp, Z80};
use gi_ay_3_8912::Ay3_8912;
use peripheral_kempston_joystick::KempstonJoystick;
use scorpion_ula::ScorpionUla;

use crate::memory::MemoryScorpion;

const AUDIO_SAMPLE_RATE: u32 = 44_100;
const AUDIO_SAMPLES_PER_FRAME: usize = 882;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ScorpionZS256 {
    pub z80: Z80,
    /// Host-side capture of `IN`/`OUT` traffic, off unless a debugger
    /// turns it on. Outside the snapshot: a saved state carries the
    /// machine, not what a debugger happened to be collecting.
    #[serde(skip)]
    io_trace: IoTrace,
    pub ula: ScorpionUla,
    pub memory: MemoryScorpion,
    pub framebuffer: Vec<u8>,
    pub keyboard: [u8; 8],
    /// Kempston Interface joystick. Defaults to unattached.
    pub kempston: KempstonJoystick,
    pub tape: TapePlayer,
    /// Holds the tape input for one frame after playback stops, then
    /// releases it to low, as FUSE's `tape_stop_mic_off` does.
    tape_release: StopRelease,
    /// Captures the MIC line during a SAVE for tape write-back (mirrors the 48K
    /// class). `#[serde(default)]` keeps pre-SAVE snapshots loadable.
    #[serde(default)]
    recorder: TapeRecorder,
    pub ay: Ay3_8912,
    pub beta: BetaDisk,
    pub audio: BeeperAudio,
    pub audio_frame: Vec<f32>,

    pub(crate) hc: u32,
    speaker: SpeakerMixer,
}

impl ScorpionZS256 {
    #[must_use]
    pub fn new() -> Self {
        let cpu_hz = (TIMING_SCORPION.master_hz / u64::from(TIMING_SCORPION.cpu_divisor)) as u32;
        let ay_hz = cpu_hz / 2;
        Self {
            z80: Z80::new(),
            io_trace: IoTrace::default(),
            ula: ScorpionUla::new(),
            memory: MemoryScorpion::new(),
            framebuffer: vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT],
            keyboard: [0xFF; 8],
            kempston: KempstonJoystick::new(),
            tape: TapePlayer::new(),
            tape_release: StopRelease::new(),
            recorder: TapeRecorder::new(),
            ay: Ay3_8912::new(ay_hz, AUDIO_SAMPLE_RATE, AUDIO_SAMPLES_PER_FRAME),
            beta: BetaDisk::new(),
            audio: BeeperAudio::new(AUDIO_SAMPLE_RATE, TIMING_SCORPION.tstates_per_frame, cpu_hz),
            audio_frame: vec![0.0; AUDIO_SAMPLES_PER_FRAME],
            hc: 0,
            speaker: SpeakerMixer::default(),
        }
    }

    /// Decodes any captured tape `SAVE` signal into standard-speed blocks.
    #[must_use]
    pub fn recorded_tape_blocks(&self) -> Vec<TapeBlock> {
        self.recorder.decode()
    }

    /// Discards captured `SAVE` signal (e.g. after flushing it to a file).
    pub fn clear_tape_recording(&mut self) {
        self.recorder.clear();
    }

    #[must_use]
    pub fn model_id(&self) -> &'static str {
        "scorpion-zs256"
    }
    pub fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        self.tape.load_blocks(blocks);
    }
    pub fn load_tape_pulses(&mut self, pulses: Vec<u32>) {
        self.tape.load_pulses(pulses);
    }
    pub fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        self.tape.load_stream(stream);
    }
    pub fn tape_play(&mut self) {
        self.tape.play();
    }
    pub fn tape_stop(&mut self) {
        self.tape.stop();
    }

    /// Reset the CPU, timing, and audio state. Keeps ROMs and RAM intact.
    pub fn reset(&mut self) {
        self.z80 = Z80::new();
        self.hc = 0;
        self.speaker = SpeakerMixer::default();
    }

    /// Reattach `&'static` references that don't survive serde's
    /// `#[serde(skip)]` round-trip, and rehydrate the Z80 walker
    /// sequence. Call once after restoring a postcard snapshot — the
    /// runtime wires this through `after_restore`. The Scorpion shares
    /// 48K timing, so the reattach is a structural mirror, but it keeps
    /// every variant on the same explicit-reattach contract.
    pub fn restore_volatile_refs(&mut self) {
        self.z80.rehydrate_walker_sequence();
        self.ula.reattach_config();
    }

    /// Apply a parsed `.z80` snapshot. Scorpion uses 128K-style page-to-bank
    /// routing; only the first 8 banks are addressable through a snapshot.
    pub fn apply_snapshot(&mut self, snap: &Snapshot) {
        apply_z80_registers(&mut self.z80, snap);
        self.ula.write_fe(snap.border);
        apply_128k_bank_pages(snap, &mut self.memory);
        self.memory.write_7ffd(snap.port_7ffd);
        apply_ay_registers(snap, &mut self.ay);
    }
    pub fn run_frame(&mut self) {
        <Self as SpectrumDriver>::run_frame(self);
    }
    pub fn advance_halfcycles(&mut self, halfcycles: u32) {
        <Self as SpectrumDriver>::advance_halfcycles(self, halfcycles);
    }
    pub fn advance_tstates(&mut self, tstates: u32) {
        <Self as SpectrumDriver>::advance_tstates(self, tstates);
    }

    fn handle_bus(&mut self) {
        // Z80 strobes remain asserted across several half-cycles. Collapse
        // them to one host transaction so stateful Beta-disk reads advance
        // once per IN rather than once per asserted phase.
        match self.z80.bus_request() {
            Some(BusOp::MemRead) => {
                // FUSE `z80_ops.c`: on a 128-type machine the Beta trap
                // pages in and out only while a non-zero ROM is selected
                // (`NOT_128_TYPE_OR_IS_48_TYPE`), so the 128 Editor in
                // ROM 0 can run through $3D00-$3DFF without TR-DOS.
                if self.z80.m1 && self.memory.current_rom() != 0 {
                    self.beta.on_m1(self.z80.addr);
                }
                if self.beta.trdos_paged && self.z80.addr < 0x4000 {
                    self.z80.data_in = self.memory.read_trdos_rom(self.z80.addr);
                } else {
                    self.z80.data_in = self.memory.read(self.z80.addr);
                }
            }
            // The overlay is ROM: while it is paged in, writes below
            // $4000 reach neither ROM nor the RAM bank $1FFD bit 0 maps.
            Some(BusOp::MemWrite) if self.beta.trdos_paged && self.z80.addr < 0x4000 => {}
            Some(BusOp::MemWrite) => self.memory.write(self.z80.addr, self.z80.data),
            Some(BusOp::IoRead) => self.z80.data_in = self.io_read(self.z80.addr),
            Some(BusOp::IoWrite) => self.io_write(self.z80.addr, self.z80.data),
            Some(BusOp::IntAck) => self.z80.data_in = 0xFF,
            None => {}
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        let value = self.io_read_untraced(port);
        let pc = self.z80.regs.pc;
        self.io_trace.record(pc, port, value, false);
        value
    }

    fn io_read_untraced(&mut self, port: u16) -> u8 {
        if self.beta.claims_port(port) {
            return self.beta.read(port);
        }
        if self.kempston.claims_port(port) {
            return self.kempston.read(port);
        }
        if port & 0x0001 == 0 {
            let mut val = self.ula.read_fe(port, &self.keyboard);
            if self.tape.is_playing() || self.tape_release.pending() {
                val = (val & !0x40) | if self.tape.ear_level() { 0x40 } else { 0x00 };
            }
            val
        } else if port & 0xC002 == 0xC000 {
            self.ay.read_data()
        } else {
            0xFF
        }
    }

    fn io_write(&mut self, port: u16, data: u8) {
        let pc = self.z80.regs.pc;
        self.io_trace.record(pc, port, data, true);
        self.io_write_untraced(port, data);
    }

    fn io_write_untraced(&mut self, port: u16, data: u8) {
        if self.beta.claims_port(port) {
            self.beta.write(port, data);
            return;
        }
        if port & 0x0001 == 0 {
            self.ula.write_fe(data);
            let beeper = data & 0x10 != 0;
            if beeper != self.speaker.beeper {
                self.speaker.beeper = beeper;
                let tstate = common_sinclair_zx_spectrum::timing::FramePosition::new(
                    self.hc,
                    &TIMING_SCORPION,
                )
                .tstate(&TIMING_SCORPION);
                self.audio.set_level(tstate, self.speaker.level());
            }
            // MIC (bit 3) carries the tape SAVE signal.
            self.recorder.set_mic_level(data & 0x08 != 0);
        }
        // +3-style decoding (FUSE `plus3_memory_ports`; MAME maps $7FFD as
        // `01xxxxxxxx1xxx01`): A14 separates $7FFD from $1FFD, so a write
        // to $1FFD does not also land in $7FFD as 128K decoding would.
        if port & 0xC002 == 0x4000 {
            self.memory.write_7ffd(data);
        }
        if port & 0xF002 == 0x1000 {
            self.memory.write_1ffd(data);
        }
        if port & 0xC002 == 0xC000 {
            self.ay.select_register(data);
        } else if port & 0xC002 == 0x8000 {
            self.ay.write_data(data);
        }
    }
    pub fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }

    /// Current host-side speaker audio controls.
    #[must_use]
    pub fn audio_controls(&self) -> common_sinclair_zx_spectrum::audio::AudioControls {
        self.audio.audio_controls()
    }

    /// Replaces the host-side speaker audio controls wholesale.
    pub fn set_audio_controls(
        &mut self,
        controls: common_sinclair_zx_spectrum::audio::AudioControls,
    ) {
        self.audio.set_audio_controls(controls);
    }

    /// Enables or disables one host-side audio channel.
    pub fn set_audio_channel_enabled(
        &mut self,
        channel: common_sinclair_zx_spectrum::audio::SpeakerChannel,
        enabled: bool,
    ) {
        self.audio.set_audio_channel_enabled(channel, enabled);
    }

    /// Sets the host-side gain for one audio channel.
    pub fn set_audio_channel_gain(
        &mut self,
        channel: common_sinclair_zx_spectrum::audio::SpeakerChannel,
        gain: f32,
    ) {
        self.audio.set_audio_channel_gain(channel, gain);
    }

    /// Start, or restart, capturing `IN`/`OUT` traffic.
    pub fn start_io_trace(&mut self) {
        self.io_trace.start();
    }

    /// Stop capturing and take the events collected since
    /// [`start_io_trace`](Self::start_io_trace).
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.io_trace.take()
    }

    /// Bus-level port read.
    pub fn port_read(&mut self, port: u16) -> u8 {
        self.io_read(port)
    }

    /// Bus-level port write.
    pub fn port_write(&mut self, port: u16, value: u8) {
        self.io_write(port, value);
    }
}

impl Default for ScorpionZS256 {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumDriver for ScorpionZS256 {
    fn frame_timing(&self) -> &common_sinclair_zx_spectrum::timing::FrameTiming {
        &TIMING_SCORPION
    }
    #[inline(always)]
    fn hc(&self) -> u32 {
        self.hc
    }
    #[inline(always)]
    fn hc_mut(&mut self) -> &mut u32 {
        &mut self.hc
    }
    /// Scorpion has no memory contention.
    #[inline(always)]
    fn contended(&self) -> bool {
        false
    }

    #[inline(always)]
    fn tick_ula(&mut self) {
        self.ula.tick(
            &self.memory,
            self.z80.addr,
            self.z80.mreq,
            self.z80.iorq,
            self.z80.rfsh,
            &mut self.framebuffer,
        );
    }

    #[inline(always)]
    fn tick_cpu_and_bus(&mut self) {
        self.z80.tick();
        self.handle_bus();
    }

    #[inline(always)]
    fn feed_irq(&mut self) {
        self.z80.irq = self.ula.interrupt_active();
    }

    #[inline(always)]
    fn on_tstate(&mut self, position: common_sinclair_zx_spectrum::timing::FramePosition) {
        let release_after = TIMING_SCORPION.tstates_per_frame;
        self.tape_release.advance(&mut self.tape, 1, release_after);
        self.recorder.advance(1);
        if position.halfcycles() % 8 == 2 {
            self.ay.tick();
        }
        let ear = self.tape.ear_level();
        if ear != self.speaker.ear {
            self.speaker.ear = ear;
            let tstate = position.tstate(&TIMING_SCORPION);
            self.audio.set_level(tstate, self.speaker.level());
        }
    }

    #[inline(always)]
    fn end_frame_ula(&mut self) {
        self.ula.end_frame();
    }

    #[inline(always)]
    fn on_end_frame(&mut self) {
        self.audio.end_frame(&mut self.audio_frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1633: a tape that ends with its input high must not leave the
    /// speaker's EAR input, or port `$FE` bit 6, latched there. FUSE's
    /// `tape_stop_mic_off` releases it one frame after the tape stops.
    #[test]
    fn tape_input_is_released_one_frame_after_the_tape_stops() {
        let idle_fe = ScorpionZS256::new().port_read(0x00FE) & 0x40;
        let mut m = ScorpionZS256::new();
        let frame = TIMING_SCORPION.tstates_per_frame;
        m.load_tape_pulses(vec![1_000]);
        m.tape_play();
        m.advance_tstates(1_010);
        assert!(
            m.speaker.ear,
            "the single pulse drained the tape and left the input high"
        );
        let held_fe = m.port_read(0x00FE) & 0x40;

        m.advance_tstates(frame - 20);
        assert!(
            m.speaker.ear,
            "the input is held for a frame after the stop"
        );
        assert_eq!(
            m.port_read(0x00FE) & 0x40,
            held_fe,
            "and so is port $FE bit 6"
        );

        m.advance_tstates(20);
        assert!(!m.speaker.ear, "one frame after the stop it is released");
        assert_eq!(
            m.port_read(0x00FE) & 0x40,
            idle_fe,
            "and port $FE bit 6 reads as it does with no tape"
        );
    }

    /// #1637: port `$FE` bit 6 reads the tape input as it is, 1 while the
    /// input is high and 0 while it is low, as the 48K's ULA does (Smith
    /// ch. 20 p. 222). Checked with the MIC bit both ways and the speaker
    /// bit clear, as the ROM loader writes them.
    #[test]
    fn port_fe_bit_6_follows_the_tape_input() {
        macro_rules! check_tape {
            ($make:expr) => {
                for written in [0x00, 0x08] {
                    let mut m = $make;
                    m.port_write(0x00FE, written);
                    m.load_tape_pulses(vec![1_000, 1_000_000]);
                    m.tape_play();
                    m.advance_tstates(500);
                    assert_eq!(
                        m.port_read(0x00FE) & 0x40,
                        0x00,
                        "{}, wrote {written:#04x}: the tape is low, so is bit 6",
                        stringify!($make)
                    );
                    m.advance_tstates(1_000);
                    assert_eq!(
                        m.port_read(0x00FE) & 0x40,
                        0x40,
                        "{}, wrote {written:#04x}: the tape is high, so is bit 6",
                        stringify!($make)
                    );
                }
            };
        }
        check_tape!(ScorpionZS256::new());
    }

    /// #1637: with no tape signal, bit 6 reads back the last speaker bit
    /// written, as on an Issue 3 48K. FUSE and ZEsarUX both do this; no
    /// primary source for the Scorpion's tape input is held. A tape that
    /// is loaded but not playing reads the same.
    #[test]
    fn port_fe_bit_6_with_no_tape_signal() {
        // (written to `$FE`, bit 6 read back)
        const IDLE: [(u8, u8); 4] = [(0x00, 0x00), (0x08, 0x00), (0x10, 0x40), (0x18, 0x40)];
        macro_rules! check_idle {
            ($make:expr) => {
                for (written, expected) in IDLE {
                    let mut m = $make;
                    m.port_write(0x00FE, written);
                    assert_eq!(
                        m.port_read(0x00FE) & 0x40,
                        expected,
                        "{}, no tape, wrote {written:#04x}",
                        stringify!($make)
                    );
                    m.load_tape_pulses(vec![1_000]);
                    m.advance_tstates(2_000);
                    assert_eq!(
                        m.port_read(0x00FE) & 0x40,
                        expected,
                        "{}, stopped tape, wrote {written:#04x}",
                        stringify!($make)
                    );
                }
            };
        }
        check_idle!(ScorpionZS256::new());
    }

    #[test]
    fn defaults_are_sane() {
        let m = ScorpionZS256::new();
        assert_eq!(m.model_id(), "scorpion-zs256");
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
    }

    #[test]
    fn run_frame_returns_to_origin() {
        let mut m = ScorpionZS256::new();
        m.run_frame();
        assert_eq!(m.hc, 0);
    }

    #[test]
    fn held_io_read_advances_beta_disk_once() {
        let mut m = ScorpionZS256::new();
        let mut disk = vec![0; 80 * 2 * 16 * 256];
        disk[..3].copy_from_slice(&[0xAB, 0xCD, 0xEF]);
        m.beta.insert_disk(0, disk);
        m.beta.trdos_paged = true;
        m.beta.write(0x5F, 1);
        m.beta.write(0x1F, 0x80);

        m.z80.addr = 0x007F;
        m.z80.iorq = true;
        m.z80.rd = true;
        m.z80.m1 = false;
        m.handle_bus();
        assert_eq!(m.z80.data_in, 0xAB);

        m.handle_bus();
        m.handle_bus();
        assert_eq!(m.z80.data_in, 0xAB);

        m.z80.iorq = false;
        m.z80.rd = false;
        m.handle_bus();
        m.z80.iorq = true;
        m.z80.rd = true;
        m.handle_bus();
        assert_eq!(m.z80.data_in, 0xCD);
    }

    #[test]
    fn port_1ffd_write_does_not_reach_7ffd() {
        let mut m = ScorpionZS256::new();
        m.port_write(0x1FFD, 0x12);
        // $1FFD = $12 selects ROM 2 and the high bank bit; $7FFD stays 0.
        assert_eq!(m.memory.current_rom(), 2);
        assert_eq!(m.memory.current_bank(), 8);
        assert_eq!(m.memory.screen_bank(), 5);

        m.port_write(0x7FFD, 0x13);
        assert_eq!(m.memory.current_bank(), 11);
    }

    fn fetch_m1(m: &mut ScorpionZS256, addr: u16) -> u8 {
        m.z80.addr = addr;
        m.z80.mreq = true;
        m.z80.rd = true;
        m.z80.m1 = true;
        m.handle_bus();
        m.z80.mreq = false;
        m.z80.rd = false;
        m.z80.m1 = false;
        m.handle_bus();
        m.z80.data_in
    }

    #[test]
    fn beta_trap_pages_rom_3_only_outside_rom_0() {
        let mut m = ScorpionZS256::new();
        let roms: [Vec<u8>; 4] = std::array::from_fn(|i| vec![0x10 * i as u8 + 1; 16384]);
        m.memory.load_roms(&roms[0], &roms[1], &roms[2], &roms[3]);

        // ROM 0 (128 Editor): $3D00 is ordinary code, no trap.
        assert_eq!(fetch_m1(&mut m, 0x3D00), 0x01);
        assert!(!m.beta.trdos_paged);

        // ROM 1 (48 BASIC): the trap pages TR-DOS from ROM 3.
        m.port_write(0x7FFD, 0x10);
        assert_eq!(fetch_m1(&mut m, 0x3D00), 0x31);
        assert!(m.beta.trdos_paged);

        // Overlay is ROM: writes below $4000 land nowhere, even with
        // RAM bank 0 mapped there by $1FFD bit 0.
        m.port_write(0x1FFD, 0x01);
        m.z80.addr = 0x0100;
        m.z80.data = 0xAA;
        m.z80.mreq = true;
        m.z80.wr = true;
        m.handle_bus();
        m.z80.mreq = false;
        m.z80.wr = false;
        m.handle_bus();
        assert_eq!(fetch_m1(&mut m, 0x0100), 0x31);

        // Leaving ROM space unpages the overlay, exposing RAM bank 0.
        fetch_m1(&mut m, 0x8000);
        assert!(!m.beta.trdos_paged);
        assert_eq!(m.memory.read(0x0100), 0x00, "the overlay write was dropped");
    }

    #[test]
    fn audio_controls_passthrough_round_trips() {
        use common_sinclair_zx_spectrum::audio::SpeakerChannel;
        let mut m = ScorpionZS256::new();
        let initial = m.audio_controls();
        assert!(initial.channel(SpeakerChannel::Speaker).enabled());

        m.set_audio_channel_enabled(SpeakerChannel::Speaker, false);
        m.set_audio_channel_gain(SpeakerChannel::Speaker, 0.25);
        let after = m.audio_controls();
        assert!(!after.channel(SpeakerChannel::Speaker).enabled());
        assert!((after.channel(SpeakerChannel::Speaker).gain() - 0.25).abs() < f32::EPSILON);

        m.set_audio_controls(initial);
        assert!(
            m.audio_controls()
                .channel(SpeakerChannel::Speaker)
                .enabled()
        );
    }
}

//! Shared 128K-class machine composition.
//!
//! The Sinclair 128K and the Sinclair-branded Amstrad-built grey +2
//! share the same chip set, ULA, memory, AY, and timing. This type holds
//! the Z80 + Sinclair 7K010E ULA + paged memory + AY-3-8912 + beeper +
//! tape composition that's identical across them. The variant marker
//! `V: Class128kVariant` is a phantom — it changes the type identity
//! (so snapshots can't cross variants) but contributes no state.
//!
//! Variants outside the 128K-class — 48K-class, +2A/+2B/+3 (Amstrad gate
//! array), Pentagon, Scorpion, Timex — have different ULAs, paging, or
//! contention models and keep their own machine implementations.

use std::marker::PhantomData;

use common_sinclair_zx_spectrum::audio::{BeeperAudio, SpeakerMixer};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use common_sinclair_zx_spectrum::snapshot::{
    Snapshot, apply_128k_bank_pages, apply_ay_registers, apply_z80_registers,
};
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::tape_recorder::TapeRecorder;
use common_sinclair_zx_spectrum::timing::{
    FramePosition, FrameTiming, SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_128K,
};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::ula_engine::floating_bus_byte;
use gi_ay_3_8912::Ay3_8912;
use peripheral_kempston_joystick::KempstonJoystick;
use sinclair_ula_7k010e::SinclairUla;
use zilog_z80::{BusOp, Z80};

use crate::memory::Memory128K;
use crate::variant::Class128kVariant;

/// Audio output sample rate (44.1 kHz).
const AUDIO_SAMPLE_RATE: u32 = 44_100;

/// Pre-allocated samples-per-frame buffer for the AY downsampler. The
/// 128K's 50 Hz frame produces ~882 samples at 44.1 kHz.
const AUDIO_SAMPLES_PER_FRAME: usize = 882;

/// 128K-class machine state.
///
/// Shared between the Sinclair 128K (`V = Sinclair128KMarker`) and the
/// Amstrad-built grey +2 (`V = AmstradPlus2Marker`). The two are the same
/// hardware; the marker distinguishes catalogue identity and keeps
/// snapshots type-bound.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Spectrum128kClassCore<V: Class128kVariant> {
    pub z80: Z80,
    pub ula: SinclairUla,
    pub memory: Memory128K,
    pub framebuffer: Vec<u8>,
    pub keyboard: [u8; 8],
    /// Kempston Interface joystick. Defaults to unattached; user code
    /// flips `attached = true` when the host plugs the interface in.
    pub kempston: KempstonJoystick,
    pub tape: TapePlayer,
    /// Captures the MIC line during a SAVE for tape write-back (mirrors the 48K
    /// class). `#[serde(default)]` keeps pre-SAVE snapshots loadable.
    #[serde(default)]
    recorder: TapeRecorder,
    pub ay: Ay3_8912,
    pub audio: BeeperAudio,
    pub audio_frame: Vec<f32>,
    /// Per-frame scratch buffer for AY samples, summed into `audio_frame`
    /// at end-of-frame. Transient — populated by `ay.end_frame(...)` and
    /// consumed in the same call, so it doesn't need to survive
    /// serialization.
    #[serde(skip, default = "default_ay_frame")]
    ay_frame: Vec<f32>,

    pub(crate) hc: u32,
    speaker: SpeakerMixer,
    /// Optional CPU memory-write tracer — mirrors the field on the
    /// 48K-class core. See
    /// `common-sinclair-zx-spectrum-48k-class::SpectrumMachineCore`
    /// for semantics.
    #[serde(default)]
    write_watch: Option<common_sinclair_zx_spectrum::MemoryWriteWatch>,
    /// Optional AY register-write tracer. Captures every `OUT
    /// ($BFFD), data` with `(pc, register, value)`. `None` means no
    /// watch is active.
    #[serde(default)]
    ay_watch: Option<common_sinclair_zx_spectrum::AyWriteWatch>,

    #[serde(skip)]
    _variant: PhantomData<V>,
}

impl<V: Class128kVariant> Spectrum128kClassCore<V> {
    #[inline(always)]
    fn frame_position(&self) -> FramePosition {
        FramePosition::new(self.hc, &TIMING_128K)
    }

    #[must_use]
    pub fn new() -> Self {
        let cpu_hz = (TIMING_128K.master_hz / u64::from(TIMING_128K.cpu_divisor)) as u32;
        let ay_hz = cpu_hz / 2;
        Self {
            z80: Z80::new(),
            ula: SinclairUla::new(),
            memory: Memory128K::new(),
            framebuffer: vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT],
            keyboard: [0xFF; 8],
            kempston: KempstonJoystick::new(),
            tape: TapePlayer::new(),
            recorder: TapeRecorder::new(),
            ay: {
                let mut ay = Ay3_8912::new(ay_hz, AUDIO_SAMPLE_RATE, AUDIO_SAMPLES_PER_FRAME);
                // Sinclair 128K wiring: AY port A bit 6 is the serial CTS
                // line, tied low on the motherboard. Reads of register 14
                // therefore mask with 0xBF — the signature late-Ocean
                // loaders (Rainbow Islands, Out Run, Bubble Bobble) probe
                // for to detect "this is a real Sinclair 128K".
                ay.set_port_a_input_mask(0xBF);
                ay
            },
            audio: BeeperAudio::new(AUDIO_SAMPLE_RATE, TIMING_128K.tstates_per_frame, cpu_hz),
            audio_frame: vec![0.0; AUDIO_SAMPLES_PER_FRAME],
            ay_frame: default_ay_frame(),
            hc: 0,
            speaker: SpeakerMixer::default(),
            write_watch: None,
            ay_watch: None,
            _variant: PhantomData,
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

    /// See `SpectrumMachineCore::start_memory_write_watch` on the
    /// 48K-class core.
    pub fn start_memory_write_watch(&mut self, addr: u16, len: u16) {
        self.write_watch = Some(common_sinclair_zx_spectrum::MemoryWriteWatch::new(
            addr, len,
        ));
    }

    /// See `SpectrumMachineCore::stop_memory_write_watch`.
    pub fn stop_memory_write_watch(&mut self) {
        self.write_watch = None;
    }

    /// See `SpectrumMachineCore::memory_write_watch_records`.
    #[must_use]
    pub fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        self.write_watch.as_ref().map(|w| w.records())
    }

    /// See `SpectrumMachineCore::clear_memory_write_watch_records`.
    pub fn clear_memory_write_watch_records(&mut self) {
        if let Some(w) = &mut self.write_watch {
            w.clear();
        }
    }

    /// See `SpectrumMachineCore::memory_write_watch_range`.
    #[must_use]
    pub fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        self.write_watch
            .as_ref()
            .map(|w| (w.lo(), w.hi().wrapping_sub(w.lo())))
    }

    /// Bus-level port read — see `SpectrumMachineCore::port_read` on
    /// the 48K-class core for the rationale.
    pub fn port_read(&mut self, port: u16) -> u8 {
        self.io_read(port)
    }

    /// Bus-level port write — see `SpectrumMachineCore::port_write`.
    pub fn port_write(&mut self, port: u16, value: u8) {
        self.io_write(port, value);
    }

    /// Begin tracing every `OUT ($BFFD), data` write. Replaces any
    /// prior watch and clears the captured log.
    pub fn start_ay_write_watch(&mut self) {
        self.ay_watch = Some(common_sinclair_zx_spectrum::AyWriteWatch::new());
    }

    /// Drop the AY watch entirely. After this call the records
    /// accessor returns `None` until `start_ay_write_watch` is
    /// called again.
    pub fn stop_ay_write_watch(&mut self) {
        self.ay_watch = None;
    }

    /// Captured AY writes since the last `start_ay_write_watch`.
    #[must_use]
    pub fn ay_write_watch_records(&self) -> Option<&[common_sinclair_zx_spectrum::AyWriteRecord]> {
        self.ay_watch.as_ref().map(|w| w.records())
    }

    /// Drop captured records without removing the watch.
    pub fn clear_ay_write_watch_records(&mut self) {
        if let Some(w) = &mut self.ay_watch {
            w.clear();
        }
    }

    /// Stable hardware identifier for this variant.
    #[must_use]
    pub fn model_id(&self) -> &'static str {
        V::MODEL_ID
    }

    /// Returns mutable access to the Kempston joystick peripheral.
    ///
    /// The runtime layer's joystick input mapping reaches in here to
    /// flip button bits when a host gamepad event arrives. Mirrors the
    /// 48K-class accessor.
    #[must_use]
    pub fn kempston_mut(&mut self) -> &mut KempstonJoystick {
        &mut self.kempston
    }

    /// Returns the current half-cycle counter within the frame.
    #[must_use]
    pub const fn hc_value(&self) -> u32 {
        self.hc
    }

    /// Reattaches `&'static` references that don't survive serde's
    /// `#[serde(skip)]` round-trip, and rehydrates the Z80 walker
    /// sequence from `(prefix, opcode)`. Call once after restoring
    /// a postcard snapshot. Without it the Sinclair 7K010E reverts
    /// to 48K timing on restore.
    pub fn restore_volatile_refs(&mut self) {
        self.z80.rehydrate_walker_sequence();
        self.ula.reattach_config();
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

    /// Reset the machine's CPU and audio/timing state, keeping the
    /// loaded ROMs and RAM contents intact (matching real-hardware
    /// power-cycle semantics).
    pub fn reset(&mut self) {
        self.z80 = Z80::new();
        self.hc = 0;
        self.speaker = SpeakerMixer::default();
    }

    /// Apply a parsed `.z80` snapshot. The 128K-family snapshot layout
    /// pages banks through `$7FFD`; the AY register file is replayed
    /// from the snapshot-captured values.
    pub fn apply_snapshot(&mut self, snap: &Snapshot) {
        apply_z80_registers(&mut self.z80, snap);
        self.ula.write_fe(snap.border);
        apply_128k_bank_pages(snap, &mut self.memory);
        self.memory.write_7ffd(snap.port_7ffd);
        apply_ay_registers(snap, &mut self.ay);
    }

    /// Run exactly one PAL frame. Delegates to `SpectrumDriver::run_frame`.
    pub fn run_frame(&mut self) {
        <Self as SpectrumDriver>::run_frame(self);
    }

    /// Advance the machine by an exact number of master-clock half-cycles.
    pub fn advance_halfcycles(&mut self, halfcycles: u32) {
        <Self as SpectrumDriver>::advance_halfcycles(self, halfcycles);
    }

    /// Advance the machine by an exact number of CPU T-states.
    pub fn advance_tstates(&mut self, tstates: u32) {
        <Self as SpectrumDriver>::advance_tstates(self, tstates);
    }

    fn handle_bus(&mut self) {
        // See `amstrad-class::handle_bus` for the rationale — Z80 bus
        // strobes are level-driven, so we use `bus_request` to collapse
        // them into one transaction per M-cycle.
        match self.z80.bus_request() {
            Some(BusOp::MemRead) => {
                self.z80.data_in = self.memory.read(self.z80.addr);
            }
            Some(BusOp::MemWrite) => {
                if let Some(w) = &mut self.write_watch {
                    w.maybe_record(self.z80.regs.pc, self.z80.addr, self.z80.data);
                }
                self.memory.write(self.z80.addr, self.z80.data);
            }
            Some(BusOp::IoRead) => {
                self.z80.data_in = self.io_read(self.z80.addr);
            }
            Some(BusOp::IoWrite) => {
                self.io_write(self.z80.addr, self.z80.data);
            }
            Some(BusOp::IntAck) => {
                self.z80.data_in = 0xFF;
            }
            None => {}
        }
    }

    pub(crate) fn io_read(&mut self, port: u16) -> u8 {
        if self.kempston.claims_port(port) {
            return self.kempston.read(port);
        }
        if port & 0x0001 == 0 {
            // ULA port ($FE). Bit 6 picks up the tape EAR if playing.
            let mut val = self.ula.read_fe(port, &self.keyboard);
            if self.tape.is_playing() {
                val = (val & !0x40) | if self.tape.ear_level() { 0x40 } else { 0x00 };
            }
            val
        } else if port & 0xC002 == 0xC000 {
            // AY register read ($FFFD).
            self.ay.read_data()
        } else if port == 0x001F {
            // An unattached Kempston port floats high, independent of the
            // ULA's display-bus phase.
            0xFF
        } else {
            // The 128K odd-port bus sample is three T-states ahead of the
            // ULA table coordinate. The origin is the first contention
            // T-state established by the Fuse/Spectron reference model.
            const LIVE_BUS_ORIGIN: u32 = 14_363;
            const SAMPLE_LEAD: u32 = 3;
            let frame_tstate =
                (self.frame_position().tstate(&TIMING_128K) + LIVE_BUS_ORIGIN + SAMPLE_LEAD)
                    % TIMING_128K.tstates_per_frame;
            floating_bus_byte(
                frame_tstate,
                14_364,
                TIMING_128K.tstates_per_line,
                &self.memory,
            )
        }
    }

    pub(crate) fn io_write(&mut self, port: u16, data: u8) {
        if port & 0x0001 == 0 {
            self.ula.write_fe(data);
            let beeper = data & 0x10 != 0;
            if beeper != self.speaker.beeper {
                self.speaker.beeper = beeper;
                let tstate = self.frame_position().tstate(&TIMING_128K);
                self.audio.set_level(tstate, self.speaker.level());
            }
            // MIC (bit 3) carries the tape SAVE signal.
            self.recorder.set_mic_level(data & 0x08 != 0);
        }

        // Memory paging: port $7FFD (active when bit 1 = 0 and bit 15 = 0).
        if port & 0x8002 == 0x0000 {
            self.memory.write_7ffd(data);
        }

        // AY register select ($FFFD) and data write ($BFFD).
        if port & 0xC002 == 0xC000 {
            self.ay.select_register(data);
        } else if port & 0xC002 == 0x8000 {
            if let Some(w) = &mut self.ay_watch {
                w.record(self.z80.regs.pc, self.ay.selected_register(), data);
            }
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
}

impl<V: Class128kVariant> Default for Spectrum128kClassCore<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Class128kVariant> SpectrumDriver for Spectrum128kClassCore<V> {
    fn frame_timing(&self) -> &FrameTiming {
        &TIMING_128K
    }
    #[inline(always)]
    fn hc(&self) -> u32 {
        self.hc
    }
    #[inline(always)]
    fn hc_mut(&mut self) -> &mut u32 {
        &mut self.hc
    }
    #[inline(always)]
    fn frame_hc(&self) -> u32 {
        TIMING_128K.halfcycles_per_frame
    }
    #[inline(always)]
    fn halfcycles_per_tstate(&self) -> u32 {
        TIMING_128K.cpu_divisor
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
    fn cpu_clock_active(&self) -> bool {
        self.ula.cpu_clock_active()
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
    fn on_tstate(&mut self, hc: u32) {
        self.tape.advance_tstates(1);
        self.recorder.advance(1);
        let tstate = TIMING_128K.hc_to_tstates(hc);
        if tstate & 1 == 0 {
            self.ay.tick();
        }
        let ear = self.tape.ear_level();
        if ear != self.speaker.ear {
            self.speaker.ear = ear;
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
        self.ay.end_frame(&mut self.ay_frame);
        mix_ay_into_audio(&mut self.audio_frame, &self.ay_frame);
    }
}

fn default_ay_frame() -> Vec<f32> {
    vec![0.0; AUDIO_SAMPLES_PER_FRAME]
}

/// AY contribution to the speaker output. The AY chip's `end_frame`
/// produces unipolar samples in `0.0..=1.0` (`0.0` is genuine silence —
/// all three voices muted, envelope at zero), so the mix adds them
/// directly to the beeper signal without centring. `AY_GAIN` is chosen
/// to leave headroom for beeper SFX stacking on top of the music: the
/// beeper output already swings -0.5..+0.5, so capping AY at +0.5 keeps
/// the combined signal inside ±1.0 even at three-voice fortissimo.
const AY_GAIN: f32 = 0.5;

fn mix_ay_into_audio(audio: &mut [f32], ay: &[f32]) {
    for (out, &ay_sample) in audio.iter_mut().zip(ay.iter()) {
        *out += ay_sample * AY_GAIN;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variant::{AmstradPlus2Marker, Sinclair128KMarker};
    use common_sinclair_zx_spectrum::audio::SpeakerChannel;

    type Spectrum128K = Spectrum128kClassCore<Sinclair128KMarker>;
    type SpectrumPlus2 = Spectrum128kClassCore<AmstradPlus2Marker>;

    #[test]
    fn defaults_are_sane() {
        let m = Spectrum128K::new();
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-128k");
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(m.keyboard, [0xFF; 8]);
        assert!(!m.kempston.attached, "Kempston defaults to unattached");
        assert_eq!(m.kempston.state, 0);
    }

    #[test]
    fn plus2_advertises_distinct_model_id() {
        let m = SpectrumPlus2::new();
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-plus2");
    }

    #[test]
    fn run_frame_returns_to_origin() {
        let mut m = Spectrum128K::new();
        m.run_frame();
        assert_eq!(m.hc, 0);
    }

    #[test]
    fn advance_tstates_tracks_halfcycle_position() {
        let mut m = Spectrum128K::new();
        m.advance_tstates(7);
        // 7 T-states × 5 hc/T = 35 half-cycles
        assert_eq!(m.hc, 35);
    }

    #[test]
    fn write_7ffd_via_io_changes_paging() {
        let mut m = Spectrum128K::new();
        // Distinct values in banks 0 and 3.
        m.memory.ram_bank_mut(0)[0] = 0xAA;
        m.memory.ram_bank_mut(3)[0] = 0xBB;
        // Default: bank 0 at $C000.
        assert_eq!(m.memory.read(0xC000), 0xAA);

        // Write 3 to $7FFD via io_write — bit 15 = 0, bit 1 = 0 satisfies the mask.
        m.io_write(0x7FFD, 0x03);
        assert_eq!(m.memory.current_bank(), 3);
        assert_eq!(m.memory.read(0xC000), 0xBB);
    }

    #[test]
    fn ay_register_select_and_read_via_io() {
        let mut m = Spectrum128K::new();
        // Select AY register 8 ($FFFD with bit 14 = 1).
        m.io_write(0xFFFD, 0x08);
        // Write a value to that register ($BFFD with bit 14 = 1, bit 15 = 1).
        m.io_write(0xBFFD, 0x0A);
        // Read it back via $FFFD (port & 0xC002 == 0xC000).
        assert_eq!(m.io_read(0xFFFD), 0x0A);
    }

    #[test]
    fn plus2_shares_same_run_loop_as_128k() {
        // Smoke test confirming the +2 variant plugs into the same
        // SpectrumDriver loop as the 128K — same TIMING_128K, same
        // halfcycles-per-frame, same cadence.
        let mut m = SpectrumPlus2::new();
        m.run_frame();
        assert_eq!(m.hc, 0);
    }

    /// `io_read` on `$FE` returns the standard ULA byte: bit 6 is the
    /// EAR feedback (floating-high when no tape is playing) and bits
    /// 0-4 carry the active-low keyboard scan for the row selected
    /// by the high byte of the port.
    #[test]
    fn io_read_fe_returns_keyboard_state() {
        let mut m = Spectrum128K::new();
        // All keys released → keyboard half-row reads 0x1F (all bits set).
        let val = m.io_read(0xFEFE);
        assert_eq!(val & 0x1F, 0x1F);

        // Press Z (row 0, bit 1).
        m.keyboard[0] &= !0x02;
        let val = m.io_read(0xFEFE);
        assert_eq!(val & 0x02, 0, "Z press should clear row 0 bit 1");
    }

    /// `io_read` on Kempston port `$1F` returns the joystick state byte
    /// once the peripheral is attached, and floats high (0xFF) before
    /// any input event flips `attached = true`. The 128K family does
    /// host a Kempston interface (unlike the Amstrad class).
    #[test]
    fn io_read_kempston_port_reads_state_when_attached() {
        let mut m = Spectrum128K::new();
        // Unattached: $1F reads $FF (floating bus convention).
        assert_eq!(m.io_read(0x001F), 0xFF);
        // Attach + press fire (bit 4).
        m.kempston.attached = true;
        m.kempston.state = 0b0001_0000;
        assert_eq!(m.io_read(0x001F), 0b0001_0000);
    }

    /// `io_write` to `$FE` toggles the beeper speaker state (bit 4).
    /// The runtime's audio mixer samples this at end-of-frame and
    /// emits PCM accordingly.
    #[test]
    fn io_write_fe_toggles_beeper_state() {
        let mut m = Spectrum128K::new();
        let before = m.speaker.beeper;
        m.io_write(0x00FE, 0x10);
        assert_ne!(m.speaker.beeper, before);
        m.io_write(0x00FE, 0x00);
        assert_eq!(m.speaker.beeper, before);
    }

    /// `reset` re-initialises the Z80 + ULA + audio buffers and
    /// returns PC to $0000 without wiping the loaded ROMs or RAM.
    /// Catches soft-reset path regressions.
    #[test]
    fn reset_returns_pc_to_zero_and_preserves_ram() {
        let mut m = Spectrum128K::new();
        m.memory.ram_bank_mut(2)[0] = 0xCC;
        m.run_frame();
        m.reset();
        assert_eq!(m.z80.regs.pc, 0x0000);
        assert_eq!(
            m.memory.ram_bank(2)[0],
            0xCC,
            "RAM preserved across soft reset"
        );
    }

    /// Tape transport methods are no-ops when no tape is loaded but
    /// must not panic. Catches regressions in the tape-controller's
    /// guard clauses.
    #[test]
    fn tape_play_stop_safe_without_loaded_tape() {
        let mut m = Spectrum128K::new();
        assert!(!m.tape.is_playing());
        m.tape_play();
        m.tape_stop();
        assert!(!m.tape.is_playing());
    }

    /// `audio_controls` round-trips through `set_audio_controls` and
    /// `set_audio_channel_gain` / `set_audio_channel_enabled`. Used
    /// by the native UI's State / View menus.
    #[test]
    fn audio_controls_round_trip() {
        let mut m = Spectrum128K::new();
        let mut controls = m.audio_controls();
        controls.set_master_gain(0.42);
        m.set_audio_controls(controls);
        assert!((m.audio_controls().master_gain() - 0.42).abs() < 1e-6);

        m.set_audio_channel_enabled(SpeakerChannel::Speaker, false);
        assert!(
            !m.audio_controls()
                .channel(SpeakerChannel::Speaker)
                .enabled()
        );
        m.set_audio_channel_gain(SpeakerChannel::Speaker, 0.25);
        assert!((m.audio_controls().channel(SpeakerChannel::Speaker).gain() - 0.25).abs() < 1e-6,);
    }
}

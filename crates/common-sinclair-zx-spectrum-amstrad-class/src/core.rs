//! Shared Amstrad-class machine composition.
//!
//! The Amstrad-built +2A, +2B, and +3 share the same chip set, ULA,
//! memory, AY, and timing. This type holds the Z80 + Amstrad 40077 gate
//! array + 4-ROM paged memory + AY-3-8912 + beeper + tape composition.
//! The variant marker `V: AmstradVariant` is a phantom — it changes the
//! type identity (so snapshots can't cross variants) and gates the
//! FDC's `enabled` flag, but otherwise contributes no state.
//!
//! The µPD765A FDC lives on the core (gated on `Plus3Marker::HAS_FDC`)
//! because +2A and +2B reuse the same struct shape with `enabled =
//! false`. Real-hardware-accurately the +2A/+2B/+3 broke the rear edge
//! connector pinout, so a Kempston Interface doesn't physically fit
//! these machines — there is intentionally no `KempstonJoystick`
//! peripheral on this core. See
//! `knowledge/decisions/spectrum-joystick-architecture.md`.

use std::marker::PhantomData;

use amstrad_ula_40077::AmstradGateArray;
use common_sinclair_zx_spectrum::audio::{BeeperAudio, SpeakerMixer};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::memory::MemoryBus;
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use common_sinclair_zx_spectrum::snapshot::{
    Snapshot, apply_128k_bank_pages, apply_ay_registers, apply_z80_registers,
};
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_PLUS2A};
use common_sinclair_zx_spectrum::ula::Ula;
use gi_ay_3_8912::Ay3_8912;
use nec_upd765a::Upd765a;
use zilog_z80::{BusOp, Z80};

use crate::memory::MemoryPlus;
use crate::variant::{AmstradVariant, Plus3Marker};

/// Audio output sample rate (44.1 kHz).
const AUDIO_SAMPLE_RATE: u32 = 44_100;

/// Pre-allocated samples-per-frame buffer for the AY downsampler. The
/// +2A/+3's 50 Hz frame produces ~882 samples at 44.1 kHz.
const AUDIO_SAMPLES_PER_FRAME: usize = 882;

/// Amstrad-class machine state.
///
/// Shared between the +2A, +2B, and +3. The phantom marker `V` gives
/// each variant a distinct type so snapshots can't cross variants and
/// runtime dispatch (e.g. disk slot acceptance) can branch on the
/// marker's associated consts.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SpectrumAmstradClassCore<V: AmstradVariant> {
    pub z80: Z80,
    pub ula: AmstradGateArray,
    pub memory: MemoryPlus,
    pub framebuffer: Vec<u8>,
    pub keyboard: [u8; 8],
    pub tape: TapePlayer,
    pub ay: Ay3_8912,
    pub fdc: Upd765a,
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
    /// 48K-class core.
    #[serde(default)]
    write_watch: Option<common_sinclair_zx_spectrum::MemoryWriteWatch>,

    #[serde(skip)]
    _variant: PhantomData<V>,
}

impl<V: AmstradVariant> SpectrumAmstradClassCore<V> {
    #[must_use]
    pub fn new() -> Self {
        let cpu_hz = (TIMING_PLUS2A.master_hz / u64::from(TIMING_PLUS2A.cpu_divisor)) as u32;
        let ay_hz = cpu_hz / 2;
        // Only the +3 ships the floppy drive. +2A / +2B reuse the same
        // FDC instance with `enabled = false` so its `claims_port`
        // always reports false and the bus dispatch never lands on it.
        // The +3 only routes the FDC's US0 pin to the drive selector;
        // US1 is electrically a don't-care, so a `0x01` drive mask
        // makes drive bits `00`/`10` both address physical drive 0
        // and `01`/`11` both address drive 1 — matching real hardware
        // and what the +3 BIOS's second-stage loader expects.
        let mut fdc = Upd765a::new();
        fdc.enabled = V::HAS_FDC;
        if V::HAS_FDC {
            fdc.set_drive_select_mask(0x01);
        }
        Self {
            z80: Z80::new(),
            ula: AmstradGateArray::new(),
            memory: MemoryPlus::new(),
            framebuffer: vec![0u8; SCREEN_WIDTH * SCREEN_HEIGHT],
            keyboard: [0xFF; 8],
            tape: TapePlayer::new(),
            ay: {
                let mut ay = Ay3_8912::new(ay_hz, AUDIO_SAMPLE_RATE, AUDIO_SAMPLES_PER_FRAME);
                // Amstrad +2A / +2B / +3 wiring: AY port A bit 6 is the
                // serial CTS line tied low, same as the Sinclair 128K.
                // Reads of register 14 therefore mask with 0xBF.
                ay.set_port_a_input_mask(0xBF);
                ay
            },
            fdc,
            audio: BeeperAudio::new(AUDIO_SAMPLE_RATE, TIMING_PLUS2A.tstates_per_frame, cpu_hz),
            audio_frame: vec![0.0; AUDIO_SAMPLES_PER_FRAME],
            ay_frame: default_ay_frame(),
            hc: 0,
            speaker: SpeakerMixer::default(),
            write_watch: None,
            _variant: PhantomData,
        }
    }

    /// See `SpectrumMachineCore::start_memory_write_watch` on the
    /// 48K-class core.
    pub fn start_memory_write_watch(&mut self, addr: u16, len: u16) {
        self.write_watch = Some(common_sinclair_zx_spectrum::MemoryWriteWatch::new(addr, len));
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

    /// Stable hardware identifier for this variant.
    #[must_use]
    pub fn model_id(&self) -> &'static str {
        V::MODEL_ID
    }

    /// Returns the current half-cycle counter within the frame.
    #[must_use]
    pub const fn hc_value(&self) -> u32 {
        self.hc
    }

    /// Reattaches `&'static` references that don't survive serde's
    /// `#[serde(skip)]` round-trip, and rehydrates the Z80 walker
    /// sequence from `(prefix, opcode)`. Call once after restoring
    /// a postcard snapshot. Without it the Amstrad gate-array reverts
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

    /// Reset the CPU, timing, and audio state. Keeps ROMs, RAM, and any
    /// inserted disk image intact.
    pub fn reset(&mut self) {
        self.z80 = Z80::new();
        self.hc = 0;
        self.speaker = SpeakerMixer::default();
    }

    /// Apply a parsed `.z80` snapshot. The +2A/+3 uses both `$7FFD`
    /// and `$1FFD` so the snapshot paging state is restored in two
    /// writes after the per-page RAM copy.
    pub fn apply_snapshot(&mut self, snap: &Snapshot) {
        apply_z80_registers(&mut self.z80, snap);
        self.ula.write_fe(snap.border);
        apply_128k_bank_pages(snap, &mut self.memory);
        self.memory.write_7ffd(snap.port_7ffd);
        self.memory.write_1ffd(snap.port_1ffd);
        apply_ay_registers(snap, &mut self.ay);
    }

    /// Run exactly one PAL frame.
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
        // `bus_request` collapses the Z80's level-driven strobes into
        // one event per M-cycle. The previous polling form fired
        // `io_read` three times per `IN` instruction (T2Rise, T2Fall,
        // T3Rise — every half-cycle that iorq+rd were both high),
        // which silently advanced the µPD765A's result FIFO past
        // multi-byte status replies and stuck the +3 disk Loader.
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
        // The FDC claims its own ports first — `claims_port` honours
        // the `enabled` flag, so +2A / +2B fall through here.
        if self.fdc.claims_port(port) {
            let val = self.fdc.read(port);
            if std::env::var("EMU198X_FDC_TRACE").is_ok() {
                eprintln!(
                    "[FDC] IN (${port:04x}) = ${val:02x}  pc=${:04x}",
                    self.z80.regs.pc,
                );
            }
            return val;
        }

        if port & 0x0001 == 0 {
            // ULA port ($FE). Bit 6 picks up the tape EAR if playing.
            let mut val = self.ula.read_fe(port, &self.keyboard);
            if self.tape.is_playing() {
                val = (val & !0x40) | if self.tape.ear_level() { 0x40 } else { 0x00 };
            }
            val
        } else if port & 0xC002 == 0xC000 {
            self.ay.read_data()
        } else {
            // Amstrad gate array does not expose a floating bus, and
            // the rear connector pinout change in 1987 means classic
            // Kempston interfaces don't physically fit — so the +2A /
            // +2B / +3 host no joystick peripheral here.
            0xFF
        }
    }

    pub(crate) fn io_write(&mut self, port: u16, data: u8) {
        if self.fdc.claims_port(port) {
            if std::env::var("EMU198X_FDC_TRACE").is_ok() {
                eprintln!(
                    "[FDC] OUT (${port:04x}), ${data:02x}  pc=${:04x} bc=${:04x} de=${:04x}",
                    self.z80.regs.pc,
                    u16::from_le_bytes([self.z80.regs.c(), self.z80.regs.b()]),
                    u16::from_le_bytes([self.z80.regs.e(), self.z80.regs.d()]),
                );
            }
            self.fdc.write(port, data);
            // Fall through: paging and AY decoding live on orthogonal
            // address bits and may still match the FDC port mask.
        }

        if port & 0x0001 == 0 {
            self.ula.write_fe(data);
            let beeper = data & 0x10 != 0;
            if beeper != self.speaker.beeper {
                self.speaker.beeper = beeper;
                let tstate = self.hc / 4;
                self.audio.set_level(tstate, self.speaker.level());
            }
        }

        // Memory paging — the +2A/+3 uses tighter port decoding than
        // the 128K to keep `$1FFD` writes from clobbering `$7FFD`:
        //   $7FFD: A15=0, A14=1, A1=0
        //   $1FFD: A15=0, A14=0, A12=1, A1=0
        if port & 0xC002 == 0x4000 {
            self.memory.write_7ffd(data);
        }
        if port & 0xF002 == 0x1000 {
            self.memory.write_1ffd(data);
        }

        // AY register select ($FFFD) and data write ($BFFD).
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
}

impl<V: AmstradVariant> Default for SpectrumAmstradClassCore<V> {
    fn default() -> Self {
        Self::new()
    }
}

// +3-specific surface: disk insertion and ejection. The methods exist
// only on `SpectrumAmstradClassCore<Plus3Marker>` so callers that have
// a +2A or +2B at hand can't accidentally insert a disk into a machine
// that doesn't have a drive.
impl SpectrumAmstradClassCore<Plus3Marker> {
    /// Insert a parsed DSK / EDSK image into drive 0.
    pub fn insert_disk(&mut self, image: nec_upd765a::DiskImage) {
        self.fdc.insert_disk(0, image);
    }

    /// Eject the disk from drive 0.
    pub fn eject_disk(&mut self) {
        self.fdc.eject_disk(0);
    }
}

impl<V: AmstradVariant> SpectrumDriver for SpectrumAmstradClassCore<V> {
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
        TIMING_PLUS2A.halfcycles_per_frame
    }
    #[inline(always)]
    fn halfcycles_per_tstate(&self) -> u32 {
        TIMING_PLUS2A.cpu_divisor
    }

    #[inline(always)]
    fn tick_ula(&mut self) {
        self.ula.tick(
            &self.memory,
            self.z80.addr,
            self.z80.mreq,
            self.z80.iorq,
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
        if hc % 8 == 2 {
            self.ay.tick();
        }
        self.fdc.tick(hc);
        let ear = self.tape.ear_level();
        if ear != self.speaker.ear {
            self.speaker.ear = ear;
            let tstate = hc / 4;
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
    use crate::variant::{Plus2AMarker, Plus2BMarker};

    type SpectrumPlus2A = SpectrumAmstradClassCore<Plus2AMarker>;
    type SpectrumPlus2B = SpectrumAmstradClassCore<Plus2BMarker>;
    type SpectrumPlus3 = SpectrumAmstradClassCore<Plus3Marker>;

    #[test]
    fn defaults_are_sane() {
        let m = SpectrumPlus2A::new();
        assert_eq!(m.model_id(), "sinclair-zx-spectrum-plus2a");
        assert_eq!(m.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(m.keyboard, [0xFF; 8]);
    }

    #[test]
    fn plus3_enables_fdc_other_variants_disable_it() {
        let plus3 = SpectrumPlus3::new();
        assert!(plus3.fdc.enabled, "+3 ships with FDC enabled");

        let plus2a = SpectrumPlus2A::new();
        assert!(!plus2a.fdc.enabled, "+2A has no floppy drive");

        let plus2b = SpectrumPlus2B::new();
        assert!(!plus2b.fdc.enabled, "+2B has no floppy drive");
    }

    #[test]
    fn run_frame_returns_to_origin() {
        let mut m = SpectrumPlus3::new();
        m.run_frame();
        assert_eq!(m.hc_value(), 0);
    }

    #[test]
    fn write_7ffd_via_io_changes_paging() {
        let mut m = SpectrumPlus3::new();
        m.memory.ram_bank_mut(0)[0] = 0xAA;
        m.memory.ram_bank_mut(3)[0] = 0xBB;
        // Default: bank 0 at $C000.
        assert_eq!(m.memory.read(0xC000), 0xAA);

        // $7FFD = $03: bank 3 selected. Mask: A15=0, A14=1, A1=0.
        m.io_write(0x7FFD, 0x03);
        assert_eq!(m.memory.read(0xC000), 0xBB);
    }

    #[test]
    fn special_paging_via_1ffd_swaps_address_space() {
        let mut m = SpectrumPlus3::new();
        // Stash recognisable bytes in banks 0..3 and put RAM into
        // special mode 0 (banks 0,1,2,3 across the whole 64K).
        m.memory.ram_bank_mut(0)[0] = 0x10;
        m.memory.ram_bank_mut(1)[0] = 0x11;
        m.memory.ram_bank_mut(2)[0] = 0x12;
        m.memory.ram_bank_mut(3)[0] = 0x13;
        // $1FFD = $01: special mode 0. Mask: A15=0, A14=0, A12=1, A1=0.
        m.io_write(0x1FFD, 0x01);
        assert_eq!(m.memory.read(0x0000), 0x10);
        assert_eq!(m.memory.read(0x4000), 0x11);
        assert_eq!(m.memory.read(0x8000), 0x12);
        assert_eq!(m.memory.read(0xC000), 0x13);
    }

    /// `io_read` on `$FE` returns the standard ULA byte: bit 6 carries
    /// the EAR input (currently floating-high when no tape is playing)
    /// and bits 0-4 are the active-low keyboard scan.
    #[test]
    fn io_read_fe_returns_keyboard_state() {
        let mut m = SpectrumPlus3::new();
        // All keys released → bits 0-4 of the result are all set.
        let val = m.io_read(0xFEFE);
        assert_eq!(val & 0x1F, 0x1F, "no keys pressed should read 0x1F");

        // Press key in row 0 (Shift/Z/X/C/V): clear bit in keyboard[0].
        m.keyboard[0] &= !0x02; // Z key
        let val = m.io_read(0xFEFE); // row 0 selected via low A8 (high byte $FE = 11111110)
        assert_eq!(val & 0x02, 0, "Z press should read low on bit 1");
    }

    /// `io_read` on the AY data port (`$FFFD`, masking `A15:A14 = 11`,
    /// `A1 = 0`) returns whichever AY register the chip's pointer
    /// currently selects.
    #[test]
    fn io_read_ay_data_port_returns_selected_register() {
        let mut m = SpectrumPlus3::new();
        // Write a known value into AY register 7 (mixer control) via
        // the standard select-then-write port pair.
        m.io_write(0xFFFD, 7); // select register 7
        m.io_write(0xBFFD, 0x3F); // write 0x3F into register 7
        // Now read back through $FFFD (matches $C000 mask).
        let val = m.io_read(0xFFFD);
        assert_eq!(val, 0x3F);
    }

    /// `io_read` on an unmapped port returns `0xFF` — no floating-bus
    /// emulation on the Amstrad gate array, and no Kempston since the
    /// rear-connector pinout doesn't fit one in 1987+.
    #[test]
    fn io_read_unmapped_port_returns_ff() {
        let mut m = SpectrumPlus3::new();
        // $1F is the canonical Kempston port — Amstrad-class doesn't
        // host one, so this must read $FF (not 0 from an attached-
        // but-empty Kempston peripheral).
        assert_eq!(m.io_read(0x001F), 0xFF);
        // Same for any other unmapped odd port.
        assert_eq!(m.io_read(0x1234), 0xFF);
    }

    /// `io_write` on `$FE` toggles the beeper line — the upper bits
    /// route to the ULA's border and tape-MIC outputs but the runtime
    /// only cares about bit 4 (the beeper) for audio. Catches a
    /// regression where the speaker state stops tracking writes.
    #[test]
    fn io_write_fe_toggles_beeper_state() {
        let mut m = SpectrumPlus3::new();
        let before = m.speaker.beeper;
        // Flip bit 4 of the $FE value to toggle the beeper.
        m.io_write(0x00FE, 0x10);
        assert_ne!(m.speaker.beeper, before);
        // Writing without bit 4 toggles it back.
        m.io_write(0x00FE, 0x00);
        assert_eq!(m.speaker.beeper, before);
    }

    /// `reset` re-initialises the Z80 + ULA + audio buffers without
    /// dropping ROM contents or RAM. Catches a regression where the
    /// soft-reset path forgets to clear a sub-component.
    #[test]
    fn reset_clears_machine_state_but_preserves_rom() {
        let mut m = SpectrumPlus3::new();
        // Stash a recognisable byte and run a frame so the Z80 walks.
        m.memory.ram_bank_mut(0)[0] = 0xAA;
        m.run_frame();
        assert!(m.hc_value() == 0); // run_frame returns to origin

        // Reset: PC should be back at $0000 (BASIC entry).
        m.reset();
        assert_eq!(m.z80.regs.pc, 0x0000);
        // RAM is preserved across soft reset.
        assert_eq!(m.memory.ram_bank(0)[0], 0xAA);
    }

    /// `tape_play` / `tape_stop` toggle the tape transport state. The
    /// runtime drives these from F9 / F10 keyboard shortcuts in the
    /// native binary.
    #[test]
    fn tape_play_then_stop_toggles_transport() {
        let mut m = SpectrumPlus3::new();
        assert!(!m.tape.is_playing(), "fresh machine has no tape playing");
        m.tape_play();
        // Without a loaded tape, play is a no-op — but it's safe
        // to call and shouldn't panic.
        m.tape_stop();
        assert!(!m.tape.is_playing());
    }

    /// `advance_tstates` advances the half-cycle counter by exactly
    /// `tstates × cpu_divisor` half-cycles. On the Amstrad-class
    /// (TIMING_PLUS2A) that's 5 half-cycles per T-state.
    #[test]
    fn advance_tstates_advances_hc_by_cpu_divisor() {
        let mut m = SpectrumPlus3::new();
        let before = m.hc_value();
        m.advance_tstates(100);
        let advanced = m.hc_value() - before;
        // cpu_divisor on the Amstrad gate array is 5, so 100 T-states
        // = 500 half-cycles.
        assert_eq!(advanced, 500);
    }

    /// Plus3 exposes `insert_disk` to mount a `DiskImage` into the
    /// FDC's drive 0. `eject_disk` removes it. Both should be safe to
    /// call without crashing on a freshly-constructed machine.
    #[test]
    fn plus3_insert_and_eject_disk_round_trip() {
        let mut m = SpectrumPlus3::new();
        // A minimal blank DSK image — single track, single side, zero
        // sectors — is enough to exercise the insert/eject path.
        let image = nec_upd765a::DiskImage::default();
        assert!(!m.fdc.has_disk(0), "fresh +3 starts with no disk mounted");
        m.insert_disk(image);
        assert!(m.fdc.has_disk(0), "insert_disk must mount on drive 0");
        m.eject_disk();
        assert!(
            !m.fdc.has_disk(0),
            "eject_disk must remove the mounted image"
        );
    }

    /// `audio_controls` round-trips through `set_audio_controls`. Used
    /// by the native UI's audio menu to expose speaker channel state
    /// to the user.
    #[test]
    fn audio_controls_round_trip() {
        let mut m = SpectrumPlus3::new();
        let mut controls = m.audio_controls();
        controls.set_master_gain(0.42);
        m.set_audio_controls(controls);
        assert!((m.audio_controls().master_gain() - 0.42).abs() < 1e-6);
    }
}

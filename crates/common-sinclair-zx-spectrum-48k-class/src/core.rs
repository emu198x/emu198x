//! Shared 48K-class machine composition.
//!
//! The 16K, 48K, and Spectrum+ share the same Ferranti-ULA-driven
//! composition (Z80, beeper, tape). This type carries that composition
//! parameterised over both the memory map (`M`) and a phantom variant
//! marker (`V`). Variant crates alias it (e.g.
//! `pub type Spectrum48k = SpectrumMachineCore<Spectrum48kMemory, Spectrum48kMarker>;`)
//! so that snapshots cannot cross variants and per-machine metadata can
//! attach at the marker level.
//!
//! Variants outside the 48K-class — 128K-family, Pentagon, Scorpion,
//! Timex — have their own ULAs and additional state (AY, paging, FDC)
//! and keep their own machine implementations.

use std::marker::PhantomData;

use common_sinclair_zx_spectrum::audio::{
    AudioControls, BeeperAudio, SpeakerChannel, SpeakerMixer,
};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::error::RomImageError;
use common_sinclair_zx_spectrum::keyboard::KeyboardMatrix;
use common_sinclair_zx_spectrum::memory::{MemoryBus, Spectrum16kMemory, Spectrum48kMemory};
use common_sinclair_zx_spectrum::peripheral::Peripheral;
use common_sinclair_zx_spectrum::snapshot::{Snapshot, apply_48k_pages, apply_z80_registers};
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::tape_recorder::TapeRecorder;
use common_sinclair_zx_spectrum::timing::{
    FramePosition, FrameTiming, SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_48K,
};
use common_sinclair_zx_spectrum::ula::Ula;
use ferranti_ula_6c001e::{FerrantiUla, UlaRevision};
use peripheral_kempston_joystick::KempstonJoystick;
use zilog_z80::{BusOp, IO_READ_DATA_LATCH_LEAD_TSTATES, Z80};

use crate::tape_input::TapeInput;
use crate::variant::Variant48kClass;

const AUDIO_SAMPLE_RATE: u32 = 44_100;

fn cpu_hz() -> u32 {
    (TIMING_48K.master_hz / u64::from(TIMING_48K.cpu_divisor)) as u32
}

fn make_audio() -> BeeperAudio {
    BeeperAudio::new(AUDIO_SAMPLE_RATE, TIMING_48K.tstates_per_frame, cpu_hz())
}

/// Machine-local state for any 48K-class Spectrum (16K, 48K, Spectrum+).
///
/// Composed of a pin-level Z80, the Ferranti 6C001E ULA, a memory map
/// `M`, the shared keyboard matrix, the tape player + EAR line, and the
/// beeper / speaker mixer feeding a single `audio_frame`. The half-cycle
/// counter `hc` and the `framebuffer` are owned here; everything else is
/// re-used from `common-sinclair-zx-spectrum`. The phantom `V` marker
/// distinguishes variants that share a memory map (48K vs Spectrum+) at
/// the type level.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SpectrumMachineCore<M: MemoryBus, V: Variant48kClass> {
    z80: Z80,
    ula: FerrantiUla,
    memory: M,
    keyboard: KeyboardMatrix,
    /// Kempston Interface joystick. Defaults to unattached — the
    /// peripheral only claims the `$1F`-mirror port range when a real
    /// user plugs in the interface (matching real hardware, where a
    /// disconnected port reads floating bus, not zero).
    pub kempston: KempstonJoystick,
    tape: TapePlayer,
    /// Captures the MIC line during a `SAVE` so the signal can be flushed back
    /// to a writable tape file. `#[serde(default)]` keeps older snapshots
    /// (written before tape SAVE) loadable.
    #[serde(default)]
    recorder: TapeRecorder,
    tape_input: TapeInput,
    audio: BeeperAudio,
    audio_frame: Vec<f32>,
    speaker: SpeakerMixer,
    framebuffer: Vec<u8>,
    hc: u32,
    /// Optional CPU memory-write tracer. When `Some`, every Z80
    /// mreq+wr cycle whose target falls inside the configured
    /// range is captured into the watch's buffer along with the
    /// current PC. Inactive by default — see
    /// [`Self::start_memory_write_watch`] and
    /// [`Self::memory_write_watch_records`].
    #[serde(default)]
    write_watch: Option<common_sinclair_zx_spectrum::MemoryWriteWatch>,

    #[serde(skip)]
    _variant: PhantomData<V>,
}

impl<M: MemoryBus, V: Variant48kClass> SpectrumMachineCore<M, V> {
    /// Creates a 48K-class machine for the requested ULA revision and
    /// caller-supplied memory map.
    #[must_use]
    pub fn with_revision_and_memory(revision: UlaRevision, memory: M) -> Self {
        let audio = make_audio();
        let audio_frame = vec![0.0; audio.samples_per_frame()];
        Self {
            z80: Z80::new(),
            ula: FerrantiUla::new(revision),
            memory,
            keyboard: KeyboardMatrix::new(),
            kempston: KempstonJoystick::new(),
            tape: TapePlayer::new(),
            recorder: TapeRecorder::new(),
            tape_input: TapeInput::new(),
            audio,
            audio_frame,
            speaker: SpeakerMixer::default(),
            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT],
            hc: 0,
            write_watch: None,
            _variant: PhantomData,
        }
    }

    /// Begin tracing every Z80 memory write whose target falls
    /// inside `[addr, addr + len)`. Replaces any prior watch.
    pub fn start_memory_write_watch(&mut self, addr: u16, len: u16) {
        self.write_watch = Some(common_sinclair_zx_spectrum::MemoryWriteWatch::new(
            addr, len,
        ));
    }

    /// Drop the current watch entirely. After this call,
    /// [`Self::memory_write_watch_records`] returns `None` until
    /// [`Self::start_memory_write_watch`] is called again.
    pub fn stop_memory_write_watch(&mut self) {
        self.write_watch = None;
    }

    /// Captured writes since the last `start_memory_write_watch`.
    /// `None` when no watch is configured.
    #[must_use]
    pub fn memory_write_watch_records(
        &self,
    ) -> Option<&[common_sinclair_zx_spectrum::MemoryWriteRecord]> {
        self.write_watch.as_ref().map(|w| w.records())
    }

    /// Drop captured records without removing the watch range.
    pub fn clear_memory_write_watch_records(&mut self) {
        if let Some(w) = &mut self.write_watch {
            w.clear();
        }
    }

    /// Current watch range as `(addr, len)`, where `len = hi - lo`.
    /// Returns `None` when no watch is configured.
    #[must_use]
    pub fn memory_write_watch_range(&self) -> Option<(u16, u16)> {
        self.write_watch
            .as_ref()
            .map(|w| (w.lo(), w.hi().wrapping_sub(w.lo())))
    }

    /// Reads one Z80 I/O port directly through the bus-level handler.
    /// Mirrors what an `IN A,(C)` would observe but without driving
    /// the CPU through the synthetic instruction — used by debug /
    /// curriculum tools to inspect the ULA, Kempston, or AY data
    /// path without disturbing CPU timing.
    pub fn port_read(&mut self, port: u16) -> u8 {
        self.io_read(port)
    }

    /// Writes one Z80 I/O port directly through the bus-level
    /// handler. Equivalent in effect to an `OUT (C),A` (border colour,
    /// beeper level, paging, AY register select, …) without driving
    /// the CPU through the synthetic instruction.
    pub fn port_write(&mut self, port: u16, value: u8) {
        self.io_write(port, value);
    }

    /// Returns mutable access to the Kempston joystick peripheral.
    ///
    /// The runtime layer's joystick input mapping reaches in here to
    /// flip button bits when a host gamepad event arrives.
    #[must_use]
    pub fn kempston_mut(&mut self) -> &mut KempstonJoystick {
        &mut self.kempston
    }

    /// Returns the configured ULA revision (5C vs 6C family).
    #[must_use]
    pub const fn revision(&self) -> UlaRevision {
        self.ula.revision()
    }

    /// Returns the memory map.
    #[must_use]
    pub fn memory(&self) -> &M {
        &self.memory
    }

    /// Returns mutable access to the memory map.
    #[must_use]
    pub fn memory_mut(&mut self) -> &mut M {
        &mut self.memory
    }

    /// Returns the pin-level Z80 core.
    #[must_use]
    pub fn z80(&self) -> &Z80 {
        &self.z80
    }

    /// Returns mutable access to the pin-level Z80 core.
    #[must_use]
    pub fn z80_mut(&mut self) -> &mut Z80 {
        &mut self.z80
    }

    /// Returns the Ferranti ULA. Used by waypoint tests that need
    /// to introspect floating bus, INT timing, and rendering pipeline
    /// state without going through the full machine surface.
    #[must_use]
    pub fn ula(&self) -> &FerrantiUla {
        &self.ula
    }

    /// Mutable ULA access, for arming the half-cycle recorder.
    #[doc(hidden)]
    pub fn ula_mut(&mut self) -> &mut FerrantiUla {
        &mut self.ula
    }

    /// Reattaches `&'static` references that don't survive serde's
    /// `#[serde(skip)]` round-trip, and rehydrates the Z80 walker
    /// sequence from the preserved sequence identity and opcode. Call
    /// once after restoring a postcard snapshot.
    pub fn restore_volatile_refs(&mut self) {
        self.z80.rehydrate_walker_sequence();
        self.ula.reattach_config();
    }

    /// Returns the current framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Returns mutable framebuffer access.
    #[must_use]
    pub fn framebuffer_mut(&mut self) -> &mut [u8] {
        &mut self.framebuffer
    }

    /// Returns the current mono audio frame.
    #[must_use]
    pub fn audio_frame(&self) -> &[f32] {
        &self.audio_frame
    }

    /// Returns the output sample rate for the beeper/EAR mixer.
    #[must_use]
    pub const fn audio_sample_rate(&self) -> u32 {
        AUDIO_SAMPLE_RATE
    }

    /// Returns the number of mono samples emitted per video frame.
    #[must_use]
    pub fn audio_samples_per_frame(&self) -> usize {
        self.audio_frame.len()
    }

    /// Current host-side speaker audio controls.
    #[must_use]
    pub const fn audio_controls(&self) -> AudioControls {
        self.audio.audio_controls()
    }

    /// Replace all host-side speaker audio controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.audio.set_audio_controls(controls);
    }

    /// Enable or disable the speaker in the host mixer.
    pub fn set_audio_channel_enabled(&mut self, channel: SpeakerChannel, enabled: bool) {
        self.audio.set_audio_channel_enabled(channel, enabled);
    }

    /// Set speaker host mixer gain.
    pub fn set_audio_channel_gain(&mut self, channel: SpeakerChannel, gain: f32) {
        self.audio.set_audio_channel_gain(channel, gain);
    }

    /// Returns the current half-cycle counter.
    #[must_use]
    pub const fn hc(&self) -> u32 {
        self.hc
    }

    /// Returns the current T-state position within the frame.
    #[must_use]
    pub fn tstate_in_frame(&self) -> u32 {
        self.frame_position().tstate(&TIMING_48K)
    }

    #[inline(always)]
    fn frame_position(&self) -> FramePosition {
        FramePosition::new(self.hc, &TIMING_48K)
    }

    /// Returns the keyboard matrix.
    #[must_use]
    pub fn keyboard(&self) -> &KeyboardMatrix {
        &self.keyboard
    }

    /// Returns mutable access to the keyboard matrix.
    #[must_use]
    pub fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        &mut self.keyboard
    }

    /// Returns the current tape input line state.
    #[must_use]
    pub const fn tape_input(&self) -> TapeInput {
        self.tape_input
    }

    /// Sets whether the tape input is connected.
    pub fn set_tape_connected(&mut self, connected: bool) {
        self.tape_input.set_connected(connected);
        self.sync_ear_level();
    }

    /// Sets the current tape EAR level.
    pub fn set_tape_level(&mut self, level: bool) {
        self.tape_input.set_level(level);
        self.sync_ear_level();
    }

    /// Loads a raw pulse stream as the current tape media.
    pub fn load_tape_pulses(&mut self, pulses: Vec<u32>) {
        self.tape.load_pulses(pulses);
        self.sync_ear_level();
    }

    /// Loads a timing stream as the current tape media.
    pub fn load_tape_stream(&mut self, stream: Vec<TapeSpan>) {
        self.tape.load_stream(stream);
        self.sync_ear_level();
    }

    /// Loads standard-speed tape blocks as the current tape media.
    pub fn load_tape_blocks(&mut self, blocks: Vec<TapeBlock>) {
        self.tape.load_blocks(blocks);
        self.sync_ear_level();
    }

    /// Starts or resumes emulated tape playback.
    pub fn play_tape(&mut self) {
        self.tape.play();
        self.sync_ear_level();
    }

    /// Stops emulated tape playback without rewinding it.
    pub fn stop_tape(&mut self) {
        self.tape.stop();
        self.sync_ear_level();
    }

    /// Returns whether emulated tape media is currently loaded.
    #[must_use]
    pub fn tape_is_loaded(&self) -> bool {
        self.tape.has_tape()
    }

    /// Returns whether emulated tape playback is currently active.
    #[must_use]
    pub fn tape_is_playing(&self) -> bool {
        self.tape.is_playing()
    }

    /// Diagnostic accessor for tape playback state.
    #[must_use]
    pub fn tape(&self) -> &TapePlayer {
        &self.tape
    }

    /// The tape SAVE recorder capturing the MIC line.
    #[must_use]
    pub fn recorder(&self) -> &TapeRecorder {
        &self.recorder
    }

    /// Decodes any captured `SAVE` signal into standard-speed tape blocks.
    #[must_use]
    pub fn recorded_tape_blocks(&self) -> Vec<TapeBlock> {
        self.recorder.decode()
    }

    /// Discards captured `SAVE` signal (e.g. after flushing it to a file).
    pub fn clear_tape_recording(&mut self) {
        self.recorder.clear();
    }

    /// Returns the current border colour.
    #[must_use]
    pub fn border_color(&self) -> u8 {
        self.ula.border_color()
    }

    /// Writes to port `$FE`.
    pub fn write_fe(&mut self, value: u8) {
        self.ula.write_fe(value);
        self.sync_beeper_level(value);
        // MIC (bit 3) carries the tape SAVE signal. It only toggles during a
        // SAVE, so capturing every write is cheap; the recorder ignores
        // no-change writes and the runtime decides whether to flush it.
        self.recorder.set_mic_level(value & 0x08 != 0);
    }

    /// Reads port `$FE`.
    #[must_use]
    pub fn read_fe(&self, port: u16) -> u8 {
        let mut value = self.ula.read_fe(port, self.keyboard.rows());
        if let Some(level) = self.current_tape_level() {
            value = (value & !0x40) | if level { 0x00 } else { 0x40 };
        }
        value
    }

    /// Resets the pin-level CPU and ULA while keeping the loaded ROM and RAM.
    pub fn reset(&mut self) {
        let revision = self.revision();
        self.z80 = Z80::new();
        self.ula = FerrantiUla::new(revision);
        self.audio = make_audio();
        self.audio_frame.fill(0.0);
        self.speaker = SpeakerMixer::default();
        self.hc = 0;
        self.framebuffer.fill(0);
        self.sync_ear_level();
    }

    /// Applies a parsed `.sna` / `.z80` snapshot. The 48K-mode page
    /// numbering (8/4/5 → $4000/$8000/$C000) is the .z80 v2/v3 spec's
    /// region-based scheme, distinct from 128K-class bank-numbered
    /// pages.
    pub fn apply_snapshot(&mut self, snap: &Snapshot) {
        apply_z80_registers(&mut self.z80, snap);
        self.ula.write_fe(snap.border);
        apply_48k_pages(snap, &mut self.memory);
    }

    /// Runs one native 48K video frame. Delegates to `SpectrumDriver::run_frame`.
    pub fn run_frame(&mut self) {
        <Self as SpectrumDriver>::run_frame(self);
    }

    /// Advances the machine by an exact number of master-clock half-cycles.
    pub fn advance_halfcycles(&mut self, halfcycles: u32) {
        <Self as SpectrumDriver>::advance_halfcycles(self, halfcycles);
    }

    /// Advances the machine by an exact number of CPU T-states.
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
                self.z80.data_in = 0xff;
            }
            None => {}
        }
    }

    fn io_read(&mut self, port: u16) -> u8 {
        if self.kempston.claims_port(port) {
            return self.kempston.read(port);
        }
        if port & 0x01 == 0 {
            self.read_fe(port)
        } else {
            self.floating_bus_read()
        }
    }

    /// Read the floating bus for an unused odd-port `IN`.
    ///
    /// Our `io_read` fires when the I/O transaction resolves — the `/IORQ`
    /// rising edge — while the CPU latches the data bus at the end of the
    /// M-cycle. The floating bus moves within that gap, so it has to be
    /// read at the latch, not at the edge.
    ///
    /// The gap is [`IO_READ_DATA_LATCH_LEAD_TSTATES`], **derived from the
    /// I/O M-cycle's geometry and shared by every variant**. It used to be
    /// a `SAMPLE_LEAD` fitted here and a second one fitted in the
    /// 128K-class core, which is how the same one-T-state error came to be
    /// hidden twice over (#851).
    ///
    /// `ORIGIN` maps our frame T-state 0 onto FUSE's frame, and it is
    /// libspectrum's `top_left_pixel` for this ULA —
    /// `timings_frame_ferranti_5c_6c` in `timings.c`. The 128K-class core
    /// uses the same rule against `timings_frame_ferranti_7c`.
    ///
    /// **One T-state is still unaccounted for, and it is not this
    /// lead.** Float48K reads 14337 against Woody's hardware-measured
    /// 14338. `io_contention_oracle`'s `ORIGIN` — pinned to the `/INT`
    /// edge by `the_frame_origin_is_pinned_by_the_interrupt`, which is a
    /// measurement — puts our T-state 0 at FUSE's **14335**, one earlier
    /// than the `top_left_pixel` used here. Dropping both origins by one
    /// to match it would put Float48K on Woody's 14338 and take Float128K
    /// off 14364, so the open question is between the interrupt anchor and
    /// `top_left_pixel`, not between the two machines. Recorded rather
    /// than fitted; see #851.
    fn floating_bus_read(&self) -> u8 {
        /// libspectrum `timings_frame_ferranti_5c_6c.top_left_pixel`.
        const ORIGIN: u32 = 14_336;
        const FLOAT_START: u32 = 14_338; // Spectron FloatingBusStartTicks (48K)
        let frame = TIMING_48K.tstates_per_frame;
        let t = (self.tstate_in_frame() + ORIGIN + IO_READ_DATA_LATCH_LEAD_TSTATES) % frame;
        common_sinclair_zx_spectrum::ula_engine::floating_bus_byte(
            t,
            FLOAT_START,
            TIMING_48K.tstates_per_line,
            &self.memory,
        )
    }

    fn io_write(&mut self, port: u16, data: u8) {
        if port & 0x01 == 0 {
            self.write_fe(data);
        }
    }

    fn current_tape_level(&self) -> Option<bool> {
        if self.tape_input.connected() {
            Some(self.tape_input.level())
        } else if self.tape.is_playing() {
            Some(self.tape.ear_level())
        } else {
            None
        }
    }

    fn current_tstate(&self) -> u32 {
        self.tstate_in_frame()
    }

    fn sync_beeper_level(&mut self, value: u8) {
        let beeper = value & 0x10 != 0;
        if beeper != self.speaker.beeper {
            self.speaker.beeper = beeper;
            self.audio
                .set_level(self.current_tstate(), self.speaker.level());
        }
    }

    fn sync_ear_level(&mut self) {
        let ear = self.current_tape_level().unwrap_or(false);
        if ear != self.speaker.ear {
            self.speaker.ear = ear;
            self.audio
                .set_level(self.current_tstate(), self.speaker.level());
        }
    }
}

impl<M: MemoryBus, V: Variant48kClass> SpectrumDriver for SpectrumMachineCore<M, V> {
    fn frame_timing(&self) -> &FrameTiming {
        &TIMING_48K
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
    fn on_tstate(&mut self, _position: common_sinclair_zx_spectrum::timing::FramePosition) {
        self.tape.advance_tstates(1);
        self.recorder.advance(1);
        self.sync_ear_level();
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

impl<M: MemoryBus, V: Variant48kClass> MemoryBus for SpectrumMachineCore<M, V> {
    fn read(&self, addr: u16) -> u8 {
        self.memory.read(addr)
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.memory.write(addr, value);
    }

    fn is_contended(&self, addr: u16) -> bool {
        self.memory.is_contended(addr)
    }

    fn read_screen(&self, addr: u16) -> u8 {
        self.memory.read_screen(addr)
    }
}

// 48K-class shortcuts (Spectrum48kMemory backed: 48K + Spectrum+).
impl<V: Variant48kClass> SpectrumMachineCore<Spectrum48kMemory, V> {
    /// Creates a Ferranti 6C 48K-class machine (Issue 3+) with
    /// deterministic startup state.
    #[must_use]
    pub fn new() -> Self {
        Self::with_revision(UlaRevision::Ferranti6C)
    }

    /// Creates a 48K-class machine for the requested ULA revision.
    #[must_use]
    pub fn with_revision(revision: UlaRevision) -> Self {
        Self::with_revision_and_memory(revision, Spectrum48kMemory::new())
    }

    /// Creates a 48K-class machine with the supplied 16 KiB ROM image.
    #[must_use]
    pub fn with_rom(revision: UlaRevision, rom: [u8; 16 * 1024]) -> Self {
        Self::with_revision_and_memory(revision, Spectrum48kMemory::with_rom(rom))
    }

    /// Loads a 16 KiB ROM image.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM is not exactly 16 KiB.
    pub fn load_rom_bytes(&mut self, bytes: &[u8]) -> Result<(), RomImageError> {
        self.memory.load_rom_bytes(bytes)
    }
}

impl<V: Variant48kClass> Default for SpectrumMachineCore<Spectrum48kMemory, V> {
    fn default() -> Self {
        Self::new()
    }
}

// 16K-specific shortcuts. Only `Spectrum16kMarker` uses this memory map,
// so the V parameter is fixed to that marker — no other variant currently
// shares the half-RAM hardware.
impl SpectrumMachineCore<Spectrum16kMemory, crate::variant::Spectrum16kMarker> {
    /// Creates a Ferranti 6C 16K machine (Issue 3+) with deterministic
    /// startup state.
    #[must_use]
    pub fn new() -> Self {
        Self::with_revision(UlaRevision::Ferranti6C)
    }

    /// Creates a 16K machine for the requested ULA revision.
    #[must_use]
    pub fn with_revision(revision: UlaRevision) -> Self {
        Self::with_revision_and_memory(revision, Spectrum16kMemory::new())
    }

    /// Creates a 16K machine with the supplied 16 KiB ROM image.
    #[must_use]
    pub fn with_rom(revision: UlaRevision, rom: [u8; 16 * 1024]) -> Self {
        Self::with_revision_and_memory(revision, Spectrum16kMemory::with_rom(rom))
    }

    /// Loads a 16 KiB ROM image.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM is not exactly 16 KiB.
    pub fn load_rom_bytes(&mut self, bytes: &[u8]) -> Result<(), RomImageError> {
        self.memory.load_rom_bytes(bytes)
    }
}

impl Default for SpectrumMachineCore<Spectrum16kMemory, crate::variant::Spectrum16kMarker> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::variant::Spectrum48kMarker;

    type Spectrum48k = SpectrumMachineCore<Spectrum48kMemory, Spectrum48kMarker>;

    #[derive(Clone, Copy, Debug)]
    struct CpuTraceSample {
        hc: u32,
        addr: u16,
        mreq: bool,
        iorq: bool,
        rd: bool,
        wr: bool,
        m1: bool,
    }

    fn configure_machine_for_timing_test(machine: &mut Spectrum48k, pc: u16) {
        machine.advance_tstates(TIMING_48K.contention_start_tstate);
        machine.z80 = Z80::new();
        machine.z80.regs.pc = pc;
        machine.z80.regs.sp = 0xffff;
    }

    fn trace_until_fetch(
        machine: &mut Spectrum48k,
        fetch_addr: u16,
        max_halfcycles: u32,
    ) -> Vec<CpuTraceSample> {
        let mut trace = Vec::new();

        for _ in 0..max_halfcycles {
            if let Some(sample) = advance_halfcycle_with_trace(machine) {
                trace.push(sample);
                if sample.m1 && sample.addr == fetch_addr {
                    return trace;
                }
            }
        }

        panic!(
            "timed out waiting for opcode fetch at {fetch_addr:#06x}; captured {} CPU samples",
            trace.len()
        );
    }

    fn advance_halfcycle_with_trace(machine: &mut Spectrum48k) -> Option<CpuTraceSample> {
        let mut sample = None;

        if machine.hc & 1 == 0 {
            machine.ula.tick(
                &machine.memory,
                machine.z80.addr,
                machine.z80.mreq,
                machine.z80.iorq,
                machine.z80.rfsh,
                &mut machine.framebuffer,
            );

            if machine.ula.cpu_clock_active() {
                machine.z80.tick();
                machine.handle_bus();
                sample = Some(CpuTraceSample {
                    hc: machine.hc,
                    addr: machine.z80.addr,
                    mreq: machine.z80.mreq,
                    iorq: machine.z80.iorq,
                    rd: machine.z80.rd,
                    wr: machine.z80.wr,
                    m1: machine.z80.m1,
                });
            }

            machine.z80.irq = machine.ula.interrupt_active();

            if machine.hc % 4 == 2 {
                machine.tape.advance_tstates(1);
                machine.sync_ear_level();
            }
        }

        machine.hc += 1;
        if machine.hc >= TIMING_48K.halfcycles_per_frame {
            machine.end_frame_ula();
            machine.on_end_frame();
            machine.hc -= TIMING_48K.halfcycles_per_frame;
        }

        sample
    }

    #[test]
    fn contended_ram_fetch_inserts_cpu_clock_gaps_during_active_display() {
        let mut contended = Spectrum48k::new();
        configure_machine_for_timing_test(&mut contended, 0x4000);
        for addr in 0x4000..=0x4004 {
            contended.write(addr, 0x00);
        }

        let contended_trace = trace_until_fetch(&mut contended, 0x4004, 2_048);

        let mut uncontended = Spectrum48k::new();
        configure_machine_for_timing_test(&mut uncontended, 0x8000);
        for addr in 0x8000..=0x8004 {
            uncontended.write(addr, 0x00);
        }

        let uncontended_trace = trace_until_fetch(&mut uncontended, 0x8004, 2_048);

        assert!(
            contended_trace
                .windows(2)
                .any(|pair| pair[1].hc.saturating_sub(pair[0].hc) > 2),
            "expected contended instruction fetch to stall the CPU clock"
        );
        assert!(
            uncontended_trace
                .windows(2)
                .all(|pair| pair[1].hc.saturating_sub(pair[0].hc) == 2),
            "expected uncontended instruction fetch to advance on every CPU half-cycle"
        );
    }

    /// The live floating bus reproduces Spectron's model byte-for-byte
    /// across a display scanline — both the idle window (data at the four
    /// fetch slots of each 8-T group, idle elsewhere and past the 128-T
    /// active region) and the byte *values* (the column Spectron predicts
    /// for each data slot). This pins #10: our floating bus is correct vs
    /// the Spectron oracle. The floatspy-tap discrepancy lives on a
    /// different axis (interrupt-acknowledge / IM2 read phase), not the
    /// bus model — see `knowledge/systems/spectrum/floating-bus-accuracy.md`.
    ///
    /// Method: drive the ULA directly (CPU parked on a HALT so it stays
    /// off screen RAM) and read `IN A,($FF)` at each frame T-state. This
    /// is deterministic and oracle-anchored — no tape, no interrupt
    /// timing in the loop.
    #[test]
    fn floating_bus_matches_spectron_model_across_a_scanline() {
        const START: u32 = 14_330;
        const SPAN: u32 = 160;
        const FS: u32 = 14_338; // Spectron FloatingBusStartTicks (48K)

        // Spectron's floating-bus model at frame T-state `t`, line 0.
        let spectron = |t: u32| -> Option<(bool, u8)> {
            // None = idle (0xFF); Some((is_attr, column)) = a data slot.
            if t <= FS {
                return None;
            }
            let rel = t - 1 - FS;
            let col = rel % 224;
            if col >= 128 {
                return None;
            }
            let goff = col % 8;
            if goff >= 4 {
                return None;
            }
            let group = (col / 8) as u16;
            let cc = (2 * group + (goff as u16 >> 1)) as u8;
            Some((goff & 1 == 1, cc))
        };

        // Column-encoded screen so a fetched byte reveals its column
        // regardless of which display line the float is reading:
        // bitmap → column, attribute → 0x80 | column.
        let mut m = Spectrum48k::new();
        for addr in 0x4000u16..0x5800 {
            m.write(addr, (addr & 0x1F) as u8);
        }
        for addr in 0x5800u16..0x5B00 {
            m.write(addr, 0x80 | (addr & 0x1F) as u8);
        }
        m.write(0x8000, 0x76); // HALT; IFF=0 after reset so it sticks
        m.z80.regs.pc = 0x8000;
        m.advance_tstates(START);

        for i in 0..SPAN {
            let t = START + i;
            // The live beam bus. `io_read` adds the I/O M-cycle's
            // edge-to-latch lead on top of this; see `floating_bus_read`.
            let live = m.ula.floating_bus();
            match spectron(t) {
                None => assert_eq!(
                    live, 0xFF,
                    "T={t}: Spectron idle but live bus = {live:#04x}"
                ),
                Some((is_attr, col)) => {
                    let expected = if is_attr { 0x80 | col } else { col };
                    assert_eq!(
                        live,
                        expected,
                        "T={t}: Spectron expects {} column {col} ({expected:#04x}), \
                         live bus = {live:#04x}",
                        if is_attr { "attribute" } else { "bitmap" },
                    );
                }
            }
            m.advance_tstates(1);
        }
    }

    /// The live floating bus matches Spectron's model across the *entire*
    /// frame — every display line, the borders, vsync, and the interrupt
    /// line — not just the one scanline the sibling test checks.
    ///
    /// One subtlety this test pins down (#62): our `tstate_in_frame()`
    /// numbers the frame from the first display line (the ULA's `scan 0`,
    /// `hc = 0`), whereas FUSE/Spectron number it from the interrupt, with
    /// the display starting at T=14336. The interrupt itself fires at the
    /// correct beam position (`int_scan = 248`), so this is purely an
    /// internal-numbering convention: `FUSE_T = (our_T + 14336) mod frame`.
    /// Feeding our raw T-states into Spectron's model therefore disagrees
    /// (an artefact); applying the offset, the two agree on every one of
    /// the 69,888 T-states. This is *why* floatspy — which times its read
    /// from the interrupt — needs the interrupt-acknowledge phase right
    /// (#62), even though the bus content is exact.
    #[test]
    fn floating_bus_matches_spectron_model_across_the_whole_frame() {
        const FS: u32 = 14_338;
        // FUSE T=0 is the interrupt; our T=0 is the first display line, so
        // the display sits +14336 later in FUSE's numbering than in ours.
        const ORIGIN_OFFSET: u32 = 14_336;
        let frame = TIMING_48K.tstates_per_frame;

        let spectron_idle = |t: u32| -> bool {
            if t <= FS {
                return true;
            }
            let rel = t - 1 - FS;
            if rel / 224 >= 192 {
                return true; // below the 192-line display
            }
            let col = rel % 224;
            col >= 128 || (col % 8) >= 4 // right border / idle half of each group
        };

        // Zeroed screen: the bus reads 0x00 at a data slot and 0xFF when
        // idle, so the returned byte directly encodes our idle flag.
        let mut m = Spectrum48k::new();
        for addr in 0x4000u16..0x5B00 {
            m.write(addr, 0x00);
        }
        m.write(0x8000, 0x76); // HALT to keep the CPU off screen RAM
        m.z80.regs.pc = 0x8000;

        for t in 0..frame {
            // The live beam bus (io_read adds a sample-lead correction).
            let our_idle = m.ula.floating_bus() == 0xFF;
            let fuse_t = (t + ORIGIN_OFFSET) % frame;
            assert_eq!(
                our_idle,
                spectron_idle(fuse_t),
                "our_T={t} (FUSE_T={fuse_t}): idle flag differs from Spectron"
            );
            m.advance_tstates(1);
        }
    }

    #[test]
    fn not_taken_djnz_reads_the_displacement_it_discards() {
        let mut machine = Spectrum48k::new();
        configure_machine_for_timing_test(&mut machine, 0x4000);
        machine.z80.regs.set_b(1);
        machine.write(0x4000, 0x10);
        machine.write(0x4001, 0xfd);
        machine.write(0x4002, 0x00);

        let trace = trace_until_fetch(&mut machine, 0x4002, 1024);

        assert!(
            trace.iter().any(|sample| {
                sample.addr == 0x4001 && sample.mreq && sample.rd && !sample.wr && !sample.iorq
            }),
            "expected DJNZ fallthrough to read the displacement address"
        );
        assert!(
            !trace
                .iter()
                .any(|sample| sample.addr == 0x4001 && sample.wr),
            "the displacement cycle is a read, never a write"
        );
    }

    #[test]
    fn not_taken_jr_cc_reads_the_displacement_it_discards() {
        let mut machine = Spectrum48k::new();
        configure_machine_for_timing_test(&mut machine, 0x4000);
        machine.z80.regs.set_f(0x00);
        machine.write(0x4000, 0x28);
        machine.write(0x4001, 0x02);
        machine.write(0x4002, 0x00);

        let trace = trace_until_fetch(&mut machine, 0x4002, 1024);

        assert!(
            trace.iter().any(|sample| {
                sample.addr == 0x4001 && sample.mreq && sample.rd && !sample.wr && !sample.iorq
            }),
            "expected not-taken JR cc to read the displacement address"
        );
        // The byte is fetched and thrown away: PC lands after the
        // displacement rather than at the branch target.
        assert_eq!(
            machine.z80.regs.pc, 0x4002,
            "the fetched displacement must not be applied"
        );
    }

    #[test]
    fn odd_port_outside_kempston_range_reads_floating_bus() {
        // $FFFF has A5 set, so it's outside the Kempston decode mask, so
        // the read falls through to the floating bus regardless of whether
        // a Kempston is attached. At the reset beam position (frame T-state
        // 0, display line 0) the floating-bus read samples bitmap column 0
        // ($4000) — a sentinel here proves the port routes to the bus.
        let mut machine = Spectrum48k::new();
        machine.write(0x4000, 0xAB);
        assert_eq!(machine.io_read(0xffff), 0xAB);
    }

    #[test]
    fn unattached_kempston_does_not_claim_port_one_f() {
        // Default state: no Kempston plugged in. A port read at $1F falls
        // through to the floating bus (bitmap column 0 at the reset beam).
        let mut machine = Spectrum48k::new();
        machine.write(0x4000, 0xAB);
        assert_eq!(machine.io_read(0x1F), 0xAB);
    }

    #[test]
    fn attached_kempston_returns_state_byte_at_one_f() {
        let mut machine = Spectrum48k::new();
        machine.kempston.attached = true;
        machine.kempston.state = 0b0001_0001; // right + fire
        assert_eq!(machine.io_read(0x1F), 0b0001_0001);
        // Any port with A5=0 and A0=1 mirrors the read.
        assert_eq!(machine.io_read(0x001F), 0b0001_0001);
        assert_eq!(machine.io_read(0xFF1F), 0b0001_0001);
    }

    #[test]
    fn kempston_mut_writes_through_to_io_read() {
        let mut machine = Spectrum48k::new();
        machine.kempston_mut().attached = true;
        machine.kempston_mut().state = 0b0000_1000; // up
        assert_eq!(machine.io_read(0x1F), 0b0000_1000);
    }

    /// `Spectrum48k::with_rom` builds a fully-initialised machine
    /// with the supplied ROM image already mapped at $0000-$3FFF.
    /// Avoids the two-step `new()` + `load_rom_bytes` dance for tests
    /// that need ROM-backed setup.
    #[test]
    fn with_rom_constructs_machine_with_rom_mapped() {
        let mut rom = [0u8; 16 * 1024];
        rom[0] = 0xAB;
        rom[0x3FFF] = 0xCD;
        let machine = Spectrum48k::with_rom(UlaRevision::Ferranti6C, rom);
        assert_eq!(machine.read(0x0000), 0xAB);
        assert_eq!(machine.read(0x3FFF), 0xCD);
        // Verify the revision wasn't silently overridden.
        assert_eq!(machine.revision(), UlaRevision::Ferranti6C);
    }

    /// `Spectrum48k::default()` produces the same machine as `new()`.
    /// Locks the trait impl against a regression that would drift
    /// the default revision or memory state.
    #[test]
    fn default_matches_new() {
        let from_new = Spectrum48k::new();
        let from_default: Spectrum48k = Default::default();
        assert_eq!(from_new.revision(), from_default.revision());
        assert_eq!(from_new.border_color(), from_default.border_color());
        assert_eq!(
            from_new.framebuffer().len(),
            from_default.framebuffer().len()
        );
    }

    /// `load_rom_bytes` rejects a ROM image that isn't exactly 16 KiB.
    /// Catches a regression where the size check disappears and we'd
    /// silently truncate / pad — both of which can produce a quiet
    /// boot failure with no diagnostic.
    #[test]
    fn load_rom_bytes_rejects_wrong_size() {
        let mut machine = Spectrum48k::new();
        // Too short.
        assert!(machine.load_rom_bytes(&[0u8; 8 * 1024]).is_err());
        // Too long.
        assert!(machine.load_rom_bytes(&[0u8; 32 * 1024]).is_err());
        // Exact size succeeds.
        assert!(machine.load_rom_bytes(&[0u8; 16 * 1024]).is_ok());
    }

    /// The 16K wrapper's constructors mirror the 48K shape: `new()`,
    /// `with_revision`, `with_rom`, plus `load_rom_bytes`. The 16K
    /// variant lives in this layer crate (rather than the dedicated
    /// machine-sinclair-zx-spectrum-16k wrapper) because the memory
    /// type differs but the rest of the composition is identical.
    #[test]
    fn spectrum_16k_constructors_round_trip() {
        use crate::variant::Spectrum16kMarker;
        type Spectrum16k = SpectrumMachineCore<Spectrum16kMemory, Spectrum16kMarker>;

        let mut rom = [0u8; 16 * 1024];
        rom[0] = 0x55;
        let machine = Spectrum16k::with_rom(UlaRevision::Ferranti5C, rom);
        assert_eq!(machine.read(0x0000), 0x55);
        assert_eq!(machine.revision(), UlaRevision::Ferranti5C);

        // Default::default() routes through new() → with_revision().
        let defaulted: Spectrum16k = Default::default();
        assert_eq!(defaulted.revision(), UlaRevision::Ferranti6C);
    }

    /// 16K wrapper's `load_rom_bytes` accepts a 16 KiB image and
    /// rejects anything else, same contract as the 48K case.
    #[test]
    fn spectrum_16k_load_rom_bytes_validates_size() {
        use crate::variant::Spectrum16kMarker;
        type Spectrum16k = SpectrumMachineCore<Spectrum16kMemory, Spectrum16kMarker>;
        let mut machine = Spectrum16k::new();
        assert!(machine.load_rom_bytes(&[0u8; 1024]).is_err());
        assert!(machine.load_rom_bytes(&[0u8; 16 * 1024]).is_ok());
    }

    #[test]
    fn audio_sample_rate_matches_constant() {
        let machine = Spectrum48k::new();
        assert_eq!(machine.audio_sample_rate(), AUDIO_SAMPLE_RATE);
    }

    #[test]
    fn frame_position_survives_machine_serialization() {
        let mut machine = Spectrum48k::new();
        machine.advance_halfcycles(137);
        let bytes = serde_json::to_vec(&machine).expect("serialize 48K machine");
        let restored: Spectrum48k = serde_json::from_slice(&bytes).expect("restore 48K machine");
        assert_eq!(machine.frame_position(), restored.frame_position());
        machine.advance_halfcycles(23);
        let mut restored = restored;
        restored.advance_halfcycles(23);
        assert_eq!(machine.frame_position(), restored.frame_position());
    }

    #[test]
    fn frame_position_snapshot_round_trip_wraps_cleanly() {
        let mut machine = Spectrum48k::new();
        machine.advance_halfcycles(TIMING_48K.halfcycles_per_frame - 4);
        let bytes = serde_json::to_vec(&machine).expect("serialize near frame boundary");
        let mut restored: Spectrum48k =
            serde_json::from_slice(&bytes).expect("restore near frame boundary");
        machine.advance_halfcycles(9);
        restored.advance_halfcycles(9);
        assert_eq!(machine.frame_position(), restored.frame_position());
        assert_eq!(machine.frame_position().halfcycles(), 5);
    }
}

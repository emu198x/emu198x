//! Shared 48K-class machine composition.
//!
//! The 48K, 16K, and Spectrum+ are electrically identical apart from
//! memory size and badge. This type holds the Z80 + Ferranti ULA + beeper
//! + tape composition that's shared across them, parameterised over the
//! memory map. Variant crates alias it (`pub type Spectrum48k =
//! SpectrumMachineCore<Spectrum48kMemory>;`) and add only their own
//! variant-specific surface.
//!
//! Variants outside the 48K-class — 128K-family, Pentagon, Scorpion,
//! Timex — have their own ULAs and additional state (AY, paging, FDC)
//! and keep their own machine implementations.

use common_sinclair_zx_spectrum::audio::{
    AudioControls, BeeperAudio, SpeakerChannel, SpeakerMixer,
};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::error::RomImageError;
use common_sinclair_zx_spectrum::keyboard::KeyboardMatrix;
use common_sinclair_zx_spectrum::memory::{MemoryBus, Spectrum16kMemory, Spectrum48kMemory};
use common_sinclair_zx_spectrum::tape::{TapeBlock, TapePlayer, TapeSpan};
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_48K};
use common_sinclair_zx_spectrum::ula::Ula;
use ferranti_ula_6c001e::{BoardIssue, FerrantiUla};
use zilog_z80::Z80;

use crate::tape_input::TapeInput;

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
/// re-used from `common-sinclair-zx-spectrum`.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SpectrumMachineCore<M: MemoryBus> {
    z80: Z80,
    ula: FerrantiUla,
    memory: M,
    keyboard: KeyboardMatrix,
    tape: TapePlayer,
    tape_input: TapeInput,
    audio: BeeperAudio,
    audio_frame: Vec<f32>,
    speaker: SpeakerMixer,
    framebuffer: Vec<u8>,
    hc: u32,
}

impl<M: MemoryBus> SpectrumMachineCore<M> {
    /// Creates a 48K-class machine for the requested board issue and
    /// caller-supplied memory map.
    #[must_use]
    pub fn with_issue_and_memory(issue: BoardIssue, memory: M) -> Self {
        let audio = make_audio();
        let audio_frame = vec![0.0; audio.samples_per_frame()];
        Self {
            z80: Z80::new(),
            ula: FerrantiUla::new(issue),
            memory,
            keyboard: KeyboardMatrix::new(),
            tape: TapePlayer::new(),
            tape_input: TapeInput::new(),
            audio,
            audio_frame,
            speaker: SpeakerMixer::default(),
            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT],
            hc: 0,
        }
    }

    /// Returns the configured board issue.
    #[must_use]
    pub const fn issue(&self) -> BoardIssue {
        self.ula.issue()
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
    pub const fn tstate_in_frame(&self) -> u32 {
        TIMING_48K.hc_to_tstates(self.hc)
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

    /// Returns the current border colour.
    #[must_use]
    pub fn border_color(&self) -> u8 {
        self.ula.border_color()
    }

    /// Writes to port `$FE`.
    pub fn write_fe(&mut self, value: u8) {
        self.ula.write_fe(value);
        self.sync_beeper_level(value);
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
        let issue = self.issue();
        self.z80 = Z80::new();
        self.ula = FerrantiUla::new(issue);
        self.audio = make_audio();
        self.audio_frame.fill(0.0);
        self.speaker = SpeakerMixer::default();
        self.hc = 0;
        self.framebuffer.fill(0);
        self.sync_ear_level();
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
        if self.z80.mreq && self.z80.rd {
            self.z80.data_in = self.memory.read(self.z80.addr);
        } else if self.z80.mreq && self.z80.wr {
            self.memory.write(self.z80.addr, self.z80.data);
        } else if self.z80.iorq && self.z80.rd && !self.z80.m1 {
            self.z80.data_in = self.io_read(self.z80.addr);
        } else if self.z80.iorq && self.z80.wr {
            self.io_write(self.z80.addr, self.z80.data);
        } else if self.z80.iorq && self.z80.m1 {
            self.z80.data_in = 0xff;
        }
    }

    fn io_read(&self, port: u16) -> u8 {
        if port & 0x01 == 0 {
            self.read_fe(port)
        } else {
            self.ula.floating_bus()
        }
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

impl<M: MemoryBus> SpectrumDriver for SpectrumMachineCore<M> {
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
        TIMING_48K.halfcycles_per_frame
    }
    #[inline(always)]
    fn halfcycles_per_tstate(&self) -> u32 {
        TIMING_48K.cpu_divisor
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
    fn on_tstate(&mut self, _hc: u32) {
        self.tape.advance_tstates(1);
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

impl<M: MemoryBus> MemoryBus for SpectrumMachineCore<M> {
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

// 48K-specific shortcuts.
impl SpectrumMachineCore<Spectrum48kMemory> {
    /// Creates an Issue 3 48K machine with deterministic startup state.
    #[must_use]
    pub fn new() -> Self {
        Self::with_issue(BoardIssue::Issue3)
    }

    /// Creates a 48K machine for the requested board issue.
    #[must_use]
    pub fn with_issue(issue: BoardIssue) -> Self {
        Self::with_issue_and_memory(issue, Spectrum48kMemory::new())
    }

    /// Creates a 48K machine with the supplied 16 KiB ROM image.
    #[must_use]
    pub fn with_rom(issue: BoardIssue, rom: [u8; 16 * 1024]) -> Self {
        Self::with_issue_and_memory(issue, Spectrum48kMemory::with_rom(rom))
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

impl Default for SpectrumMachineCore<Spectrum48kMemory> {
    fn default() -> Self {
        Self::new()
    }
}

// 16K-specific shortcuts. The 16K wrapper crate (Phase 1A step 5) aliases
// `SpectrumMachineCore<Spectrum16kMemory>` and inherits these.
impl SpectrumMachineCore<Spectrum16kMemory> {
    /// Creates an Issue 3 16K machine with deterministic startup state.
    #[must_use]
    pub fn new() -> Self {
        Self::with_issue(BoardIssue::Issue3)
    }

    /// Creates a 16K machine for the requested board issue.
    #[must_use]
    pub fn with_issue(issue: BoardIssue) -> Self {
        Self::with_issue_and_memory(issue, Spectrum16kMemory::new())
    }

    /// Creates a 16K machine with the supplied 16 KiB ROM image.
    #[must_use]
    pub fn with_rom(issue: BoardIssue, rom: [u8; 16 * 1024]) -> Self {
        Self::with_issue_and_memory(issue, Spectrum16kMemory::with_rom(rom))
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

impl Default for SpectrumMachineCore<Spectrum16kMemory> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Spectrum48k = SpectrumMachineCore<Spectrum48kMemory>;

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

    #[test]
    fn not_taken_djnz_uses_mreq_only_fallthrough_cycle() {
        let mut machine = Spectrum48k::new();
        configure_machine_for_timing_test(&mut machine, 0x4000);
        machine.z80.regs.set_b(1);
        machine.write(0x4000, 0x10);
        machine.write(0x4001, 0xfd);
        machine.write(0x4002, 0x00);

        let trace = trace_until_fetch(&mut machine, 0x4002, 1024);

        assert!(
            trace.iter().any(|sample| {
                sample.addr == 0x4001 && sample.mreq && !sample.rd && !sample.wr && !sample.iorq
            }),
            "expected DJNZ fallthrough to expose a contended PC cycle at the displacement address"
        );
        assert!(
            !trace
                .iter()
                .any(|sample| sample.addr == 0x4001 && sample.mreq && sample.rd),
            "not-taken DJNZ must not read the displacement byte"
        );
    }

    #[test]
    fn not_taken_jr_cc_uses_mreq_only_fallthrough_cycle() {
        let mut machine = Spectrum48k::new();
        configure_machine_for_timing_test(&mut machine, 0x4000);
        machine.z80.regs.set_f(0x00);
        machine.write(0x4000, 0x28);
        machine.write(0x4001, 0x02);
        machine.write(0x4002, 0x00);

        let trace = trace_until_fetch(&mut machine, 0x4002, 1024);

        assert!(
            trace.iter().any(|sample| {
                sample.addr == 0x4001 && sample.mreq && !sample.rd && !sample.wr && !sample.iorq
            }),
            "expected not-taken JR cc to expose a contended PC cycle at the displacement address"
        );
        assert!(
            !trace
                .iter()
                .any(|sample| sample.addr == 0x4001 && sample.mreq && sample.rd),
            "not-taken JR cc must not read the displacement byte"
        );
    }

    #[test]
    fn unattached_odd_port_reads_idle_floating_bus() {
        let machine = Spectrum48k::new();

        assert_eq!(machine.io_read(0xffff), 0xff);
    }

    #[test]
    fn audio_sample_rate_matches_constant() {
        let machine = Spectrum48k::new();
        assert_eq!(machine.audio_sample_rate(), AUDIO_SAMPLE_RATE);
    }
}

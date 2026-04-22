//! ZX Spectrum 48K machine-local composition.
//!
//! This crate now owns the first working 48K machine loop: the Z80 and Ferranti
//! ULA are wired together against the 48K memory map, keyboard matrix, tape
//! path, and beeper/EAR audio path.

use common_sinclair_zx_spectrum::audio::{BeeperAudio, SpeakerMixer};
use common_sinclair_zx_spectrum::driver::SpectrumDriver;
use common_sinclair_zx_spectrum::timing::{SCREEN_HEIGHT, SCREEN_WIDTH, TIMING_48K};
use common_sinclair_zx_spectrum::ula::Ula;
use common_sinclair_zx_spectrum::{
    MemoryBus, RomImageError, Spectrum48kMemory, TapeBlock, TapePlayer, TapeSpan,
};
use emu198x_shell::InputEvent;
use ferranti_ula_6c001e::{BoardIssue, FerrantiUla};
use zilog_z80::Z80;

use common_sinclair_zx_spectrum::keyboard::KeyboardMatrix;
use crate::port::TapeInput;

const AUDIO_SAMPLE_RATE: u32 = 44_100;

/// Machine-local state for a stock ZX Spectrum 48K.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Spectrum48k {
    z80: Z80,
    ula: FerrantiUla,
    memory: Spectrum48kMemory,
    keyboard: KeyboardMatrix,
    tape: TapePlayer,
    tape_input: TapeInput,
    audio: BeeperAudio,
    audio_frame: Vec<f32>,
    speaker: SpeakerMixer,
    framebuffer: Vec<u8>,
    hc: u32,
}

impl Spectrum48k {
    /// Creates an Issue 3 48K machine with deterministic startup state.
    #[must_use]
    pub fn new() -> Self {
        Self::with_issue(BoardIssue::Issue3)
    }

    /// Creates a 48K machine for the requested board issue.
    #[must_use]
    pub fn with_issue(issue: BoardIssue) -> Self {
        Self {
            z80: Z80::new(),
            ula: FerrantiUla::new(issue),
            memory: Spectrum48kMemory::new(),
            keyboard: KeyboardMatrix::new(),
            tape: TapePlayer::new(),
            tape_input: TapeInput::new(),
            audio: BeeperAudio::new(
                AUDIO_SAMPLE_RATE,
                TIMING_48K.tstates_per_frame,
                (TIMING_48K.master_hz / u64::from(TIMING_48K.cpu_divisor)) as u32,
            ),
            audio_frame: vec![
                0.0;
                BeeperAudio::new(
                    AUDIO_SAMPLE_RATE,
                    TIMING_48K.tstates_per_frame,
                    (TIMING_48K.master_hz / u64::from(TIMING_48K.cpu_divisor)) as u32,
                )
                .samples_per_frame()
            ],
            speaker: SpeakerMixer::default(),
            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT],
            hc: 0,
        }
    }

    /// Creates a 48K machine with the supplied 16 KiB ROM image.
    #[must_use]
    pub fn with_rom(issue: BoardIssue, rom: [u8; 16 * 1024]) -> Self {
        Self {
            z80: Z80::new(),
            ula: FerrantiUla::new(issue),
            memory: Spectrum48kMemory::with_rom(rom),
            keyboard: KeyboardMatrix::new(),
            tape: TapePlayer::new(),
            tape_input: TapeInput::new(),
            audio: BeeperAudio::new(
                AUDIO_SAMPLE_RATE,
                TIMING_48K.tstates_per_frame,
                (TIMING_48K.master_hz / u64::from(TIMING_48K.cpu_divisor)) as u32,
            ),
            audio_frame: vec![
                0.0;
                BeeperAudio::new(
                    AUDIO_SAMPLE_RATE,
                    TIMING_48K.tstates_per_frame,
                    (TIMING_48K.master_hz / u64::from(TIMING_48K.cpu_divisor)) as u32,
                )
                .samples_per_frame()
            ],
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

    /// Returns the 48K memory map.
    #[must_use]
    pub fn memory(&self) -> &Spectrum48kMemory {
        &self.memory
    }

    /// Returns mutable access to the 48K memory map.
    #[must_use]
    pub fn memory_mut(&mut self) -> &mut Spectrum48kMemory {
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

    /// Loads a 16 KiB ROM image.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM is not exactly 16 KiB.
    pub fn load_rom_bytes(&mut self, bytes: &[u8]) -> Result<(), RomImageError> {
        self.memory.load_rom_bytes(bytes)
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

    /// Applies one host input event to the keyboard matrix.
    ///
    /// Returns `true` when the event maps to a physical key.
    pub fn apply_input_event(&mut self, event: &InputEvent) -> bool {
        self.keyboard.apply_input_event(event)
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
        self.audio = BeeperAudio::new(
            AUDIO_SAMPLE_RATE,
            TIMING_48K.tstates_per_frame,
            (TIMING_48K.master_hz / u64::from(TIMING_48K.cpu_divisor)) as u32,
        );
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

    /// Advances the machine by an exact number of master-clock
    /// half-cycles. Inherent shim around the trait default so existing
    /// callers don't need an explicit `<Self as SpectrumDriver>` cast.
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

impl SpectrumDriver for Spectrum48k {
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

impl Default for Spectrum48k {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBus for Spectrum48k {
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

#[cfg(test)]
mod tests {
    use super::*;
    use common_sinclair_zx_spectrum::keyboard::SpectrumKey;
    use std::fs;
    use std::path::PathBuf;

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
    fn machine_defaults_to_issue3() {
        let machine = Spectrum48k::new();

        assert_eq!(machine.issue(), BoardIssue::Issue3);
        assert_eq!(machine.border_color(), 7);
        assert_eq!(machine.framebuffer().len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert_eq!(machine.read_fe(0xfffe), 0xbf);
    }

    #[test]
    fn machine_loads_rom_and_exposes_memory_bus() {
        let mut machine = Spectrum48k::with_issue(BoardIssue::Issue3);
        let rom = [0xa5; 16 * 1024];

        machine
            .load_rom_bytes(&rom)
            .expect("16 KiB ROM image should load");
        machine.write(0x8000, 0x42);

        assert_eq!(machine.read(0x0000), 0xa5);
        assert_eq!(machine.read(0x3fff), 0xa5);
        assert_eq!(machine.read(0x8000), 0x42);
    }

    #[test]
    fn machine_runs_frame_without_rom() {
        let mut machine = Spectrum48k::new();
        machine.run_frame();

        assert!(machine.z80().regs.pc > 0 || machine.z80().halt);
        assert_eq!(machine.hc(), 0);
    }

    #[test]
    fn machine_applies_host_key_events() {
        let mut machine = Spectrum48k::new();
        let pressed = InputEvent::Key {
            name: "q".into(),
            pressed: true,
        };

        assert!(machine.apply_input_event(&pressed));
        assert_eq!(machine.read_fe(0xfbfe) & 0x01, 0x00);
    }

    #[test]
    fn machine_exposes_issue_specific_feedback() {
        let mut issue2 = Spectrum48k::with_issue(BoardIssue::Issue2);
        let mut issue3 = Spectrum48k::with_issue(BoardIssue::Issue3);

        issue2.write_fe(0x08);
        issue3.write_fe(0x08);

        assert_eq!(issue2.read_fe(0xfffe) & 0x40, 0x40);
        assert_eq!(issue3.read_fe(0xfffe) & 0x40, 0x00);
    }

    #[test]
    fn connected_tape_input_overrides_feedback() {
        let mut machine = Spectrum48k::new();
        machine.write_fe(0x10);
        machine.set_tape_connected(true);
        machine.set_tape_level(false);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);

        machine.set_tape_level(true);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);
    }

    #[test]
    fn stopped_tape_does_not_override_ula_feedback() {
        let mut machine = Spectrum48k::new();

        machine.write_fe(0x10);
        machine.load_tape_pulses(vec![1, 1, 1]);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);

        machine.write_fe(0x00);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);
    }

    #[test]
    fn emulated_tape_advances_on_tstate_boundaries() {
        let mut machine = Spectrum48k::new();

        machine.load_tape_pulses(vec![1, 1, 2]);
        machine.play_tape();
        assert!(machine.tape_is_loaded());
        assert!(machine.tape_is_playing());
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);

        machine.advance_halfcycles(3);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);

        machine.advance_halfcycles(4);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);

        machine.advance_halfcycles(8);
        assert!(!machine.tape_is_playing());
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);
    }

    #[test]
    fn advance_tstates_tracks_frame_tstate_position() {
        let mut machine = Spectrum48k::new();

        machine.advance_tstates(7);
        assert_eq!(machine.tstate_in_frame(), 7);

        machine.advance_tstates(5);
        assert_eq!(machine.tstate_in_frame(), 12);
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
    fn external_tape_input_overrides_emulated_tape() {
        let mut machine = Spectrum48k::new();

        machine.load_tape_pulses(vec![1, 2]);
        machine.play_tape();
        machine.advance_halfcycles(3);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x00);

        machine.set_tape_connected(true);
        machine.set_tape_level(false);
        assert_eq!(machine.read_fe(0xfffe) & 0x40, 0x40);
    }

    #[test]
    fn beeper_audio_is_emitted_per_frame() {
        let mut machine = Spectrum48k::new();

        machine.write_fe(0x10);
        machine.run_frame();

        assert_eq!(machine.audio_sample_rate(), AUDIO_SAMPLE_RATE);
        assert!(machine.audio_samples_per_frame() > 0);
        assert!(machine.audio_frame().iter().any(|&sample| sample > 0.0));
    }

    #[test]
    fn machine_allows_direct_keyboard_access_for_tests() {
        let mut machine = Spectrum48k::new();
        machine.keyboard_mut().press_key(SpectrumKey::Enter);

        assert_eq!(machine.read_fe(0xbffe) & 0x01, 0x00);
    }

    #[test]
    fn unattached_odd_port_reads_idle_floating_bus() {
        let machine = Spectrum48k::new();

        assert_eq!(machine.io_read(0xffff), 0xff);
    }

    #[test]
    #[ignore = "requires local 48K ROM at ~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom"]
    fn boot_rom_populates_screen_memory() {
        let Some(rom_path) = spectrum_48k_rom_path() else {
            eprintln!("HOME is not set; skipping ROM-backed boot smoke test");
            return;
        };

        if !rom_path.is_file() {
            eprintln!("ROM not found at {}", rom_path.display());
            return;
        }

        let rom = match fs::read(&rom_path) {
            Ok(rom) => rom,
            Err(err) => panic!("failed to read {}: {err}", rom_path.display()),
        };

        let mut machine = Spectrum48k::new();
        machine
            .load_rom_bytes(&rom)
            .expect("48K ROM path should contain a 16 KiB image");
        machine.reset();

        for _ in 0..200 {
            machine.run_frame();
        }

        let pixel_non_zero = (0x4000u16..=0x57ff)
            .filter(|&addr| machine.read(addr) != 0)
            .count();
        let attribute_non_zero = (0x5800u16..=0x5aff)
            .filter(|&addr| machine.read(addr) != 0)
            .count();

        assert!(pixel_non_zero > 0, "expected boot ROM to draw pixel data");
        assert!(
            attribute_non_zero > 0,
            "expected boot ROM to program attribute memory"
        );
    }

    fn spectrum_48k_rom_path() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".emu198x/roms/sinclair-zx-spectrum-48k/48.rom"))
    }
}

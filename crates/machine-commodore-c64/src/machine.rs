//! Board-level C64 machine substrate.

use common_commodore_c64::timing::C64Timing;
use format_commodore_c64_tap::{TapParseError, TapSystem, parse_tap};
use mos_6502::M6502;
use mos_cia_6526::Cia6526;
use mos_sid_6581::{Sid6581, SidModel};
use mos_vic_ii::{Vic, VicModel};

use crate::config::{C64Config, C64Model};
use crate::datasette::Datasette;
use crate::keyboard::KeyboardMatrix;
use crate::memory::{C64Memory, C64MemorySnapshot, MemoryInitError};

const AUDIO_SAMPLE_RATE: u32 = 48_000;
const PORT_INPUT_PULLUPS: u8 = 0x37;

/// Fresh-workspace C64 machine substrate.
#[derive(Clone)]
pub struct C64 {
    model: C64Model,
    cpu: M6502,
    vic: Vic,
    cia1: Cia6526,
    cia2: Cia6526,
    sid: Sid6581,
    datasette: Datasette,
    memory: C64Memory,
    keyboard: KeyboardMatrix,
    phi2_cycles: u64,
    frame_count: u64,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct C64Snapshot {
    model: C64Model,
    cpu: M6502,
    vic: Vic,
    cia1: Cia6526,
    cia2: Cia6526,
    sid: Sid6581,
    datasette: Datasette,
    memory: C64MemorySnapshot,
    keyboard: KeyboardMatrix,
    phi2_cycles: u64,
    frame_count: u64,
}

impl C64 {
    /// Constructs a new C64 machine substrate from ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if any ROM size is incorrect.
    pub fn new(config: C64Config<'_>) -> Result<Self, MemoryInitError> {
        let memory = C64Memory::new(config.kernal_rom, config.basic_rom, config.character_rom)?;
        let mut cpu = M6502::new();
        cpu.reset();
        let timing = config.model.timing();

        let mut cia1 = Cia6526::new_with_tod(timing.cia_tod_divider);
        cia1.write(0x02, 0xFF);
        cia1.write(0x03, 0x00);
        cia1.write(0x00, 0xFF);

        let mut cia2 = Cia6526::new_with_tod(timing.cia_tod_divider);
        cia2.write(0x02, 0x03);
        cia2.write(0x00, 0x03);

        let vic_model = match config.model {
            C64Model::PalBreadbin => VicModel::Pal6569,
            C64Model::NtscBreadbin => VicModel::Ntsc6567,
        };
        let mut vic = Vic::new(vic_model);
        vic.set_bank(0);
        let sid = Sid6581::new_with_model(timing.cpu_hz, AUDIO_SAMPLE_RATE, SidModel::Mos6581);

        let mut machine = Self {
            model: config.model,
            cpu,
            vic,
            cia1,
            cia2,
            sid,
            datasette: Datasette::new(),
            memory,
            keyboard: KeyboardMatrix::new(),
            phi2_cycles: 0,
            frame_count: 0,
        };
        machine.refresh_keyboard_scan();
        machine.refresh_vic_bank();
        machine.refresh_datasette_port_lines();
        Ok(machine)
    }

    /// Hardware model.
    #[must_use]
    pub const fn model(&self) -> C64Model {
        self.model
    }

    /// CPU state.
    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    /// VIC-II state.
    #[must_use]
    pub const fn vic(&self) -> &Vic {
        &self.vic
    }

    /// CIA1 state.
    #[must_use]
    pub const fn cia1(&self) -> &Cia6526 {
        &self.cia1
    }

    /// CIA2 state.
    #[must_use]
    pub const fn cia2(&self) -> &Cia6526 {
        &self.cia2
    }

    /// Timing descriptor for the current model.
    #[must_use]
    pub const fn timing(&self) -> C64Timing {
        self.model.timing()
    }

    /// Underlying memory subsystem.
    #[must_use]
    pub const fn memory(&self) -> &C64Memory {
        &self.memory
    }

    /// Mutable access to the keyboard matrix.
    #[must_use]
    pub fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        &mut self.keyboard
    }

    /// `phi2` cycles elapsed since construction.
    #[must_use]
    pub const fn phi2_cycles(&self) -> u64 {
        self.phi2_cycles
    }

    /// Completed video frames.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Current raster line within the frame.
    #[must_use]
    pub fn raster_line(&self) -> u16 {
        self.vic.raster_line()
    }

    /// Current `phi2` cycle within the raster line.
    #[must_use]
    pub fn cycle_in_line(&self) -> u8 {
        self.vic.raster_cycle()
    }

    /// Current VIC bank selected by CIA2 port A bits 0-1, inverted.
    #[must_use]
    pub fn vic_bank(&self) -> u8 {
        self.vic.bank()
    }

    /// Current CIA1 Port B input value after keyboard scan.
    #[must_use]
    pub const fn cia1_port_b_input(&self) -> u8 {
        self.cia1.pb_in
    }

    /// Reads one live VIC-II register without side effects.
    #[must_use]
    pub fn vic_register(&self, index: u8) -> u8 {
        self.vic.peek(index)
    }

    /// Live SID state.
    #[must_use]
    pub const fn sid(&self) -> &Sid6581 {
        &self.sid
    }

    /// Returns `true` when one tape image is currently inserted.
    #[must_use]
    pub const fn tape_is_loaded(&self) -> bool {
        self.datasette.is_loaded()
    }

    /// Returns `true` when the datasette transport is engaged.
    #[must_use]
    pub const fn tape_is_playing(&self) -> bool {
        self.datasette.is_playing()
    }

    /// Output sample rate used by the machine-local SID mixer.
    #[must_use]
    pub const fn audio_sample_rate(&self) -> u32 {
        AUDIO_SAMPLE_RATE
    }

    /// Drains the current mixed SID output buffer.
    #[must_use]
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.sid.take_buffer()
    }

    /// Borrow the VIC-II framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vic.framebuffer()
    }

    /// Loads one Commodore TAP image into the datasette.
    ///
    /// # Errors
    ///
    /// Returns an error if the TAP header or pulse stream is invalid.
    pub fn load_tap_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        let image = parse_tap(bytes).map_err(|reason| match reason {
            TapParseError::UnsupportedVersion { version } => {
                format!("unsupported TAP version {version}")
            }
            other => other.to_string(),
        })?;

        if image.system != TapSystem::C64 {
            return Err(format!(
                "expected a C64 TAP image, found {:?}",
                image.system
            ));
        }

        self.datasette.load_tap(image);
        self.refresh_datasette_port_lines();
        Ok(())
    }

    /// Presses PLAY on the currently inserted datasette image.
    pub fn play_tape(&mut self) {
        self.datasette.play();
        self.refresh_datasette_port_lines();
    }

    /// Stops the datasette transport without ejecting the image.
    pub fn stop_tape(&mut self) {
        self.datasette.stop();
        self.refresh_datasette_port_lines();
    }

    /// Advances the board by one `phi2` cycle.
    ///
    /// Returns `true` when this tick completed a frame.
    pub fn tick(&mut self) -> bool {
        self.phi2_cycles = self.phi2_cycles.saturating_add(1);
        let _cpu_stalled = self.vic.tick(&self.memory);
        self.cia1.flag = !self.datasette.advance_phi2_cycle();
        self.refresh_keyboard_scan();
        self.cia1.tick();
        self.cia2.tick();
        self.refresh_vic_bank();
        self.cpu.irq = self.vic.irq || self.cia1.irq;
        self.cpu.nmi = self.cia2.irq;
        self.cpu.rdy = !self.vic.ba_low || !self.cpu.rw;

        if self.cpu.rdy {
            if self.cpu.rw {
                self.cpu.data_in = self.cpu_read(self.cpu.addr);
            } else {
                self.cpu_write(self.cpu.addr, self.cpu.data);
            }
            self.cpu.tick();
        }
        self.sid.tick();

        if self.vic.take_frame_complete() {
            self.frame_count = self.frame_count.saturating_add(1);
            return true;
        }

        false
    }

    /// Advances the board by a fixed number of `phi2` cycles.
    pub fn advance_phi2_cycles(&mut self, cycles: u64) {
        for _ in 0..cycles {
            self.tick();
        }
    }

    /// Advances exactly one frame and returns the number of cycles executed.
    pub fn run_frame(&mut self) -> u32 {
        let start = self.phi2_cycles;
        while !self.tick() {}
        (self.phi2_cycles - start) as u32
    }

    /// Loads one PRG file into raw RAM and returns its load address.
    ///
    /// This is a host-side import convenience, not an emulated disk or tape
    /// path. It matches the direct-RAM effect of a completed KERNAL LOAD.
    ///
    /// # Errors
    ///
    /// Returns an error if the PRG header is malformed.
    pub fn load_prg(&mut self, data: &[u8]) -> Result<u16, String> {
        format_commodore_c64_prg::load_prg(&mut self.memory, data)
    }

    /// Captures the machine state for runtime snapshot serialization.
    #[must_use]
    pub fn snapshot_state(&self) -> C64Snapshot {
        C64Snapshot {
            model: self.model,
            cpu: self.cpu.clone(),
            vic: self.vic.clone(),
            cia1: self.cia1.clone(),
            cia2: self.cia2.clone(),
            sid: self.sid.clone(),
            datasette: self.datasette.clone(),
            memory: self.memory.snapshot_state(),
            keyboard: self.keyboard.clone(),
            phi2_cycles: self.phi2_cycles,
            frame_count: self.frame_count,
        }
    }

    /// Restores a machine state produced by [`Self::snapshot_state`].
    ///
    /// # Errors
    ///
    /// Returns an error if the snapshot belongs to a different model or any
    /// captured memory image has the wrong size.
    pub fn restore_snapshot_state(&mut self, snapshot: C64Snapshot) -> Result<(), String> {
        if snapshot.model != self.model {
            return Err(format!(
                "snapshot model {:?} does not match machine model {:?}",
                snapshot.model, self.model
            ));
        }

        self.cpu = snapshot.cpu;
        self.vic = snapshot.vic;
        self.cia1 = snapshot.cia1;
        self.cia2 = snapshot.cia2;
        self.sid = snapshot.sid;
        self.datasette = snapshot.datasette;
        self.memory = C64Memory::from_snapshot(snapshot.memory)?;
        self.keyboard = snapshot.keyboard;
        self.phi2_cycles = snapshot.phi2_cycles;
        self.frame_count = snapshot.frame_count;
        self.refresh_keyboard_scan();
        self.refresh_vic_bank();
        self.refresh_datasette_port_lines();
        Ok(())
    }

    /// CPU-visible read through banked memory and the current board I/O state.
    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        if addr == 0x0001 {
            return self.cpu_port_read();
        }
        if (0xD000..=0xDFFF).contains(&addr) && self.memory.is_io_visible() {
            return self.io_read(addr);
        }
        self.memory.cpu_read(addr)
    }

    /// CPU-visible write through banked memory and the current board I/O state.
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        self.memory.cpu_write(addr, value);
        if matches!(addr, 0x0000 | 0x0001) {
            self.refresh_datasette_port_lines();
        }
        if (0xD000..=0xDFFF).contains(&addr) && self.memory.is_io_visible() {
            self.io_write(addr, value);
        }
    }

    /// Reads the current VIC-visible byte from the active bank.
    #[must_use]
    pub fn vic_read(&self, offset: u16) -> u8 {
        self.memory.vic_read(self.vic.bank(), offset)
    }

    fn io_read(&mut self, addr: u16) -> u8 {
        match addr {
            0xD000..=0xD3FF => self.vic.read((addr & 0x3F) as u8),
            0xD400..=0xD7FF => self.sid.read((addr & 0x1F) as u8),
            0xD800..=0xDBFF => self.memory.colour_ram_read(addr - 0xD800),
            0xDC00..=0xDCFF => {
                self.refresh_keyboard_scan();
                self.cia1.read((addr & 0x0F) as u8)
            }
            0xDD00..=0xDDFF => self.cia2.read((addr & 0x0F) as u8),
            0xDE00..=0xDFFF => 0xFF,
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, addr: u16, value: u8) {
        match addr {
            0xD000..=0xD3FF => self.vic.write((addr & 0x3F) as u8, value),
            0xD400..=0xD7FF => self.sid.write((addr & 0x1F) as u8, value),
            0xD800..=0xDBFF => self.memory.colour_ram_write(addr - 0xD800, value),
            0xDC00..=0xDCFF => {
                self.cia1.write((addr & 0x0F) as u8, value);
                self.refresh_keyboard_scan();
            }
            0xDD00..=0xDDFF => {
                self.cia2.write((addr & 0x0F) as u8, value);
                self.refresh_vic_bank();
            }
            0xDE00..=0xDFFF => {}
            _ => {}
        }
    }

    fn refresh_keyboard_scan(&mut self) {
        self.cia1.pb_in = self.keyboard.scan(self.cia1.pa);
    }

    fn refresh_vic_bank(&mut self) {
        self.vic.set_bank((!self.cia2.pa) & 0x03);
    }

    fn cpu_port_read(&self) -> u8 {
        let ddr = self.memory.port_ddr();
        let data = self.memory.port_data();
        let mut value = (data & ddr) | (PORT_INPUT_PULLUPS & !ddr);

        if self.datasette.sense_active() && (ddr & 0x10) == 0 {
            value &= !0x10;
        }

        if self.datasette.write_input_active() && (ddr & 0x08) == 0 {
            value &= !0x08;
        }

        if self.datasette.motor_input_active() && (ddr & 0x20) == 0 {
            value &= !0x20;
        }

        value
    }

    fn refresh_datasette_port_lines(&mut self) {
        let ddr = self.memory.port_ddr();
        let data = self.memory.port_data();
        let motor_on = (ddr & 0x20) != 0 && (data & 0x20) == 0;
        self.datasette.set_motor_on(motor_on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn stub_machine(model: C64Model) -> C64 {
        stub_machine_with_reset_vector(model, 0xE000)
    }

    fn stub_machine_with_reset_vector(model: C64Model, start_pc: u16) -> C64 {
        let mut kernal = [0xEA; 0x2000];
        kernal[0x1FFC] = start_pc as u8;
        kernal[0x1FFD] = (start_pc >> 8) as u8;
        C64::new(C64Config {
            model,
            kernal_rom: &kernal,
            basic_rom: &[0xBB; 0x2000],
            character_rom: &[0xCC; 0x1000],
        })
        .expect("stub ROM sizes should be valid")
    }

    fn c64_rom_dir() -> PathBuf {
        PathBuf::from(
            std::env::var("HOME").expect("HOME should be available for ROM-backed C64 tests"),
        )
        .join(".emu198x/roms/commodore-c64")
    }

    #[test]
    fn constructs_with_expected_initial_state() {
        let machine = stub_machine(C64Model::PalBreadbin);
        assert_eq!(machine.phi2_cycles(), 0);
        assert_eq!(machine.frame_count(), 0);
        assert_eq!(machine.raster_line(), 0);
        assert_eq!(machine.cycle_in_line(), 0);
        assert_eq!(machine.vic_bank(), 0);
        assert_eq!(machine.cpu().addr, 0xFFFC);
        assert!(machine.cpu().rw);
        assert!(!machine.cpu().sync);
    }

    #[test]
    fn one_tick_advances_cycle_position() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        assert!(!machine.tick());
        assert_eq!(machine.phi2_cycles(), 1);
        assert_eq!(machine.cycle_in_line(), 1);
        assert_eq!(machine.raster_line(), 0);
    }

    #[test]
    fn pal_line_wraps_after_sixty_three_cycles() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.advance_phi2_cycles(63);
        assert_eq!(machine.cycle_in_line(), 0);
        assert_eq!(machine.raster_line(), 1);
    }

    #[test]
    fn run_frame_matches_pal_geometry() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let cycles = machine.run_frame();
        assert_eq!(cycles, 19_656);
        assert_eq!(machine.frame_count(), 1);
        assert_eq!(machine.raster_line(), 0);
        assert_eq!(machine.cycle_in_line(), 0);
    }

    #[test]
    fn ntsc_frame_geometry_is_honoured() {
        let mut machine = stub_machine(C64Model::NtscBreadbin);
        machine.advance_phi2_cycles(17_095);
        assert_eq!(machine.frame_count(), 1);
        assert_eq!(machine.raster_line(), 0);
        assert_eq!(machine.cycle_in_line(), 0);
    }

    #[test]
    fn cpu_reset_bootstrap_reaches_first_opcode_fetch() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        assert!(!machine.tick());
        assert!(machine.cpu().rw);
        assert_eq!(machine.cpu().addr, 0xFFFD);

        assert!(!machine.tick());
        assert!(machine.cpu().sync);
        assert_eq!(machine.cpu().addr, 0xE000);
        assert_eq!(machine.cpu().regs.pc, 0xE000);
        assert!(machine.cpu().instruction_complete());
    }

    #[test]
    fn cpu_can_execute_load_and_store_through_board_bus() {
        let mut machine = stub_machine_with_reset_vector(C64Model::PalBreadbin, 0x0400);
        machine.memory.ram_write(0x0400, 0xA9);
        machine.memory.ram_write(0x0401, 0x42);
        machine.memory.ram_write(0x0402, 0x8D);
        machine.memory.ram_write(0x0403, 0x00);
        machine.memory.ram_write(0x0404, 0x02);

        machine.tick();
        machine.tick();
        for _ in 0..6 {
            machine.tick();
        }

        assert_eq!(machine.cpu().regs.a, 0x42);
        assert_eq!(machine.memory().ram_read(0x0200), 0x42);
        assert_eq!(machine.cpu().regs.pc, 0x0405);
    }

    #[test]
    fn keyboard_scan_flows_through_dc01() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.keyboard_mut().set_key(0, 1, true);
        machine.cpu_write(0xDC00, 0xFE);
        assert_eq!(machine.cpu_read(0xDC01) & 0x02, 0x00);
        assert_eq!(machine.cia1_port_b_input() & 0x02, 0x00);
    }

    #[test]
    fn keyboard_scan_does_not_transpose_row_and_column() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.keyboard_mut().set_key(0, 1, true);

        machine.cpu_write(0xDC00, 0xFD);
        assert_eq!(machine.cpu_read(0xDC01), 0xFF);

        machine.cpu_write(0xDC00, 0xFE);
        assert_eq!(machine.cpu_read(0xDC01) & 0x02, 0x00);
    }

    #[test]
    fn cia2_port_a_selects_vic_bank() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDD02, 0x03);
        machine.cpu_write(0xDD00, 0x01);
        assert_eq!(machine.vic_bank(), 2);
    }

    fn make_tap(payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 20];
        bytes[..12].copy_from_slice(b"C64-TAPE-RAW");
        bytes[12] = 1;
        bytes[13] = 0;
        bytes[14] = 0;
        bytes[16..20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn tape_start_pulls_sense_low_on_cpu_port() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine
            .load_tap_bytes(&make_tap(&[0x24]))
            .expect("synthetic TAP should load");

        assert_ne!(machine.cpu_read(0x0001) & 0x10, 0);
        machine.play_tape();
        assert_eq!(machine.cpu_read(0x0001) & 0x10, 0);
    }

    #[test]
    fn tape_pulses_raise_cia1_flag_when_motor_runs() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine
            .load_tap_bytes(&make_tap(&[0x01]))
            .expect("synthetic TAP should load");
        machine.play_tape();

        for _ in 0..7 {
            machine.tick();
        }
        assert_eq!(machine.cia1.read(0x0D) & 0x10, 0x00);

        machine.cpu_write(0x0001, machine.memory().port_data() & !0x20);
        for _ in 0..8 {
            machine.tick();
        }

        assert_eq!(machine.cia1.read(0x0D) & 0x10, 0x10);
        assert!(!machine.tape_is_playing());
    }

    #[test]
    fn visible_io_writes_hit_vic_and_underlying_ram() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xD020, 0x06);
        assert_eq!(machine.vic_register(0x20) & 0x0F, 0x06);
        assert_eq!(machine.memory().ram_read(0xD020), 0x06);
    }

    #[test]
    fn visible_sid_io_writes_reach_live_sid_and_underlying_ram() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xD400, 0x34);
        machine.cpu_write(0xD401, 0x12);
        machine.cpu_write(0xD418, 0x0F);

        assert_eq!(machine.sid().voices[0].frequency, 0x1234);
        assert_eq!(machine.sid().volume, 0x0F);
        assert_eq!(machine.memory().ram_read(0xD400), 0x34);
        assert_eq!(machine.memory().ram_read(0xD401), 0x12);
        assert_eq!(machine.memory().ram_read(0xD418), 0x0F);
    }

    #[test]
    fn hidden_io_reads_and_writes_fall_back_to_ram() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0x0000, 0xFF);
        machine.cpu_write(0x0001, 0x00);
        machine.cpu_write(0xD020, 0x44);
        assert_eq!(machine.memory().ram_read(0xD020), 0x44);
        assert_eq!(machine.vic_register(0x20), 0x00);
        assert_eq!(machine.cpu_read(0xD020), 0x44);
    }

    #[test]
    fn active_vic_bank_feeds_vic_reads() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.memory.ram_write(0x5000, 0xAA);
        machine.cpu_write(0xDD02, 0x03);
        machine.cpu_write(0xDD00, 0x02);
        assert_eq!(machine.vic_bank(), 1);
        assert_eq!(machine.vic_read(0x1000), 0xAA);
    }

    #[test]
    fn sid_generates_audio_samples_after_voice_programming() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xD400, 0x37);
        machine.cpu_write(0xD401, 0x1D);
        machine.cpu_write(0xD404, 0x21);
        machine.cpu_write(0xD405, 0x00);
        machine.cpu_write(0xD406, 0xF0);
        machine.cpu_write(0xD418, 0x0F);

        machine.advance_phi2_cycles(19_656);
        let audio = machine.take_audio_buffer();

        assert!(!audio.is_empty(), "SID should emit mixed audio samples");
        assert!(audio.iter().any(|sample| sample.abs() > 0.001));
        assert_eq!(machine.audio_sample_rate(), AUDIO_SAMPLE_RATE);
    }

    #[test]
    fn cia1_timer_irq_reaches_cpu_irq_line() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDC04, 0x00);
        machine.cpu_write(0xDC05, 0x00);
        machine.cpu_write(0xDC0D, 0x81);
        machine.cpu_write(0xDC0E, 0x01);
        assert!(!machine.cpu().irq);
        machine.tick();
        assert!(machine.cia1().irq);
        assert!(machine.cpu().irq);
    }

    #[test]
    fn cia2_timer_irq_reaches_cpu_nmi_line() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDD04, 0x00);
        machine.cpu_write(0xDD05, 0x00);
        machine.cpu_write(0xDD0D, 0x81);
        machine.cpu_write(0xDD0E, 0x01);
        assert!(!machine.cpu().nmi);
        machine.tick();
        assert!(machine.cia2().irq);
        assert!(machine.cpu().nmi);
    }

    #[test]
    fn vic_raster_irq_reaches_cpu_irq_line() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xD012, 0x01);
        machine.cpu_write(0xD01A, 0x01);
        for _ in 0..63 {
            machine.tick();
        }
        assert!(machine.vic().irq);
        assert!(machine.cpu().irq);
    }

    #[test]
    fn badline_ba_stalls_cpu_reads() {
        let mut machine = stub_machine_with_reset_vector(C64Model::PalBreadbin, 0x0400);
        machine.memory.ram_write(0x0400, 0xEA);
        machine.memory.ram_write(0x0401, 0xEA);
        machine.memory.ram_write(0x0402, 0xEA);
        machine.cpu_write(0xD011, 0x1B);

        machine.tick();
        machine.tick();
        let target_cycles = (0x33u64 * 63) + 13;
        while machine.phi2_cycles() < target_cycles {
            machine.tick();
        }

        let pc_before = machine.cpu().regs.pc;
        assert!(machine.vic().ba_low);
        assert!(machine.cpu().rw);
        machine.tick();
        assert_eq!(machine.cpu().regs.pc, pc_before);
    }

    #[test]
    #[ignore = "requires real C64 BASIC/KERNAL/CHARGEN ROMs at ~/.emu198x/roms/commodore-c64"]
    fn boots_kernal_to_ready_prompt() {
        let rom_dir = c64_rom_dir();
        let kernal = fs::read(rom_dir.join("kernal.rom")).expect("KERNAL ROM");
        let basic = fs::read(rom_dir.join("basic.rom")).expect("BASIC ROM");
        let chargen = fs::read(rom_dir.join("chargen.rom")).expect("character ROM");

        let mut machine = C64::new(C64Config {
            model: C64Model::PalBreadbin,
            kernal_rom: &kernal,
            basic_rom: &basic,
            character_rom: &chargen,
        })
        .expect("real C64 ROM set should construct a machine");

        // Screen codes for READY.
        const READY: [u8; 6] = [18, 5, 1, 4, 25, 46];

        let mut found = None;
        for frame in 0..200u32 {
            machine.run_frame();

            for offset in 0..=(0x07E8u16 - 0x0400 - READY.len() as u16) {
                let mut matched = true;
                for (i, &expected) in READY.iter().enumerate() {
                    if machine.memory().ram_read(0x0400 + offset + i as u16) != expected {
                        matched = false;
                        break;
                    }
                }
                if matched {
                    found = Some((frame + 1, offset));
                    break;
                }
            }

            if found.is_some() {
                break;
            }
        }

        assert!(
            found.is_some(),
            "C64 did not reach READY. prompt within 200 frames"
        );
    }

    #[test]
    fn snapshot_round_trip_restores_machine_mid_instruction() {
        let mut machine = stub_machine_with_reset_vector(C64Model::PalBreadbin, 0x0400);
        machine.memory.ram_write(0x0400, 0xAD);
        machine.memory.ram_write(0x0401, 0x00);
        machine.memory.ram_write(0x0402, 0x20);
        machine.memory.ram_write(0x2000, 0x42);
        machine.keyboard_mut().set_key(2, 3, true);

        machine.tick();
        machine.tick();
        machine.tick();

        let snapshot = machine.snapshot_state();
        let mut expected = machine.clone();
        machine.tick();
        machine.tick();
        machine
            .restore_snapshot_state(snapshot)
            .expect("snapshot restore should succeed");

        assert_eq!(machine.cpu().regs.pc, expected.cpu().regs.pc);
        assert_eq!(machine.cpu().addr, expected.cpu().addr);
        assert_eq!(machine.cpu().rw, expected.cpu().rw);
        assert_eq!(machine.vic_bank(), expected.vic_bank());
        assert_eq!(machine.cia1_port_b_input(), expected.cia1_port_b_input());
        assert_eq!(machine.memory().ram(), expected.memory().ram());
        assert_eq!(
            machine.memory().colour_ram(),
            expected.memory().colour_ram()
        );

        for _ in 0..8 {
            let expected_frame_complete = expected.tick();
            let restored_frame_complete = machine.tick();
            assert_eq!(restored_frame_complete, expected_frame_complete);
            assert_eq!(machine.cpu().regs, expected.cpu().regs);
            assert_eq!(machine.cpu().addr, expected.cpu().addr);
            assert_eq!(machine.cpu().rw, expected.cpu().rw);
            assert_eq!(machine.cpu().sync, expected.cpu().sync);
            assert_eq!(machine.cpu().total_cycles, expected.cpu().total_cycles);
            assert_eq!(machine.raster_line(), expected.raster_line());
            assert_eq!(machine.cycle_in_line(), expected.cycle_in_line());
            assert_eq!(machine.vic().irq, expected.vic().irq);
            assert_eq!(machine.vic().ba_low, expected.vic().ba_low);
            assert_eq!(machine.framebuffer(), expected.framebuffer());
        }
    }

    #[test]
    fn load_prg_imports_basic_program_and_updates_vartab() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let prg = [0x01, 0x08, 0x07, 0x08, 0x0A, 0x00, 0x80, 0x00, 0x00, 0x00];

        let load_addr = machine.load_prg(&prg).expect("PRG should load");

        assert_eq!(load_addr, 0x0801);
        assert_eq!(machine.memory().ram_read(0x0801), 0x07);
        assert_eq!(machine.memory().ram_read(0x0802), 0x08);
        let vartab = u16::from(machine.memory().ram_read(0x2D))
            | (u16::from(machine.memory().ram_read(0x2E)) << 8);
        assert_eq!(vartab, 0x0809);
    }
}

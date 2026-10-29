//! Static family selection among fixed Dragon and CoCo boards.
use crate::Model;
use common_dragon::DragonState;
use common_dragon::*;
use emu198x_motorola_vdg_6847::{TextPalette, TextScreen, VdgPalette};
use machine_dragon_32::Dragon32;
use machine_dragon_64::Dragon32 as Dragon64;
use machine_tandy_coco_1::Dragon32 as CoCo1;
use machine_tandy_coco_2::Dragon32 as CoCo2;
/// Physical board selected by the existing runtime profile.
#[derive(Debug, Clone)]
pub enum DragonMachine {
    Dragon32(Dragon32),
    Dragon64(Dragon64),
    CoCo1(CoCo1),
    CoCo2(CoCo2),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            DragonMachine::Dragon32($m) => $body,
            DragonMachine::Dragon64($m) => $body,
            DragonMachine::CoCo1($m) => $body,
            DragonMachine::CoCo2($m) => $body,
        }
    };
}
impl DragonMachine {
    /// Cold boot a fixed board selected by its family profile.
    pub(crate) fn new(model: Model, rom: &[u8; ROM_SIZE]) -> Self {
        match model {
            Model::Dragon32Pal => Self::Dragon32(Dragon32::new(rom)),
            Model::Dragon64Pal => Self::Dragon64(Dragon64::new(rom)),
            Model::CoCo1Ntsc => Self::CoCo1(CoCo1::new(rom)),
            Model::CoCo2Ntsc => Self::CoCo2(CoCo2::new(rom)),
        }
    }
    /// Validate and bind an original payload, retaining operating mode and native phases.
    pub(crate) fn from_state(model: Model, state: DragonState) -> Result<Self, String> {
        match model {
            Model::Dragon32Pal => Dragon32::from_state(state).map(Self::Dragon32),
            Model::Dragon64Pal => Dragon64::from_state(state).map(Self::Dragon64),
            Model::CoCo1Ntsc => CoCo1::from_state(state).map(Self::CoCo1),
            Model::CoCo2Ntsc => CoCo2::from_state(state).map(Self::CoCo2),
        }
    }
    /// Active physical owner.
    pub const fn hardware(&self) -> DragonHardware {
        dispatch!(self,m=>m.hardware())
    }
    /// Delegate to the active fixed board.
    pub fn pc(&self) -> u16 {
        dispatch!(self,m=>m.pc())
    }
    /// Delegate to the active fixed board.
    pub const fn hardware_model(&self) -> DragonHardwareModel {
        dispatch!(self,m=>m.hardware_model())
    }
    /// Delegate to the active fixed board.
    pub const fn video_standard(&self) -> VideoStandard {
        dispatch!(self,m=>m.video_standard())
    }
    /// Delegate to the active fixed board.
    pub fn bus_addr(&self) -> u16 {
        dispatch!(self,m=>m.bus_addr())
    }
    /// Delegate to the active fixed board.
    pub fn bus_rw(&self) -> bool {
        dispatch!(self,m=>m.bus_rw())
    }
    /// Delegate to the active fixed board.
    pub fn cpu_clock_phase(&self) -> Mc6809ClockPhase {
        dispatch!(self,m=>m.cpu_clock_phase())
    }
    /// Delegate to the active fixed board.
    pub fn cycles(&self) -> u64 {
        dispatch!(self,m=>m.cycles())
    }
    /// Delegate to the active fixed board.
    pub fn master_ticks(&self) -> u64 {
        dispatch!(self,m=>m.master_ticks())
    }
    /// Delegate to the active fixed board.
    pub fn instructions(&self) -> u64 {
        dispatch!(self,m=>m.instructions())
    }
    /// Delegate to the active fixed board.
    pub fn is_halted(&self) -> bool {
        dispatch!(self,m=>m.is_halted())
    }
    /// Delegate to the active fixed board.
    pub fn stack_pointer(&self) -> u16 {
        dispatch!(self,m=>m.stack_pointer())
    }
    /// Delegate to the active fixed board.
    pub fn set_keyboard(&mut self, keyboard: DragonKeyboard) {
        dispatch!(self,m=>m.set_keyboard(keyboard))
    }
    /// Delegate to the active fixed board.
    pub fn keyboard_mut(&mut self) -> &mut DragonKeyboard {
        dispatch!(self,m=>m.keyboard_mut())
    }
    /// Delegate to the active fixed board.
    pub fn joystick_button(&self, button: u8) -> Result<bool, DragonJoystickError> {
        dispatch!(self,m=>m.joystick_button(button))
    }
    /// Delegate to the active fixed board.
    pub fn load_cassette_bytes(&mut self, bytes: Vec<u8>) {
        dispatch!(self,m=>m.load_cassette_bytes(bytes))
    }
    /// Delegate to the active fixed board.
    pub fn load_cartridge(&mut self, kind: DragonCartridgeKind, rom: &[u8], autorun: bool) {
        dispatch!(self,m=>m.load_cartridge(kind, rom, autorun))
    }
    /// Delegate to the active fixed board.
    pub fn install_dragon64_mode_rom(&mut self, rom: &[u8; ROM_SIZE]) {
        dispatch!(self,m=>m.install_dragon64_mode_rom(rom))
    }
    /// Delegate to the active fixed board.
    pub fn clear_cartridge(&mut self) {
        dispatch!(self,m=>m.clear_cartridge())
    }
    /// Delegate to the active fixed board.
    pub fn eject_disk(&mut self, drive: usize) -> Result<(), DragonDiskError> {
        dispatch!(self,m=>m.eject_disk(drive))
    }
    /// Delegate to the active fixed board.
    pub fn disk_summary(&self, drive: usize) -> Option<DragonDiskSummary> {
        dispatch!(self,m=>m.disk_summary(drive))
    }
    /// Delegate to the active fixed board.
    pub fn disk_image(&self, drive: usize) -> Option<&DragonDiskImage> {
        dispatch!(self,m=>m.disk_image(drive))
    }
    /// Delegate to the active fixed board.
    pub fn disk_inserted(&self, drive: usize) -> bool {
        dispatch!(self,m=>m.disk_inserted(drive))
    }
    /// Delegate to the active fixed board.
    pub fn set_cartridge_sound_level(&mut self, level: f32) {
        dispatch!(self,m=>m.set_cartridge_sound_level(level))
    }
    /// Delegate to the active fixed board.
    pub fn ram(&self) -> &[u8; RAM_SIZE] {
        dispatch!(self,m=>m.ram())
    }
    /// Delegate to the active fixed board.
    pub fn cpu(&self) -> &Mc6809 {
        dispatch!(self,m=>m.cpu())
    }
    /// Delegate to the active fixed board.
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self,m=>m.peek(addr))
    }
    /// Delegate to the active fixed board.
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self,m=>m.poke(addr, value))
    }
    /// Delegate to the active fixed board.
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self,m=>m.step_instruction())
    }
    /// Delegate to the active fixed board.
    pub fn clear_cassette(&mut self) {
        dispatch!(self,m=>m.clear_cassette())
    }
    /// Delegate to the active fixed board.
    pub const fn deck_running(&self) -> bool {
        dispatch!(self,m=>m.deck_running())
    }
    /// Delegate to the active fixed board.
    pub const fn set_deck_running(&mut self, running: bool) {
        dispatch!(self,m=>m.set_deck_running(running))
    }
    /// Delegate to the active fixed board.
    pub fn cassette_motor_on(&self) -> bool {
        dispatch!(self,m=>m.cassette_motor_on())
    }
    /// Delegate to the active fixed board.
    pub fn cassette_finished(&self) -> bool {
        dispatch!(self,m=>m.cassette_finished())
    }
    /// Delegate to the active fixed board.
    pub fn cassette_position_bits(&self) -> usize {
        dispatch!(self,m=>m.cassette_position_bits())
    }
    /// Delegate to the active fixed board.
    pub fn cassette_len_bits(&self) -> usize {
        dispatch!(self,m=>m.cassette_len_bits())
    }
    /// Delegate to the active fixed board.
    pub fn cassette_line_level(&self) -> bool {
        dispatch!(self,m=>m.cassette_line_level())
    }
    /// Delegate to the active fixed board.
    pub fn pia1_ca2(&self) -> bool {
        dispatch!(self,m=>m.pia1_ca2())
    }
    /// Delegate to the active fixed board.
    pub fn pia0_ddr_a(&self) -> u8 {
        dispatch!(self,m=>m.pia0_ddr_a())
    }
    /// Delegate to the active fixed board.
    pub fn pia0_ddr_b(&self) -> u8 {
        dispatch!(self,m=>m.pia0_ddr_b())
    }
    /// Delegate to the active fixed board.
    pub fn pia0_control_a(&self) -> u8 {
        dispatch!(self,m=>m.pia0_control_a())
    }
    /// Delegate to the active fixed board.
    pub fn pia0_control_b(&self) -> u8 {
        dispatch!(self,m=>m.pia0_control_b())
    }
    /// Delegate to the active fixed board.
    pub fn pia1_cb2(&self) -> bool {
        dispatch!(self,m=>m.pia1_cb2())
    }
    /// Delegate to the active fixed board.
    pub fn pia1_control_a(&self) -> u8 {
        dispatch!(self,m=>m.pia1_control_a())
    }
    /// Delegate to the active fixed board.
    pub fn pia1_control_b(&self) -> u8 {
        dispatch!(self,m=>m.pia1_control_b())
    }
    /// Delegate to the active fixed board.
    pub fn pia1_ddr_b(&self) -> u8 {
        dispatch!(self,m=>m.pia1_ddr_b())
    }
    /// Delegate to the active fixed board.
    pub fn pia1_output_b(&self) -> u8 {
        dispatch!(self,m=>m.pia1_output_b())
    }
    /// Delegate to the active fixed board.
    pub fn pia1_pins_b(&self) -> u8 {
        dispatch!(self,m=>m.pia1_pins_b())
    }
    /// Delegate to the active fixed board.
    pub fn sam_video_mode(&self) -> u8 {
        dispatch!(self,m=>m.sam_video_mode())
    }
    /// Delegate to the active fixed board.
    pub fn sam_display_offset(&self) -> u8 {
        dispatch!(self,m=>m.sam_display_offset())
    }
    /// Delegate to the active fixed board.
    pub fn text_screen_base(&self) -> u16 {
        dispatch!(self,m=>m.text_screen_base())
    }
    /// Delegate to the active fixed board.
    pub fn video_display_base(&self) -> u16 {
        dispatch!(self,m=>m.video_display_base())
    }
    /// Delegate to the active fixed board.
    pub fn capture_text_screen(&self) -> TextScreen {
        dispatch!(self,m=>m.capture_text_screen())
    }
    /// Delegate to the active fixed board.
    pub fn render_visible_text_argb(&self, palette: TextPalette) -> Vec<u32> {
        dispatch!(self,m=>m.render_visible_text_argb(palette))
    }
    /// Delegate to the active fixed board.
    pub fn render_visible_argb(&self, palette: VdgPalette) -> Vec<u32> {
        dispatch!(self,m=>m.render_visible_argb(palette))
    }
    /// Delegate to the active fixed board.
    pub fn beam_visible_argb(&self) -> &[u32] {
        dispatch!(self,m=>m.beam_visible_argb())
    }
    /// Delegate to the active fixed board.
    pub fn video_phase(&self) -> DragonVideoPhase {
        dispatch!(self,m=>m.video_phase())
    }
    /// Delegate to the active fixed board.
    pub fn beam_pal_overscan_argb(&self) -> Vec<u32> {
        dispatch!(self,m=>m.beam_pal_overscan_argb())
    }
    /// Delegate to the active fixed board.
    pub const fn audio_sample_rate(&self) -> u32 {
        dispatch!(self,m=>m.audio_sample_rate())
    }
    /// Delegate to the active fixed board.
    pub fn drain_audio_samples(&mut self, dest: &mut Vec<f32>) {
        dispatch!(self,m=>m.drain_audio_samples(dest))
    }
    /// Delegate to the active fixed board.
    pub fn has_serial_port(&self) -> bool {
        dispatch!(self,m=>m.has_serial_port())
    }
    /// Delegate to the active fixed board.
    pub fn queue_serial_input(&mut self, bytes: &[u8]) -> bool {
        dispatch!(self,m=>m.queue_serial_input(bytes))
    }
    /// Delegate to the active fixed board.
    pub fn serial_input_pending(&self) -> usize {
        dispatch!(self,m=>m.serial_input_pending())
    }
    /// Delegate to the active fixed board.
    pub fn drain_serial_output(&mut self) -> Vec<u8> {
        dispatch!(self,m=>m.drain_serial_output())
    }
    /// Delegate to the active fixed board.
    pub fn set_serial_modem_inputs(&mut self, cts: bool, dsr: bool, dcd: bool) -> bool {
        dispatch!(self,m=>m.set_serial_modem_inputs(cts, dsr, dcd))
    }
    /// Delegate to the active fixed board.
    pub fn acia_registers(&self) -> Option<Acia6551Registers> {
        dispatch!(self,m=>m.acia_registers())
    }
    /// Delegate to the active fixed board.
    pub fn acia_baud_rate(&self) -> Option<f64> {
        dispatch!(self,m=>m.acia_baud_rate())
    }
    /// Delegate to the active fixed board.
    pub fn step_cycle(&mut self) -> Option<MemoryEvent> {
        dispatch!(self,m=>m.step_cycle())
    }
    /// Delegate to the active fixed board.
    pub fn run_cycles(&mut self, cycle_limit: u64, trace_limit: usize) -> RunReport {
        dispatch!(self,m=>m.run_cycles(cycle_limit, trace_limit))
    }
    /// Delegate to the active fixed board.
    pub fn run_cycles_with_options(&mut self, cycle_limit: u64, options: RunOptions) -> RunReport {
        dispatch!(self,m=>m.run_cycles_with_options(cycle_limit, options))
    }
    /// Delegate to the active fixed board.
    pub fn run_frame(&mut self, trace_limit: usize) -> (RunReport, bool) {
        dispatch!(self,m=>m.run_frame(trace_limit))
    }
}
impl serde::Serialize for DragonMachine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>m.serialize(s))
    }
}

impl DragonMachine {
    /// Delegate to the active fixed board.
    pub fn set_joystick_axis(
        &mut self,
        port: u8,
        axis: DragonJoystickAxis,
        value: u16,
    ) -> Result<(), DragonJoystickError> {
        dispatch!(self,m=>m.set_joystick_axis(port, axis, value))
    }
    /// Delegate to the active fixed board.
    pub fn set_joystick_button(
        &mut self,
        button: u8,
        pressed: bool,
    ) -> Result<(), DragonJoystickError> {
        dispatch!(self,m=>m.set_joystick_button(button, pressed))
    }
    /// Delegate to the active fixed board.
    pub fn joystick_axis(
        &self,
        port: u8,
        axis: DragonJoystickAxis,
    ) -> Result<u16, DragonJoystickError> {
        dispatch!(self,m=>m.joystick_axis(port, axis))
    }
    /// Delegate to the active fixed board.
    pub fn insert_disk(
        &mut self,
        drive: usize,
        image: DragonDiskImage,
    ) -> Result<(), DragonDiskError> {
        dispatch!(self,m=>m.insert_disk(drive, image))
    }
    /// Delegate to the active fixed board.
    pub fn set_disk_write_protected(
        &mut self,
        drive: usize,
        protected: bool,
    ) -> Result<(), DragonDiskError> {
        dispatch!(self,m=>m.set_disk_write_protected(drive, protected))
    }
    /// Delegate to the active fixed board.
    pub fn load_binary_program(
        &mut self,
        load_address: u16,
        payload: &[u8],
        exec_address: u16,
        autorun: bool,
    ) -> Result<(), DragonProgramLoadError> {
        dispatch!(self,m=>m.load_binary_program(load_address, payload, exec_address, autorun))
    }
    /// Delegate to the active fixed board.
    pub fn load_pcdragon_snapshot(
        &mut self,
        load_address: u16,
        ram: &[u8],
        registers: DragonSnapshotRegisters,
        peripherals: Option<DragonSnapshotPeripherals>,
        display_base: Option<u16>,
    ) {
        dispatch!(self,m=>m.load_pcdragon_snapshot(load_address, ram, registers, peripherals, display_base))
    }
}

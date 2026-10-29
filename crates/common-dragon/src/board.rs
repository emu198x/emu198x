//! Fixed physical owners over the original Dragon/CoCo state engine.
use super::*;
/// Physical board; Dragon 64 compatible and native modes share one owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragonHardware {
    Dragon32,
    Dragon64,
    CoCo1,
    CoCo2,
}
/// Static construction and restore policy supplied by a concrete owner.
pub trait DragonBoard {
    const HARDWARE: DragonHardware;
    const COLD_BOOT_MODEL: DragonHardwareModel;
    const VIDEO: VideoStandard;
    fn accepts(model: DragonHardwareModel) -> bool {
        model == Self::COLD_BOOT_MODEL
    }
}
/// A fixed board with transparent legacy state serialization.
#[derive(Debug, Clone)]
pub struct DragonCore<B: DragonBoard> {
    state: DragonState,
    board: std::marker::PhantomData<B>,
}
impl<B: DragonBoard> Serialize for DragonCore<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<B: DragonBoard> DragonCore<B> {
    /// Cold boot this physical board.
    pub fn new(rom: &[u8; ROM_SIZE]) -> Self {
        Self::new_with_keyboard(rom, DragonKeyboard::new())
    }
    /// Cold boot this board with an initial keyboard matrix.
    pub fn new_with_keyboard(rom: &[u8; ROM_SIZE], keyboard: DragonKeyboard) -> Self {
        Self {
            state: DragonState::new_with_keyboard_and_model(rom, keyboard, B::COLD_BOOT_MODEL)
                .with_video_standard(B::VIDEO),
            board: std::marker::PhantomData,
        }
    }
    /// Bind legacy state without resetting any hardware or pending phase.
    pub fn from_state(state: DragonState) -> Result<Self, String> {
        if !B::accepts(state.hardware_model()) || state.video_standard() != B::VIDEO {
            return Err("snapshot state belongs to another Dragon/CoCo board".into());
        }
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    /// This owner's immutable physical board identity.
    pub const fn hardware(&self) -> DragonHardware {
        B::HARDWARE
    }
    /// Delegate to the unchanged hardware state.
    pub fn pc(&self) -> u16 {
        self.state.pc()
    }
    /// Delegate to the unchanged hardware state.
    pub const fn hardware_model(&self) -> DragonHardwareModel {
        self.state.hardware_model()
    }
    /// Delegate to the unchanged hardware state.
    pub const fn video_standard(&self) -> VideoStandard {
        self.state.video_standard()
    }
    /// Delegate to the unchanged hardware state.
    pub fn bus_addr(&self) -> u16 {
        self.state.bus_addr()
    }
    /// Delegate to the unchanged hardware state.
    pub fn bus_rw(&self) -> bool {
        self.state.bus_rw()
    }
    /// Delegate to the unchanged hardware state.
    pub fn cpu_clock_phase(&self) -> Mc6809ClockPhase {
        self.state.cpu_clock_phase()
    }
    /// Delegate to the unchanged hardware state.
    pub fn cycles(&self) -> u64 {
        self.state.cycles()
    }
    /// Delegate to the unchanged hardware state.
    pub fn master_ticks(&self) -> u64 {
        self.state.master_ticks()
    }
    /// Delegate to the unchanged hardware state.
    pub fn instructions(&self) -> u64 {
        self.state.instructions()
    }
    /// Delegate to the unchanged hardware state.
    pub fn is_halted(&self) -> bool {
        self.state.is_halted()
    }
    /// Delegate to the unchanged hardware state.
    pub fn stack_pointer(&self) -> u16 {
        self.state.stack_pointer()
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_keyboard(&mut self, keyboard: DragonKeyboard) {
        self.state.set_keyboard(keyboard)
    }
    /// Delegate to the unchanged hardware state.
    pub fn keyboard_mut(&mut self) -> &mut DragonKeyboard {
        self.state.keyboard_mut()
    }
    /// Delegate to the unchanged hardware state.
    pub fn joystick_button(&self, button: u8) -> Result<bool, DragonJoystickError> {
        self.state.joystick_button(button)
    }
    /// Delegate to the unchanged hardware state.
    pub fn load_cassette_bytes(&mut self, bytes: Vec<u8>) {
        self.state.load_cassette_bytes(bytes)
    }
    /// Delegate to the unchanged hardware state.
    pub fn load_cartridge(&mut self, kind: DragonCartridgeKind, rom: &[u8], autorun: bool) {
        self.state.load_cartridge(kind, rom, autorun)
    }
    /// Delegate to the unchanged hardware state.
    pub fn install_dragon64_mode_rom(&mut self, rom: &[u8; ROM_SIZE]) {
        self.state.install_dragon64_mode_rom(rom)
    }
    /// Delegate to the unchanged hardware state.
    pub fn clear_cartridge(&mut self) {
        self.state.clear_cartridge()
    }
    /// Delegate to the unchanged hardware state.
    pub fn eject_disk(&mut self, drive: usize) -> Result<(), DragonDiskError> {
        self.state.eject_disk(drive)
    }
    /// Delegate to the unchanged hardware state.
    pub fn disk_summary(&self, drive: usize) -> Option<DragonDiskSummary> {
        self.state.disk_summary(drive)
    }
    /// Delegate to the unchanged hardware state.
    pub fn disk_image(&self, drive: usize) -> Option<&DragonDiskImage> {
        self.state.disk_image(drive)
    }
    /// Delegate to the unchanged hardware state.
    pub fn disk_inserted(&self, drive: usize) -> bool {
        self.state.disk_inserted(drive)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_cartridge_sound_level(&mut self, level: f32) {
        self.state.set_cartridge_sound_level(level)
    }
    /// Delegate to the unchanged hardware state.
    pub fn ram(&self) -> &[u8; RAM_SIZE] {
        self.state.ram()
    }
    /// Delegate to the unchanged hardware state.
    pub fn cpu(&self) -> &Mc6809 {
        self.state.cpu()
    }
    /// Delegate to the unchanged hardware state.
    pub fn peek(&self, addr: u16) -> u8 {
        self.state.peek(addr)
    }
    /// Delegate to the unchanged hardware state.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.state.poke(addr, value)
    }
    /// Delegate to the unchanged hardware state.
    pub fn step_instruction(&mut self) -> u64 {
        self.state.step_instruction()
    }
    /// Delegate to the unchanged hardware state.
    pub fn clear_cassette(&mut self) {
        self.state.clear_cassette()
    }
    /// Delegate to the unchanged hardware state.
    pub const fn deck_running(&self) -> bool {
        self.state.deck_running()
    }
    /// Delegate to the unchanged hardware state.
    pub const fn set_deck_running(&mut self, running: bool) {
        self.state.set_deck_running(running)
    }
    /// Delegate to the unchanged hardware state.
    pub fn cassette_motor_on(&self) -> bool {
        self.state.cassette_motor_on()
    }
    /// Delegate to the unchanged hardware state.
    pub fn cassette_finished(&self) -> bool {
        self.state.cassette_finished()
    }
    /// Delegate to the unchanged hardware state.
    pub fn cassette_position_bits(&self) -> usize {
        self.state.cassette_position_bits()
    }
    /// Delegate to the unchanged hardware state.
    pub fn cassette_len_bits(&self) -> usize {
        self.state.cassette_len_bits()
    }
    /// Delegate to the unchanged hardware state.
    pub fn cassette_line_level(&self) -> bool {
        self.state.cassette_line_level()
    }
    /// Delegate to the unchanged hardware state.
    pub fn pia1_ca2(&self) -> bool {
        self.state.pia1_ca2()
    }
    /// Delegate to the unchanged hardware state.
    pub fn pia0_ddr_a(&self) -> u8 {
        self.state.pia0_ddr_a()
    }
    /// Delegate to the unchanged hardware state.
    pub fn pia0_ddr_b(&self) -> u8 {
        self.state.pia0_ddr_b()
    }
    /// Delegate to the unchanged hardware state.
    pub fn pia0_control_a(&self) -> u8 {
        self.state.pia0_control_a()
    }
    /// Delegate to the unchanged hardware state.
    pub fn pia0_control_b(&self) -> u8 {
        self.state.pia0_control_b()
    }
    /// Delegate to the unchanged hardware state.
    pub fn pia1_cb2(&self) -> bool {
        self.state.pia1_cb2()
    }
    /// Delegate to the unchanged hardware state.
    pub fn pia1_control_a(&self) -> u8 {
        self.state.pia1_control_a()
    }
    /// Delegate to the unchanged hardware state.
    pub fn pia1_control_b(&self) -> u8 {
        self.state.pia1_control_b()
    }
    /// Delegate to the unchanged hardware state.
    pub fn pia1_ddr_b(&self) -> u8 {
        self.state.pia1_ddr_b()
    }
    /// Delegate to the unchanged hardware state.
    pub fn pia1_output_b(&self) -> u8 {
        self.state.pia1_output_b()
    }
    /// Delegate to the unchanged hardware state.
    pub fn pia1_pins_b(&self) -> u8 {
        self.state.pia1_pins_b()
    }
    /// Delegate to the unchanged hardware state.
    pub fn sam_video_mode(&self) -> u8 {
        self.state.sam_video_mode()
    }
    /// Delegate to the unchanged hardware state.
    pub fn sam_display_offset(&self) -> u8 {
        self.state.sam_display_offset()
    }
    /// Delegate to the unchanged hardware state.
    pub fn text_screen_base(&self) -> u16 {
        self.state.text_screen_base()
    }
    /// Delegate to the unchanged hardware state.
    pub fn video_display_base(&self) -> u16 {
        self.state.video_display_base()
    }
    /// Delegate to the unchanged hardware state.
    pub fn capture_text_screen(&self) -> TextScreen {
        self.state.capture_text_screen()
    }
    /// Delegate to the unchanged hardware state.
    pub fn render_visible_text_argb(&self, palette: TextPalette) -> Vec<u32> {
        self.state.render_visible_text_argb(palette)
    }
    /// Delegate to the unchanged hardware state.
    pub fn render_visible_argb(&self, palette: VdgPalette) -> Vec<u32> {
        self.state.render_visible_argb(palette)
    }
    /// Delegate to the unchanged hardware state.
    pub fn beam_visible_argb(&self) -> &[u32] {
        self.state.beam_visible_argb()
    }
    /// Delegate to the unchanged hardware state.
    pub fn video_phase(&self) -> DragonVideoPhase {
        self.state.video_phase()
    }
    /// Delegate to the unchanged hardware state.
    pub fn beam_pal_overscan_argb(&self) -> Vec<u32> {
        self.state.beam_pal_overscan_argb()
    }
    /// Delegate to the unchanged hardware state.
    pub const fn audio_sample_rate(&self) -> u32 {
        self.state.audio_sample_rate()
    }
    /// Delegate to the unchanged hardware state.
    pub fn drain_audio_samples(&mut self, dest: &mut Vec<f32>) {
        self.state.drain_audio_samples(dest)
    }
    /// Delegate to the unchanged hardware state.
    pub fn has_serial_port(&self) -> bool {
        self.state.has_serial_port()
    }
    /// Delegate to the unchanged hardware state.
    pub fn queue_serial_input(&mut self, bytes: &[u8]) -> bool {
        self.state.queue_serial_input(bytes)
    }
    /// Delegate to the unchanged hardware state.
    pub fn serial_input_pending(&self) -> usize {
        self.state.serial_input_pending()
    }
    /// Delegate to the unchanged hardware state.
    pub fn drain_serial_output(&mut self) -> Vec<u8> {
        self.state.drain_serial_output()
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_serial_modem_inputs(&mut self, cts: bool, dsr: bool, dcd: bool) -> bool {
        self.state.set_serial_modem_inputs(cts, dsr, dcd)
    }
    /// Delegate to the unchanged hardware state.
    pub fn acia_registers(&self) -> Option<Acia6551Registers> {
        self.state.acia_registers()
    }
    /// Delegate to the unchanged hardware state.
    pub fn acia_baud_rate(&self) -> Option<f64> {
        self.state.acia_baud_rate()
    }
    /// Delegate to the unchanged hardware state.
    pub fn step_cycle(&mut self) -> Option<MemoryEvent> {
        self.state.step_cycle()
    }
    /// Delegate to the unchanged hardware state.
    pub fn run_cycles(&mut self, cycle_limit: u64, trace_limit: usize) -> RunReport {
        self.state.run_cycles(cycle_limit, trace_limit)
    }
    /// Delegate to the unchanged hardware state.
    pub fn run_cycles_with_options(&mut self, cycle_limit: u64, options: RunOptions) -> RunReport {
        self.state.run_cycles_with_options(cycle_limit, options)
    }
    /// Delegate to the unchanged hardware state.
    pub fn run_frame(&mut self, trace_limit: usize) -> (RunReport, bool) {
        self.state.run_frame(trace_limit)
    }
}

impl<B: DragonBoard> DragonCore<B> {
    /// Delegate to the unchanged hardware state.
    pub fn set_joystick_axis(
        &mut self,
        port: u8,
        axis: DragonJoystickAxis,
        value: u16,
    ) -> Result<(), DragonJoystickError> {
        self.state.set_joystick_axis(port, axis, value)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_joystick_button(
        &mut self,
        button: u8,
        pressed: bool,
    ) -> Result<(), DragonJoystickError> {
        self.state.set_joystick_button(button, pressed)
    }
    /// Delegate to the unchanged hardware state.
    pub fn joystick_axis(
        &self,
        port: u8,
        axis: DragonJoystickAxis,
    ) -> Result<u16, DragonJoystickError> {
        self.state.joystick_axis(port, axis)
    }
    /// Delegate to the unchanged hardware state.
    pub fn insert_disk(
        &mut self,
        drive: usize,
        image: DragonDiskImage,
    ) -> Result<(), DragonDiskError> {
        self.state.insert_disk(drive, image)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_disk_write_protected(
        &mut self,
        drive: usize,
        protected: bool,
    ) -> Result<(), DragonDiskError> {
        self.state.set_disk_write_protected(drive, protected)
    }
    /// Delegate to the unchanged hardware state.
    pub fn load_binary_program(
        &mut self,
        load_address: u16,
        payload: &[u8],
        exec_address: u16,
        autorun: bool,
    ) -> Result<(), DragonProgramLoadError> {
        self.state
            .load_binary_program(load_address, payload, exec_address, autorun)
    }
    /// Delegate to the unchanged hardware state.
    pub fn load_pcdragon_snapshot(
        &mut self,
        load_address: u16,
        ram: &[u8],
        registers: DragonSnapshotRegisters,
        peripherals: Option<DragonSnapshotPeripherals>,
        display_base: Option<u16>,
    ) {
        self.state
            .load_pcdragon_snapshot(load_address, ram, registers, peripherals, display_base)
    }
}

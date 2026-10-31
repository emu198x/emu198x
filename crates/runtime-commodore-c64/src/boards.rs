//! Fixed-owner dispatch for the existing generic family runtime.
use common_commodore_c64::board::Sid6581;
use common_commodore_c64::machine::SuperCpu;
use common_commodore_c64::*;
use common_commodore_c128::Z80;
use common_commodore_c128::{C128Model, C128Roms, RomError};
use common_commodore_iec::IecBus;
use emu198x_mos_6502::M6502;
use emu198x_mos_cia_6526::Cia6526;
use emu198x_mos_vic_ii::Vic;
#[derive(Clone)]
pub enum C64 {
    PalBreadbin(Box<machine_commodore_c64_pal_breadbin::C64>),
    NtscBreadbin(Box<machine_commodore_c64_ntsc_breadbin::C64>),
    PalC64c(Box<machine_commodore_c64c_pal::C64>),
    NtscC64c(Box<machine_commodore_c64c_ntsc::C64>),
}
impl C64 {
    pub fn new(config: C64Config<'_>) -> Result<Self, MemoryInitError> {
        Ok(match config.model {
            C64Model::PalBreadbin => {
                Self::PalBreadbin(Box::new(machine_commodore_c64_pal_breadbin::C64::new(
                    config.kernal_rom,
                    config.basic_rom,
                    config.character_rom,
                )?))
            }
            C64Model::NtscBreadbin => {
                Self::NtscBreadbin(Box::new(machine_commodore_c64_ntsc_breadbin::C64::new(
                    config.kernal_rom,
                    config.basic_rom,
                    config.character_rom,
                )?))
            }
            C64Model::PalC64c => Self::PalC64c(Box::new(machine_commodore_c64c_pal::C64::new(
                config.kernal_rom,
                config.basic_rom,
                config.character_rom,
            )?)),
            C64Model::NtscC64c => Self::NtscC64c(Box::new(machine_commodore_c64c_ntsc::C64::new(
                config.kernal_rom,
                config.basic_rom,
                config.character_rom,
            )?)),
        })
    }
    pub fn model(&self) -> C64Model {
        match self {
            Self::PalBreadbin(m) => m.model(),
            Self::NtscBreadbin(m) => m.model(),
            Self::PalC64c(m) => m.model(),
            Self::NtscC64c(m) => m.model(),
        }
    }
    pub fn cpu(&self) -> &M6502 {
        match self {
            Self::PalBreadbin(m) => m.cpu(),
            Self::NtscBreadbin(m) => m.cpu(),
            Self::PalC64c(m) => m.cpu(),
            Self::NtscC64c(m) => m.cpu(),
        }
    }
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        match self {
            Self::PalBreadbin(m) => m.cpu_mut(),
            Self::NtscBreadbin(m) => m.cpu_mut(),
            Self::PalC64c(m) => m.cpu_mut(),
            Self::NtscC64c(m) => m.cpu_mut(),
        }
    }
    pub fn peek(&self, addr: u16) -> u8 {
        match self {
            Self::PalBreadbin(m) => m.peek(addr),
            Self::NtscBreadbin(m) => m.peek(addr),
            Self::PalC64c(m) => m.peek(addr),
            Self::NtscC64c(m) => m.peek(addr),
        }
    }
    pub fn poke(&mut self, addr: u16, value: u8) {
        match self {
            Self::PalBreadbin(m) => m.poke(addr, value),
            Self::NtscBreadbin(m) => m.poke(addr, value),
            Self::PalC64c(m) => m.poke(addr, value),
            Self::NtscC64c(m) => m.poke(addr, value),
        }
    }
    pub fn step_instruction(&mut self) -> u64 {
        match self {
            Self::PalBreadbin(m) => m.step_instruction(),
            Self::NtscBreadbin(m) => m.step_instruction(),
            Self::PalC64c(m) => m.step_instruction(),
            Self::NtscC64c(m) => m.step_instruction(),
        }
    }
    pub fn vic(&self) -> &Vic {
        match self {
            Self::PalBreadbin(m) => m.vic(),
            Self::NtscBreadbin(m) => m.vic(),
            Self::PalC64c(m) => m.vic(),
            Self::NtscC64c(m) => m.vic(),
        }
    }
    pub fn cia1(&self) -> &Cia6526 {
        match self {
            Self::PalBreadbin(m) => m.cia1(),
            Self::NtscBreadbin(m) => m.cia1(),
            Self::PalC64c(m) => m.cia1(),
            Self::NtscC64c(m) => m.cia1(),
        }
    }
    pub fn cia2(&self) -> &Cia6526 {
        match self {
            Self::PalBreadbin(m) => m.cia2(),
            Self::NtscBreadbin(m) => m.cia2(),
            Self::PalC64c(m) => m.cia2(),
            Self::NtscC64c(m) => m.cia2(),
        }
    }
    pub fn timing(&self) -> C64Timing {
        match self {
            Self::PalBreadbin(m) => m.timing(),
            Self::NtscBreadbin(m) => m.timing(),
            Self::PalC64c(m) => m.timing(),
            Self::NtscC64c(m) => m.timing(),
        }
    }
    pub fn memory(&self) -> &C64Memory {
        match self {
            Self::PalBreadbin(m) => m.memory(),
            Self::NtscBreadbin(m) => m.memory(),
            Self::PalC64c(m) => m.memory(),
            Self::NtscC64c(m) => m.memory(),
        }
    }
    pub fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        match self {
            Self::PalBreadbin(m) => m.keyboard_mut(),
            Self::NtscBreadbin(m) => m.keyboard_mut(),
            Self::PalC64c(m) => m.keyboard_mut(),
            Self::NtscC64c(m) => m.keyboard_mut(),
        }
    }
    pub fn set_restore(&mut self, pressed: bool) {
        match self {
            Self::PalBreadbin(m) => m.set_restore(pressed),
            Self::NtscBreadbin(m) => m.set_restore(pressed),
            Self::PalC64c(m) => m.set_restore(pressed),
            Self::NtscC64c(m) => m.set_restore(pressed),
        }
    }
    pub fn set_cart_freeze(&mut self, pressed: bool) -> bool {
        match self {
            Self::PalBreadbin(m) => m.set_cart_freeze(pressed),
            Self::NtscBreadbin(m) => m.set_cart_freeze(pressed),
            Self::PalC64c(m) => m.set_cart_freeze(pressed),
            Self::NtscC64c(m) => m.set_cart_freeze(pressed),
        }
    }
    pub fn freeze_cart_registers(&self) -> Option<(u8, u8)> {
        match self {
            Self::PalBreadbin(m) => m.freeze_cart_registers(),
            Self::NtscBreadbin(m) => m.freeze_cart_registers(),
            Self::PalC64c(m) => m.freeze_cart_registers(),
            Self::NtscC64c(m) => m.freeze_cart_registers(),
        }
    }
    pub fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool) -> bool {
        match self {
            Self::PalBreadbin(m) => m.set_joystick_control(port, name, pressed),
            Self::NtscBreadbin(m) => m.set_joystick_control(port, name, pressed),
            Self::PalC64c(m) => m.set_joystick_control(port, name, pressed),
            Self::NtscC64c(m) => m.set_joystick_control(port, name, pressed),
        }
    }
    pub fn set_paddle(&mut self, port: u8, axis: u8, value: u8) -> bool {
        match self {
            Self::PalBreadbin(m) => m.set_paddle(port, axis, value),
            Self::NtscBreadbin(m) => m.set_paddle(port, axis, value),
            Self::PalC64c(m) => m.set_paddle(port, axis, value),
            Self::NtscC64c(m) => m.set_paddle(port, axis, value),
        }
    }
    pub fn attach_mouse_1351(&mut self, port: u8) -> bool {
        match self {
            Self::PalBreadbin(m) => m.attach_mouse_1351(port),
            Self::NtscBreadbin(m) => m.attach_mouse_1351(port),
            Self::PalC64c(m) => m.attach_mouse_1351(port),
            Self::NtscC64c(m) => m.attach_mouse_1351(port),
        }
    }
    pub fn detach_mouse_1351(&mut self, port: u8) -> bool {
        match self {
            Self::PalBreadbin(m) => m.detach_mouse_1351(port),
            Self::NtscBreadbin(m) => m.detach_mouse_1351(port),
            Self::PalC64c(m) => m.detach_mouse_1351(port),
            Self::NtscC64c(m) => m.detach_mouse_1351(port),
        }
    }
    pub fn has_mouse_1351(&self, port: u8) -> bool {
        match self {
            Self::PalBreadbin(m) => m.has_mouse_1351(port),
            Self::NtscBreadbin(m) => m.has_mouse_1351(port),
            Self::PalC64c(m) => m.has_mouse_1351(port),
            Self::NtscC64c(m) => m.has_mouse_1351(port),
        }
    }
    pub fn move_mouse_1351(&mut self, port: u8, dx: i32, dy: i32) -> bool {
        match self {
            Self::PalBreadbin(m) => m.move_mouse_1351(port, dx, dy),
            Self::NtscBreadbin(m) => m.move_mouse_1351(port, dx, dy),
            Self::PalC64c(m) => m.move_mouse_1351(port, dx, dy),
            Self::NtscC64c(m) => m.move_mouse_1351(port, dx, dy),
        }
    }
    pub fn set_mouse_1351_button(&mut self, port: u8, button: &str, pressed: bool) -> bool {
        match self {
            Self::PalBreadbin(m) => m.set_mouse_1351_button(port, button, pressed),
            Self::NtscBreadbin(m) => m.set_mouse_1351_button(port, button, pressed),
            Self::PalC64c(m) => m.set_mouse_1351_button(port, button, pressed),
            Self::NtscC64c(m) => m.set_mouse_1351_button(port, button, pressed),
        }
    }
    pub fn phi2_cycles(&self) -> u64 {
        match self {
            Self::PalBreadbin(m) => m.phi2_cycles(),
            Self::NtscBreadbin(m) => m.phi2_cycles(),
            Self::PalC64c(m) => m.phi2_cycles(),
            Self::NtscC64c(m) => m.phi2_cycles(),
        }
    }
    pub fn frame_count(&self) -> u64 {
        match self {
            Self::PalBreadbin(m) => m.frame_count(),
            Self::NtscBreadbin(m) => m.frame_count(),
            Self::PalC64c(m) => m.frame_count(),
            Self::NtscC64c(m) => m.frame_count(),
        }
    }
    pub fn raster_line(&self) -> u16 {
        match self {
            Self::PalBreadbin(m) => m.raster_line(),
            Self::NtscBreadbin(m) => m.raster_line(),
            Self::PalC64c(m) => m.raster_line(),
            Self::NtscC64c(m) => m.raster_line(),
        }
    }
    pub fn cycle_in_line(&self) -> u8 {
        match self {
            Self::PalBreadbin(m) => m.cycle_in_line(),
            Self::NtscBreadbin(m) => m.cycle_in_line(),
            Self::PalC64c(m) => m.cycle_in_line(),
            Self::NtscC64c(m) => m.cycle_in_line(),
        }
    }
    pub fn vic_bank(&self) -> u8 {
        match self {
            Self::PalBreadbin(m) => m.vic_bank(),
            Self::NtscBreadbin(m) => m.vic_bank(),
            Self::PalC64c(m) => m.vic_bank(),
            Self::NtscC64c(m) => m.vic_bank(),
        }
    }
    pub fn cia1_port_b_input(&self) -> u8 {
        match self {
            Self::PalBreadbin(m) => m.cia1_port_b_input(),
            Self::NtscBreadbin(m) => m.cia1_port_b_input(),
            Self::PalC64c(m) => m.cia1_port_b_input(),
            Self::NtscC64c(m) => m.cia1_port_b_input(),
        }
    }
    pub fn vic_register(&self, index: u8) -> u8 {
        match self {
            Self::PalBreadbin(m) => m.vic_register(index),
            Self::NtscBreadbin(m) => m.vic_register(index),
            Self::PalC64c(m) => m.vic_register(index),
            Self::NtscC64c(m) => m.vic_register(index),
        }
    }
    pub fn sid(&self) -> &Sid6581 {
        match self {
            Self::PalBreadbin(m) => m.sid(),
            Self::NtscBreadbin(m) => m.sid(),
            Self::PalC64c(m) => m.sid(),
            Self::NtscC64c(m) => m.sid(),
        }
    }
    pub fn tape_is_loaded(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.tape_is_loaded(),
            Self::NtscBreadbin(m) => m.tape_is_loaded(),
            Self::PalC64c(m) => m.tape_is_loaded(),
            Self::NtscC64c(m) => m.tape_is_loaded(),
        }
    }
    pub fn tape_sense_active(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.tape_sense_active(),
            Self::NtscBreadbin(m) => m.tape_sense_active(),
            Self::PalC64c(m) => m.tape_sense_active(),
            Self::NtscC64c(m) => m.tape_sense_active(),
        }
    }
    pub fn tape_motor_on(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.tape_motor_on(),
            Self::NtscBreadbin(m) => m.tape_motor_on(),
            Self::PalC64c(m) => m.tape_motor_on(),
            Self::NtscC64c(m) => m.tape_motor_on(),
        }
    }
    pub fn tape_pulse_index(&self) -> usize {
        match self {
            Self::PalBreadbin(m) => m.tape_pulse_index(),
            Self::NtscBreadbin(m) => m.tape_pulse_index(),
            Self::PalC64c(m) => m.tape_pulse_index(),
            Self::NtscC64c(m) => m.tape_pulse_index(),
        }
    }
    pub fn tape_pulse_count(&self) -> usize {
        match self {
            Self::PalBreadbin(m) => m.tape_pulse_count(),
            Self::NtscBreadbin(m) => m.tape_pulse_count(),
            Self::PalC64c(m) => m.tape_pulse_count(),
            Self::NtscC64c(m) => m.tape_pulse_count(),
        }
    }
    pub fn tape_is_playing(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.tape_is_playing(),
            Self::NtscBreadbin(m) => m.tape_is_playing(),
            Self::PalC64c(m) => m.tape_is_playing(),
            Self::NtscC64c(m) => m.tape_is_playing(),
        }
    }
    pub fn audio_sample_rate(&self) -> u32 {
        match self {
            Self::PalBreadbin(m) => m.audio_sample_rate(),
            Self::NtscBreadbin(m) => m.audio_sample_rate(),
            Self::PalC64c(m) => m.audio_sample_rate(),
            Self::NtscC64c(m) => m.audio_sample_rate(),
        }
    }
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        match self {
            Self::PalBreadbin(m) => m.take_audio_buffer(),
            Self::NtscBreadbin(m) => m.take_audio_buffer(),
            Self::PalC64c(m) => m.take_audio_buffer(),
            Self::NtscC64c(m) => m.take_audio_buffer(),
        }
    }
    pub fn take_audio_channel_buffers(&mut self) -> [Vec<f32>; 3] {
        match self {
            Self::PalBreadbin(m) => m.take_audio_channel_buffers(),
            Self::NtscBreadbin(m) => m.take_audio_channel_buffers(),
            Self::PalC64c(m) => m.take_audio_channel_buffers(),
            Self::NtscC64c(m) => m.take_audio_channel_buffers(),
        }
    }
    pub fn audio_controls(&self) -> AudioControls {
        match self {
            Self::PalBreadbin(m) => m.audio_controls(),
            Self::NtscBreadbin(m) => m.audio_controls(),
            Self::PalC64c(m) => m.audio_controls(),
            Self::NtscC64c(m) => m.audio_controls(),
        }
    }
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        match self {
            Self::PalBreadbin(m) => m.set_audio_controls(controls),
            Self::NtscBreadbin(m) => m.set_audio_controls(controls),
            Self::PalC64c(m) => m.set_audio_controls(controls),
            Self::NtscC64c(m) => m.set_audio_controls(controls),
        }
    }
    pub fn set_audio_channel_enabled(&mut self, channel: SidChannel, enabled: bool) {
        match self {
            Self::PalBreadbin(m) => m.set_audio_channel_enabled(channel, enabled),
            Self::NtscBreadbin(m) => m.set_audio_channel_enabled(channel, enabled),
            Self::PalC64c(m) => m.set_audio_channel_enabled(channel, enabled),
            Self::NtscC64c(m) => m.set_audio_channel_enabled(channel, enabled),
        }
    }
    pub fn set_audio_channel_gain(&mut self, channel: SidChannel, gain: f32) {
        match self {
            Self::PalBreadbin(m) => m.set_audio_channel_gain(channel, gain),
            Self::NtscBreadbin(m) => m.set_audio_channel_gain(channel, gain),
            Self::PalC64c(m) => m.set_audio_channel_gain(channel, gain),
            Self::NtscC64c(m) => m.set_audio_channel_gain(channel, gain),
        }
    }
    pub fn framebuffer(&self) -> &[u32] {
        match self {
            Self::PalBreadbin(m) => m.framebuffer(),
            Self::NtscBreadbin(m) => m.framebuffer(),
            Self::PalC64c(m) => m.framebuffer(),
            Self::NtscC64c(m) => m.framebuffer(),
        }
    }
    pub fn load_tap_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Self::PalBreadbin(m) => m.load_tap_bytes(bytes),
            Self::NtscBreadbin(m) => m.load_tap_bytes(bytes),
            Self::PalC64c(m) => m.load_tap_bytes(bytes),
            Self::NtscC64c(m) => m.load_tap_bytes(bytes),
        }
    }
    pub fn set_tape_play_phase_cycles(&mut self, cycles: u32) {
        match self {
            Self::PalBreadbin(m) => m.set_tape_play_phase_cycles(cycles),
            Self::NtscBreadbin(m) => m.set_tape_play_phase_cycles(cycles),
            Self::PalC64c(m) => m.set_tape_play_phase_cycles(cycles),
            Self::NtscC64c(m) => m.set_tape_play_phase_cycles(cycles),
        }
    }
    pub fn insert_blank_writable_tape(&mut self) {
        match self {
            Self::PalBreadbin(m) => m.insert_blank_writable_tape(),
            Self::NtscBreadbin(m) => m.insert_blank_writable_tape(),
            Self::PalC64c(m) => m.insert_blank_writable_tape(),
            Self::NtscC64c(m) => m.insert_blank_writable_tape(),
        }
    }
    pub fn flush_tape_image(&self) -> Option<Vec<u8>> {
        match self {
            Self::PalBreadbin(m) => m.flush_tape_image(),
            Self::NtscBreadbin(m) => m.flush_tape_image(),
            Self::PalC64c(m) => m.flush_tape_image(),
            Self::NtscC64c(m) => m.flush_tape_image(),
        }
    }
    pub fn insert_crt_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Self::PalBreadbin(m) => m.insert_crt_bytes(bytes),
            Self::NtscBreadbin(m) => m.insert_crt_bytes(bytes),
            Self::PalC64c(m) => m.insert_crt_bytes(bytes),
            Self::NtscC64c(m) => m.insert_crt_bytes(bytes),
        }
    }
    pub fn remove_cartridge(&mut self) {
        match self {
            Self::PalBreadbin(m) => m.remove_cartridge(),
            Self::NtscBreadbin(m) => m.remove_cartridge(),
            Self::PalC64c(m) => m.remove_cartridge(),
            Self::NtscC64c(m) => m.remove_cartridge(),
        }
    }
    pub fn attach_georam(&mut self, size_kb: usize) {
        match self {
            Self::PalBreadbin(m) => m.attach_georam(size_kb),
            Self::NtscBreadbin(m) => m.attach_georam(size_kb),
            Self::PalC64c(m) => m.attach_georam(size_kb),
            Self::NtscC64c(m) => m.attach_georam(size_kb),
        }
    }
    pub fn detach_georam(&mut self) {
        match self {
            Self::PalBreadbin(m) => m.detach_georam(),
            Self::NtscBreadbin(m) => m.detach_georam(),
            Self::PalC64c(m) => m.detach_georam(),
            Self::NtscC64c(m) => m.detach_georam(),
        }
    }
    pub fn has_georam(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.has_georam(),
            Self::NtscBreadbin(m) => m.has_georam(),
            Self::PalC64c(m) => m.has_georam(),
            Self::NtscC64c(m) => m.has_georam(),
        }
    }
    pub fn attach_reu(&mut self, size_kb: usize) {
        match self {
            Self::PalBreadbin(m) => m.attach_reu(size_kb),
            Self::NtscBreadbin(m) => m.attach_reu(size_kb),
            Self::PalC64c(m) => m.attach_reu(size_kb),
            Self::NtscC64c(m) => m.attach_reu(size_kb),
        }
    }
    pub fn detach_reu(&mut self) {
        match self {
            Self::PalBreadbin(m) => m.detach_reu(),
            Self::NtscBreadbin(m) => m.detach_reu(),
            Self::PalC64c(m) => m.detach_reu(),
            Self::NtscC64c(m) => m.detach_reu(),
        }
    }
    pub fn has_reu(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.has_reu(),
            Self::NtscBreadbin(m) => m.has_reu(),
            Self::PalC64c(m) => m.has_reu(),
            Self::NtscC64c(m) => m.has_reu(),
        }
    }
    pub fn play_tape(&mut self) {
        match self {
            Self::PalBreadbin(m) => m.play_tape(),
            Self::NtscBreadbin(m) => m.play_tape(),
            Self::PalC64c(m) => m.play_tape(),
            Self::NtscC64c(m) => m.play_tape(),
        }
    }
    pub fn stop_tape(&mut self) {
        match self {
            Self::PalBreadbin(m) => m.stop_tape(),
            Self::NtscBreadbin(m) => m.stop_tape(),
            Self::PalC64c(m) => m.stop_tape(),
            Self::NtscC64c(m) => m.stop_tape(),
        }
    }
    pub fn tick(&mut self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.tick(),
            Self::NtscBreadbin(m) => m.tick(),
            Self::PalC64c(m) => m.tick(),
            Self::NtscC64c(m) => m.tick(),
        }
    }
    pub fn tick_with_iec_bus(&mut self, bus: &mut IecBus) -> bool {
        match self {
            Self::PalBreadbin(m) => m.tick_with_iec_bus(bus),
            Self::NtscBreadbin(m) => m.tick_with_iec_bus(bus),
            Self::PalC64c(m) => m.tick_with_iec_bus(bus),
            Self::NtscC64c(m) => m.tick_with_iec_bus(bus),
        }
    }
    pub fn advance_phi2_cycles(&mut self, cycles: u64) {
        match self {
            Self::PalBreadbin(m) => m.advance_phi2_cycles(cycles),
            Self::NtscBreadbin(m) => m.advance_phi2_cycles(cycles),
            Self::PalC64c(m) => m.advance_phi2_cycles(cycles),
            Self::NtscC64c(m) => m.advance_phi2_cycles(cycles),
        }
    }
    pub fn run_frame(&mut self) -> u32 {
        match self {
            Self::PalBreadbin(m) => m.run_frame(),
            Self::NtscBreadbin(m) => m.run_frame(),
            Self::PalC64c(m) => m.run_frame(),
            Self::NtscC64c(m) => m.run_frame(),
        }
    }
    pub fn attach_ultimate_uci(&mut self) {
        match self {
            Self::PalBreadbin(m) => m.attach_ultimate_uci(),
            Self::NtscBreadbin(m) => m.attach_ultimate_uci(),
            Self::PalC64c(m) => m.attach_ultimate_uci(),
            Self::NtscC64c(m) => m.attach_ultimate_uci(),
        }
    }
    pub fn detach_ultimate_uci(&mut self) {
        match self {
            Self::PalBreadbin(m) => m.detach_ultimate_uci(),
            Self::NtscBreadbin(m) => m.detach_ultimate_uci(),
            Self::PalC64c(m) => m.detach_ultimate_uci(),
            Self::NtscC64c(m) => m.detach_ultimate_uci(),
        }
    }
    pub fn ultimate_uci(&self) -> Option<&common_commodore_c64::board::UltimateUciNet> {
        match self {
            Self::PalBreadbin(m) => m.ultimate_uci(),
            Self::NtscBreadbin(m) => m.ultimate_uci(),
            Self::PalC64c(m) => m.ultimate_uci(),
            Self::NtscC64c(m) => m.ultimate_uci(),
        }
    }
    pub fn poll_ultimate_uci(&mut self) {
        match self {
            Self::PalBreadbin(m) => m.poll_ultimate_uci(),
            Self::NtscBreadbin(m) => m.poll_ultimate_uci(),
            Self::PalC64c(m) => m.poll_ultimate_uci(),
            Self::NtscC64c(m) => m.poll_ultimate_uci(),
        }
    }
    pub fn user_port_pa2(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.user_port_pa2(),
            Self::NtscBreadbin(m) => m.user_port_pa2(),
            Self::PalC64c(m) => m.user_port_pa2(),
            Self::NtscC64c(m) => m.user_port_pa2(),
        }
    }
    pub fn user_port_pb0(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.user_port_pb0(),
            Self::NtscBreadbin(m) => m.user_port_pb0(),
            Self::PalC64c(m) => m.user_port_pb0(),
            Self::NtscC64c(m) => m.user_port_pb0(),
        }
    }
    pub fn set_user_port_pb0(&mut self, high: bool) {
        match self {
            Self::PalBreadbin(m) => m.set_user_port_pb0(high),
            Self::NtscBreadbin(m) => m.set_user_port_pb0(high),
            Self::PalC64c(m) => m.set_user_port_pb0(high),
            Self::NtscC64c(m) => m.set_user_port_pb0(high),
        }
    }
    pub fn load_prg(&mut self, data: &[u8]) -> Result<u16, String> {
        match self {
            Self::PalBreadbin(m) => m.load_prg(data),
            Self::NtscBreadbin(m) => m.load_prg(data),
            Self::PalC64c(m) => m.load_prg(data),
            Self::NtscC64c(m) => m.load_prg(data),
        }
    }
    pub fn snapshot_state(&self) -> C64Snapshot {
        match self {
            Self::PalBreadbin(m) => m.snapshot_state(),
            Self::NtscBreadbin(m) => m.snapshot_state(),
            Self::PalC64c(m) => m.snapshot_state(),
            Self::NtscC64c(m) => m.snapshot_state(),
        }
    }
    pub fn restore_snapshot_state(&mut self, snapshot: C64Snapshot) -> Result<(), String> {
        match self {
            Self::PalBreadbin(m) => m.restore_snapshot_state(snapshot),
            Self::NtscBreadbin(m) => m.restore_snapshot_state(snapshot),
            Self::PalC64c(m) => m.restore_snapshot_state(snapshot),
            Self::NtscC64c(m) => m.restore_snapshot_state(snapshot),
        }
    }
    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        match self {
            Self::PalBreadbin(m) => m.cpu_read(addr),
            Self::NtscBreadbin(m) => m.cpu_read(addr),
            Self::PalC64c(m) => m.cpu_read(addr),
            Self::NtscC64c(m) => m.cpu_read(addr),
        }
    }
    pub fn cpu_read_with_iec_bus(&mut self, addr: u16, bus: &mut IecBus) -> u8 {
        match self {
            Self::PalBreadbin(m) => m.cpu_read_with_iec_bus(addr, bus),
            Self::NtscBreadbin(m) => m.cpu_read_with_iec_bus(addr, bus),
            Self::PalC64c(m) => m.cpu_read_with_iec_bus(addr, bus),
            Self::NtscC64c(m) => m.cpu_read_with_iec_bus(addr, bus),
        }
    }
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        match self {
            Self::PalBreadbin(m) => m.cpu_write(addr, value),
            Self::NtscBreadbin(m) => m.cpu_write(addr, value),
            Self::PalC64c(m) => m.cpu_write(addr, value),
            Self::NtscC64c(m) => m.cpu_write(addr, value),
        }
    }
    pub fn cpu_write_with_iec_bus(&mut self, addr: u16, value: u8, bus: &mut IecBus) {
        match self {
            Self::PalBreadbin(m) => m.cpu_write_with_iec_bus(addr, value, bus),
            Self::NtscBreadbin(m) => m.cpu_write_with_iec_bus(addr, value, bus),
            Self::PalC64c(m) => m.cpu_write_with_iec_bus(addr, value, bus),
            Self::NtscC64c(m) => m.cpu_write_with_iec_bus(addr, value, bus),
        }
    }
    pub fn vic_read(&self, offset: u16) -> u8 {
        match self {
            Self::PalBreadbin(m) => m.vic_read(offset),
            Self::NtscBreadbin(m) => m.vic_read(offset),
            Self::PalC64c(m) => m.vic_read(offset),
            Self::NtscC64c(m) => m.vic_read(offset),
        }
    }
    pub fn sync_iec_bus(&mut self, bus: &mut IecBus) {
        match self {
            Self::PalBreadbin(m) => m.sync_iec_bus(bus),
            Self::NtscBreadbin(m) => m.sync_iec_bus(bus),
            Self::PalC64c(m) => m.sync_iec_bus(bus),
            Self::NtscC64c(m) => m.sync_iec_bus(bus),
        }
    }
    pub fn user_port_pb(&self) -> u8 {
        match self {
            Self::PalBreadbin(m) => m.user_port_pb(),
            Self::NtscBreadbin(m) => m.user_port_pb(),
            Self::PalC64c(m) => m.user_port_pb(),
            Self::NtscC64c(m) => m.user_port_pb(),
        }
    }
    pub fn set_user_port_pb(&mut self, levels: u8) {
        match self {
            Self::PalBreadbin(m) => m.set_user_port_pb(levels),
            Self::NtscBreadbin(m) => m.set_user_port_pb(levels),
            Self::PalC64c(m) => m.set_user_port_pb(levels),
            Self::NtscC64c(m) => m.set_user_port_pb(levels),
        }
    }
    pub fn set_user_port_pa2(&mut self, high: bool) {
        match self {
            Self::PalBreadbin(m) => m.set_user_port_pa2(high),
            Self::NtscBreadbin(m) => m.set_user_port_pa2(high),
            Self::PalC64c(m) => m.set_user_port_pa2(high),
            Self::NtscC64c(m) => m.set_user_port_pa2(high),
        }
    }
    pub fn set_user_port_flag2(&mut self, high: bool) {
        match self {
            Self::PalBreadbin(m) => m.set_user_port_flag2(high),
            Self::NtscBreadbin(m) => m.set_user_port_flag2(high),
            Self::PalC64c(m) => m.set_user_port_flag2(high),
            Self::NtscC64c(m) => m.set_user_port_flag2(high),
        }
    }
    pub fn user_port_pc2(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.user_port_pc2(),
            Self::NtscBreadbin(m) => m.user_port_pc2(),
            Self::PalC64c(m) => m.user_port_pc2(),
            Self::NtscC64c(m) => m.user_port_pc2(),
        }
    }
    pub fn user_port_cnt1(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.user_port_cnt1(),
            Self::NtscBreadbin(m) => m.user_port_cnt1(),
            Self::PalC64c(m) => m.user_port_cnt1(),
            Self::NtscC64c(m) => m.user_port_cnt1(),
        }
    }
    pub fn set_user_port_cnt1(&mut self, high: bool) {
        match self {
            Self::PalBreadbin(m) => m.set_user_port_cnt1(high),
            Self::NtscBreadbin(m) => m.set_user_port_cnt1(high),
            Self::PalC64c(m) => m.set_user_port_cnt1(high),
            Self::NtscC64c(m) => m.set_user_port_cnt1(high),
        }
    }
    pub fn user_port_sp1(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.user_port_sp1(),
            Self::NtscBreadbin(m) => m.user_port_sp1(),
            Self::PalC64c(m) => m.user_port_sp1(),
            Self::NtscC64c(m) => m.user_port_sp1(),
        }
    }
    pub fn set_user_port_sp1(&mut self, high: bool) {
        match self {
            Self::PalBreadbin(m) => m.set_user_port_sp1(high),
            Self::NtscBreadbin(m) => m.set_user_port_sp1(high),
            Self::PalC64c(m) => m.set_user_port_sp1(high),
            Self::NtscC64c(m) => m.set_user_port_sp1(high),
        }
    }
    pub fn user_port_cnt2(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.user_port_cnt2(),
            Self::NtscBreadbin(m) => m.user_port_cnt2(),
            Self::PalC64c(m) => m.user_port_cnt2(),
            Self::NtscC64c(m) => m.user_port_cnt2(),
        }
    }
    pub fn set_user_port_cnt2(&mut self, high: bool) {
        match self {
            Self::PalBreadbin(m) => m.set_user_port_cnt2(high),
            Self::NtscBreadbin(m) => m.set_user_port_cnt2(high),
            Self::PalC64c(m) => m.set_user_port_cnt2(high),
            Self::NtscC64c(m) => m.set_user_port_cnt2(high),
        }
    }
    pub fn user_port_sp2(&self) -> bool {
        match self {
            Self::PalBreadbin(m) => m.user_port_sp2(),
            Self::NtscBreadbin(m) => m.user_port_sp2(),
            Self::PalC64c(m) => m.user_port_sp2(),
            Self::NtscC64c(m) => m.user_port_sp2(),
        }
    }
    pub fn set_user_port_sp2(&mut self, high: bool) {
        match self {
            Self::PalBreadbin(m) => m.set_user_port_sp2(high),
            Self::NtscBreadbin(m) => m.set_user_port_sp2(high),
            Self::PalC64c(m) => m.set_user_port_sp2(high),
            Self::NtscC64c(m) => m.set_user_port_sp2(high),
        }
    }
    pub fn fit_supercpu(&mut self, rom: Vec<u8>, superram: usize) -> Result<(), String> {
        match self {
            Self::PalBreadbin(m) => m.fit_supercpu(rom, superram),
            Self::NtscBreadbin(m) => m.fit_supercpu(rom, superram),
            Self::PalC64c(m) => m.fit_supercpu(rom, superram),
            Self::NtscC64c(m) => m.fit_supercpu(rom, superram),
        }
    }
    pub fn remove_supercpu(&mut self) {
        match self {
            Self::PalBreadbin(m) => m.remove_supercpu(),
            Self::NtscBreadbin(m) => m.remove_supercpu(),
            Self::PalC64c(m) => m.remove_supercpu(),
            Self::NtscC64c(m) => m.remove_supercpu(),
        }
    }
    pub fn supercpu(&self) -> Option<&SuperCpu> {
        match self {
            Self::PalBreadbin(m) => m.supercpu(),
            Self::NtscBreadbin(m) => m.supercpu(),
            Self::PalC64c(m) => m.supercpu(),
            Self::NtscC64c(m) => m.supercpu(),
        }
    }
    pub fn supercpu_mut(&mut self) -> Option<&mut SuperCpu> {
        match self {
            Self::PalBreadbin(m) => m.supercpu_mut(),
            Self::NtscBreadbin(m) => m.supercpu_mut(),
            Self::PalC64c(m) => m.supercpu_mut(),
            Self::NtscC64c(m) => m.supercpu_mut(),
        }
    }
}
#[derive(Clone)]
pub enum C128 {
    Pal(Box<machine_commodore_c128_pal::C128>),
    Ntsc(Box<machine_commodore_c128_ntsc::C128>),
}
impl C128 {
    pub const VDC_WIDTH: usize = common_commodore_c128::C128::VDC_WIDTH;
    pub const VDC_HEIGHT: usize = common_commodore_c128::C128::VDC_HEIGHT;
    pub fn with_model(roms: &C128Roms<'_>, model: C128Model) -> Result<Self, RomError> {
        Ok(match model {
            C128Model::Pal => Self::Pal(Box::new(machine_commodore_c128_pal::C128::new(roms)?)),
            C128Model::Ntsc => Self::Ntsc(Box::new(machine_commodore_c128_ntsc::C128::new(roms)?)),
        })
    }
    pub fn snapshot_state(&self) -> common_commodore_c128::C128 {
        match self {
            Self::Pal(m) => m.snapshot_state(),
            Self::Ntsc(m) => m.snapshot_state(),
        }
    }
    pub fn restore_snapshot_state(
        &mut self,
        state: common_commodore_c128::C128,
    ) -> Result<(), String> {
        if state.model() != self.model() {
            return Err("C128 snapshot model does not match selected board".into());
        }
        *self = match self.model() {
            C128Model::Pal => Self::Pal(Box::new(machine_commodore_c128_pal::C128::from_state(
                state,
            )?)),
            C128Model::Ntsc => Self::Ntsc(Box::new(machine_commodore_c128_ntsc::C128::from_state(
                state,
            )?)),
        };
        Ok(())
    }
    pub fn model(&self) -> C128Model {
        match self {
            Self::Pal(m) => m.model(),
            Self::Ntsc(m) => m.model(),
        }
    }
    pub fn timing(&self) -> C64Timing {
        match self {
            Self::Pal(m) => m.timing(),
            Self::Ntsc(m) => m.timing(),
        }
    }
    pub fn cpu(&self) -> &M6502 {
        match self {
            Self::Pal(m) => m.cpu(),
            Self::Ntsc(m) => m.cpu(),
        }
    }
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        match self {
            Self::Pal(m) => m.cpu_mut(),
            Self::Ntsc(m) => m.cpu_mut(),
        }
    }
    pub fn vic(&self) -> &Vic {
        match self {
            Self::Pal(m) => m.vic(),
            Self::Ntsc(m) => m.vic(),
        }
    }
    pub fn cia1(&self) -> &Cia6526 {
        match self {
            Self::Pal(m) => m.cia1(),
            Self::Ntsc(m) => m.cia1(),
        }
    }
    pub fn cia2(&self) -> &Cia6526 {
        match self {
            Self::Pal(m) => m.cia2(),
            Self::Ntsc(m) => m.cia2(),
        }
    }
    pub fn vic_register(&self, index: u8) -> u8 {
        match self {
            Self::Pal(m) => m.vic_register(index),
            Self::Ntsc(m) => m.vic_register(index),
        }
    }
    pub fn raster_line(&self) -> u16 {
        match self {
            Self::Pal(m) => m.raster_line(),
            Self::Ntsc(m) => m.raster_line(),
        }
    }
    pub fn cycle_in_line(&self) -> u8 {
        match self {
            Self::Pal(m) => m.cycle_in_line(),
            Self::Ntsc(m) => m.cycle_in_line(),
        }
    }
    pub fn audio_sample_rate(&self) -> u32 {
        match self {
            Self::Pal(m) => m.audio_sample_rate(),
            Self::Ntsc(m) => m.audio_sample_rate(),
        }
    }
    pub fn step_instruction(&mut self) -> u64 {
        match self {
            Self::Pal(m) => m.step_instruction(),
            Self::Ntsc(m) => m.step_instruction(),
        }
    }
    pub fn z80(&self) -> &Z80 {
        match self {
            Self::Pal(m) => m.z80(),
            Self::Ntsc(m) => m.z80(),
        }
    }
    pub fn running_8502(&self) -> bool {
        match self {
            Self::Pal(m) => m.running_8502(),
            Self::Ntsc(m) => m.running_8502(),
        }
    }
    pub fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        match self {
            Self::Pal(m) => m.keyboard_mut(),
            Self::Ntsc(m) => m.keyboard_mut(),
        }
    }
    pub fn set_80_column_key(&mut self, down: bool) {
        match self {
            Self::Pal(m) => m.set_80_column_key(down),
            Self::Ntsc(m) => m.set_80_column_key(down),
        }
    }
    pub fn framebuffer(&self) -> &[u32] {
        match self {
            Self::Pal(m) => m.framebuffer(),
            Self::Ntsc(m) => m.framebuffer(),
        }
    }
    pub fn vdc_framebuffer(&self) -> &[u32] {
        match self {
            Self::Pal(m) => m.vdc_framebuffer(),
            Self::Ntsc(m) => m.vdc_framebuffer(),
        }
    }
    pub fn vdc_ram(&self) -> &[u8] {
        match self {
            Self::Pal(m) => m.vdc_ram(),
            Self::Ntsc(m) => m.vdc_ram(),
        }
    }
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        match self {
            Self::Pal(m) => m.take_audio_buffer(),
            Self::Ntsc(m) => m.take_audio_buffer(),
        }
    }
    pub fn phi2_cycles(&self) -> u64 {
        match self {
            Self::Pal(m) => m.phi2_cycles(),
            Self::Ntsc(m) => m.phi2_cycles(),
        }
    }
    pub fn frame_count(&self) -> u64 {
        match self {
            Self::Pal(m) => m.frame_count(),
            Self::Ntsc(m) => m.frame_count(),
        }
    }
    pub fn ram(&self, address: u32) -> u8 {
        match self {
            Self::Pal(m) => m.ram(address),
            Self::Ntsc(m) => m.ram(address),
        }
    }
    pub fn peek(&self, addr: u16) -> u8 {
        match self {
            Self::Pal(m) => m.peek(addr),
            Self::Ntsc(m) => m.peek(addr),
        }
    }
    pub fn poke(&mut self, addr: u16, value: u8) {
        match self {
            Self::Pal(m) => m.poke(addr, value),
            Self::Ntsc(m) => m.poke(addr, value),
        }
    }
    pub fn load_prg(&mut self, data: &[u8]) -> Result<u16, String> {
        match self {
            Self::Pal(m) => m.load_prg(data),
            Self::Ntsc(m) => m.load_prg(data),
        }
    }
    pub fn screen_text(&self) -> Vec<String> {
        match self {
            Self::Pal(m) => m.screen_text(),
            Self::Ntsc(m) => m.screen_text(),
        }
    }
    pub fn run_frame(&mut self) -> u64 {
        match self {
            Self::Pal(m) => m.run_frame(),
            Self::Ntsc(m) => m.run_frame(),
        }
    }
    pub fn tick(&mut self) -> bool {
        match self {
            Self::Pal(m) => m.tick(),
            Self::Ntsc(m) => m.tick(),
        }
    }
}

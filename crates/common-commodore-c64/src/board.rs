//! Fixed configuration binding over the original family engine.
use super::*;
use crate::machine::SuperCpu;
use common_commodore_iec::IecBus;
use emu198x_mos_6502::M6502;
use emu198x_mos_cia_6526::Cia6526;
pub use emu198x_mos_sid_6581::Sid6581;
use emu198x_mos_vic_ii::{Vic, VicModel};
use std::marker::PhantomData;
/// Configuration fixed by a machine owner.
pub trait Board: Clone + 'static {
    const MODEL: C64Model;
    const MACHINE_ID: &'static str;
}
/// State bound to one board; the original engine is never replaceable by callers.
#[derive(Clone)]
pub struct C64Core<B: Board> {
    inner: C64,
    board: PhantomData<B>,
}
impl<B: Board> C64Core<B> {
    /// Construct with the fixed chip/clock selection.
    pub fn new(
        kernal_rom: &[u8],
        basic_rom: &[u8],
        character_rom: &[u8],
    ) -> Result<Self, MemoryInitError> {
        Ok(Self {
            inner: C64::new(C64Config {
                model: B::MODEL,
                kernal_rom,
                basic_rom,
                character_rom,
            })?,
            board: PhantomData,
        })
    }
    /// Restore atomically, rejecting a foreign model or chip population.
    pub fn restore_snapshot_state(&mut self, state: C64Snapshot) -> Result<(), String> {
        let mut candidate = self.inner.clone();
        candidate.restore_snapshot_state(state)?;
        validate(&candidate, B::MODEL)?;
        self.inner = candidate;
        Ok(())
    }
    /// Forward the original `C64::model` operation.
    pub fn model(&self) -> C64Model {
        self.inner.model()
    }
    /// Forward the original `C64::cpu` operation.
    pub fn cpu(&self) -> &M6502 {
        self.inner.cpu()
    }
    /// Forward the original `C64::cpu_mut` operation.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        self.inner.cpu_mut()
    }
    /// Forward the original `C64::peek` operation.
    pub fn peek(&self, addr: u16) -> u8 {
        self.inner.peek(addr)
    }
    /// Forward the original `C64::poke` operation.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.inner.poke(addr, value)
    }
    /// Forward the original `C64::step_instruction` operation.
    pub fn step_instruction(&mut self) -> u64 {
        self.inner.step_instruction()
    }
    /// Forward the original `C64::vic` operation.
    pub fn vic(&self) -> &Vic {
        self.inner.vic()
    }
    /// Forward the original `C64::cia1` operation.
    pub fn cia1(&self) -> &Cia6526 {
        self.inner.cia1()
    }
    /// Forward the original `C64::cia2` operation.
    pub fn cia2(&self) -> &Cia6526 {
        self.inner.cia2()
    }
    /// Forward the original `C64::timing` operation.
    pub fn timing(&self) -> C64Timing {
        self.inner.timing()
    }
    /// Forward the original `C64::memory` operation.
    pub fn memory(&self) -> &C64Memory {
        self.inner.memory()
    }
    /// Forward the original `C64::keyboard_mut` operation.
    pub fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        self.inner.keyboard_mut()
    }
    /// Forward the original `C64::set_restore` operation.
    pub fn set_restore(&mut self, pressed: bool) {
        self.inner.set_restore(pressed)
    }
    /// Forward the original `C64::set_cart_freeze` operation.
    pub fn set_cart_freeze(&mut self, pressed: bool) -> bool {
        self.inner.set_cart_freeze(pressed)
    }
    /// Forward the original `C64::freeze_cart_registers` operation.
    pub fn freeze_cart_registers(&self) -> Option<(u8, u8)> {
        self.inner.freeze_cart_registers()
    }
    /// Forward the original `C64::set_joystick_control` operation.
    pub fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool) -> bool {
        self.inner.set_joystick_control(port, name, pressed)
    }
    /// Forward the original `C64::set_paddle` operation.
    pub fn set_paddle(&mut self, port: u8, axis: u8, value: u8) -> bool {
        self.inner.set_paddle(port, axis, value)
    }
    /// Forward the original `C64::attach_mouse_1351` operation.
    pub fn attach_mouse_1351(&mut self, port: u8) -> bool {
        self.inner.attach_mouse_1351(port)
    }
    /// Forward the original `C64::detach_mouse_1351` operation.
    pub fn detach_mouse_1351(&mut self, port: u8) -> bool {
        self.inner.detach_mouse_1351(port)
    }
    /// Forward the original `C64::has_mouse_1351` operation.
    pub fn has_mouse_1351(&self, port: u8) -> bool {
        self.inner.has_mouse_1351(port)
    }
    /// Forward the original `C64::move_mouse_1351` operation.
    pub fn move_mouse_1351(&mut self, port: u8, dx: i32, dy: i32) -> bool {
        self.inner.move_mouse_1351(port, dx, dy)
    }
    /// Forward the original `C64::set_mouse_1351_button` operation.
    pub fn set_mouse_1351_button(&mut self, port: u8, button: &str, pressed: bool) -> bool {
        self.inner.set_mouse_1351_button(port, button, pressed)
    }
    /// Forward the original `C64::phi2_cycles` operation.
    pub fn phi2_cycles(&self) -> u64 {
        self.inner.phi2_cycles()
    }
    /// Forward the original `C64::frame_count` operation.
    pub fn frame_count(&self) -> u64 {
        self.inner.frame_count()
    }
    /// Forward the original `C64::raster_line` operation.
    pub fn raster_line(&self) -> u16 {
        self.inner.raster_line()
    }
    /// Forward the original `C64::cycle_in_line` operation.
    pub fn cycle_in_line(&self) -> u8 {
        self.inner.cycle_in_line()
    }
    /// Forward the original `C64::vic_bank` operation.
    pub fn vic_bank(&self) -> u8 {
        self.inner.vic_bank()
    }
    /// Forward the original `C64::cia1_port_b_input` operation.
    pub fn cia1_port_b_input(&self) -> u8 {
        self.inner.cia1_port_b_input()
    }
    /// Forward the original `C64::vic_register` operation.
    pub fn vic_register(&self, index: u8) -> u8 {
        self.inner.vic_register(index)
    }
    /// Forward the original `C64::sid` operation.
    pub fn sid(&self) -> &Sid6581 {
        self.inner.sid()
    }
    /// Forward the original `C64::tape_is_loaded` operation.
    pub fn tape_is_loaded(&self) -> bool {
        self.inner.tape_is_loaded()
    }
    /// Forward the original `C64::tape_sense_active` operation.
    pub fn tape_sense_active(&self) -> bool {
        self.inner.tape_sense_active()
    }
    /// Forward the original `C64::tape_motor_on` operation.
    pub fn tape_motor_on(&self) -> bool {
        self.inner.tape_motor_on()
    }
    /// Forward the original `C64::tape_pulse_index` operation.
    pub fn tape_pulse_index(&self) -> usize {
        self.inner.tape_pulse_index()
    }
    /// Forward the original `C64::tape_pulse_count` operation.
    pub fn tape_pulse_count(&self) -> usize {
        self.inner.tape_pulse_count()
    }
    /// Forward the original `C64::tape_is_playing` operation.
    pub fn tape_is_playing(&self) -> bool {
        self.inner.tape_is_playing()
    }
    /// Forward the original `C64::audio_sample_rate` operation.
    pub fn audio_sample_rate(&self) -> u32 {
        self.inner.audio_sample_rate()
    }
    /// Forward the original `C64::take_audio_buffer` operation.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.inner.take_audio_buffer()
    }
    /// Forward the original `C64::take_audio_channel_buffers` operation.
    pub fn take_audio_channel_buffers(&mut self) -> [Vec<f32>; 3] {
        self.inner.take_audio_channel_buffers()
    }
    /// Forward the original `C64::audio_controls` operation.
    pub fn audio_controls(&self) -> AudioControls {
        self.inner.audio_controls()
    }
    /// Forward the original `C64::set_audio_controls` operation.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.inner.set_audio_controls(controls)
    }
    /// Forward the original `C64::set_audio_channel_enabled` operation.
    pub fn set_audio_channel_enabled(&mut self, channel: SidChannel, enabled: bool) {
        self.inner.set_audio_channel_enabled(channel, enabled)
    }
    /// Forward the original `C64::set_audio_channel_gain` operation.
    pub fn set_audio_channel_gain(&mut self, channel: SidChannel, gain: f32) {
        self.inner.set_audio_channel_gain(channel, gain)
    }
    /// Forward the original `C64::framebuffer` operation.
    pub fn framebuffer(&self) -> &[u32] {
        self.inner.framebuffer()
    }
    /// Forward the original `C64::load_tap_bytes` operation.
    pub fn load_tap_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.inner.load_tap_bytes(bytes)
    }
    /// Forward the original `C64::set_tape_play_phase_cycles` operation.
    pub fn set_tape_play_phase_cycles(&mut self, cycles: u32) {
        self.inner.set_tape_play_phase_cycles(cycles)
    }
    /// Forward the original `C64::insert_blank_writable_tape` operation.
    pub fn insert_blank_writable_tape(&mut self) {
        self.inner.insert_blank_writable_tape()
    }
    /// Forward the original `C64::flush_tape_image` operation.
    pub fn flush_tape_image(&self) -> Option<Vec<u8>> {
        self.inner.flush_tape_image()
    }
    /// Forward the original `C64::insert_crt_bytes` operation.
    pub fn insert_crt_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.inner.insert_crt_bytes(bytes)
    }
    /// Forward the original `C64::remove_cartridge` operation.
    pub fn remove_cartridge(&mut self) {
        self.inner.remove_cartridge()
    }
    /// Forward the original `C64::attach_georam` operation.
    pub fn attach_georam(&mut self, size_kb: usize) {
        self.inner.attach_georam(size_kb)
    }
    /// Forward the original `C64::detach_georam` operation.
    pub fn detach_georam(&mut self) {
        self.inner.detach_georam()
    }
    /// Forward the original `C64::has_georam` operation.
    pub fn has_georam(&self) -> bool {
        self.inner.has_georam()
    }
    /// Forward the original `C64::attach_reu` operation.
    pub fn attach_reu(&mut self, size_kb: usize) {
        self.inner.attach_reu(size_kb)
    }
    /// Forward the original `C64::detach_reu` operation.
    pub fn detach_reu(&mut self) {
        self.inner.detach_reu()
    }
    /// Forward the original `C64::has_reu` operation.
    pub fn has_reu(&self) -> bool {
        self.inner.has_reu()
    }
    /// Forward the original `C64::play_tape` operation.
    pub fn play_tape(&mut self) {
        self.inner.play_tape()
    }
    /// Forward the original `C64::stop_tape` operation.
    pub fn stop_tape(&mut self) {
        self.inner.stop_tape()
    }
    /// Forward the original `C64::tick` operation.
    pub fn tick(&mut self) -> bool {
        self.inner.tick()
    }
    /// Forward the original `C64::tick_with_iec_bus` operation.
    pub fn tick_with_iec_bus(&mut self, bus: &mut IecBus) -> bool {
        self.inner.tick_with_iec_bus(bus)
    }
    /// Forward the original `C64::advance_phi2_cycles` operation.
    pub fn advance_phi2_cycles(&mut self, cycles: u64) {
        self.inner.advance_phi2_cycles(cycles)
    }
    /// Forward the original `C64::run_frame` operation.
    pub fn run_frame(&mut self) -> u32 {
        self.inner.run_frame()
    }
    /// Forward the original `C64::attach_ultimate_uci` operation.
    pub fn attach_ultimate_uci(&mut self) {
        self.inner.attach_ultimate_uci()
    }
    /// Forward the original `C64::detach_ultimate_uci` operation.
    pub fn detach_ultimate_uci(&mut self) {
        self.inner.detach_ultimate_uci()
    }
    /// Forward the original `C64::ultimate_uci` operation.
    pub fn ultimate_uci(&self) -> Option<&emu198x_peripheral_ultimate_uci_net::UltimateUciNet> {
        self.inner.ultimate_uci()
    }
    /// Forward the original `C64::poll_ultimate_uci` operation.
    pub fn poll_ultimate_uci(&mut self) {
        self.inner.poll_ultimate_uci()
    }
    /// Forward the original `C64::user_port_pa2` operation.
    pub fn user_port_pa2(&self) -> bool {
        self.inner.user_port_pa2()
    }
    /// Forward the original `C64::user_port_pb0` operation.
    pub fn user_port_pb0(&self) -> bool {
        self.inner.user_port_pb0()
    }
    /// Forward the original `C64::set_user_port_pb0` operation.
    pub fn set_user_port_pb0(&mut self, high: bool) {
        self.inner.set_user_port_pb0(high)
    }
    /// Forward the original `C64::load_prg` operation.
    pub fn load_prg(&mut self, data: &[u8]) -> Result<u16, String> {
        self.inner.load_prg(data)
    }
    /// Forward the original `C64::snapshot_state` operation.
    pub fn snapshot_state(&self) -> C64Snapshot {
        self.inner.snapshot_state()
    }
    /// Forward the original `C64::cpu_read` operation.
    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        self.inner.cpu_read(addr)
    }
    /// Forward the original `C64::cpu_read_with_iec_bus` operation.
    pub fn cpu_read_with_iec_bus(&mut self, addr: u16, bus: &mut IecBus) -> u8 {
        self.inner.cpu_read_with_iec_bus(addr, bus)
    }
    /// Forward the original `C64::cpu_write` operation.
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        self.inner.cpu_write(addr, value)
    }
    /// Forward the original `C64::cpu_write_with_iec_bus` operation.
    pub fn cpu_write_with_iec_bus(&mut self, addr: u16, value: u8, bus: &mut IecBus) {
        self.inner.cpu_write_with_iec_bus(addr, value, bus)
    }
    /// Forward the original `C64::vic_read` operation.
    pub fn vic_read(&self, offset: u16) -> u8 {
        self.inner.vic_read(offset)
    }
    /// Forward the original `C64::sync_iec_bus` operation.
    pub fn sync_iec_bus(&mut self, bus: &mut IecBus) {
        self.inner.sync_iec_bus(bus)
    }
    /// Forward the original `C64::user_port_pb` operation.
    pub fn user_port_pb(&self) -> u8 {
        self.inner.user_port_pb()
    }
    /// Forward the original `C64::set_user_port_pb` operation.
    pub fn set_user_port_pb(&mut self, levels: u8) {
        self.inner.set_user_port_pb(levels)
    }
    /// Forward the original `C64::set_user_port_pa2` operation.
    pub fn set_user_port_pa2(&mut self, high: bool) {
        self.inner.set_user_port_pa2(high)
    }
    /// Forward the original `C64::set_user_port_flag2` operation.
    pub fn set_user_port_flag2(&mut self, high: bool) {
        self.inner.set_user_port_flag2(high)
    }
    /// Forward the original `C64::user_port_pc2` operation.
    pub fn user_port_pc2(&self) -> bool {
        self.inner.user_port_pc2()
    }
    /// Forward the original `C64::user_port_cnt1` operation.
    pub fn user_port_cnt1(&self) -> bool {
        self.inner.user_port_cnt1()
    }
    /// Forward the original `C64::set_user_port_cnt1` operation.
    pub fn set_user_port_cnt1(&mut self, high: bool) {
        self.inner.set_user_port_cnt1(high)
    }
    /// Forward the original `C64::user_port_sp1` operation.
    pub fn user_port_sp1(&self) -> bool {
        self.inner.user_port_sp1()
    }
    /// Forward the original `C64::set_user_port_sp1` operation.
    pub fn set_user_port_sp1(&mut self, high: bool) {
        self.inner.set_user_port_sp1(high)
    }
    /// Forward the original `C64::user_port_cnt2` operation.
    pub fn user_port_cnt2(&self) -> bool {
        self.inner.user_port_cnt2()
    }
    /// Forward the original `C64::set_user_port_cnt2` operation.
    pub fn set_user_port_cnt2(&mut self, high: bool) {
        self.inner.set_user_port_cnt2(high)
    }
    /// Forward the original `C64::user_port_sp2` operation.
    pub fn user_port_sp2(&self) -> bool {
        self.inner.user_port_sp2()
    }
    /// Forward the original `C64::set_user_port_sp2` operation.
    pub fn set_user_port_sp2(&mut self, high: bool) {
        self.inner.set_user_port_sp2(high)
    }
    /// Forward the original `C64::fit_supercpu` operation.
    pub fn fit_supercpu(&mut self, rom: Vec<u8>, superram: usize) -> Result<(), String> {
        self.inner.fit_supercpu(rom, superram)
    }
    /// Forward the original `C64::remove_supercpu` operation.
    pub fn remove_supercpu(&mut self) {
        self.inner.remove_supercpu()
    }
    /// Forward the original `C64::supercpu` operation.
    pub fn supercpu(&self) -> Option<&SuperCpu> {
        self.inner.supercpu()
    }
    /// Forward the original `C64::supercpu_mut` operation.
    pub fn supercpu_mut(&mut self) -> Option<&mut SuperCpu> {
        self.inner.supercpu_mut()
    }
}
fn validate(m: &C64, model: C64Model) -> Result<(), String> {
    let vic = match model {
        C64Model::PalBreadbin => VicModel::Pal6569,
        C64Model::NtscBreadbin => VicModel::Ntsc6567,
        C64Model::PalC64c => VicModel::Pal8565,
        C64Model::NtscC64c => VicModel::Ntsc8562,
    };
    if m.model() != model
        || m.vic().model() != vic
        || m.sid().model != model.sid_model()
        || m.cia1().model() != model.cia_model()
        || m.cia2().model() != model.cia_model()
    {
        return Err("C64 snapshot chip population does not match fixed owner".into());
    }
    Ok(())
}

pub use emu198x_peripheral_ultimate_uci_net::UltimateUciNet;

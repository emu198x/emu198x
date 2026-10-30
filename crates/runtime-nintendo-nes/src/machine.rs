//! Closed dispatch over the two fixed NES regional owners.
use common_nintendo_nes::*;
use common_nintendo_nes::{Apu, M6502, Ppu};
use nintendo_nes_cartridge::Mapper;
/// The concrete board held by the shared family runtime.
pub enum Nes {
    Ntsc(Box<machine_nintendo_nes_ntsc::Nes>),
    Pal(Box<machine_nintendo_nes_pal::Nes>),
}
macro_rules! dispatch {
    ($s:expr, $m:ident => $body:expr) => {
        match $s {
            Nes::Ntsc($m) => $body,
            Nes::Pal($m) => $body,
        }
    };
}
impl Nes {
    #[cfg(test)]
    pub(crate) fn new(mapper: Box<dyn Mapper>) -> Self {
        Self::new_with_region(mapper, Region::Ntsc)
    }
    pub(crate) fn new_with_region(mapper: Box<dyn Mapper>, region: Region) -> Self {
        match region {
            Region::Ntsc => Self::Ntsc(Box::new(machine_nintendo_nes_ntsc::Nes::new(mapper))),
            Region::Pal => Self::Pal(Box::new(machine_nintendo_nes_pal::Nes::new(mapper))),
        }
    }
    pub(crate) fn from_snapshot(
        snapshot: NesSnapshot,
        region: Region,
    ) -> Result<Self, &'static str> {
        match region {
            Region::Ntsc => machine_nintendo_nes_ntsc::Nes::from_snapshot(snapshot)
                .map(|m| Self::Ntsc(Box::new(m))),
            Region::Pal => machine_nintendo_nes_pal::Nes::from_snapshot(snapshot)
                .map(|m| Self::Pal(Box::new(m))),
        }
    }
    /// Delegate to the selected physical owner.
    pub fn soft_reset(&mut self) {
        dispatch!(self, m => m.soft_reset())
    }
    /// Delegate to the selected physical owner.
    pub fn tick(&mut self) {
        dispatch!(self, m => m.tick())
    }
    /// Delegate to the selected physical owner.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self, m => m.run_native_frame())
    }
    /// Delegate to the selected physical owner.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self, m => m.run_frame())
    }
    /// Delegate to the selected physical owner.
    pub fn start_dma_trace(&mut self) {
        dispatch!(self, m => m.start_dma_trace())
    }
    /// Delegate to the selected physical owner.
    pub fn dma_trace_episodes(&self) -> usize {
        dispatch!(self, m => m.dma_trace_episodes())
    }
    /// Delegate to the selected physical owner.
    pub fn take_reg_4015_trace(&mut self) -> Vec<u64> {
        dispatch!(self, m => m.take_reg_4015_trace())
    }
    /// Delegate to the selected physical owner.
    pub fn take_dma_trace(&mut self) -> Vec<(u64, u16, bool)> {
        dispatch!(self, m => m.take_dma_trace())
    }
    /// Delegate to the selected physical owner.
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self, m => m.framebuffer())
    }
    /// Delegate to the selected physical owner.
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self, m => m.framebuffer_width())
    }
    /// Delegate to the selected physical owner.
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self, m => m.framebuffer_height())
    }
    /// Delegate to the selected physical owner.
    pub fn framebuffer_tv_visible(&self) -> Vec<u32> {
        dispatch!(self, m => m.framebuffer_tv_visible())
    }
    /// Delegate to the selected physical owner.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        dispatch!(self, m => m.take_audio_buffer())
    }
    /// Delegate to the selected physical owner.
    pub fn audio_controls(&self) -> AudioControls {
        dispatch!(self, m => m.audio_controls())
    }
    /// Delegate to the selected physical owner.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        dispatch!(self, m => m.set_audio_controls(controls))
    }
    /// Delegate to the selected physical owner.
    pub fn set_audio_channel_enabled(&mut self, channel: ApuChannel, enabled: bool) {
        dispatch!(self, m => m.set_audio_channel_enabled(channel, enabled))
    }
    /// Delegate to the selected physical owner.
    pub fn set_audio_channel_gain(&mut self, channel: ApuChannel, gain: f32) {
        dispatch!(self, m => m.set_audio_channel_gain(channel, gain))
    }
    /// Delegate to the selected physical owner.
    pub fn effective_nametable(&self) -> [u8; 2048] {
        dispatch!(self, m => m.effective_nametable())
    }
    /// Delegate to the selected physical owner.
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self, m => m.peek(addr))
    }
    /// Delegate to the selected physical owner.
    pub fn cpu(&self) -> &M6502 {
        dispatch!(self, m => m.cpu())
    }
    /// Delegate to the selected physical owner.
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self, m => m.poke(addr, value))
    }
    /// Delegate to the selected physical owner.
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self, m => m.step_instruction())
    }
    /// Delegate to the selected physical owner.
    pub fn region(&self) -> Region {
        dispatch!(self, m => m.region())
    }
    /// Delegate to the selected physical owner.
    pub fn master_clock(&self) -> u64 {
        dispatch!(self, m => m.master_clock())
    }
    /// Delegate to the selected physical owner.
    pub fn cpu_cycle_count(&self) -> u64 {
        dispatch!(self, m => m.cpu_cycle_count())
    }
    /// Delegate to the selected physical owner.
    pub fn frame_count(&self) -> u64 {
        dispatch!(self, m => m.frame_count())
    }
    /// Delegate to the selected physical owner.
    pub fn set_controller1(&mut self, state: u8) {
        dispatch!(self, m => m.set_controller1(state))
    }
    /// Delegate to the selected physical owner.
    pub fn set_controller2(&mut self, state: u8) {
        dispatch!(self, m => m.set_controller2(state))
    }
    /// Delegate to the selected physical owner.
    pub fn set_controller3(&mut self, state: u8) {
        dispatch!(self, m => m.set_controller3(state))
    }
    /// Delegate to the selected physical owner.
    pub fn set_controller4(&mut self, state: u8) {
        dispatch!(self, m => m.set_controller4(state))
    }
    /// Delegate to the selected physical owner.
    pub fn controller3_state(&self) -> u8 {
        dispatch!(self, m => m.controller3_state())
    }
    /// Delegate to the selected physical owner.
    pub fn controller4_state(&self) -> u8 {
        dispatch!(self, m => m.controller4_state())
    }
    /// Delegate to the selected physical owner.
    pub fn input_devices(&self) -> InputDevices {
        dispatch!(self, m => m.input_devices())
    }
    /// Delegate to the selected physical owner.
    pub fn set_input_devices(&mut self, devices: InputDevices) {
        dispatch!(self, m => m.set_input_devices(devices))
    }
    /// Delegate to the selected physical owner.
    pub fn set_zapper_aim(&mut self, aim: Option<(u16, u16)>) {
        dispatch!(self, m => m.set_zapper_aim(aim))
    }
    /// Delegate to the selected physical owner.
    pub fn zapper_aim(&self) -> Option<(u16, u16)> {
        dispatch!(self, m => m.zapper_aim())
    }
    /// Delegate to the selected physical owner.
    pub fn set_zapper_trigger(&mut self, pressed: bool) {
        dispatch!(self, m => m.set_zapper_trigger(pressed))
    }
    /// Delegate to the selected physical owner.
    pub fn snapshot(&self) -> NesSnapshot {
        dispatch!(self, m => m.snapshot())
    }
    /// Access the selected physical owner.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        dispatch!(self, m => m.cpu_mut())
    }
    /// Access the selected physical owner.
    pub fn ppu(&self) -> &Ppu {
        dispatch!(self, m => m.ppu())
    }
    /// Access the selected physical owner.
    pub fn apu(&self) -> &Apu {
        dispatch!(self, m => m.apu())
    }
    /// Access the selected physical owner.
    pub fn mapper(&self) -> &dyn Mapper {
        dispatch!(self, m => m.mapper())
    }
    /// Access the selected physical owner.
    pub fn mapper_mut(&mut self) -> &mut dyn Mapper {
        dispatch!(self, m => m.mapper_mut())
    }
    /// Access the selected physical owner.
    pub fn controller1_state(&self) -> u8 {
        dispatch!(self, m => m.controller1_state())
    }
    /// Access the selected physical owner.
    pub fn controller2_state(&self) -> u8 {
        dispatch!(self, m => m.controller2_state())
    }
    /// Access the selected physical owner.
    pub fn write_oam(&mut self, index: u8, value: u8) {
        dispatch!(self, m => m.write_oam(index, value))
    }
    /// Access the selected physical owner.
    pub fn board_id(&self) -> &'static str {
        dispatch!(self, m => m.board_id())
    }
    /// Write the selected board's PPU register bus.
    pub fn write_ppu_register(&mut self, reg: u16, value: u8) {
        dispatch!(self, m => m.write_ppu_register(reg, value));
    }
}

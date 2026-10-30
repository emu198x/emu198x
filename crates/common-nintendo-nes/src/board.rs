//! A fixed physical board over shared NES wiring.
use super::*;
/// A board owner supplies immutable clock and chip selection.
pub trait NesBoard {
    const REGION: Region;
    const MACHINE_ID: &'static str;
}
/// One physical board. The raw engine cannot be replaced through this API.
pub struct NesCore<B: NesBoard> {
    state: Nes,
    board: std::marker::PhantomData<B>,
}
impl<B: NesBoard> NesCore<B> {
    /// Construct this board with a cartridge.
    pub fn new(mapper: Box<dyn Mapper>) -> Self {
        Self {
            state: Nes::new_with_region(mapper, B::REGION),
            board: std::marker::PhantomData,
        }
    }
    fn validate(snapshot: &NesSnapshot) -> Result<(), &'static str> {
        if snapshot.region != B::REGION
            || snapshot.apu.region() != B::REGION.apu_region()
            || snapshot.ppu.master_divider() != B::REGION.dot_units()
            || snapshot.ppu.pre_render_line() != B::REGION.pre_render_line()
            || snapshot.ppu.scanline() > B::REGION.pre_render_line()
            || snapshot.cpu_phase >= B::REGION.cpu_units()
            || snapshot.ppu.framebuffer().len() != (FB_WIDTH * FB_HEIGHT) as usize
            || snapshot.signal_codes.len() != (FB_WIDTH * FB_HEIGHT) as usize
            || snapshot.signal_codes.iter().any(|code| *code >= 512)
        {
            return Err("snapshot region or chip configuration does not match this NES board");
        }
        Ok(())
    }
    /// Reject foreign hardware before constructing this board.
    pub fn from_snapshot(snapshot: NesSnapshot) -> Result<Self, &'static str> {
        Self::validate(&snapshot)?;
        Ok(Self {
            state: Nes::from_snapshot(snapshot),
            board: std::marker::PhantomData,
        })
    }
    /// Reject foreign hardware before changing the live board.
    pub fn restore_snapshot(&mut self, snapshot: NesSnapshot) -> Result<(), &'static str> {
        Self::validate(&snapshot)?;
        self.state.restore_snapshot(snapshot);
        Ok(())
    }
    /// Delegate to this board’s shared wiring.
    pub fn soft_reset(&mut self) {
        self.state.soft_reset()
    }
    /// Delegate to this board’s shared wiring.
    pub fn tick(&mut self) {
        self.state.tick()
    }
    /// Delegate to this board’s shared wiring.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        self.state.run_native_frame()
    }
    /// Delegate to this board’s shared wiring.
    pub fn run_frame(&mut self) -> u64 {
        self.state.run_frame()
    }
    /// Delegate to this board’s shared wiring.
    pub fn start_dma_trace(&mut self) {
        self.state.start_dma_trace()
    }
    /// Delegate to this board’s shared wiring.
    pub fn dma_trace_episodes(&self) -> usize {
        self.state.dma_trace_episodes()
    }
    /// Delegate to this board’s shared wiring.
    pub fn take_reg_4015_trace(&mut self) -> Vec<u64> {
        self.state.take_reg_4015_trace()
    }
    /// Delegate to this board’s shared wiring.
    pub fn take_dma_trace(&mut self) -> Vec<(u64, u16, bool)> {
        self.state.take_dma_trace()
    }
    /// Delegate to this board’s shared wiring.
    pub fn framebuffer(&self) -> &[u32] {
        self.state.framebuffer()
    }
    /// Delegate to this board’s shared wiring.
    pub fn framebuffer_width(&self) -> u32 {
        self.state.framebuffer_width()
    }
    /// Delegate to this board’s shared wiring.
    pub fn framebuffer_height(&self) -> u32 {
        self.state.framebuffer_height()
    }
    /// Delegate to this board’s shared wiring.
    pub fn framebuffer_tv_visible(&self) -> Vec<u32> {
        self.state.framebuffer_tv_visible()
    }
    /// Delegate to this board’s shared wiring.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.state.take_audio_buffer()
    }
    /// Delegate to this board’s shared wiring.
    pub fn audio_controls(&self) -> AudioControls {
        self.state.audio_controls()
    }
    /// Delegate to this board’s shared wiring.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.state.set_audio_controls(controls)
    }
    /// Delegate to this board’s shared wiring.
    pub fn set_audio_channel_enabled(&mut self, channel: ApuChannel, enabled: bool) {
        self.state.set_audio_channel_enabled(channel, enabled)
    }
    /// Delegate to this board’s shared wiring.
    pub fn set_audio_channel_gain(&mut self, channel: ApuChannel, gain: f32) {
        self.state.set_audio_channel_gain(channel, gain)
    }
    /// Delegate to this board’s shared wiring.
    pub fn effective_nametable(&self) -> [u8; 2048] {
        self.state.effective_nametable()
    }
    /// Delegate to this board’s shared wiring.
    pub fn peek(&self, addr: u16) -> u8 {
        self.state.peek(addr)
    }
    /// Delegate to this board’s shared wiring.
    pub fn cpu(&self) -> &M6502 {
        self.state.cpu()
    }
    /// Delegate to this board’s shared wiring.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.state.poke(addr, value)
    }
    /// Delegate to this board’s shared wiring.
    pub fn step_instruction(&mut self) -> u64 {
        self.state.step_instruction()
    }
    /// Delegate to this board’s shared wiring.
    pub fn region(&self) -> Region {
        self.state.region()
    }
    /// Delegate to this board’s shared wiring.
    pub fn master_clock(&self) -> u64 {
        self.state.master_clock()
    }
    /// Delegate to this board’s shared wiring.
    pub fn cpu_cycle_count(&self) -> u64 {
        self.state.cpu_cycle_count()
    }
    /// Delegate to this board’s shared wiring.
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count()
    }
    /// Delegate to this board’s shared wiring.
    pub fn set_controller1(&mut self, state: u8) {
        self.state.set_controller1(state)
    }
    /// Delegate to this board’s shared wiring.
    pub fn set_controller2(&mut self, state: u8) {
        self.state.set_controller2(state)
    }
    /// Delegate to this board’s shared wiring.
    pub fn set_controller3(&mut self, state: u8) {
        self.state.set_controller3(state)
    }
    /// Delegate to this board’s shared wiring.
    pub fn set_controller4(&mut self, state: u8) {
        self.state.set_controller4(state)
    }
    /// Delegate to this board’s shared wiring.
    pub fn controller3_state(&self) -> u8 {
        self.state.controller3_state()
    }
    /// Delegate to this board’s shared wiring.
    pub fn controller4_state(&self) -> u8 {
        self.state.controller4_state()
    }
    /// Delegate to this board’s shared wiring.
    pub fn input_devices(&self) -> InputDevices {
        self.state.input_devices()
    }
    /// Delegate to this board’s shared wiring.
    pub fn set_input_devices(&mut self, devices: InputDevices) {
        self.state.set_input_devices(devices)
    }
    /// Delegate to this board’s shared wiring.
    pub fn set_zapper_aim(&mut self, aim: Option<(u16, u16)>) {
        self.state.set_zapper_aim(aim)
    }
    /// Delegate to this board’s shared wiring.
    pub fn zapper_aim(&self) -> Option<(u16, u16)> {
        self.state.zapper_aim()
    }
    /// Delegate to this board’s shared wiring.
    pub fn set_zapper_trigger(&mut self, pressed: bool) {
        self.state.set_zapper_trigger(pressed)
    }
    /// Delegate to this board’s shared wiring.
    pub fn snapshot(&self) -> NesSnapshot {
        self.state.snapshot()
    }
    /// Access this board’s live hardware.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.state.cpu
    }
    /// Access this board’s live hardware.
    pub fn ppu(&self) -> &Ppu {
        &self.state.ppu
    }
    /// Access this board’s live hardware.
    pub fn apu(&self) -> &Apu {
        &self.state.apu
    }
    /// Access this board’s live hardware.
    pub fn mapper(&self) -> &dyn Mapper {
        self.state.mapper.as_ref()
    }
    /// Access this board’s live hardware.
    pub fn mapper_mut(&mut self) -> &mut dyn Mapper {
        self.state.mapper.as_mut()
    }
    /// Access this board’s live hardware.
    pub fn controller1_state(&self) -> u8 {
        self.state.controller1_state
    }
    /// Access this board’s live hardware.
    pub fn controller2_state(&self) -> u8 {
        self.state.controller2_state
    }
    /// Access this board’s live hardware.
    pub fn write_oam(&mut self, index: u8, value: u8) {
        self.state.ppu.write_oam(index, value)
    }
    /// Access this board’s live hardware.
    pub fn board_id(&self) -> &'static str {
        B::MACHINE_ID
    }
    /// Write the PPU register bus without exposing replaceable board configuration.
    pub fn write_ppu_register(&mut self, reg: u16, value: u8) {
        self.state
            .ppu
            .cpu_write(reg, value, self.state.mapper.as_mut());
    }
}

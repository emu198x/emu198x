//! Fixed physical ownership over the unchanged legacy Sega state.
use crate::{
    CartridgeHeader, CartridgeMapper, FmUnit, IoEvent, SegaVdp, SmsState, SmsVariant, Ym2413, Z80,
};
use serde::Serialize;
use std::sync::mpsc::{Receiver, Sender};
/// Physical board identity, independent of the legacy execution enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmsHardware {
    MasterSystemNtsc,
    MasterSystemPal,
    MasterSystemJapan,
    MasterSystem1Ntsc,
    MasterSystem1Pal,
    MasterSystem2Ntsc,
    MasterSystem2Pal,
    MarkIII,
    GameGear,
}
/// Compile-time board construction policy.
pub trait SmsBoard {
    const HARDWARE: SmsHardware;
    const VARIANT: SmsVariant;
}
/// Fixed board owner. Legacy saves require the runtime's existing model ID;
/// no enum tag is added to the serialized state.
pub struct SmsCore<B: SmsBoard> {
    state: SmsState,
    board: std::marker::PhantomData<B>,
}
impl<B: SmsBoard> Serialize for SmsCore<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<B: SmsBoard> SmsCore<B> {
    /// Construct this board with cartridge boot.
    pub fn new(rom: Vec<u8>) -> Self {
        Self {
            state: SmsState::new(rom, B::VARIANT),
            board: std::marker::PhantomData,
        }
    }
    /// Construct this board with its optional base-unit firmware.
    pub fn new_with_bios(rom: Vec<u8>, bios: Vec<u8>) -> Self {
        Self {
            state: SmsState::new_with_bios(rom, bios, B::VARIANT),
            board: std::marker::PhantomData,
        }
    }
    /// Bind a legacy payload after validating its existing execution variant.
    pub fn from_state(state: SmsState) -> Result<Self, String> {
        if state.variant() != B::VARIANT {
            return Err("snapshot execution variant belongs to another Sega board".into());
        }
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    /// This owner's immutable physical identity.
    pub const fn hardware(&self) -> SmsHardware {
        B::HARDWARE
    }
    /// Delegate to this board's original hardware state.
    pub fn attach_virtual_uart(&mut self, tx: Sender<u8>, rx: Receiver<u8>) {
        self.state.attach_virtual_uart(tx, rx)
    }
    /// Delegate to this board's original hardware state.
    pub fn set_fm_unit(&mut self, unit: FmUnit) {
        self.state.set_fm_unit(unit)
    }
    /// Delegate to this board's original hardware state.
    pub fn fm_unit(&self) -> FmUnit {
        self.state.fm_unit()
    }
    /// Delegate to this board's original hardware state.
    pub fn ym2413(&self) -> Option<&Ym2413> {
        self.state.ym2413()
    }
    /// Delegate to this board's original hardware state.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        self.state.run_native_frame()
    }
    /// Delegate to this board's original hardware state.
    pub fn run_frame(&mut self) -> u64 {
        self.state.run_frame()
    }
    /// Delegate to this board's original hardware state.
    pub fn set_light_phaser_aim(&mut self, port: u8, aim: Option<(u16, u16)>) {
        self.state.set_light_phaser_aim(port, aim)
    }
    /// Delegate to this board's original hardware state.
    pub fn light_phaser_aim(&self, port: u8) -> Option<(u16, u16)> {
        self.state.light_phaser_aim(port)
    }
    /// Delegate to this board's original hardware state.
    pub fn set_light_phaser_trigger(&mut self, port: u8, pressed: bool) {
        self.state.set_light_phaser_trigger(port, pressed)
    }
    /// Delegate to this board's original hardware state.
    pub fn read_controller_port(&self, port: u8) -> u8 {
        self.state.read_controller_port(port)
    }
    /// Delegate to this board's original hardware state.
    pub fn framebuffer(&self) -> &[u32] {
        self.state.framebuffer()
    }
    /// Delegate to this board's original hardware state.
    pub fn framebuffer_width(&self) -> u32 {
        self.state.framebuffer_width()
    }
    /// Delegate to this board's original hardware state.
    pub fn framebuffer_height(&self) -> u32 {
        self.state.framebuffer_height()
    }
    /// Delegate to this board's original hardware state.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.state.take_audio_buffer()
    }
    /// Delegate to this board's original hardware state.
    pub fn set_port_dc(&mut self, value: u8) {
        self.state.set_port_dc(value)
    }
    /// Delegate to this board's original hardware state.
    pub fn set_port_dd(&mut self, value: u8) {
        self.state.set_port_dd(value)
    }
    /// Delegate to this board's original hardware state.
    pub fn set_gg_start(&mut self, value: u8) {
        self.state.set_gg_start(value)
    }
    /// Delegate to this board's original hardware state.
    pub fn set_pause_pressed(&mut self, pressed: bool) {
        self.state.set_pause_pressed(pressed)
    }
    /// Delegate to this board's original hardware state.
    pub fn cpu(&self) -> &Z80 {
        self.state.cpu()
    }
    /// Delegate to this board's original hardware state.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        self.state.cpu_mut()
    }
    /// Delegate to this board's original hardware state.
    pub fn vdp(&self) -> &SegaVdp {
        self.state.vdp()
    }
    /// Delegate to this board's original hardware state.
    pub fn variant(&self) -> SmsVariant {
        self.state.variant()
    }
    /// Delegate to this board's original hardware state.
    pub fn cartridge_mapper(&self) -> CartridgeMapper {
        self.state.cartridge_mapper()
    }
    /// Delegate to this board's original hardware state.
    pub fn mapper_regs(&self) -> &[u8; 4] {
        self.state.mapper_regs()
    }
    /// Delegate to this board's original hardware state.
    pub fn cartridge_ram(&self) -> &[u8] {
        self.state.cartridge_ram()
    }
    /// Delegate to this board's original hardware state.
    pub const fn cartridge_ram_dirty(&self) -> bool {
        self.state.cartridge_ram_dirty()
    }
    /// Delegate to this board's original hardware state.
    pub fn restore_cartridge_ram(&mut self, bytes: &[u8], dirty: bool) -> bool {
        self.state.restore_cartridge_ram(bytes, dirty)
    }
    /// Delegate to this board's original hardware state.
    pub fn cartridge_header(&self) -> Option<&CartridgeHeader> {
        self.state.cartridge_header()
    }
    /// Delegate to this board's original hardware state.
    pub fn cpu_tstates(&self) -> u64 {
        self.state.cpu_tstates()
    }
    /// Delegate to this board's original hardware state.
    pub fn peek(&self, addr: u16) -> u8 {
        self.state.peek(addr)
    }
    /// Delegate to this board's original hardware state.
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count()
    }
    /// Delegate to this board's original hardware state.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.state.poke(addr, value)
    }
    /// Delegate to this board's original hardware state.
    pub fn start_io_trace(&mut self) {
        self.state.start_io_trace()
    }
    /// Delegate to this board's original hardware state.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.state.take_io_trace()
    }
}

impl<B: SmsBoard> emu198x_zilog_z80::Z80Stepper for SmsCore<B> {
    fn z80_instructions_retired(&self) -> u64 {
        emu198x_zilog_z80::Z80Stepper::z80_instructions_retired(&self.state)
    }
    fn step_tick(&mut self) {
        emu198x_zilog_z80::Z80Stepper::step_tick(&mut self.state);
    }
}

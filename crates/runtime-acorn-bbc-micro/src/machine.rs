//! Family dispatch for the three concrete BBC motherboards.
use common_acorn_bbc_micro::{BbcModel, BbcState, Crtc6845, Disk, M6502};
use common_acorn_cassette::TapePulse;
use machine_acorn_bbc_master_128::BbcMicro as Master;
use machine_acorn_bbc_micro::BbcMicro as ModelB;
use machine_acorn_bbc_micro_b_plus::BbcMicro as BPlus;
use serde::{Deserialize, Serialize};
/// The existing BBC boards served by one family runtime.
pub enum BbcMachine {
    ModelB(ModelB),
    BPlus(BPlus),
    Master128(Master),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            BbcMachine::ModelB($m) => $body,
            BbcMachine::BPlus($m) => $body,
            BbcMachine::Master128($m) => $body,
        }
    };
}
impl BbcMachine {
    /// Construct the profile's fixed physical board.
    pub(crate) fn new(model: crate::Model, mos: Vec<u8>) -> Self {
        match model {
            crate::Model::BbcModelB => Self::ModelB(ModelB::new(mos)),
            crate::Model::BbcBPlus => Self::BPlus(BPlus::new(mos)),
            crate::Model::Master128 => Self::Master128(Master::new(mos)),
        }
    }
    /// Forward this operation to the active BBC board.
    pub fn finish_restore(&mut self) {
        dispatch!(self,m=>m.finish_restore())
    }
    /// Forward this operation to the active BBC board.
    pub fn model(&self) -> BbcModel {
        dispatch!(self,m=>m.model())
    }
    /// Forward this operation to the active BBC board.
    pub fn insert_disk(&mut self, drive: usize, disk: Disk) {
        dispatch!(self,m=>m.insert_disk(drive,disk))
    }
    /// Forward this operation to the active BBC board.
    pub fn eject_disk(&mut self, drive: usize) -> Option<Disk> {
        dispatch!(self,m=>m.eject_disk(drive))
    }
    /// Forward this operation to the active BBC board.
    pub fn disk(&self, drive: usize) -> Option<&Disk> {
        dispatch!(self,m=>m.disk(drive))
    }
    /// Forward this operation to the active BBC board.
    pub fn dfs_disk(image: &[u8], sides: usize, writable: bool) -> Disk {
        common_acorn_bbc_micro::dfs_disk(image, sides, writable)
    }
    /// Forward this operation to the active BBC board.
    pub fn dfs_sides(image: &[u8]) -> usize {
        common_acorn_bbc_micro::dfs_sides(image)
    }
    /// Forward this operation to the active BBC board.
    pub fn insert_tape(&mut self, pulses: Vec<TapePulse>) {
        dispatch!(self,m=>m.insert_tape(pulses))
    }
    /// Forward this operation to the active BBC board.
    pub fn eject_tape(&mut self) {
        dispatch!(self,m=>m.eject_tape())
    }
    /// Forward this operation to the active BBC board.
    pub fn tape_loaded(&self) -> bool {
        dispatch!(self,m=>m.tape_loaded())
    }
    /// Forward this operation to the active BBC board.
    pub fn deck_running(&self) -> bool {
        dispatch!(self,m=>m.deck_running())
    }
    /// Forward this operation to the active BBC board.
    pub fn set_deck_running(&mut self, running: bool) {
        dispatch!(self,m=>m.set_deck_running(running))
    }
    /// Forward this operation to the active BBC board.
    pub fn cassette_motor_on(&self) -> bool {
        dispatch!(self,m=>m.cassette_motor_on())
    }
    /// Forward this operation to the active BBC board.
    pub fn set_fire_button(&mut self, port: u8, pressed: bool) {
        dispatch!(self,m=>m.set_fire_button(port,pressed))
    }
    /// Forward this operation to the active BBC board.
    pub fn set_adc_channel(&mut self, channel: u8, value: u16) {
        dispatch!(self,m=>m.set_adc_channel(channel,value))
    }
    /// Forward this operation to the active BBC board.
    pub fn adc_channel(&self, channel: u8) -> u16 {
        dispatch!(self,m=>m.adc_channel(channel))
    }
    /// Forward this operation to the active BBC board.
    pub fn set_teletext_font(&mut self, font: Vec<u8>) {
        dispatch!(self,m=>m.set_teletext_font(font))
    }
    /// Forward this operation to the active BBC board.
    pub fn teletext_font_missing(&self) -> bool {
        dispatch!(self,m=>m.teletext_font_missing())
    }
    /// Forward this operation to the active BBC board.
    pub fn insert_rom(&mut self, bank: usize, rom: Vec<u8>) {
        dispatch!(self,m=>m.insert_rom(bank,rom))
    }
    /// Forward this operation to the active BBC board.
    pub fn sideways_rom_count(&self) -> usize {
        dispatch!(self,m=>m.sideways_rom_count())
    }
    /// Forward this operation to the active BBC board.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self,m=>m.run_native_frame())
    }
    /// Forward this operation to the active BBC board.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self,m=>m.run_frame())
    }
    /// Forward this operation to the active BBC board.
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self,m=>m.framebuffer())
    }
    /// Forward this operation to the active BBC board.
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_width())
    }
    /// Forward this operation to the active BBC board.
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_height())
    }
    /// Forward this operation to the active BBC board.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_audio_buffer())
    }
    /// Forward this operation to the active BBC board.
    pub fn press_key(&mut self, col: usize, row: usize) {
        dispatch!(self,m=>m.press_key(col,row))
    }
    /// Forward this operation to the active BBC board.
    pub fn release_key(&mut self, col: usize, row: usize) {
        dispatch!(self,m=>m.release_key(col,row))
    }
    /// Forward this operation to the active BBC board.
    pub fn cpu(&self) -> &M6502 {
        dispatch!(self,m=>m.cpu())
    }
    /// Forward this operation to the active BBC board.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        dispatch!(self,m=>m.cpu_mut())
    }
    /// Forward this operation to the active BBC board.
    pub fn crtc(&self) -> &Crtc6845 {
        dispatch!(self,m=>m.crtc())
    }
    /// Forward this operation to the active BBC board.
    pub fn rom_bank(&self) -> u8 {
        dispatch!(self,m=>m.rom_bank())
    }
    /// Forward this operation to the active BBC board.
    pub fn frame_count(&self) -> u64 {
        dispatch!(self,m=>m.frame_count())
    }
    /// Forward this operation to the active BBC board.
    pub fn cpu_cycles(&self) -> u64 {
        dispatch!(self,m=>m.cpu_cycles())
    }
    /// Forward this operation to the active BBC board.
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self,m=>m.peek(addr))
    }
    /// Forward this operation to the active BBC board.
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self,m=>m.poke(addr,value))
    }
    /// Forward this operation to the active BBC board.
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self,m=>m.step_instruction())
    }
}
impl Serialize for BbcMachine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>m.serialize(s))
    }
}
impl<'de> Deserialize<'de> for BbcMachine {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let state = BbcState::deserialize(d)?;
        match state.model() {
            BbcModel::ModelB => ModelB::from_state(state)
                .map(Self::ModelB)
                .map_err(serde::de::Error::custom),
            BbcModel::ModelBPlus => BPlus::from_state(state)
                .map(Self::BPlus)
                .map_err(serde::de::Error::custom),
            BbcModel::Master128 => Master::from_state(state)
                .map(Self::Master128)
                .map_err(serde::de::Error::custom),
        }
    }
}

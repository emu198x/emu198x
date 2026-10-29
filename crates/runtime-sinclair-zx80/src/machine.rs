//! Runtime selection among fixed physical boards.
use crate::Model;
use common_sinclair_zx80::*;
use machine_sinclair_zx80::Zx80 as Uk;
use machine_sinclair_zx80_usa::Zx80 as Usa;
use serde::Serialize;
/// Concrete board held by the shared family runtime.
pub enum Zx80Machine {
    Uk(Uk),
    Usa(Usa),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            Zx80Machine::Uk($m) => $body,
            Zx80Machine::Usa($m) => $body,
        }
    };
}
impl Zx80Machine {
    pub(crate) fn new(rom: Vec<u8>, ram_size: usize, model: Model) -> Result<Self, String> {
        match model {
            Model::Zx80 | Model::Zx80RamPack => Uk::new(rom, ram_size).map(Self::Uk),
            Model::Zx80Usa => Usa::new(rom, ram_size).map(Self::Usa),
        }
    }
    pub(crate) fn from_state(model: Model, state: Zx80State) -> Result<Self, String> {
        match model {
            Model::Zx80 | Model::Zx80RamPack => Uk::from_state(state).map(Self::Uk),
            Model::Zx80Usa => Usa::from_state(state).map(Self::Usa),
        }
    }
    /// Immutable physical identity.
    pub fn board_id(&self) -> &'static str {
        dispatch!(self,m=>m.board_id())
    }
    /// Delegate to the selected fixed board.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self,m=>m.run_frame())
    }
    /// Delegate to the selected fixed board.
    pub fn insert_tape(&mut self, edges: &[u64]) {
        dispatch!(self,m=>m.insert_tape(edges))
    }
    /// Delegate to the selected fixed board.
    pub fn tape_remaining(&self) -> usize {
        dispatch!(self,m=>m.tape_remaining())
    }
    /// Delegate to the selected fixed board.
    pub fn start_tape_recording(&mut self) {
        dispatch!(self,m=>m.start_tape_recording())
    }
    /// Delegate to the selected fixed board.
    pub fn take_tape_recording(&mut self) -> Vec<u64> {
        dispatch!(self,m=>m.take_tape_recording())
    }
    /// Delegate to the selected fixed board.
    pub fn painted_pixels(&self) -> u32 {
        dispatch!(self,m=>m.painted_pixels())
    }
    /// Delegate to the selected fixed board.
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self,m=>m.framebuffer())
    }
    /// Delegate to the selected fixed board.
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_width())
    }
    /// Delegate to the selected fixed board.
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self,m=>m.framebuffer_height())
    }
    /// Delegate to the selected fixed board.
    pub fn text_top(&self) -> u32 {
        dispatch!(self,m=>m.text_top())
    }
    /// Delegate to the selected fixed board.
    pub fn ram_bytes(&self) -> usize {
        dispatch!(self,m=>m.ram_bytes())
    }
    /// Delegate to the selected fixed board.
    pub fn television_standard(&self) -> TelevisionStandard {
        dispatch!(self,m=>m.television_standard())
    }
    /// Delegate to the selected fixed board.
    pub fn press_key(&mut self, key: Zx80Key) {
        dispatch!(self,m=>m.press_key(key))
    }
    /// Delegate to the selected fixed board.
    pub fn release_key(&mut self, key: Zx80Key) {
        dispatch!(self,m=>m.release_key(key))
    }
    /// Delegate to the selected fixed board.
    pub fn release_all_keys(&mut self) {
        dispatch!(self,m=>m.release_all_keys())
    }
    /// Delegate to the selected fixed board.
    pub fn peek_memory(&self, addr: u16) -> u8 {
        dispatch!(self,m=>m.peek_memory(addr))
    }
    /// Delegate to the selected fixed board.
    pub fn cpu(&self) -> &Z80 {
        dispatch!(self,m=>m.cpu())
    }
    /// Delegate to the selected fixed board.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        dispatch!(self,m=>m.cpu_mut())
    }
    /// Delegate to the selected fixed board.
    pub fn master_clock(&self) -> u64 {
        dispatch!(self,m=>m.master_clock())
    }
    /// Delegate to the selected fixed board.
    pub fn frame_count(&self) -> u64 {
        dispatch!(self,m=>m.frame_count())
    }
    /// Delegate to the selected fixed board.
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self,m=>m.peek(addr))
    }
    /// Delegate to the selected fixed board.
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self,m=>m.poke(addr, value))
    }
    /// Delegate to the selected fixed board.
    pub fn start_io_trace(&mut self) {
        dispatch!(self,m=>m.start_io_trace())
    }
    /// Delegate to the selected fixed board.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        dispatch!(self,m=>m.take_io_trace())
    }
    /// Execute through the selected board.
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self,m=>m.step_instruction())
    }
}
impl Serialize for Zx80Machine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>m.serialize(s))
    }
}

//! Runtime selection among fixed physical boards.
use crate::Model;
use common_sinclair_zx81::*;
use machine_sinclair_zx81::Zx81 as Uk;
use machine_timex_ts1000::Zx81 as Usa;
use serde::Serialize;
/// Concrete board held by the shared family runtime.
pub enum Zx81Machine {
    Uk(Uk),
    Usa(Usa),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            Zx81Machine::Uk($m) => $body,
            Zx81Machine::Usa($m) => $body,
        }
    };
}
impl Zx81Machine {
    pub(crate) fn new(rom: Vec<u8>, ram_size: usize, model: Model) -> Result<Self, String> {
        match model {
            Model::Zx81 | Model::Zx81Ram16k => Uk::new(rom, ram_size).map(Self::Uk),
            Model::Ts1000 => Usa::new(rom, ram_size).map(Self::Usa),
        }
    }
    pub(crate) fn from_state(model: Model, state: Zx81State) -> Result<Self, String> {
        match model {
            Model::Zx81 | Model::Zx81Ram16k => Uk::from_state(state).map(Self::Uk),
            Model::Ts1000 => Usa::from_state(state).map(Self::Usa),
        }
    }
    /// Immutable physical identity.
    pub fn board_id(&self) -> &'static str {
        dispatch!(self,m=>m.board_id())
    }
    /// Delegate to the selected fixed board.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_audio_buffer())
    }
    /// Delegate to the selected fixed board.
    pub fn insert_tape(&mut self, edges: &[u64]) {
        dispatch!(self,m=>m.insert_tape(edges))
    }
    /// Delegate to the selected fixed board.
    pub fn eject_tape(&mut self) {
        dispatch!(self,m=>m.eject_tape())
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
    pub fn ram_bytes(&self) -> usize {
        dispatch!(self,m=>m.ram_bytes())
    }
    /// Delegate to the selected fixed board.
    pub fn television_standard(&self) -> TelevisionStandard {
        dispatch!(self,m=>m.television_standard())
    }
    /// Delegate to the selected fixed board.
    pub fn text_top(&self) -> u32 {
        dispatch!(self,m=>m.text_top())
    }
    /// Delegate to the selected fixed board.
    pub fn ear_input(&self) -> bool {
        dispatch!(self,m=>m.ear_input())
    }
    /// Delegate to the selected fixed board.
    pub fn set_ear_input(&mut self, level: bool) {
        dispatch!(self,m=>m.set_ear_input(level))
    }
    /// Delegate to the selected fixed board.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self,m=>m.run_frame())
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
    pub fn press_key(&mut self, key: Zx81Key) {
        dispatch!(self,m=>m.press_key(key))
    }
    /// Delegate to the selected fixed board.
    pub fn release_key(&mut self, key: Zx81Key) {
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
    pub fn nmi_enabled(&self) -> bool {
        dispatch!(self,m=>m.nmi_enabled())
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
    pub fn set_hires_board(&mut self, board: HiResBoard) {
        dispatch!(self,m=>m.set_hires_board(board))
    }
    /// Delegate to the selected fixed board.
    pub fn hires_board(&self) -> HiResBoard {
        dispatch!(self,m=>m.hires_board())
    }
    /// Delegate to the selected fixed board.
    pub fn load_hires_rom(&mut self, image: &[u8]) -> Result<(), String> {
        dispatch!(self,m=>m.load_hires_rom(image))
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
impl Serialize for Zx81Machine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>m.serialize(s))
    }
}

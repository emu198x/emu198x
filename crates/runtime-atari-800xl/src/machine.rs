//! Select among statically fixed XL/XE boards.
use crate::Model;
use common_atari_800xl::*;
use machine_atari_130xe_ntsc::Atari130xe as A130xeNtsc;
use machine_atari_130xe_pal::Atari130xe as A130xePal;
use machine_atari_800xl::Atari800xl as A800xlNtsc;
use machine_atari_800xl_pal::Atari800xl as A800xlPal;
use serde::Serialize;
/// Concrete owner held by the family runtime.
pub enum Atari800xlMachine {
    A800xlNtsc(A800xlNtsc),
    A800xlPal(A800xlPal),
    A130xeNtsc(A130xeNtsc),
    A130xePal(A130xePal),
}
macro_rules! dispatch {
    ($s:expr,$m:ident=>$body:expr) => {
        match $s {
            Atari800xlMachine::A800xlNtsc($m) => $body,
            Atari800xlMachine::A800xlPal($m) => $body,
            Atari800xlMachine::A130xeNtsc($m) => $body,
            Atari800xlMachine::A130xePal($m) => $body,
        }
    };
}
impl Atari800xlMachine {
    pub(crate) fn with_cartridge(
        os: Option<Vec<u8>>,
        basic: Option<Vec<u8>>,
        cart: Option<Cartridge>,
        model: Model,
        basic_enabled: bool,
    ) -> Self {
        match model {
            Model::A800xlNtsc => {
                Self::A800xlNtsc(A800xlNtsc::with_cartridge(os, basic, cart, basic_enabled))
            }
            Model::A800xlPal => {
                Self::A800xlPal(A800xlPal::with_cartridge(os, basic, cart, basic_enabled))
            }
            Model::A130xeNtsc => {
                Self::A130xeNtsc(A130xeNtsc::with_cartridge(os, basic, cart, basic_enabled))
            }
            Model::A130xePal => {
                Self::A130xePal(A130xePal::with_cartridge(os, basic, cart, basic_enabled))
            }
        }
    }
    pub(crate) fn from_state(model: Model, state: Atari800xlState) -> Result<Self, String> {
        match model {
            Model::A800xlNtsc => A800xlNtsc::from_state(state).map(Self::A800xlNtsc),
            Model::A800xlPal => A800xlPal::from_state(state).map(Self::A800xlPal),
            Model::A130xeNtsc => A130xeNtsc::from_state(state).map(Self::A130xeNtsc),
            Model::A130xePal => A130xePal::from_state(state).map(Self::A130xePal),
        }
    }
    /// Immutable physical identity.
    pub fn board_id(&self) -> &'static str {
        dispatch!(self,m=>m.board_id())
    }
    /// Delegate to the selected fixed board.
    pub fn memory(&self) -> MemoryModel {
        dispatch!(self,m=>m.memory())
    }
    /// Delegate to the selected fixed board.
    pub fn extended_ram(&self, bank: usize, offset: usize) -> Option<u8> {
        dispatch!(self,m=>m.extended_ram(bank, offset))
    }
    /// Delegate to the selected fixed board.
    pub fn os_rom(&self) -> Option<&[u8]> {
        dispatch!(self,m=>m.os_rom())
    }
    /// Delegate to the selected fixed board.
    pub fn basic_rom(&self) -> Option<&[u8]> {
        dispatch!(self,m=>m.basic_rom())
    }
    /// Delegate to the selected fixed board.
    pub fn cartridge(&self) -> Option<&Cartridge> {
        dispatch!(self,m=>m.cartridge())
    }
    /// Delegate to the selected fixed board.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self,m=>m.run_native_frame())
    }
    /// Delegate to the selected fixed board.
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self,m=>m.run_frame())
    }
    /// Delegate to the selected fixed board.
    pub fn run_until_pc(&mut self, target_pc: u16, max_ticks: u64) -> (u64, bool) {
        dispatch!(self,m=>m.run_until_pc(target_pc, max_ticks))
    }
    /// Delegate to the selected fixed board.
    pub fn sio(&self) -> &SioBus {
        dispatch!(self,m=>m.sio())
    }
    /// Delegate to the selected fixed board.
    pub fn sio_mut(&mut self) -> &mut SioBus {
        dispatch!(self,m=>m.sio_mut())
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
    pub fn load_program_byte(&mut self, addr: u16, value: u8) {
        dispatch!(self,m=>m.load_program_byte(addr, value))
    }
    /// Delegate to the selected fixed board.
    pub fn call_loaded_subroutine(&mut self, entry: u16, max_ticks: u64) -> bool {
        dispatch!(self,m=>m.call_loaded_subroutine(entry, max_ticks))
    }
    /// Delegate to the selected fixed board.
    pub fn launch_loaded_program(&mut self, entry: u16, max_ticks: u64) -> bool {
        dispatch!(self,m=>m.launch_loaded_program(entry, max_ticks))
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
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        dispatch!(self,m=>m.take_audio_buffer())
    }
    /// Delegate to the selected fixed board.
    pub fn set_joystick(&mut self, up: bool, down: bool, left: bool, right: bool) {
        dispatch!(self,m=>m.set_joystick(up, down, left, right))
    }
    /// Delegate to the selected fixed board.
    pub fn set_fire(&mut self, pressed: bool) {
        dispatch!(self,m=>m.set_fire(pressed))
    }
    /// Delegate to the selected fixed board.
    pub fn set_console_keys(&mut self, start: bool, select: bool, option: bool) {
        dispatch!(self,m=>m.set_console_keys(start, select, option))
    }
    /// Delegate to the selected fixed board.
    pub fn press_key(&mut self, scancode: u8) {
        dispatch!(self,m=>m.press_key(scancode))
    }
    /// Delegate to the selected fixed board.
    pub fn release_key(&mut self) {
        dispatch!(self,m=>m.release_key())
    }
    /// Delegate to the selected fixed board.
    pub fn cpu(&self) -> &M6502 {
        dispatch!(self,m=>m.cpu())
    }
    /// Delegate to the selected fixed board.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        dispatch!(self,m=>m.cpu_mut())
    }
    /// Delegate to the selected fixed board.
    pub fn antic(&self) -> &Antic {
        dispatch!(self,m=>m.antic())
    }
    /// Delegate to the selected fixed board.
    pub fn gtia(&self) -> &Gtia {
        dispatch!(self,m=>m.gtia())
    }
    /// Delegate to the selected fixed board.
    pub fn pokey(&self) -> &Pokey {
        dispatch!(self,m=>m.pokey())
    }
    /// Delegate to the selected fixed board.
    pub fn pia(&self) -> &Pia6520 {
        dispatch!(self,m=>m.pia())
    }
    /// Delegate to the selected fixed board.
    pub fn ram(&self) -> &[u8] {
        dispatch!(self,m=>m.ram())
    }
    /// Delegate to the selected fixed board.
    pub fn region(&self) -> Atari800xlRegion {
        dispatch!(self,m=>m.region())
    }
    /// Delegate to the selected fixed board.
    pub fn master_clock(&self) -> u64 {
        dispatch!(self,m=>m.master_clock())
    }
    /// Delegate to the selected fixed board.
    pub fn clocks_per_frame(&self) -> u64 {
        dispatch!(self,m=>m.clocks_per_frame())
    }
    /// Delegate to the selected fixed board.
    pub fn frame_count(&self) -> u64 {
        dispatch!(self,m=>m.frame_count())
    }
    /// Delegate to the selected fixed board.
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self,m=>m.step_instruction())
    }
}
impl Serialize for Atari800xlMachine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m=>m.serialize(s))
    }
}

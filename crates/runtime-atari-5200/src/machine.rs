//! The selected physical 5200 owner. Board identity comes from its explicit profile.
use crate::Model;
use common_atari_5200::*;
use serde::Serialize;
pub enum Atari5200Machine {
    Ca018087(machine_atari_5200_ca018087::Atari5200),
    Ca020108(machine_atari_5200_ca020108::Atari5200),
    Ca021374(machine_atari_5200_ca021374::Atari5200),
}
macro_rules! dispatch {
    ($s:expr, $m:ident => $body:expr) => {
        match $s {
            Atari5200Machine::Ca018087($m) => $body,
            Atari5200Machine::Ca020108($m) => $body,
            Atari5200Machine::Ca021374($m) => $body,
        }
    };
}
impl Atari5200Machine {
    pub(crate) fn from_cartridge(cart: Cartridge, bios: Vec<u8>, model: Model) -> Self {
        match model {
            Model::Ca018087 => Self::Ca018087(
                machine_atari_5200_ca018087::Atari5200::from_cartridge(cart, bios),
            ),
            Model::Ca020108 => Self::Ca020108(
                machine_atari_5200_ca020108::Atari5200::from_cartridge(cart, bios),
            ),
            Model::Ca021374 => Self::Ca021374(
                machine_atari_5200_ca021374::Atari5200::from_cartridge(cart, bios),
            ),
        }
    }
    pub(crate) fn from_state(model: Model, state: Atari5200State) -> Result<Self, String> {
        match model {
            Model::Ca018087 => {
                machine_atari_5200_ca018087::Atari5200::from_state(state).map(Self::Ca018087)
            }
            Model::Ca020108 => {
                machine_atari_5200_ca020108::Atari5200::from_state(state).map(Self::Ca020108)
            }
            Model::Ca021374 => {
                machine_atari_5200_ca021374::Atari5200::from_state(state).map(Self::Ca021374)
            }
        }
    }
    pub fn board_id(&self) -> &'static str {
        dispatch!(self, m => m.board_id())
    }
    pub fn controller_ports(&self) -> usize {
        dispatch!(self, m => m.controller_ports())
    }
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        dispatch!(self, m => m.run_native_frame())
    }
    pub fn run_frame(&mut self) -> u64 {
        dispatch!(self, m => m.run_frame())
    }
    pub fn framebuffer(&self) -> &[u32] {
        dispatch!(self, m => m.framebuffer())
    }
    pub fn framebuffer_width(&self) -> u32 {
        dispatch!(self, m => m.framebuffer_width())
    }
    pub fn framebuffer_height(&self) -> u32 {
        dispatch!(self, m => m.framebuffer_height())
    }
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        dispatch!(self, m => m.take_audio_buffer())
    }
    pub fn set_joystick(&mut self, x: u8, y: u8) {
        dispatch!(self, m => m.set_joystick(x, y))
    }
    pub fn set_controller_joystick(&mut self, port: usize, x: u8, y: u8) {
        dispatch!(self, m => m.set_controller_joystick(port, x, y))
    }
    pub fn set_fire(&mut self, pressed: bool) {
        dispatch!(self, m => m.set_fire(pressed))
    }
    pub fn set_controller_fire(&mut self, port: usize, upper: bool, pressed: bool) {
        dispatch!(self, m => m.set_controller_fire(port, upper, pressed))
    }
    pub fn set_keypad(&mut self, code: u8, pressed: bool) {
        dispatch!(self, m => m.set_keypad(code, pressed))
    }
    pub fn set_controller_keypad(&mut self, port: usize, code: u8, pressed: bool) {
        dispatch!(self, m => m.set_controller_keypad(port, code, pressed))
    }
    pub fn bios_bytes(&self) -> &[u8] {
        dispatch!(self, m => m.bios_bytes())
    }
    pub fn cart(&self) -> &Cartridge {
        dispatch!(self, m => m.cart())
    }
    pub fn cpu(&self) -> &M6502 {
        dispatch!(self, m => m.cpu())
    }
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        dispatch!(self, m => m.cpu_mut())
    }
    pub fn antic(&self) -> &Antic {
        dispatch!(self, m => m.antic())
    }
    pub fn gtia(&self) -> &Gtia {
        dispatch!(self, m => m.gtia())
    }
    pub fn pokey(&self) -> &Pokey {
        dispatch!(self, m => m.pokey())
    }
    pub fn region(&self) -> Atari5200Region {
        dispatch!(self, m => m.region())
    }
    pub fn master_clock(&self) -> u64 {
        dispatch!(self, m => m.master_clock())
    }
    pub fn frame_count(&self) -> u64 {
        dispatch!(self, m => m.frame_count())
    }
    pub fn peek(&self, addr: u16) -> u8 {
        dispatch!(self, m => m.peek(addr))
    }
    pub fn poke(&mut self, addr: u16, value: u8) {
        dispatch!(self, m => m.poke(addr, value))
    }
    pub fn step_instruction(&mut self) -> u64 {
        dispatch!(self, m => m.step_instruction())
    }
}
impl Serialize for Atari5200Machine {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        dispatch!(self,m => m.serialize(s))
    }
}

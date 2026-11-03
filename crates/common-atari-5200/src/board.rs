//! A concrete physical assembly over the shared v20 hardware payload.
use super::*;
pub trait Atari5200Board {
    const MACHINE_ID: &'static str;
    const CONTROLLER_PORTS: usize;
}
pub struct Atari5200Core<B: Atari5200Board> {
    state: Atari5200State,
    board: std::marker::PhantomData<B>,
}
impl<B: Atari5200Board> Serialize for Atari5200Core<B> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(serializer)
    }
}
impl<B: Atari5200Board> Atari5200Core<B> {
    pub fn new(rom: Vec<u8>, bios: Vec<u8>) -> Result<Self, String> {
        Ok(Self::from_cartridge(Cartridge::from_rom(&rom)?, bios))
    }
    pub fn from_cartridge(cart: Cartridge, bios: Vec<u8>) -> Self {
        let mut state = Atari5200State::from_cartridge(cart, bios, Atari5200Region::Ntsc);
        state.controller_ports = B::CONTROLLER_PORTS;
        Self {
            state,
            board: std::marker::PhantomData,
        }
    }
    pub fn from_state(mut state: Atari5200State) -> Result<Self, String> {
        if !matches!(B::CONTROLLER_PORTS, 2 | 4)
            || state.region != Atari5200Region::Ntsc
            || state.clocks_per_frame != 262 * u64::from(COLOUR_CLOCKS_PER_LINE)
            || state.framebuffer().len()
                != state.framebuffer_width() as usize * state.framebuffer_height() as usize
            || state.framebuffer_height()
                != emu198x_atari_gtia::GtiaRegion::Ntsc.framebuffer_height()
            || (B::CONTROLLER_PORTS..4).any(|port| state.gtia.read(0x10 + port as u8) != 1)
            || state.controllers[B::CONTROLLER_PORTS..]
                .iter()
                .any(|c| c.keys != 0 || c.upper_fire || c.pots != [POT_CENTER; 2])
        {
            return Err("snapshot hardware does not match this 5200 board".into());
        }
        state.controller_ports = B::CONTROLLER_PORTS;
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    pub const fn board_id(&self) -> &'static str {
        B::MACHINE_ID
    }
    pub const fn controller_ports(&self) -> usize {
        B::CONTROLLER_PORTS
    }
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        self.state.run_native_frame()
    }
    pub fn run_frame(&mut self) -> u64 {
        self.state.run_frame()
    }
    pub fn framebuffer(&self) -> &[u32] {
        self.state.framebuffer()
    }
    pub fn framebuffer_width(&self) -> u32 {
        self.state.framebuffer_width()
    }
    pub fn framebuffer_height(&self) -> u32 {
        self.state.framebuffer_height()
    }
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.state.take_audio_buffer()
    }
    pub fn set_joystick(&mut self, x: u8, y: u8) {
        self.state.set_joystick(x, y)
    }
    pub fn set_controller_joystick(&mut self, port: usize, x: u8, y: u8) {
        self.state.set_controller_joystick(port, x, y)
    }
    pub fn set_fire(&mut self, pressed: bool) {
        self.state.set_fire(pressed)
    }
    pub fn set_controller_fire(&mut self, port: usize, upper: bool, pressed: bool) {
        self.state.set_controller_fire(port, upper, pressed)
    }
    pub fn set_keypad(&mut self, code: u8, pressed: bool) {
        self.state.set_keypad(code, pressed)
    }
    pub fn set_controller_keypad(&mut self, port: usize, code: u8, pressed: bool) {
        self.state.set_controller_keypad(port, code, pressed)
    }
    pub fn bios_bytes(&self) -> &[u8] {
        self.state.bios_bytes()
    }
    pub fn cart(&self) -> &Cartridge {
        self.state.cart()
    }
    pub fn cpu(&self) -> &M6502 {
        self.state.cpu()
    }
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        self.state.cpu_mut()
    }
    pub fn antic(&self) -> &Antic {
        self.state.antic()
    }
    pub fn gtia(&self) -> &Gtia {
        self.state.gtia()
    }
    pub fn pokey(&self) -> &Pokey {
        self.state.pokey()
    }
    pub fn region(&self) -> Atari5200Region {
        self.state.region()
    }
    pub fn master_clock(&self) -> u64 {
        self.state.master_clock()
    }
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count()
    }
    pub fn peek(&self, addr: u16) -> u8 {
        self.state.peek(addr)
    }
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.state.poke(addr, value)
    }
    pub fn step_instruction(&mut self) -> u64 {
        self.state.step_instruction()
    }
}

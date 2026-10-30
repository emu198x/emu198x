//! Fixed physical wiring over the original serialized XL/XE engine.
use super::*;
/// Controller and video hardware fixed by a concrete machine owner.
pub trait Atari800xlBoard {
    const REGION: Atari800xlRegion;
    const MEMORY: MemoryModel;
    const MACHINE_ID: &'static str;
}
/// One fixed board, encoded as the original version-21 hardware payload.
pub struct Atari800xlCore<B: Atari800xlBoard> {
    state: Atari800xlState,
    board: std::marker::PhantomData<B>,
}
impl<B: Atari800xlBoard> Serialize for Atari800xlCore<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<B: Atari800xlBoard> Atari800xlCore<B> {
    /// Construct this board from optional firmware and a cartridge image.
    pub fn new(
        os: Option<Vec<u8>>,
        basic: Option<Vec<u8>>,
        cart: Option<Vec<u8>>,
        basic_enabled: bool,
    ) -> Result<Self, String> {
        let state =
            Atari800xlState::new(os, basic, cart, B::REGION, basic_enabled)?.with_memory(B::MEMORY);
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    /// Cold boot this board while retaining an already parsed cartridge mapper.
    pub fn with_cartridge(
        os: Option<Vec<u8>>,
        basic: Option<Vec<u8>>,
        cart: Option<Cartridge>,
        basic_enabled: bool,
    ) -> Self {
        let state = Atari800xlState::with_cartridge(os, basic, cart, B::REGION, basic_enabled)
            .with_memory(B::MEMORY);
        Self {
            state,
            board: std::marker::PhantomData,
        }
    }
    /// Bind legacy state only when its board and complete raster match.
    pub fn from_state(state: Atari800xlState) -> Result<Self, String> {
        let ram_len = match B::MEMORY {
            MemoryModel::Xl64k => 65536,
            MemoryModel::Xe128k => 131072,
        };
        if state.region() != B::REGION
            || state.memory() != B::MEMORY
            || state.ram().len() != ram_len
            || state.clocks_per_frame()
                != u64::from(B::REGION.lines_per_frame()) * u64::from(COLOUR_CLOCKS_PER_LINE)
            || state.framebuffer_height() != B::REGION.gtia_region().framebuffer_height()
            || state.framebuffer().len()
                != state.framebuffer_width() as usize * state.framebuffer_height() as usize
        {
            return Err("snapshot hardware does not match this Atari XL/XE board".into());
        }
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    /// Immutable physical identity.
    pub const fn board_id(&self) -> &'static str {
        B::MACHINE_ID
    }
    /// Delegate to the unchanged common engine.
    pub fn memory(&self) -> MemoryModel {
        self.state.memory()
    }
    /// Delegate to the unchanged common engine.
    pub fn extended_ram(&self, bank: usize, offset: usize) -> Option<u8> {
        self.state.extended_ram(bank, offset)
    }
    /// Delegate to the unchanged common engine.
    pub fn os_rom(&self) -> Option<&[u8]> {
        self.state.os_rom()
    }
    /// Delegate to the unchanged common engine.
    pub fn basic_rom(&self) -> Option<&[u8]> {
        self.state.basic_rom()
    }
    /// Delegate to the unchanged common engine.
    pub fn cartridge(&self) -> Option<&Cartridge> {
        self.state.cartridge()
    }
    /// Delegate to the unchanged common engine.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        self.state.run_native_frame()
    }
    /// Delegate to the unchanged common engine.
    pub fn run_frame(&mut self) -> u64 {
        self.state.run_frame()
    }
    /// Delegate to the unchanged common engine.
    pub fn run_until_pc(&mut self, target_pc: u16, max_ticks: u64) -> (u64, bool) {
        self.state.run_until_pc(target_pc, max_ticks)
    }
    /// Delegate to the unchanged common engine.
    pub fn sio(&self) -> &SioBus {
        self.state.sio()
    }
    /// Delegate to the unchanged common engine.
    pub fn sio_mut(&mut self) -> &mut SioBus {
        self.state.sio_mut()
    }
    /// Delegate to the unchanged common engine.
    pub fn peek(&self, addr: u16) -> u8 {
        self.state.peek(addr)
    }
    /// Delegate to the unchanged common engine.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.state.poke(addr, value)
    }
    /// Delegate to the unchanged common engine.
    pub fn load_program_byte(&mut self, addr: u16, value: u8) {
        self.state.load_program_byte(addr, value)
    }
    /// Delegate to the unchanged common engine.
    pub fn call_loaded_subroutine(&mut self, entry: u16, max_ticks: u64) -> bool {
        self.state.call_loaded_subroutine(entry, max_ticks)
    }
    /// Delegate to the unchanged common engine.
    pub fn launch_loaded_program(&mut self, entry: u16, max_ticks: u64) -> bool {
        self.state.launch_loaded_program(entry, max_ticks)
    }
    /// Delegate to the unchanged common engine.
    pub fn framebuffer(&self) -> &[u32] {
        self.state.framebuffer()
    }
    /// Delegate to the unchanged common engine.
    pub fn framebuffer_width(&self) -> u32 {
        self.state.framebuffer_width()
    }
    /// Delegate to the unchanged common engine.
    pub fn framebuffer_height(&self) -> u32 {
        self.state.framebuffer_height()
    }
    /// Delegate to the unchanged common engine.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.state.take_audio_buffer()
    }
    /// Delegate to the unchanged common engine.
    pub fn set_joystick(&mut self, up: bool, down: bool, left: bool, right: bool) {
        self.state.set_joystick(up, down, left, right)
    }
    /// Delegate to the unchanged common engine.
    pub fn set_fire(&mut self, pressed: bool) {
        self.state.set_fire(pressed)
    }
    /// Delegate to the unchanged common engine.
    pub fn set_console_keys(&mut self, start: bool, select: bool, option: bool) {
        self.state.set_console_keys(start, select, option)
    }
    /// Delegate to the unchanged common engine.
    pub fn press_key(&mut self, scancode: u8) {
        self.state.press_key(scancode)
    }
    /// Delegate to the unchanged common engine.
    pub fn release_key(&mut self) {
        self.state.release_key()
    }
    /// Delegate to the unchanged common engine.
    pub fn cpu(&self) -> &M6502 {
        self.state.cpu()
    }
    /// Delegate to the unchanged common engine.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        self.state.cpu_mut()
    }
    /// Delegate to the unchanged common engine.
    pub fn antic(&self) -> &Antic {
        self.state.antic()
    }
    /// Delegate to the unchanged common engine.
    pub fn gtia(&self) -> &Gtia {
        self.state.gtia()
    }
    /// Delegate to the unchanged common engine.
    pub fn pokey(&self) -> &Pokey {
        self.state.pokey()
    }
    /// Delegate to the unchanged common engine.
    pub fn pia(&self) -> &Pia6520 {
        self.state.pia()
    }
    /// Delegate to the unchanged common engine.
    pub fn ram(&self) -> &[u8] {
        self.state.ram()
    }
    /// Delegate to the unchanged common engine.
    pub fn region(&self) -> Atari800xlRegion {
        self.state.region()
    }
    /// Delegate to the unchanged common engine.
    pub fn master_clock(&self) -> u64 {
        self.state.master_clock()
    }
    /// Delegate to the unchanged common engine.
    pub fn clocks_per_frame(&self) -> u64 {
        self.state.clocks_per_frame()
    }
    /// Delegate to the unchanged common engine.
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count()
    }
    /// Delegate to the unchanged common engine.
    pub fn step_instruction(&mut self) -> u64 {
        self.state.step_instruction()
    }
}

//! Fixed physical board ownership over the unchanged legacy state.
use super::*;
/// Region and identity fixed by a concrete machine crate.
pub trait Atari7800Board {
    const REGION: Atari7800Region;
    const MACHINE_ID: &'static str;
}
/// One fixed board, serialized transparently as its original hardware state.
pub struct Atari7800Core<B: Atari7800Board> {
    state: Atari7800State,
    board: std::marker::PhantomData<B>,
}
impl<B: Atari7800Board> Serialize for Atari7800Core<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<B: Atari7800Board> Atari7800Core<B> {
    /// Construct this fixed board from a cartridge image.
    pub fn new(rom: Vec<u8>) -> Result<Self, String> {
        Ok(Self {
            state: Atari7800State::new(rom, B::REGION)?,
            board: std::marker::PhantomData,
        })
    }
    /// Bind original state only when its board/raster configuration matches.
    pub fn from_state(state: Atari7800State) -> Result<Self, String> {
        if state.region() != B::REGION
            || state.clocks_per_frame != B::REGION.frame_ticks()
            || state.framebuffer_height() != B::REGION.maria_region().framebuffer_height()
            || state.framebuffer().len()
                != state.framebuffer_width() as usize * state.framebuffer_height() as usize
        {
            return Err("snapshot hardware does not match this Atari board".into());
        }
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    /// This board's immutable physical identity.
    pub const fn board_id(&self) -> &'static str {
        B::MACHINE_ID
    }
    /// Cold boot this board while retaining the installed cartridge configuration.
    pub fn cold_boot(&self) -> Self {
        self.cold_boot_for::<B>()
    }
    /// Common family operation: build another statically fixed board with this cartridge.
    pub fn cold_boot_for<C: Atari7800Board>(&self) -> Atari7800Core<C> {
        Atari7800Core {
            state: self.state.cold_boot(C::REGION),
            board: std::marker::PhantomData,
        }
    }
    /// Delegate to the unchanged hardware state.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        self.state.run_native_frame()
    }
    /// Delegate to the unchanged hardware state.
    pub fn run_frame(&mut self) -> u64 {
        self.state.run_frame()
    }
    /// Delegate to the unchanged hardware state.
    pub fn framebuffer(&self) -> &[u32] {
        self.state.framebuffer()
    }
    /// Delegate to the unchanged hardware state.
    pub fn framebuffer_width(&self) -> u32 {
        self.state.framebuffer_width()
    }
    /// Delegate to the unchanged hardware state.
    pub fn framebuffer_height(&self) -> u32 {
        self.state.framebuffer_height()
    }
    /// Delegate to the unchanged hardware state.
    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        self.state.take_audio_samples()
    }
    /// Delegate to the unchanged hardware state.
    pub fn take_pokey_audio_samples(&mut self) -> Vec<f32> {
        self.state.take_pokey_audio_samples()
    }
    /// Delegate to the unchanged hardware state.
    pub fn audio_sample_rate(&self) -> u32 {
        self.state.audio_sample_rate()
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_joystick(&mut self, up: bool, down: bool, left: bool, right: bool) {
        self.state.set_joystick(up, down, left, right)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_console(&mut self, reset: bool, select: bool, pause: bool) {
        self.state.set_console(reset, select, pause)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_fire(&mut self, pressed: bool) {
        self.state.set_fire(pressed)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_fire2(&mut self, pressed: bool) {
        self.state.set_fire2(pressed)
    }
    /// Delegate to the unchanged hardware state.
    pub fn cpu(&self) -> &M6502 {
        self.state.cpu()
    }
    /// Delegate to the unchanged hardware state.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        self.state.cpu_mut()
    }
    /// Delegate to the unchanged hardware state.
    pub fn maria(&self) -> &Maria {
        self.state.maria()
    }
    /// Delegate to the unchanged hardware state.
    pub fn region(&self) -> Atari7800Region {
        self.state.region()
    }
    /// Delegate to the unchanged hardware state.
    pub fn master_clock(&self) -> u64 {
        self.state.master_clock()
    }
    /// Delegate to the unchanged hardware state.
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count()
    }
    /// Delegate to the unchanged hardware state.
    pub fn peek(&self, addr: u16) -> u8 {
        self.state.peek(addr)
    }
    /// Delegate to the unchanged hardware state.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.state.poke(addr, value)
    }
    /// Delegate to the unchanged hardware state.
    pub fn step_instruction(&mut self) -> u64 {
        self.state.step_instruction()
    }
}

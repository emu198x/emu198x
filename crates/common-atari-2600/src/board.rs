//! Fixed physical board ownership over the unchanged legacy state.
use super::*;
/// Region and identity fixed by a concrete machine crate.
pub trait Atari2600Board {
    const REGION: Atari2600Region;
    const MACHINE_ID: &'static str;
}
/// One fixed board, serialized transparently as its original hardware state.
pub struct Atari2600Core<B: Atari2600Board> {
    state: Atari2600State,
    board: std::marker::PhantomData<B>,
}
impl<B: Atari2600Board> Serialize for Atari2600Core<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<B: Atari2600Board> Atari2600Core<B> {
    /// Construct this fixed board from a cartridge image.
    pub fn new(rom: Vec<u8>) -> Result<Self, String> {
        Ok(Self {
            state: Atari2600State::new(rom, B::REGION)?,
            board: std::marker::PhantomData,
        })
    }
    /// Bind original state only when its board/raster configuration matches.
    pub fn from_state(state: Atari2600State) -> Result<Self, String> {
        if state.region() != B::REGION
            || state.clocks_per_frame
                != u64::from(B::REGION.lines_per_frame()) * u64::from(CLOCKS_PER_LINE)
            || state.framebuffer_height() != u32::from(B::REGION.tia_region().lines_per_frame())
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
    pub fn cold_boot_for<C: Atari2600Board>(&self) -> Atari2600Core<C> {
        Atari2600Core {
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
    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        self.state.take_audio_samples()
    }
    /// Delegate to the unchanged hardware state.
    pub fn audio_sample_rate(&self) -> u32 {
        self.state.audio_sample_rate()
    }
    /// Delegate to the unchanged hardware state.
    pub fn framebuffer_width(&self) -> u32 {
        self.state.framebuffer_width()
    }
    /// Delegate to the unchanged hardware state.
    pub fn visible_framebuffer_width(&self) -> u32 {
        self.state.visible_framebuffer_width()
    }
    /// Delegate to the unchanged hardware state.
    pub fn hblank_clocks(&self) -> u32 {
        self.state.hblank_clocks()
    }
    /// Delegate to the unchanged hardware state.
    pub fn framebuffer_height(&self) -> u32 {
        self.state.framebuffer_height()
    }
    /// Delegate to the unchanged hardware state.
    pub fn visible_framebuffer_height(&self) -> u32 {
        self.state.visible_framebuffer_height()
    }
    /// Delegate to the unchanged hardware state.
    pub fn visible_first_line(&self) -> u32 {
        self.state.visible_first_line()
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_joystick_input(&mut self, value: u8) {
        self.state.set_joystick_input(value)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_switch_input(&mut self, value: u8) {
        self.state.set_switch_input(value)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_paddle(&mut self, index: u8, value: u8) {
        self.state.set_paddle(index, value)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_fire(&mut self, port: u8, pressed: bool) {
        self.state.set_fire(port, pressed)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_booster_button(&mut self, port: u8, booster: bool, pressed: bool) {
        self.state.set_booster_button(port, booster, pressed)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_genesis_button_c(&mut self, port: u8, pressed: bool) {
        self.state.set_genesis_button_c(port, pressed)
    }
    /// Delegate to the unchanged hardware state.
    pub fn set_keypad_key(&mut self, port: u8, row: u8, col: u8, pressed: bool) {
        self.state.set_keypad_key(port, row, col, pressed)
    }
    /// Delegate to the unchanged hardware state.
    pub fn detach_keypad(&mut self, port: u8) {
        self.state.detach_keypad(port)
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
    pub fn tia(&self) -> &Tia {
        self.state.tia()
    }
    /// Delegate to the unchanged hardware state.
    pub fn riot(&self) -> &Riot6532 {
        self.state.riot()
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
    pub fn region(&self) -> Atari2600Region {
        self.state.region()
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

//! Fixed TV strap over the original hardware state.
use super::*;
/// Wiring fixed by each concrete board owner.
pub trait Zx81Board {
    const STANDARD: TelevisionStandard;
    const MACHINE_ID: &'static str;
}
/// A fixed board, encoded transparently in the original save format.
pub struct Zx81Core<B: Zx81Board> {
    state: Zx81State,
    board: std::marker::PhantomData<B>,
}
impl<B: Zx81Board> Serialize for Zx81Core<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<B: Zx81Board> Zx81Core<B> {
    /// Construct this board with its fixed strap and fitted RAM.
    pub fn new(rom: Vec<u8>, ram_size: usize) -> Result<Self, String> {
        let mut state = Zx81State::new(rom, ram_size)?;
        state.set_television_standard(B::STANDARD);
        Self::from_state(state)
    }
    /// Bind an original state to this physical board without changing its bytes.
    pub fn from_state(state: Zx81State) -> Result<Self, String> {
        if state.television_standard() != B::STANDARD
            || state.rom.len() != 8192
            || !state.ram.len().is_power_of_two()
            || state.ram.len() > 16384
            || state.ram_mask as usize != state.ram.len() - 1
            || state.framebuffer_width() != FB_WIDTH
            || state.framebuffer_height() != B::STANDARD.framebuffer_height()
            || state.framebuffer().len()
                != state.framebuffer_width() as usize * state.framebuffer_height() as usize
            || state.tape_pos > state.tape_in.len()
        {
            return Err("snapshot hardware does not match this ZX board".into());
        }
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    /// Immutable physical identity; RAM packs remain fitted configurations.
    pub const fn board_id(&self) -> &'static str {
        B::MACHINE_ID
    }
    /// Delegate to the unchanged common engine.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.state.take_audio_buffer()
    }
    /// Delegate to the unchanged common engine.
    pub fn insert_tape(&mut self, edges: &[u64]) {
        self.state.insert_tape(edges)
    }
    /// Delegate to the unchanged common engine.
    pub fn eject_tape(&mut self) {
        self.state.eject_tape()
    }
    /// Delegate to the unchanged common engine.
    pub fn tape_remaining(&self) -> usize {
        self.state.tape_remaining()
    }
    /// Delegate to the unchanged common engine.
    pub fn start_tape_recording(&mut self) {
        self.state.start_tape_recording()
    }
    /// Delegate to the unchanged common engine.
    pub fn take_tape_recording(&mut self) -> Vec<u64> {
        self.state.take_tape_recording()
    }
    /// Delegate to the unchanged common engine.
    pub fn ram_bytes(&self) -> usize {
        self.state.ram_bytes()
    }
    /// Delegate to the unchanged common engine.
    pub fn television_standard(&self) -> TelevisionStandard {
        self.state.television_standard()
    }
    /// Delegate to the unchanged common engine.
    pub fn text_top(&self) -> u32 {
        self.state.text_top()
    }
    /// Delegate to the unchanged common engine.
    pub fn ear_input(&self) -> bool {
        self.state.ear_input()
    }
    /// Delegate to the unchanged common engine.
    pub fn set_ear_input(&mut self, level: bool) {
        self.state.set_ear_input(level)
    }
    /// Delegate to the unchanged common engine.
    pub fn run_frame(&mut self) -> u64 {
        self.state.run_frame()
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
    pub fn press_key(&mut self, key: Zx81Key) {
        self.state.press_key(key)
    }
    /// Delegate to the unchanged common engine.
    pub fn release_key(&mut self, key: Zx81Key) {
        self.state.release_key(key)
    }
    /// Delegate to the unchanged common engine.
    pub fn release_all_keys(&mut self) {
        self.state.release_all_keys()
    }
    /// Delegate to the unchanged common engine.
    pub fn peek_memory(&self, addr: u16) -> u8 {
        self.state.peek_memory(addr)
    }
    /// Delegate to the unchanged common engine.
    pub fn nmi_enabled(&self) -> bool {
        self.state.nmi_enabled()
    }
    /// Delegate to the unchanged common engine.
    pub fn cpu(&self) -> &Z80 {
        self.state.cpu()
    }
    /// Delegate to the unchanged common engine.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        self.state.cpu_mut()
    }
    /// Delegate to the unchanged common engine.
    pub fn master_clock(&self) -> u64 {
        self.state.master_clock()
    }
    /// Delegate to the unchanged common engine.
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count()
    }
    /// Delegate to the unchanged common engine.
    pub fn set_hires_board(&mut self, board: HiResBoard) {
        self.state.set_hires_board(board)
    }
    /// Delegate to the unchanged common engine.
    pub fn hires_board(&self) -> HiResBoard {
        self.state.hires_board()
    }
    /// Delegate to the unchanged common engine.
    pub fn load_hires_rom(&mut self, image: &[u8]) -> Result<(), String> {
        self.state.load_hires_rom(image)
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
    pub fn start_io_trace(&mut self) {
        self.state.start_io_trace()
    }
    /// Delegate to the unchanged common engine.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.state.take_io_trace()
    }
    /// Execute through the original Z80 debugger stepping contract.
    pub fn step_instruction(&mut self) -> u64 {
        emu198x_zilog_z80::Z80Stepper::step_instruction(&mut self.state)
    }
}

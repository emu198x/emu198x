//! Fixed physical board over the original serialized regional engine.
use super::*;
/// Regional wiring selected by a concrete machine crate.
pub trait AquariusBoard {
    const REGION: AquariusRegion;
    const MACHINE_ID: &'static str;
}
/// A fixed board. Serialization is the original hardware payload.
pub struct AquariusCore<B: AquariusBoard> {
    state: AquariusState,
    board: std::marker::PhantomData<B>,
}
impl<B: AquariusBoard> Serialize for AquariusCore<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<B: AquariusBoard> AquariusCore<B> {
    /// Construct this board with firmware and an optional RAM expansion.
    pub fn new(rom: Vec<u8>, expansion_kb: usize) -> Self {
        Self {
            state: AquariusState::new(rom, expansion_kb, B::REGION),
            board: std::marker::PhantomData,
        }
    }
    /// Bind a saved payload only when its regional wiring and geometry match.
    pub fn from_state(state: AquariusState) -> Result<Self, String> {
        if state.region != B::REGION
            || state.tstates_per_frame != B::REGION.tstates_per_frame()
            || state.framebuffer.len() != (FB_WIDTH * FB_HEIGHT) as usize
        {
            return Err("snapshot hardware does not match this Aquarius board".into());
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
    /// Advance the unchanged engine by a T-state budget.
    pub fn advance_tstates(&mut self, tstates: u64) {
        self.state.advance_tstates(tstates);
    }
    /// Delegate to the original common engine.
    pub fn insert_cart(&mut self, rom: Vec<u8>) {
        self.state.insert_cart(rom)
    }
    /// Delegate to the original common engine.
    pub fn set_char_rom(&mut self, char_rom: Vec<u8>) {
        self.state.set_char_rom(char_rom)
    }
    /// Delegate to the original common engine.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        self.state.run_native_frame()
    }
    /// Delegate to the original common engine.
    pub fn run_frame(&mut self) -> u64 {
        self.state.run_frame()
    }
    /// Delegate to the original common engine.
    pub fn region(&self) -> AquariusRegion {
        self.state.region()
    }
    /// Delegate to the original common engine.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.state.take_audio_buffer()
    }
    /// Delegate to the original common engine.
    pub fn run_until_pc(&mut self, target: u16, max_tstates: u64) -> bool {
        self.state.run_until_pc(target, max_tstates)
    }
    /// Delegate to the original common engine.
    pub fn framebuffer(&self) -> &[u32] {
        self.state.framebuffer()
    }
    /// Delegate to the original common engine.
    pub fn framebuffer_width(&self) -> u32 {
        self.state.framebuffer_width()
    }
    /// Delegate to the original common engine.
    pub fn framebuffer_height(&self) -> u32 {
        self.state.framebuffer_height()
    }
    /// Delegate to the original common engine.
    pub fn peek(&self, addr: u16) -> u8 {
        self.state.peek(addr)
    }
    /// Delegate to the original common engine.
    pub fn set_key(&mut self, row: usize, col: u8, pressed: bool) {
        self.state.set_key(row, col, pressed)
    }
    /// Delegate to the original common engine.
    pub fn set_joystick(
        &mut self,
        port: u8,
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        fire: bool,
    ) {
        self.state.set_joystick(port, up, down, left, right, fire)
    }
    /// Delegate to the original common engine.
    pub fn controller_bytes(&self) -> [u8; 2] {
        self.state.controller_bytes()
    }
    /// Delegate to the original common engine.
    pub fn cpu(&self) -> &Z80 {
        self.state.cpu()
    }
    /// Delegate to the original common engine.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        self.state.cpu_mut()
    }
    /// Delegate to the original common engine.
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count()
    }
    /// Delegate to the original common engine.
    pub fn cpu_tstates(&self) -> u64 {
        self.state.cpu_tstates()
    }
    /// Delegate to the original common engine.
    pub fn speaker_bit(&self) -> bool {
        self.state.speaker_bit()
    }
    /// Delegate to the original common engine.
    pub fn start_ay_write_watch(&mut self) -> u32 {
        self.state.start_ay_write_watch()
    }
    /// Delegate to the original common engine.
    pub fn stop_ay_write_watch(&mut self) {
        self.state.stop_ay_write_watch()
    }
    /// Delegate to the original common engine.
    pub fn ay_write_watch_records(&self) -> Option<&[AyWriteRecord]> {
        self.state.ay_write_watch_records()
    }
    /// Delegate to the original common engine.
    pub fn clear_ay_write_watch_records(&mut self) {
        self.state.clear_ay_write_watch_records()
    }
    /// Delegate to the original common engine.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.state.poke(addr, value)
    }
    /// Delegate to the original common engine.
    pub fn start_io_trace(&mut self) {
        self.state.start_io_trace()
    }
    /// Delegate to the original common engine.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.state.take_io_trace()
    }
    /// Delegate to the original common engine.
    pub fn bios(&self) -> &[u8] {
        self.state.bios()
    }
    /// Delegate to the original common engine.
    pub fn character_rom(&self) -> &[u8] {
        self.state.character_rom()
    }
    /// Delegate to the original common engine.
    pub fn cartridge(&self) -> &[u8] {
        self.state.cartridge()
    }
    /// Delegate to the original common engine.
    pub fn expansion_kb(&self) -> usize {
        self.state.expansion_kb()
    }
}
impl<B: AquariusBoard> emu198x_zilog_z80::Z80Stepper for AquariusCore<B> {
    fn z80_instructions_retired(&self) -> u64 {
        self.state.cpu.instructions_retired()
    }
    fn step_tick(&mut self) {
        self.state.tick_tstate();
    }
}

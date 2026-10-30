//! Fixed board binding over the unchanged version-2 engine.
use super::*;
/// Market and video wiring selected by a physical owner.
pub trait MegaDriveBoard {
    const REGION: Region;
    const MACHINE_ID: &'static str;
}
/// One fixed regional board, encoded as the original hardware payload.
pub struct MegaDriveCore<B: MegaDriveBoard> {
    state: MegaDriveState,
    board: std::marker::PhantomData<B>,
}
impl<B: MegaDriveBoard> Serialize for MegaDriveCore<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<B: MegaDriveBoard> MegaDriveCore<B> {
    /// Construct this physical board with a native cartridge.
    pub fn new(rom: Vec<u8>) -> Self {
        Self {
            state: MegaDriveState::new(rom, B::REGION),
            board: std::marker::PhantomData,
        }
    }
    /// Construct this physical board with a Power Base Converter cartridge.
    pub fn new_power_base(rom: Vec<u8>) -> Self {
        Self {
            state: MegaDriveState::new_power_base(rom, B::REGION),
            board: std::marker::PhantomData,
        }
    }
    /// Bind legacy hardware only when region, VDP standard and RAM/geometry match.
    pub fn from_state(state: MegaDriveState) -> Result<Self, String> {
        if state.region != B::REGION
            || state.vdp.pal_machine != B::REGION.pal()
            || state.ram.len() != 0x10000
            || state.z80_ram.len() != 0x2000
            || state.frame_width() as usize > FRAME_STRIDE
            || state.framebuffer().len() < FRAME_STRIDE * state.frame_height() as usize
        {
            return Err("snapshot hardware does not match this Mega Drive board".into());
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
    /// Access this board’s live hardware.
    pub fn cpu(&self) -> &Cpu68000 {
        &self.state.cpu
    }
    /// Access this board’s live hardware.
    pub fn z80(&self) -> &Z80 {
        &self.state.z80
    }
    /// Access this board’s live hardware.
    pub fn vdp(&self) -> &Vdp {
        &self.state.vdp
    }
    /// Access this board’s live hardware.
    pub fn ports(&self) -> &[io::Port; 3] {
        &self.state.ports
    }
    /// Access this board’s live hardware.
    pub fn ports_mut(&mut self) -> &mut [io::Port; 3] {
        &mut self.state.ports
    }
    /// Access this board’s live hardware.
    pub fn region(&self) -> Region {
        self.state.region
    }
    /// Delegate to the original common engine.
    pub fn master_system_mode(&self) -> bool {
        self.state.master_system_mode()
    }
    /// Delegate to the original common engine.
    pub fn reset(&mut self) {
        self.state.reset()
    }
    /// Delegate to the original common engine.
    pub fn tick(&mut self) {
        self.state.tick()
    }
    /// Delegate to the original common engine.
    pub fn run_frame(&mut self) -> u64 {
        self.state.run_frame()
    }
    /// Delegate to the original common engine.
    pub fn master_hz(&self) -> u64 {
        self.state.master_hz()
    }
    /// Delegate to the original common engine.
    pub fn framebuffer(&self) -> &[u32] {
        self.state.framebuffer()
    }
    /// Delegate to the original common engine.
    pub fn frame_width(&self) -> u32 {
        self.state.frame_width()
    }
    /// Delegate to the original common engine.
    pub fn frame_height(&self) -> u32 {
        self.state.frame_height()
    }
    /// Delegate to the original common engine.
    pub fn take_audio(&mut self) -> Vec<f32> {
        self.state.take_audio()
    }
    /// Delegate to the original common engine.
    pub fn mclk(&self) -> u64 {
        self.state.mclk()
    }
    /// Delegate to the original common engine.
    pub fn ram(&self) -> &[u8] {
        self.state.ram()
    }
    /// Delegate to the original common engine.
    pub fn z80_ram(&self) -> &[u8] {
        self.state.z80_ram()
    }
    /// Delegate to the original common engine.
    pub fn peek(&self, address: u32) -> u8 {
        self.state.peek(address)
    }
    /// Delegate to the original common engine.
    pub fn poke(&mut self, address: u32, value: u8) {
        self.state.poke(address, value)
    }
    /// Delegate to the original common engine.
    pub fn rom(&self) -> &[u8] {
        self.state.rom()
    }
    /// Delegate to the original common engine.
    pub fn z80_bus_acknowledged(&self) -> bool {
        self.state.z80_bus_acknowledged()
    }
    /// Delegate to the original common engine.
    pub fn z80_in_reset(&self) -> bool {
        self.state.z80_in_reset()
    }
    /// Delegate to the original common engine.
    pub fn set_buttons(&mut self, port: usize, buttons: Buttons) {
        self.state.set_buttons(port, buttons)
    }
    /// Delegate to the original common engine.
    pub fn finish_restore(&mut self) {
        self.state.finish_restore()
    }
}

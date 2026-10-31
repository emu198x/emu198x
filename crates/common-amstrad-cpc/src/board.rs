//! Immutable board binding over the original version-4 CPC engine.
use super::*;
/// Physical configuration selected by a machine owner.
pub trait CpcBoard {
    const MODEL: CpcModel;
    const MACHINE_ID: &'static str;
}
/// One fixed board; its engine cannot be replaced through mutable access.
pub struct CpcCore<B: CpcBoard> {
    state: AmstradCpc,
    board: std::marker::PhantomData<B>,
}
impl<B: CpcBoard> Serialize for CpcCore<B> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(serializer)
    }
}
impl<B: CpcBoard> CpcCore<B> {
    /// Build the owner's fixed board from OS and BASIC firmware.
    ///
    /// # Errors
    /// Returns an error unless firmware is exactly 32 KB.
    pub fn new(firmware: &[u8]) -> Result<Self, String> {
        Self::from_state(AmstradCpc::with_model(firmware, B::MODEL)?)
    }
    /// Bind a v4 hardware payload after validating its physical configuration.
    ///
    /// # Errors
    /// Rejects foreign boards, invalid RAM/ROM sizes, banking, chip or raster state.
    pub fn from_state(mut state: AmstradCpc) -> Result<Self, String> {
        if state.model != B::MODEL
            || state.ram.len() != B::MODEL.ram_bytes()
            || state.ram_config >= 8
            || (!B::MODEL.has_banked_ram() && state.ram_config != 0)
            || state.os_rom.len() != 0x4000
            || state.basic_rom.len() != 0x4000
            || state
                .upper_roms
                .iter()
                .any(|(n, bytes)| *n == 0 || bytes.len() != 0x4000)
            || state.crtc.variant() != Crtc6845Variant::Hd6845s
            || state.crtc_phase >= TSTATES_PER_CRTC_TICK
            || state.framebuffer.len() != (FB_WIDTH * FB_HEIGHT) as usize
            || (!B::MODEL.has_disc()
                && (state.fdc_motor || state.disk(0).is_some() || state.disk(1).is_some()))
        {
            return Err("snapshot hardware does not match this CPC board".into());
        }
        state.cpu_mut().rehydrate_walker_sequence();
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    /// Immutable machine identity.
    pub const fn board_id(&self) -> &'static str {
        B::MACHINE_ID
    }
    /// Advance one instruction through the unchanged CPU and bus path.
    pub fn step_instruction(&mut self) -> u64 {
        emu198x_zilog_z80::Z80Stepper::step_instruction(&mut self.state)
    }
    /// Delegate to this board’s shared hardware.
    pub fn firmware(&self) -> Vec<u8> {
        self.state.firmware()
    }
    /// Delegate to this board’s shared hardware.
    pub fn upper_roms(&self) -> &std::collections::BTreeMap<u8, Vec<u8>> {
        self.state.upper_roms()
    }
    /// Delegate to this board’s shared hardware.
    pub fn cpu_tstates(&self) -> u64 {
        self.state.cpu_tstates()
    }
    /// Delegate to this board’s shared hardware.
    pub fn frame_count(&self) -> u64 {
        self.state.frame_count()
    }
    /// Delegate to this board’s shared hardware.
    pub fn peek(&self, addr: u16) -> u8 {
        self.state.peek(addr)
    }
    /// Delegate to this board’s shared hardware.
    pub fn model(&self) -> CpcModel {
        self.state.model()
    }
    /// Delegate to this board’s shared hardware.
    pub fn ram_config(&self) -> u8 {
        self.state.ram_config()
    }
    /// Delegate to this board’s shared hardware.
    pub fn ram_byte(&self, addr: u16) -> u8 {
        self.state.ram_byte(addr)
    }
    /// Delegate to this board’s shared hardware.
    pub fn ram_byte_at(&self, offset: usize) -> u8 {
        self.state.ram_byte_at(offset)
    }
    /// Delegate to this board’s shared hardware.
    pub fn screen_text(&self) -> Vec<String> {
        self.state.screen_text()
    }
    /// Delegate to this board’s shared hardware.
    pub fn screen_base(&self) -> u16 {
        self.state.screen_base()
    }
    /// Delegate to this board’s shared hardware.
    pub fn z80(&self) -> &Z80 {
        self.state.z80()
    }
    /// Delegate to this board’s shared hardware.
    pub fn z80_mut(&mut self) -> &mut Z80 {
        self.state.z80_mut()
    }
    /// Delegate to this board’s shared hardware.
    pub fn advance_tstates(&mut self, tstates: u32) {
        self.state.advance_tstates(tstates)
    }
    /// Delegate to this board’s shared hardware.
    pub fn gate_array(&self) -> &GateArray {
        self.state.gate_array()
    }
    /// Delegate to this board’s shared hardware.
    pub fn crtc(&self) -> &Crtc6845 {
        self.state.crtc()
    }
    /// Delegate to this board’s shared hardware.
    pub fn psg(&self) -> &Ay3_8912 {
        self.state.psg()
    }
    /// Delegate to this board’s shared hardware.
    pub fn cpu(&self) -> &Z80 {
        self.state.cpu()
    }
    /// Delegate to this board’s shared hardware.
    pub fn cpu_mut(&mut self) -> &mut Z80 {
        self.state.cpu_mut()
    }
    /// Delegate to this board’s shared hardware.
    pub fn ram(&self) -> &[u8] {
        self.state.ram()
    }
    /// Delegate to this board’s shared hardware.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.state.poke(addr, value)
    }
    /// Delegate to this board’s shared hardware.
    pub fn start_io_trace(&mut self) {
        self.state.start_io_trace()
    }
    /// Delegate to this board’s shared hardware.
    pub fn take_io_trace(&mut self) -> Vec<IoEvent> {
        self.state.take_io_trace()
    }
    /// Delegate to this board’s shared hardware.
    pub fn out(&mut self, port: u16, value: u8) {
        self.state.out(port, value)
    }
    /// Delegate to this board’s shared hardware.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.state.take_audio_buffer()
    }
    /// Delegate to this board’s shared hardware.
    pub fn framebuffer(&self) -> &[u32] {
        self.state.framebuffer()
    }
    /// Delegate to this board’s shared hardware.
    pub fn framebuffer_width(&self) -> u32 {
        self.state.framebuffer_width()
    }
    /// Delegate to this board’s shared hardware.
    pub fn framebuffer_height(&self) -> u32 {
        self.state.framebuffer_height()
    }
    /// Delegate to this board’s shared hardware.
    pub fn insert_tape(&mut self, spans: Vec<TapeSpan>) {
        self.state.insert_tape(spans)
    }
    /// Delegate to this board’s shared hardware.
    pub fn fit_upper_rom(&mut self, number: u8, image: &[u8]) -> Result<(), String> {
        self.state.fit_upper_rom(number, image)
    }
    /// Delegate to this board’s shared hardware.
    pub fn disk_motor_on(&self) -> bool {
        self.state.disk_motor_on()
    }
    /// Delegate to this board’s shared hardware.
    pub fn insert_disk(&mut self, drive: usize, image: DiskImage, writable: bool) {
        self.state.insert_disk(drive, image, writable)
    }
    /// Delegate to this board’s shared hardware.
    pub fn eject_disk(&mut self, drive: usize) {
        self.state.eject_disk(drive)
    }
    /// Delegate to this board’s shared hardware.
    pub fn disk(&self, drive: usize) -> Option<&DiskImage> {
        self.state.disk(drive)
    }
    /// Delegate to this board’s shared hardware.
    pub fn fdc(&self) -> &Upd765a {
        self.state.fdc()
    }
    /// Delegate to this board’s shared hardware.
    pub fn tape_motor_on(&self) -> bool {
        self.state.tape_motor_on()
    }
    /// Delegate to this board’s shared hardware.
    pub fn tape(&self) -> &TapePlayer {
        self.state.tape()
    }
    /// Delegate to this board’s shared hardware.
    pub fn press_key(&mut self, row: usize, bit: u8) {
        self.state.press_key(row, bit)
    }
    /// Delegate to this board’s shared hardware.
    pub fn release_key(&mut self, row: usize, bit: u8) {
        self.state.release_key(row, bit)
    }
    /// Delegate to this board’s shared hardware.
    pub fn press_char(&mut self, c: char) -> bool {
        self.state.press_char(c)
    }
    /// Delegate to this board’s shared hardware.
    pub fn release_char(&mut self, c: char) {
        self.state.release_char(c)
    }
    /// Delegate to this board’s shared hardware.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        self.state.run_native_frame()
    }
    /// Delegate to this board’s shared hardware.
    pub fn run_frame(&mut self) -> u64 {
        self.state.run_frame()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestBoard;
    impl CpcBoard for TestBoard {
        const MODEL: CpcModel = CpcModel::Cpc464;
        const MACHINE_ID: &'static str = "test-cpc464";
    }
    fn state() -> AmstradCpc {
        AmstradCpc::new(&[0; 32768]).expect("firmware")
    }
    #[test]
    fn checked_binding_rejects_damaged_memory_chip_and_clock_state() {
        let corruptions: [fn(&mut AmstradCpc); 9] = [
            |s| {
                s.ram.pop();
            },
            |s| s.ram_config = 1,
            |s| s.ram_config = 8,
            |s| {
                s.os_rom.pop();
            },
            |s| {
                s.basic_rom.pop();
            },
            |s| {
                s.upper_roms.insert(0, vec![0; 0x4000]);
            },
            |s| s.crtc.set_variant(Crtc6845Variant::Mc6845),
            |s| s.crtc_phase = 4,
            |s| {
                s.framebuffer.pop();
            },
        ];
        assert!(CpcCore::<TestBoard>::from_state(state()).is_ok());
        for corrupt in corruptions {
            let mut s = state();
            corrupt(&mut s);
            assert!(CpcCore::<TestBoard>::from_state(s).is_err());
        }
    }
}

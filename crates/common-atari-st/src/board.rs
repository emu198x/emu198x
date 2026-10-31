//! Fixed model and master clock over the original v1 engine.
use super::*;
/// Immutable board selection supplied by a physical owner.
pub trait StBoard {
    const MODEL: Model;
    const STANDARD: VideoStandard;
    const MACHINE_ID: &'static str;
}
/// One fixed configuration; monitor and TOS remain host choices.
pub struct StCore<B: StBoard> {
    state: AtariSt,
    board: std::marker::PhantomData<B>,
}
impl<B: StBoard> Serialize for StCore<B> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.state.serialize(s)
    }
}
impl<B: StBoard> StCore<B> {
    /// Power on this board with the attached monitor and supplied TOS.
    ///
    /// # Errors
    /// Rejects TOS images other than 192 or 256 KB.
    pub fn new(monitor: Monitor, tos: Vec<u8>) -> Result<Self, RomError> {
        Ok(Self {
            state: AtariSt::new(B::MODEL, B::STANDARD, monitor, tos)?,
            board: std::marker::PhantomData,
        })
    }
    /// Bind original v1 state to this board with the externally supplied TOS.
    ///
    /// # Errors
    /// Rejects foreign models/clocks, RAM banks, output geometry and invalid TOS.
    pub fn from_state(mut state: AtariSt, tos: Vec<u8>) -> Result<Self, String> {
        let banks = B::MODEL.banks();
        if state.model != B::MODEL
            || state.standard != B::STANDARD
            || state.glue.physical_banks() != [banks.0, banks.1]
            || state.ram.len() != (banks.0 + banks.1) as usize
            || state.ikbd.cpu_hz() != (master_hz(B::STANDARD) / 4) as u32
            || state.mfp_phase >= master_hz(B::STANDARD) / 4
            || state.framebuffer.len()
                != if state.frame_mono {
                    MONO_WIDTH * MONO_HEIGHT
                } else {
                    COLOUR_WIDTH * COLOUR_HEIGHT
                }
        {
            return Err("snapshot hardware does not match this Atari ST board".into());
        }
        state.finish_restore(tos).map_err(|e| e.to_string())?;
        Ok(Self {
            state,
            board: std::marker::PhantomData,
        })
    }
    /// Immutable physical identity.
    pub const fn board_id(&self) -> &'static str {
        B::MACHINE_ID
    }
    /// Observe this board’s live cpu hardware.
    pub fn cpu(&self) -> &Cpu68000 {
        &self.state.cpu
    }
    /// Observe this board’s live glue hardware.
    pub fn glue(&self) -> &GlueMmu {
        &self.state.glue
    }
    /// Observe this board’s live shifter hardware.
    pub fn shifter(&self) -> &Shifter {
        &self.state.shifter
    }
    /// Observe this board’s live mfp hardware.
    pub fn mfp(&self) -> &Mfp68901 {
        &self.state.mfp
    }
    /// Observe this board’s live acia_keyboard hardware.
    pub fn acia_keyboard(&self) -> &Acia6850 {
        &self.state.acia_keyboard
    }
    /// Observe this board’s live acia_midi hardware.
    pub fn acia_midi(&self) -> &Acia6850 {
        &self.state.acia_midi
    }
    /// Observe this board’s live psg hardware.
    pub fn psg(&self) -> &Ay3_8910 {
        &self.state.psg
    }
    /// Observe this board’s live fdc hardware.
    pub fn fdc(&self) -> &Wd1770 {
        &self.state.fdc
    }
    /// Observe this board’s live dma hardware.
    pub fn dma(&self) -> &Dma {
        &self.state.dma
    }
    /// Observe this board’s live ikbd hardware.
    pub fn ikbd(&self) -> &Ikbd {
        &self.state.ikbd
    }
    /// Explicit debug/register access without replacing fixed wiring.
    pub fn cpu_mut(&mut self) -> &mut Cpu68000 {
        &mut self.state.cpu
    }
    /// Explicit debug/register access without replacing fixed wiring.
    pub fn write_glue(&mut self, offset: u16, value: u8) -> bool {
        self.state.glue.write(offset, value)
    }
    /// Explicit debug/register access without replacing fixed wiring.
    pub fn set_memory_config(&mut self, value: u8) {
        self.state.glue.set_config(value)
    }
    /// Explicit debug/register access without replacing fixed wiring.
    pub fn set_resolution(&mut self, value: u8) {
        {
            self.state.shifter.write_resolution(value);
            self.state.glue.set_resolution(value);
        }
    }
    /// Explicit debug/register access without replacing fixed wiring.
    pub fn write_palette(&mut self, index: usize, value: u16) {
        self.state.shifter.write_palette(index, value)
    }
    /// Explicit debug/register access without replacing fixed wiring.
    pub fn select_psg_register(&mut self, value: u8) {
        self.state.psg.select_register(value)
    }
    /// Explicit debug/register access without replacing fixed wiring.
    pub fn write_psg_data(&mut self, value: u8) {
        self.state.psg.write_data(value)
    }
    /// Delegate to this board’s shared hardware.
    pub fn reset(&mut self) {
        self.state.reset()
    }
    /// Delegate to this board’s shared hardware.
    pub fn model(&self) -> Model {
        self.state.model()
    }
    /// Delegate to this board’s shared hardware.
    pub fn standard(&self) -> VideoStandard {
        self.state.standard()
    }
    /// Delegate to this board’s shared hardware.
    pub fn monitor(&self) -> Monitor {
        self.state.monitor()
    }
    /// Delegate to this board’s shared hardware.
    pub fn cpu_hz(&self) -> u64 {
        self.state.cpu_hz()
    }
    /// Delegate to this board’s shared hardware.
    pub fn master_hz(&self) -> u64 {
        self.state.master_hz()
    }
    /// Delegate to this board’s shared hardware.
    pub fn insert_disk(&mut self, drive: usize, disk: Disk) {
        self.state.insert_disk(drive, disk)
    }
    /// Delegate to this board’s shared hardware.
    pub fn eject_disk(&mut self, drive: usize) -> Option<Disk> {
        self.state.eject_disk(drive)
    }
    /// Delegate to this board’s shared hardware.
    pub fn disk(&self, drive: usize) -> Option<&Disk> {
        self.state.disk(drive)
    }
    /// Delegate to this board’s shared hardware.
    pub fn cycles(&self) -> u64 {
        self.state.cycles()
    }
    /// Delegate to this board’s shared hardware.
    pub fn run_frame(&mut self) -> u64 {
        self.state.run_frame()
    }
    /// Delegate to this board’s shared hardware.
    pub fn tick(&mut self) {
        self.state.tick()
    }
    /// Delegate to this board’s shared hardware.
    pub fn framebuffer(&self) -> &[u32] {
        self.state.framebuffer()
    }
    /// Delegate to this board’s shared hardware.
    pub fn frame_width(&self) -> usize {
        self.state.frame_width()
    }
    /// Delegate to this board’s shared hardware.
    pub fn frame_height(&self) -> usize {
        self.state.frame_height()
    }
    /// Delegate to this board’s shared hardware.
    pub fn display_origin(&self) -> (usize, usize) {
        self.state.display_origin()
    }
    /// Delegate to this board’s shared hardware.
    pub fn take_audio(&mut self) -> Vec<f32> {
        self.state.take_audio()
    }
    /// Delegate to this board’s shared hardware.
    pub fn key(&mut self, scancode: u8, pressed: bool) {
        self.state.key(scancode, pressed)
    }
    /// Delegate to this board’s shared hardware.
    pub fn mouse_move(&mut self, dx: i32, dy: i32) {
        self.state.mouse_move(dx, dy)
    }
    /// Delegate to this board’s shared hardware.
    pub fn mouse_buttons(&mut self, left: bool, right: bool) {
        self.state.mouse_buttons(left, right)
    }
    /// Delegate to this board’s shared hardware.
    pub fn mouse_button(&mut self, left: bool, pressed: bool) {
        self.state.mouse_button(left, pressed)
    }
    /// Delegate to this board’s shared hardware.
    pub fn joystick(&mut self, port: usize, stick: Stick) {
        self.state.joystick(port, stick)
    }
    /// Delegate to this board’s shared hardware.
    pub fn peek(&self, address: u32) -> u8 {
        self.state.peek(address)
    }
    /// Delegate to this board’s shared hardware.
    pub fn poke(&mut self, address: u32, value: u8) {
        self.state.poke(address, value)
    }
    /// Delegate to this board’s shared hardware.
    pub fn ram(&self) -> &[u8] {
        self.state.ram()
    }
    /// Delegate to this board’s shared hardware.
    pub fn rom(&self) -> &[u8] {
        self.state.rom()
    }
}

impl<B: StBoard> StCore<B> {
    /// Ask TOS for its line-A base without changing live memory.
    pub fn line_a_base(&self) -> Option<u32> {
        self.state.line_a_base()
    }
    /// Consume the next protocol byte queued by the keyboard controller.
    pub fn take_ikbd_output(&mut self) -> Option<u8> {
        self.state.ikbd.next_output()
    }
}

impl<B: StBoard> StCore<B> {
    /// Read keyboard ACIA status or consume received data.
    pub fn read_keyboard_acia(&mut self, data: bool) -> u8 {
        self.state.acia_keyboard.read(data)
    }
    /// Write keyboard ACIA control or transmit data.
    pub fn write_keyboard_acia(&mut self, data: bool, value: u8) {
        self.state.acia_keyboard.write(data, value)
    }
}

#[cfg(test)]
mod binding_tests {
    use super::*;
    struct TestBoard;
    impl StBoard for TestBoard {
        const MODEL: Model = Model::St520;
        const STANDARD: VideoStandard = VideoStandard::Pal;
        const MACHINE_ID: &'static str = "test-520st-pal";
    }
    fn state() -> AtariSt {
        AtariSt::new(
            Model::St520,
            VideoStandard::Pal,
            Monitor::Colour,
            vec![0; 0x30000],
        )
        .expect("TOS")
    }
    #[test]
    fn binding_checks_physical_banks_even_when_total_ram_matches() {
        assert!(StCore::<TestBoard>::from_state(state(), vec![0; 0x30000]).is_ok());
        let corruptions: [fn(&mut AtariSt); 6] = [
            |s| s.standard = VideoStandard::Ntsc,
            |s| s.glue = GlueMmu::new(0, 524288),
            |s| {
                s.ram.pop();
            },
            |s| {
                s.framebuffer.pop();
            },
            |s| s.ikbd = Ikbd::new(1),
            |s| s.mfp_phase = s.cpu_hz(),
        ];
        for corrupt in corruptions {
            let mut s = state();
            corrupt(&mut s);
            assert!(StCore::<TestBoard>::from_state(s, vec![0; 0x30000]).is_err());
        }
    }
}

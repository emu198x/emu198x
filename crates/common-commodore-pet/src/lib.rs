//! Shared PET board wiring; concrete owners select one physical configuration.

pub mod input;
mod keyboard;
pub mod owner;
mod video;
pub use video::Board;
use video::Video;

pub use input::PetKey;
pub use keyboard::KeyboardState;

pub use emu198x_mos_6502::M6502;
use emu198x_mos_pia_6520::Pia6520;
use emu198x_mos_via_6522::Via6522;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

pub const ACTIVE_WIDTH_40: u32 = 320;
pub const ACTIVE_WIDTH_80: u32 = 640;
pub const ACTIVE_HEIGHT: u32 = 200;

/// Border thickness around the active text display. The PET's
/// monochrome P1 phosphor display always shows black around the
/// green-on-black active region — no programmable border colour.
pub const BORDER_LEFT: u32 = 32;
pub const BORDER_RIGHT: u32 = 32;
pub const BORDER_TOP: u32 = 24;
pub const BORDER_BOTTOM: u32 = 24;

pub const SCREEN_WIDTH_40: u32 = ACTIVE_WIDTH_40 + BORDER_LEFT + BORDER_RIGHT;
pub const SCREEN_WIDTH_80: u32 = ACTIVE_WIDTH_80 + BORDER_LEFT + BORDER_RIGHT;
pub const SCREEN_HEIGHT: u32 = ACTIVE_HEIGHT + BORDER_TOP + BORDER_BOTTOM;

/// Φ2 machine clock: 1 MHz, one tick per [`Pet::tick`].
const CPU_CLOCK_HZ: u64 = 1_000_000;
/// Host audio output rate for the CB2 piezo, matching the runtime's declared
/// `AudioPacket` sample rate.
const AUDIO_SAMPLE_RATE: u64 = 48_000;

/// Commodore PET machine.
#[derive(Serialize, Deserialize)]
pub struct Pet {
    cpu: M6502,
    #[serde(with = "BigArray")]
    ram: [u8; 0x8000],
    #[serde(with = "BigArray")]
    video_ram: [u8; 0x0800],
    basic_rom: Vec<u8>,
    editor_rom: Vec<u8>,
    kernal_rom: Vec<u8>,
    char_rom: Vec<u8>,
    video: Video,
    board: Board,
    pia: Pia6520,
    via: Via6522,
    keyboard: KeyboardState,
    pressed_keys: Vec<PetKey>,
    framebuffer: Vec<u32>,
    screen_chars: u32,
    screen_width_px: u32,
    frame_complete: bool,
    /// Undelivered waveform samples are part of exact restored state.
    audio_buffer: Vec<f32>,
    /// Fractional accumulator converting the 1 MHz Φ2 clock to the 48 kHz
    /// output rate: one sample is emitted each time it crosses
    /// [`CPU_CLOCK_HZ`].
    #[serde(default)]
    audio_accum: u64,
    master_clock: u64,
    frame_count: u64,
}

impl Pet {
    /// Cold-boot with the installed firmware and the selected display profile.
    #[must_use]
    pub fn cold_boot(&self) -> Self {
        Self::new(
            self.kernal_rom.clone(),
            self.basic_rom.clone(),
            self.editor_rom.clone(),
            self.char_rom.clone(),
            self.board,
        )
    }

    /// Construct shared state. Fixed owners validate board-specific ROM sizes.
    pub fn new(
        kernal_rom: Vec<u8>,
        basic_rom: Vec<u8>,
        editor_rom: Vec<u8>,
        char_rom: Vec<u8>,
        board: Board,
    ) -> Self {
        let screen_chars = board.columns();
        let screen_width_px = if screen_chars >= 80 {
            SCREEN_WIDTH_80
        } else {
            SCREEN_WIDTH_40
        };
        let video = Video::new(board);
        // Run the 6502 reset sequence so the first fetch comes from the KERNAL
        // reset vector ($FFFC); without it the CPU powers on at PC=$0000,
        // executes the BRK there, and never cold-starts (the screen sticks on
        // the uninitialised "@" grid).
        let mut cpu = M6502::new();
        cpu.reset();
        Self {
            cpu,
            ram: [0; 0x8000],
            video_ram: [0; 0x0800],
            basic_rom,
            editor_rom,
            kernal_rom,
            char_rom,
            video,
            board,
            pia: Pia6520::new(),
            via: Via6522::new(),
            keyboard: KeyboardState::new(),
            pressed_keys: Vec::new(),
            framebuffer: vec![
                0xFF00_0000;
                (screen_width_px * (board.active_height() + BORDER_TOP + BORDER_BOTTOM))
                    as usize
            ],
            screen_chars,
            screen_width_px,
            frame_complete: false,
            audio_buffer: Vec::new(),
            audio_accum: 0,
            master_clock: 0,
            frame_count: 0,
        }
    }

    /// Run one full frame and return the number of CPU cycles executed.
    /// Bounded frame step; false means the safety budget expired.
    pub fn run_native_frame(&mut self) -> (u64, bool) {
        let start = self.master_clock;
        self.take_frame_complete();
        let mut completed = false;
        for _ in 0..30_000 {
            self.tick();
            if self.take_frame_complete() {
                completed = true;
                break;
            }
        }
        self.frame_count += u64::from(completed);
        (self.master_clock - start, completed)
    }

    pub fn run_frame(&mut self) -> u64 {
        self.run_native_frame().0
    }

    fn tick(&mut self) {
        self.master_clock += 1;
        self.tick_display();
        let sync = self.video.retrace_level();
        self.via.pb_in = if sync { 0xff } else { !0x20 };
        self.via.tick();
        self.tick_audio();
        self.pia.set_cb1(sync);
        self.cpu.irq = self.pia.irq_pending() || self.via.irq;
        self.cpu.tick();
        if self.cpu.rw {
            self.cpu.data_in = self.mem_read(self.cpu.addr);
        } else {
            self.mem_write(self.cpu.addr, self.cpu.data);
        }
    }

    /// Sample the piezo speaker into the audio buffer, downsampling the 1 MHz
    /// Φ2 clock to the 48 kHz output rate with a fractional accumulator.
    ///
    /// The PET's only sound source is a piezo disc on the VIA's CB2 line. BASIC
    /// tones come from the shift register in free-running output mode, which
    /// rotates the SR pattern onto CB2 at the T2 rate — T2 sets the pitch, the
    /// SR byte the timbre — but CB2 driven directly from the PCR clicks the
    /// disc just as well, and both resolve to `cb2_out`.
    fn tick_audio(&mut self) {
        self.audio_accum += AUDIO_SAMPLE_RATE;
        if self.audio_accum >= CPU_CLOCK_HZ {
            self.audio_accum -= CPU_CLOCK_HZ;
            let sample = if self.via.cb2_out { 0.5 } else { -0.5 };
            self.audio_buffer.push(sample);
        }
    }

    /// Drain the piezo waveform accumulated since the last call (the runtime
    /// pushes it into an `AudioPacket` each frame).
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        core::mem::take(&mut self.audio_buffer)
    }

    fn tick_display(&mut self) {
        let beam = self.video.tick();
        self.frame_complete |= beam.frame;
        let pairs = if self.board == Board::Pet8032 { 2 } else { 1 };
        for half in 0..pairs {
            let address = (usize::from(beam.ma) * pairs + half) & self.board.video_mask();
            let code = self.video_ram[address];
            let bank = usize::from(self.via.ca2_out) * 1024;
            let inverse = (code & 0x80 != 0) ^ beam.reverse;
            let glyph = if !beam.display {
                0
            } else if beam.ra < 8 {
                let byte = self
                    .char_rom
                    .get(bank + usize::from(code & 0x7f) * 8 + usize::from(beam.ra))
                    .copied()
                    .unwrap_or(0);
                if inverse { !byte } else { byte }
            } else {
                0
            };
            let y = BORDER_TOP + u32::from(beam.y);
            let x = BORDER_LEFT + (u32::from(beam.x) * pairs as u32 + half as u32) * 8;
            if y >= self.framebuffer_height() - BORDER_BOTTOM {
                continue;
            }
            for pixel in 0..8 {
                if x + pixel >= self.screen_width_px - BORDER_RIGHT {
                    break;
                }
                let on = glyph & (0x80 >> pixel) != 0;
                self.framebuffer[(y * self.screen_width_px + x + pixel) as usize] =
                    if on { 0xff00_ff00 } else { 0xff00_0000 };
            }
        }
    }

    /// Reject forged board metadata and inconsistent allocations before binding.
    pub fn validate(&self, board: Board) -> Result<(), String> {
        let basic_len = if board == Board::Pet2001n32k {
            8192
        } else {
            12288
        };
        if self.keyboard.rows() != self.contacts().rows()
            || self.board != board
            || !self.video.matches(board)
            || self.screen_chars != board.columns()
            || self.screen_width_px
                != if board == Board::Pet8032 {
                    SCREEN_WIDTH_80
                } else {
                    SCREEN_WIDTH_40
                }
            || self.framebuffer.len() != (self.screen_width_px * self.framebuffer_height()) as usize
            || self.kernal_rom.len() != 4096
            || self.basic_rom.len() != basic_len
            || self.editor_rom.len() != 2048
            || self.char_rom.len() != 2048
            || self.audio_accum >= CPU_CLOCK_HZ
            || self.audio_buffer.iter().any(|v| !v.is_finite())
        {
            return Err("PET state does not match its physical board or firmware layout".into());
        }
        Ok(())
    }

    /// The immutable board identity used to validate restored state.
    pub const fn board(&self) -> Board {
        self.board
    }

    fn take_frame_complete(&mut self) -> bool {
        let v = self.frame_complete;
        self.frame_complete = false;
        v
    }

    fn update_keyboard(&mut self) {
        // PIA #1 port A drives the binary row number (0-9); port B reads
        // that row's column lines.
        let row = self.pia.port_a_output() & 0x0F;
        let columns = self.keyboard.read_row(row);
        self.pia.set_port_b_input(columns);
    }

    fn mem_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize],
            0x8000..=0x87FF => self.video_ram[(addr as usize) & self.board.video_mask()],
            0x8800..=0x8FFF => 0xFF,
            0x9000..=0xAFFF => 0xFF,
            0xB000..=0xDFFF => {
                let Some(offset) = addr.checked_sub(self.board.basic_start()) else {
                    return 0xff;
                };
                let offset = usize::from(offset);
                self.basic_rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xE000..=0xE7FF => {
                let offset = (addr - 0xE000) as usize;
                self.editor_rom.get(offset).copied().unwrap_or(0xFF)
            }
            0xE810..=0xE81F => {
                self.update_keyboard();
                self.pia.read((addr & 0x03) as u8)
            }
            0xE840..=0xE84F => self.via.read((addr & 0x0F) as u8),
            0xE880 => self.video.read_status(),
            0xE881 => self.video.read_data(),
            0xE800..=0xEFFF => 0xFF,
            0xF000..=0xFFFF => {
                let offset = (addr - 0xF000) as usize;
                self.kernal_rom.get(offset).copied().unwrap_or(0xFF)
            }
        }
    }

    fn mem_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize] = value,
            0x8000..=0x87FF => self.video_ram[(addr as usize) & self.board.video_mask()] = value,
            0xE810..=0xE81F => self.pia.write((addr & 0x03) as u8, value),
            0xE840..=0xE84F => self.via.write((addr & 0x0F) as u8, value),
            0xE880 => self.video.write_address(value),
            0xE881 => {
                self.video.write_data(value);
                self.framebuffer.resize(
                    (self.screen_width_px * self.framebuffer_height()) as usize,
                    0xff00_0000,
                );
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        &self.framebuffer
    }

    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.screen_width_px
    }

    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.video.height() + BORDER_TOP + BORDER_BOTTOM
    }

    pub fn press_key(&mut self, key: PetKey) {
        if !self.pressed_keys.contains(&key) {
            self.pressed_keys.push(key);
        }
        self.update_contacts();
    }
    pub fn release_key(&mut self, key: PetKey) {
        self.pressed_keys.retain(|held| *held != key);
        self.update_contacts();
    }
    pub fn release_all_keys(&mut self) {
        self.pressed_keys.clear();
        self.update_contacts();
    }
    fn contacts(&self) -> KeyboardState {
        let mut keyboard = KeyboardState::new();
        for &key in &self.pressed_keys {
            let ((row, col), shift) = if self.board == Board::Pet8032 {
                key.business_contact()
            } else {
                (key.matrix(), false)
            };
            keyboard.set_key(row, col, true);
            if shift {
                keyboard.set_key(6, 0, true);
            }
        }
        keyboard
    }

    fn update_contacts(&mut self) {
        self.keyboard = self.contacts();
    }

    #[must_use]
    pub fn peek_memory(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.ram[addr as usize],
            0x8000..=0x87FF => self.video_ram[(addr as usize) & self.board.video_mask()],
            0xB000..=0xDFFF => self
                .basic_rom
                .get(
                    addr.checked_sub(self.board.basic_start())
                        .map_or(usize::MAX, usize::from),
                )
                .copied()
                .unwrap_or(0xFF),
            0xE000..=0xE7FF => self
                .editor_rom
                .get((addr - 0xE000) as usize)
                .copied()
                .unwrap_or(0xFF),
            0xF000..=0xFFFF => self
                .kernal_rom
                .get((addr - 0xF000) as usize)
                .copied()
                .unwrap_or(0xFF),
            _ => 0xFF,
        }
    }

    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }

    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

impl Pet {
    /// Read one byte with no side effects (alias of `peek_memory`).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.peek_memory(addr)
    }

    /// Write one byte through the bus (RAM accepts it; ROM ignores it).
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.mem_write(addr, value);
    }

    /// Run exactly one whole 6502 instruction, returning the clocks it
    /// consumed. A safety cap prevents an unbounded spin.
    pub fn step_instruction(&mut self) -> u64 {
        let mut ticks = 0u64;
        while self.cpu.instruction_complete() && ticks < 4096 {
            self.tick();
            ticks += 1;
        }
        while !self.cpu.instruction_complete() && ticks < 4096 {
            self.tick();
            ticks += 1;
        }
        ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pet() -> Pet {
        let mut kernal = vec![0u8; 0x1000];
        // Reset vector at $FFFC/$FFFD = $F000 — execute NOPs forever.
        kernal[0x0FFC] = 0x00;
        kernal[0x0FFD] = 0xF0;
        for byte in kernal.iter_mut().take(0x0FFC) {
            *byte = 0xEA;
        }
        Pet::new(
            kernal,
            vec![0u8; 0x2000],
            vec![0u8; 0x0800],
            vec![0u8; 0x1000],
            Board::Pet4032,
        )
    }

    /// VIA register addresses on the PET's $E840 block.
    const VIA_SR: u16 = 0xE84A;
    const VIA_ACR: u16 = 0xE84B;
    const VIA_PCR: u16 = 0xE84C;
    const VIA_T2L_L: u16 = 0xE848;

    #[test]
    fn the_shift_register_tone_reaches_the_piezo() {
        // The PET's BASIC tone path: shift register in free-running output mode
        // (ACR bits 4-2 = 100), clocked at the T2 rate, rotating an SR pattern
        // onto CB2. T2 sets the pitch, the SR byte the timbre.
        let mut sys = make_pet();
        sys.mem_write(VIA_T2L_L, 100);
        sys.mem_write(VIA_ACR, 0b0001_0000);
        sys.mem_write(VIA_SR, 0xF0);
        sys.run_frame();

        let audio = sys.take_audio_buffer();
        assert!(!audio.is_empty(), "a frame yields audio samples");
        assert!(
            audio.iter().any(|&s| s > 0.0),
            "piezo-high samples are present"
        );
        assert!(
            audio.iter().any(|&s| s < 0.0),
            "piezo-low samples are present"
        );
    }

    #[test]
    fn a_pcr_driven_cb2_clicks_the_piezo() {
        // CB2 driven straight from the PCR, no shift register: manual-output
        // low (110) then manual-output high (111). Type-in programs click the
        // disc this way.
        let mut sys = make_pet();
        sys.mem_write(VIA_PCR, 0b1100_0000);
        sys.run_frame();
        let low = sys.take_audio_buffer();
        assert!(
            low.iter().all(|&s| s < 0.0),
            "manual-output low holds the piezo down"
        );

        sys.mem_write(VIA_PCR, 0b1110_0000);
        sys.run_frame();
        let high = sys.take_audio_buffer();
        assert!(
            high.iter().all(|&s| s > 0.0),
            "manual-output high holds the piezo up"
        );
    }

    #[test]
    fn a_silent_machine_emits_a_constant_level() {
        let mut sys = make_pet();
        sys.run_frame();
        let audio = sys.take_audio_buffer();
        assert!(!audio.is_empty(), "samples are emitted even when silent");
        assert!(
            audio.iter().all(|&s| (s - audio[0]).abs() < f32::EPSILON),
            "an untouched CB2 produces no waveform"
        );
    }

    #[test]
    fn a_frame_yields_close_to_the_declared_sample_rate() {
        let mut sys = make_pet();
        let cycles = sys.run_frame();
        let expected = cycles * AUDIO_SAMPLE_RATE / CPU_CLOCK_HZ;
        let got = sys.take_audio_buffer().len() as u64;
        assert!(
            got.abs_diff(expected) <= 1,
            "{got} samples for {cycles} cycles, expected about {expected}"
        );
    }

    #[test]
    fn the_audio_buffer_drains() {
        let mut sys = make_pet();
        sys.run_frame();
        assert!(!sys.take_audio_buffer().is_empty());
        assert!(
            sys.take_audio_buffer().is_empty(),
            "a second take returns nothing without another frame"
        );
    }

    #[test]
    fn frame_advances_frame_count() {
        let mut pet = make_pet();
        let _ = pet.run_frame();
        assert_eq!(pet.frame_count(), 1);
    }

    #[test]
    fn framebuffer_correct_size_40_col() {
        let pet = make_pet();
        assert_eq!(pet.framebuffer_width(), SCREEN_WIDTH_40);
        assert_eq!(pet.framebuffer_height(), SCREEN_HEIGHT);
        assert_eq!(
            pet.framebuffer().len(),
            (SCREEN_WIDTH_40 * SCREEN_HEIGHT) as usize
        );
    }

    #[test]
    fn framebuffer_correct_size_80_col() {
        let pet = Pet::new(
            vec![0u8; 0x1000],
            vec![0u8; 0x2000],
            vec![0u8; 0x0800],
            vec![0u8; 0x1000],
            Board::Pet8032,
        );
        assert_eq!(pet.framebuffer_width(), SCREEN_WIDTH_80);
    }

    #[test]
    fn ram_round_trips() {
        let mut pet = make_pet();
        pet.mem_write(0x0100, 0x55);
        assert_eq!(pet.mem_read(0x0100), 0x55);
    }

    #[test]
    fn video_ram_round_trips() {
        let mut pet = make_pet();
        pet.mem_write(0x8000, 0xAA);
        assert_eq!(pet.mem_read(0x8000), 0xAA);
    }

    #[test]
    fn rom_writes_ignored() {
        let mut pet = make_pet();
        let before = pet.mem_read(0xF000);
        pet.mem_write(0xF000, 0xFF);
        assert_eq!(pet.mem_read(0xF000), before);
    }

    #[test]
    fn snapshot_round_trips_live_state() {
        let mut pet = make_pet();
        pet.run_frame();
        // PET RAM is $0000-$7FFF; poke a low address and confirm via the read
        // path that it took.
        pet.poke(0x0400, 0x5A);
        assert_eq!(pet.mem_read(0x0400), 0x5A);

        pet.run_frame();
        let first = postcard::to_allocvec(&pet).expect("encode first");
        pet.run_frame();
        let second = postcard::to_allocvec(&pet).expect("encode second");
        assert_ne!(
            first, second,
            "advancing a frame must change the serialised state"
        );

        let restored: Pet = postcard::from_bytes(&first).expect("decode first");
        let reserialised = postcard::to_allocvec(&restored).expect("re-encode restored");
        assert_eq!(
            first, reserialised,
            "restoring then re-serialising must be byte-identical"
        );
    }

    #[test]
    fn frame_matches_the_901498_01_editor_in_vice() {
        let mut pet = make_pet();
        pet.run_frame();
        assert_eq!(pet.run_frame(), 20_000);
    }
    #[test]
    fn paired_fetches_do_not_double_the_crtc_clock() {
        let mut pet = Pet::new(
            vec![0; 4096],
            vec![0; 12288],
            vec![0; 2048],
            vec![0; 2048],
            Board::Pet8032,
        );
        pet.run_frame();
        assert_eq!(pet.run_frame(), 19_992);
    }
}

#[cfg(test)]
mod hardware_tests;

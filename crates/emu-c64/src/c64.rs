//! Top-level C64 system.
//!
//! The master clock ticks at CPU cycle rate (985,248 Hz PAL). All components
//! tick every master clock tick. One frame = 312 lines × 63 cycles = 19,656
//! CPU cycles.
//!
//! # Tick loop
//!
//! Each tick:
//! 1. VIC-II: advance beam, render 8 pixels, detect badline
//! 2. Check VIC-II raster IRQ → CPU IRQ
//! 3. CPU: tick if not stalled by VIC-II badline
//! 4. CIA1: tick timer, check IRQ → CPU IRQ
//! 5. CIA2: tick timer, check NMI → CPU NMI (edge-triggered)

#![allow(clippy::cast_possible_truncation)]

use emu_core::{Bus, Cpu, Observable, Tickable, Value};
use mos_6502::Mos6502;

use crate::bus::C64Bus;
use crate::config::{C64Config, C64Model};
use crate::d64::D64;
use crate::drive1541::Drive1541;
use crate::iec::IecBus;
use crate::input::{C64Key, InputQueue};
use crate::memory::C64Memory;
use crate::tape::C64TapeDeck;

/// Cycles per frame (PAL): 312 lines × 63 cycles.
#[cfg(test)]
const CYCLES_PER_FRAME: u64 = 312 * 63;

/// Kernal LOAD entry: $FFD5 jumps to $F49E (STX $C3; STY $C4; JMP ($0330)).
/// We trap here when device == 1 (datasette) to deliver tape blocks directly.
const TAPE_LOAD_ADDR: u16 = 0xF49E;

/// C64 system.
pub struct C64 {
    cpu: Mos6502,
    bus: C64Bus,
    /// Master clock: counts CPU cycles.
    master_clock: u64,
    /// Completed frame counter.
    frame_count: u64,
    /// Timed input event queue.
    input_queue: InputQueue,
    /// Previous CIA2 IRQ active state (for NMI edge detection).
    cia2_nmi_prev: bool,
    /// Virtual tape deck for TAP file loading.
    tape: C64TapeDeck,
    /// 1541 floppy drive (present only if drive ROM is provided).
    drive: Option<Drive1541>,
    /// IEC serial bus connecting C64 to the drive.
    iec: IecBus,
}

impl C64 {
    /// Create a new C64 from the given configuration.
    ///
    /// # Panics
    ///
    /// Panics if the model is not PAL (only PAL supported in v1).
    #[must_use]
    pub fn new(config: &C64Config) -> Self {
        assert!(
            config.model == C64Model::C64Pal,
            "Only PAL model is supported in v1"
        );

        let memory = C64Memory::new(&config.kernal_rom, &config.basic_rom, &config.char_rom);
        let mut bus = C64Bus::new(memory);

        // Set up CIA1 for keyboard scanning: port A = output, port B = input
        bus.cia1.write(0x02, 0xFF); // DDR A: all output
        bus.cia1.write(0x03, 0x00); // DDR B: all input
        bus.cia1.write(0x00, 0xFF); // Port A: all columns deselected

        // Set up CIA2 for default VIC bank (bank 0)
        bus.cia2.write(0x02, 0x03); // DDR A: bits 0-1 output
        bus.cia2.write(0x00, 0x03); // Port A: %11 → bank 0 (inverted: !%11 & 3 = 0)
        bus.update_vic_bank();

        // Create the CPU
        let mut cpu = Mos6502::new();

        // Read reset vector from Kernal ROM at $FFFC-$FFFD
        let reset_lo = bus.read(0xFFFC).data;
        let reset_hi = bus.read(0xFFFD).data;
        cpu.regs.pc = u16::from(reset_lo) | (u16::from(reset_hi) << 8);

        // Create 1541 drive if ROM is provided
        let drive = config
            .drive_rom
            .as_ref()
            .map(|rom| Drive1541::new(rom.clone()));

        Self {
            cpu,
            bus,
            master_clock: 0,
            frame_count: 0,
            input_queue: InputQueue::new(),
            cia2_nmi_prev: false,
            tape: C64TapeDeck::new(),
            drive,
            iec: IecBus::new(),
        }
    }

    /// Run one complete frame (until VIC-II signals frame complete).
    ///
    /// Processes any pending input queue events at the start of the frame,
    /// then ticks the master clock until VIC-II signals frame complete.
    ///
    /// Returns the number of CPU cycles executed during the frame.
    pub fn run_frame(&mut self) -> u64 {
        self.input_queue
            .process(self.frame_count, &mut self.bus.keyboard);
        self.frame_count += 1;

        let start_clock = self.master_clock;

        loop {
            self.tick();
            if self.bus.vic.take_frame_complete() {
                break;
            }
        }

        self.master_clock - start_clock
    }

    /// Reference to the framebuffer (ARGB32).
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.bus.vic.framebuffer()
    }

    /// Framebuffer width in pixels.
    #[must_use]
    pub fn framebuffer_width(&self) -> u32 {
        self.bus.vic.framebuffer_width()
    }

    /// Framebuffer height in pixels.
    #[must_use]
    pub fn framebuffer_height(&self) -> u32 {
        self.bus.vic.framebuffer_height()
    }

    /// Reference to the CPU.
    #[must_use]
    pub fn cpu(&self) -> &Mos6502 {
        &self.cpu
    }

    /// Mutable reference to the CPU.
    pub fn cpu_mut(&mut self) -> &mut Mos6502 {
        &mut self.cpu
    }

    /// Reference to the bus.
    #[must_use]
    pub fn bus(&self) -> &C64Bus {
        &self.bus
    }

    /// Mutable reference to the bus.
    pub fn bus_mut(&mut self) -> &mut C64Bus {
        &mut self.bus
    }

    /// Master clock tick count (CPU cycles).
    #[must_use]
    pub fn master_clock(&self) -> u64 {
        self.master_clock
    }

    /// Completed frame count.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Mutable reference to the timed input queue.
    pub fn input_queue(&mut self) -> &mut InputQueue {
        &mut self.input_queue
    }

    /// Press a key immediately.
    pub fn press_key(&mut self, key: C64Key) {
        let (row, col) = key.matrix();
        self.bus.keyboard.set_key(row, col, true);
    }

    /// Release a key.
    pub fn release_key(&mut self, key: C64Key) {
        let (row, col) = key.matrix();
        self.bus.keyboard.set_key(row, col, false);
    }

    /// Release all keys.
    pub fn release_all_keys(&mut self) {
        self.bus.keyboard.release_all();
    }

    /// Take the SID audio output buffer (drains it).
    ///
    /// Returns mono f32 samples in the range -1.0 to 1.0, at 48 kHz.
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.bus.sid.take_buffer()
    }

    /// Number of audio samples pending in the SID buffer.
    #[must_use]
    pub fn audio_buffer_len(&self) -> usize {
        self.bus.sid.buffer_len()
    }

    /// Load a D64 disk image into the 1541 drive.
    ///
    /// Requires a drive ROM to have been provided in the config.
    /// Returns an error if no drive is present or the D64 is invalid.
    pub fn load_d64(&mut self, data: &[u8]) -> Result<(), String> {
        let drive = self
            .drive
            .as_mut()
            .ok_or_else(|| "No 1541 drive (drive ROM not provided)".to_string())?;
        let d64 = D64::from_bytes(data)?;
        drive.insert_disk(d64);
        Ok(())
    }

    /// Eject the D64 disk from the drive.
    pub fn eject_d64(&mut self) {
        if let Some(ref mut drive) = self.drive {
            drive.eject_disk();
        }
    }

    /// Reference to the 1541 drive (if present).
    #[must_use]
    pub fn drive(&self) -> Option<&Drive1541> {
        self.drive.as_ref()
    }

    /// Load a PRG file into memory.
    pub fn load_prg(&mut self, data: &[u8]) -> Result<u16, String> {
        crate::prg::load_prg(&mut self.bus.memory, data)
    }

    /// Load a CRT cartridge file.
    ///
    /// Parses the CRT, inserts it into the memory subsystem, and re-reads
    /// the reset vector (the cartridge may provide its own kernal at $E000).
    /// Returns the cartridge name from the CRT header.
    pub fn load_crt(&mut self, data: &[u8]) -> Result<String, String> {
        let cart = crate::cartridge::parse_crt(data)?;
        let name = crate::cartridge::crt_name(data);
        self.bus.memory.cartridge = Some(cart);
        // Re-read reset vector — cartridge may override $FFFC/$FFFD
        let lo = self.bus.read(0xFFFC).data;
        let hi = self.bus.read(0xFFFD).data;
        self.cpu.regs.pc = u16::from(lo) | (u16::from(hi) << 8);
        Ok(name)
    }

    /// Load a C64 TAP tape file.
    ///
    /// Parses the TAP pulse data into logical blocks and inserts them
    /// into the virtual tape deck. Returns the number of decoded blocks.
    pub fn load_tap(&mut self, data: &[u8]) -> Result<usize, String> {
        let tap = crate::tap::C64TapFile::parse(data)?;
        let count = tap.blocks.len();
        self.tape.insert(tap);
        Ok(count)
    }

    /// Reference to the tape deck.
    #[must_use]
    pub fn tape(&self) -> &C64TapeDeck {
        &self.tape
    }

    /// Mutable reference to the tape deck.
    pub fn tape_mut(&mut self) -> &mut C64TapeDeck {
        &mut self.tape
    }

    /// Check for and handle the ROM tape-loading trap.
    ///
    /// The kernal LOAD entry at $FFD5 jumps to $F49E. When the CPU reaches
    /// $F49E and device == 1 (datasette), we intercept and copy the next
    /// tape block directly into memory instead of emulating the cassette
    /// signal timing.
    ///
    /// Kernal register conventions on entry to LOAD ($FFD5):
    ///   A  = 0 (LOAD) or 1 (VERIFY)
    ///   X  = start address low byte
    ///   Y  = start address high byte
    ///   $BA = device number (1 = tape, 8 = disk)
    ///   $B9 = secondary address (0 = use header address)
    fn check_tape_trap(&mut self) {
        if self.cpu.regs.pc != TAPE_LOAD_ADDR
            || !self.cpu.is_instruction_complete()
            || !self.tape.is_loaded()
        {
            return;
        }

        // Check device number at zero-page $BA
        let device = self.bus.memory.ram_read(0x00BA);
        if device != 1 {
            return; // Not tape — let normal ROM run
        }

        let Some(block) = self.tape.next_block() else {
            return; // No more blocks — let ROM routine run (will time out)
        };

        // Secondary address at $B9: 0 = use header address, non-zero = use X/Y
        let secondary = self.bus.memory.ram_read(0x00B9);
        let load_addr = if secondary == 0 {
            block.start_address
        } else {
            u16::from(self.cpu.regs.x) | (u16::from(self.cpu.regs.y) << 8)
        };

        // A=0 means LOAD, A=1 means VERIFY
        let is_load = self.cpu.regs.a == 0;

        if is_load {
            // Copy block data into RAM
            for (i, &byte) in block.data.iter().enumerate() {
                self.bus
                    .memory
                    .ram_write(load_addr.wrapping_add(i as u16), byte);
            }
        }

        // Set end address in X/Y (kernal convention for LOAD return)
        let end_addr = load_addr.wrapping_add(block.data.len() as u16);
        self.cpu.regs.x = end_addr as u8;
        self.cpu.regs.y = (end_addr >> 8) as u8;

        // Clear carry (success) and clear status byte at $90
        self.cpu.regs.p.0 &= !0x01; // Clear carry
        self.bus.memory.ram_write(0x0090, 0x00); // Clear I/O status

        // Also store end address at $AE/$AF (kernal convention)
        self.bus.memory.ram_write(0x00AE, end_addr as u8);
        self.bus.memory.ram_write(0x00AF, (end_addr >> 8) as u8);

        // Return to caller by popping the return address from the stack
        self.pop_ret_6502();
    }

    /// Pop the return address from the 6502 stack and redirect the CPU.
    ///
    /// The 6502 JSR pushes PC-1 (address of the last byte of the JSR
    /// instruction). RTS pops and adds 1. We replicate that here.
    fn pop_ret_6502(&mut self) {
        let sp = self.cpu.regs.s;
        let lo = self.bus.memory.ram_read(0x0100 | u16::from(sp.wrapping_add(1)));
        let hi = self.bus.memory.ram_read(0x0100 | u16::from(sp.wrapping_add(2)));
        self.cpu.regs.s = sp.wrapping_add(2);
        // RTS adds 1 to the popped address
        let ret_addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(1);
        self.cpu.force_pc(ret_addr);
    }
}

impl Tickable for C64 {
    fn tick(&mut self) {
        self.master_clock += 1;

        // 1. VIC-II: advance beam, render 8 pixels, detect badline
        let cpu_stalled = self.bus.vic.tick(&self.bus.memory);

        // 2. Check VIC-II raster IRQ → CPU IRQ
        if self.bus.vic.irq_active() {
            self.cpu.interrupt();
        }

        // 3. CPU: tick if not stalled by VIC-II badline
        if !cpu_stalled {
            self.cpu.tick(&mut self.bus);
            // Check for tape loading trap after each CPU tick
            self.check_tape_trap();
        }

        // 4. CIA1: tick timer, check IRQ → CPU IRQ
        self.bus.cia1.tick();
        if self.bus.cia1.irq_active() {
            self.cpu.interrupt();
        }

        // 5. CIA2: tick timer, check NMI → CPU NMI (edge-triggered)
        self.bus.cia2.tick();
        let cia2_nmi_now = self.bus.cia2.irq_active();
        if cia2_nmi_now && !self.cia2_nmi_prev {
            self.cpu.nmi();
        }
        self.cia2_nmi_prev = cia2_nmi_now;

        // 6. SID: tick oscillators, envelopes, filter, and downsample
        self.bus.sid.tick();

        // 7. IEC bus + 1541 drive: read CIA2 output, tick drive, feed back
        if let Some(ref mut drive) = self.drive {
            // CIA2 port A output → IEC bus (bit=1 means pull low)
            let pa = self.bus.cia2.port_a_output();
            self.iec.set_c64_atn(pa & 0x08 != 0);
            self.iec.set_c64_clk(pa & 0x10 != 0);
            self.iec.set_c64_data(pa & 0x20 != 0);

            // Tick the drive (reads/writes IEC bus)
            drive.tick(&mut self.iec);

            // Feed IEC bus state back into CIA2 external_a bits 6-7.
            // Bit 6 = CLK IN (0 = line low), Bit 7 = DATA IN (0 = line low).
            // The bus methods return true when high, but CIA2 reads
            // inverted sense: bit=0 when line is low.
            self.bus.cia2.external_a = (self.bus.cia2.external_a & 0x3F)
                | if self.iec.clk() { 0x40 } else { 0x00 }
                | if self.iec.data() { 0x80 } else { 0x00 };
        }
    }
}

impl Observable for C64 {
    fn query(&self, path: &str) -> Option<Value> {
        if let Some(rest) = path.strip_prefix("cpu.") {
            self.cpu.query(rest)
        } else if let Some(rest) = path.strip_prefix("vic.") {
            match rest {
                "line" => Some(self.bus.vic.raster_line().into()),
                "cycle" => Some(self.bus.vic.raster_cycle().into()),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("cia1.") {
            match rest {
                "timer_a" => Some(self.bus.cia1.timer_a().into()),
                "timer_b" => Some(self.bus.cia1.timer_b().into()),
                "icr_status" => Some(self.bus.cia1.icr_status().into()),
                "icr_mask" => Some(self.bus.cia1.icr_mask().into()),
                "cra" => Some(self.bus.cia1.cra().into()),
                "crb" => Some(self.bus.cia1.crb().into()),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("cia2.") {
            match rest {
                "timer_a" => Some(self.bus.cia2.timer_a().into()),
                "timer_b" => Some(self.bus.cia2.timer_b().into()),
                "icr_status" => Some(self.bus.cia2.icr_status().into()),
                "icr_mask" => Some(self.bus.cia2.icr_mask().into()),
                "cra" => Some(self.bus.cia2.cra().into()),
                "crb" => Some(self.bus.cia2.crb().into()),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("sid.") {
            match rest {
                "volume" => Some(Value::U8(self.bus.sid.volume)),
                "voice3_off" => Some(Value::Bool(self.bus.sid.voice3_off)),
                "voice0.freq" => Some(self.bus.sid.voices[0].frequency.into()),
                "voice0.pw" => Some(self.bus.sid.voices[0].pulse_width.into()),
                "voice0.control" => Some(Value::U8(self.bus.sid.voices[0].control)),
                "voice0.envelope" => Some(Value::U8(self.bus.sid.envelopes[0].level)),
                "voice1.freq" => Some(self.bus.sid.voices[1].frequency.into()),
                "voice1.pw" => Some(self.bus.sid.voices[1].pulse_width.into()),
                "voice1.control" => Some(Value::U8(self.bus.sid.voices[1].control)),
                "voice1.envelope" => Some(Value::U8(self.bus.sid.envelopes[1].level)),
                "voice2.freq" => Some(self.bus.sid.voices[2].frequency.into()),
                "voice2.pw" => Some(self.bus.sid.voices[2].pulse_width.into()),
                "voice2.control" => Some(Value::U8(self.bus.sid.voices[2].control)),
                "voice2.envelope" => Some(Value::U8(self.bus.sid.envelopes[2].level)),
                "filter.cutoff" => Some(self.bus.sid.filter.cutoff.into()),
                "filter.resonance" => Some(Value::U8(self.bus.sid.filter.resonance)),
                "filter.mode" => Some(Value::U8(self.bus.sid.filter.mode)),
                "filter.routing" => Some(Value::U8(self.bus.sid.filter.routing)),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("drive.") {
            match rest {
                "track" => self.drive.as_ref().map(|d| Value::U8(d.track())),
                "motor" => self.drive.as_ref().map(|d| Value::Bool(d.motor_on())),
                "led" => self.drive.as_ref().map(|d| Value::Bool(d.led_on())),
                "has_disk" => self.drive.as_ref().map(|d| Value::Bool(d.has_disk())),
                _ => None,
            }
        } else if let Some(rest) = path.strip_prefix("memory.") {
            let addr =
                if let Some(hex) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
                    u16::from_str_radix(hex, 16).ok()
                } else if let Some(hex) = rest.strip_prefix('$') {
                    u16::from_str_radix(hex, 16).ok()
                } else {
                    rest.parse().ok()
                };
            addr.map(|a| Value::U8(self.bus.memory.peek(a)))
        } else {
            match path {
                "master_clock" => Some(self.master_clock.into()),
                "frame_count" => Some(self.frame_count.into()),
                _ => self.cpu.query(path),
            }
        }
    }

    fn query_paths(&self) -> &'static [&'static str] {
        &[
            "cpu.<6502_paths>",
            "sid.volume",
            "sid.voice3_off",
            "sid.voice{0,1,2}.freq",
            "sid.voice{0,1,2}.pw",
            "sid.voice{0,1,2}.control",
            "sid.voice{0,1,2}.envelope",
            "sid.filter.cutoff",
            "sid.filter.resonance",
            "sid.filter.mode",
            "sid.filter.routing",
            "vic.line",
            "vic.cycle",
            "cia1.timer_a",
            "cia1.timer_b",
            "cia1.icr_status",
            "cia1.icr_mask",
            "cia1.cra",
            "cia1.crb",
            "cia2.timer_a",
            "cia2.timer_b",
            "cia2.icr_status",
            "cia2.icr_mask",
            "cia2.cra",
            "cia2.crb",
            "drive.track",
            "drive.motor",
            "drive.led",
            "drive.has_disk",
            "memory.<address>",
            "master_clock",
            "frame_count",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vic;

    fn make_c64() -> C64 {
        // Minimal ROMs: Kernal with a reset vector pointing to a HALT-like loop
        let mut kernal = vec![0xEA; 8192]; // NOP sled
        // Reset vector at $FFFC-$FFFD (offset $1FFC-$1FFD in Kernal ROM)
        // Point to $E000 (start of Kernal)
        kernal[0x1FFC] = 0x00; // Low byte
        kernal[0x1FFD] = 0xE0; // High byte

        let basic = vec![0; 8192];
        let chargen = vec![0; 4096];

        C64::new(&C64Config {
            model: C64Model::C64Pal,
            kernal_rom: kernal,
            basic_rom: basic,
            char_rom: chargen,
            drive_rom: None,
        })
    }

    #[test]
    fn master_clock_advances() {
        let mut c64 = make_c64();
        assert_eq!(c64.master_clock(), 0);
        c64.tick();
        assert_eq!(c64.master_clock(), 1);
    }

    #[test]
    fn run_frame_returns_cycle_count() {
        let mut c64 = make_c64();
        let cycles = c64.run_frame();
        // Should be close to CYCLES_PER_FRAME (may vary slightly due to
        // instruction boundaries and badlines)
        assert!(
            cycles >= CYCLES_PER_FRAME - 100 && cycles <= CYCLES_PER_FRAME + 100,
            "Expected ~{CYCLES_PER_FRAME} cycles, got {cycles}"
        );
    }

    #[test]
    fn framebuffer_correct_size() {
        let c64 = make_c64();
        assert_eq!(c64.framebuffer_width(), vic::FB_WIDTH);
        assert_eq!(c64.framebuffer_height(), vic::FB_HEIGHT);
        assert_eq!(
            c64.framebuffer().len(),
            vic::FB_WIDTH as usize * vic::FB_HEIGHT as usize
        );
    }

    #[test]
    fn observable_cpu_pc() {
        let c64 = make_c64();
        let pc = c64.query("cpu.pc");
        assert_eq!(pc, Some(Value::U16(0xE000)));
    }

    #[test]
    fn observable_memory() {
        let mut c64 = make_c64();
        c64.bus_mut().memory.ram_write(0x8000, 0xAB);
        assert_eq!(c64.query("memory.0x8000"), Some(Value::U8(0xAB)));
    }

    #[test]
    fn tape_trap_not_triggered_without_tape() {
        let mut c64 = make_c64();
        // Set PC to trap address and mark instruction complete
        c64.cpu.force_pc(TAPE_LOAD_ADDR);
        // Set device to tape
        c64.bus.memory.ram_write(0x00BA, 1);
        // No tape loaded — trap should be a no-op
        assert!(!c64.tape.is_loaded());
        c64.check_tape_trap();
        // PC unchanged (trap didn't fire)
        assert_eq!(c64.cpu.regs.pc, TAPE_LOAD_ADDR);
    }

    #[test]
    fn tape_trap_ignores_non_tape_device() {
        use crate::tap::{C64TapBlock, C64TapFile};

        let mut c64 = make_c64();
        // Insert a tape with one block
        c64.tape.insert(C64TapFile {
            blocks: vec![C64TapBlock {
                file_type: 1,
                start_address: 0x0801,
                end_address: 0x0803,
                filename: "TEST".to_string(),
                data: vec![1, 2],
            }],
        });
        // Set PC to trap address
        c64.cpu.force_pc(TAPE_LOAD_ADDR);
        // Set device to disk (8), not tape (1)
        c64.bus.memory.ram_write(0x00BA, 8);
        c64.check_tape_trap();
        // PC unchanged — trap didn't fire for non-tape device
        assert_eq!(c64.cpu.regs.pc, TAPE_LOAD_ADDR);
    }

    #[test]
    fn tape_trap_loads_block() {
        use crate::tap::{C64TapBlock, C64TapFile};

        let mut c64 = make_c64();

        // Push a fake return address onto the stack (simulating JSR)
        // JSR pushes PC-1, so for return address $E010, push $E00F
        c64.cpu.regs.s = 0xFD;
        c64.bus.memory.ram_write(0x01FE, 0x0F); // Low byte of $E00F
        c64.bus.memory.ram_write(0x01FF, 0xE0); // High byte of $E00F

        // Insert a tape
        c64.tape.insert(C64TapFile {
            blocks: vec![C64TapBlock {
                file_type: 1,
                start_address: 0x0801,
                end_address: 0x0804,
                filename: "HELLO".to_string(),
                data: vec![0xAA, 0xBB, 0xCC],
            }],
        });

        // Set up the trap conditions
        c64.cpu.force_pc(TAPE_LOAD_ADDR);
        c64.cpu.regs.a = 0; // LOAD (not VERIFY)
        c64.bus.memory.ram_write(0x00BA, 1); // Device = tape
        c64.bus.memory.ram_write(0x00B9, 0); // Secondary = 0 (use header address)

        c64.check_tape_trap();

        // Data should be in RAM at $0801
        assert_eq!(c64.bus.memory.ram_read(0x0801), 0xAA);
        assert_eq!(c64.bus.memory.ram_read(0x0802), 0xBB);
        assert_eq!(c64.bus.memory.ram_read(0x0803), 0xCC);

        // PC should be at the return address ($E00F + 1 = $E010)
        assert_eq!(c64.cpu.regs.pc, 0xE010);

        // End address in X/Y
        assert_eq!(c64.cpu.regs.x, 0x04); // Low byte of $0804
        assert_eq!(c64.cpu.regs.y, 0x08); // High byte of $0804

        // Carry should be clear (success)
        assert_eq!(c64.cpu.regs.p.0 & 0x01, 0);

        // Status byte at $90 should be clear
        assert_eq!(c64.bus.memory.ram_read(0x0090), 0x00);
    }
}

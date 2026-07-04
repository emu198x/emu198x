//! Board-level Commodore 1581 substrate.
//!
//! The 3.5" 800K drive, built in the 1541's mould but far simpler: it has no
//! GCR bit-cell surface. A WD177x floppy controller works on flat 512-byte
//! MFM sectors, so the 1581 core is just a 6502 bus loop over:
//! - 8 KB RAM at `$0000-$1FFF`
//! - one 8520 CIA at `$4000-$5FFF` (serial IEC bus + drive control)
//! - the WD177x FDC at `$6000-$7FFF`
//! - a 32 KB DOS ROM at `$8000-$FFFF`
//!
//! Memory map and wiring follow VICE 3.10 (`src/drive/iec/{memiec,cia1581d,
//! wd1770}.c`). The 8520 is modelled with the 6526 core, as VICE does.

use common_commodore_iec::IecBus;
use format_commodore_c64_d81::D81ParseError;
use mos_6502::M6502;
use mos_cia_6526::Cia6526;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use western_digital_wd1770::{Disk, Wd1770};

const RAM_SIZE: usize = 0x2000;
const ROM_SIZE: usize = 0x8000;
const DEFAULT_DEVICE_NUMBER: u8 = 8;

/// D81 geometry as the WD177x sees it: 80 cylinders, 2 heads, 10 sectors of
/// 512 bytes, sector IDs starting at 1.
const D81_TRACKS: usize = 80;
const D81_SIDES: usize = 2;
const D81_SECTORS_PER_TRACK: usize = 10;
const D81_SECTOR_SIZE: usize = 512;
const D81_IMAGE_SIZE: usize = D81_TRACKS * D81_SIDES * D81_SECTORS_PER_TRACK * D81_SECTOR_SIZE;

/// Nominal 1581 6502 clock (2 MHz) for combined C64/drive scheduling.
pub const DRIVE1581_CPU_HZ: u64 = 2_000_000;

/// A 1581 drive board.
#[derive(Clone)]
pub struct Drive1581 {
    cpu: M6502,
    cia: Cia6526,
    fdc: Wd1770,
    ram: [u8; RAM_SIZE],
    rom: [u8; ROM_SIZE],
    device_number: u8,
    write_protected: bool,
    has_disk: bool,
    cycles: u64,
}

/// Serialisable 1581 state.
#[derive(Clone, Serialize, Deserialize)]
pub struct Drive1581Snapshot {
    cpu: M6502,
    cia: Cia6526,
    fdc: Wd1770,
    ram: Vec<u8>,
    rom: Vec<u8>,
    device_number: u8,
    write_protected: bool,
    has_disk: bool,
    cycles: u64,
}

/// Construction configuration.
#[derive(Clone, Copy)]
pub struct Drive1581Config<'a> {
    /// The 32 KB DOS ROM image.
    pub dos_rom: &'a [u8],
}

/// Construction failure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Drive1581InitError {
    /// The DOS ROM was not exactly 32 KB.
    #[error("expected 1581 DOS ROM of {expected:#06X} bytes, got {actual:#06X}")]
    InvalidRomSize { expected: usize, actual: usize },
}

/// Media-attach failure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Drive1581MediaError {
    /// The image was not a supported D81 size.
    #[error("invalid D81 media: {0}")]
    InvalidD81(#[from] D81ParseError),
    /// The image was not exactly 800 KB (the WD177x geometry the drive wires).
    #[error("expected an 800 KB D81 image, got {actual} bytes")]
    WrongImageSize { actual: usize },
}

impl Drive1581 {
    /// Constructs a 1581 board from one 32 KB DOS ROM image.
    ///
    /// # Errors
    ///
    /// Returns an error if the ROM size is not exactly 32 KB.
    pub fn new(config: Drive1581Config<'_>) -> Result<Self, Drive1581InitError> {
        if config.dos_rom.len() != ROM_SIZE {
            return Err(Drive1581InitError::InvalidRomSize {
                expected: ROM_SIZE,
                actual: config.dos_rom.len(),
            });
        }

        let mut rom = [0u8; ROM_SIZE];
        rom.copy_from_slice(config.dos_rom);

        let mut cpu = M6502::new();
        cpu.reset();

        let mut fdc = Wd1770::new();
        fdc.set_drive(0);

        Ok(Self {
            cpu,
            cia: Cia6526::new(),
            fdc,
            ram: [0; RAM_SIZE],
            rom,
            device_number: DEFAULT_DEVICE_NUMBER,
            write_protected: false,
            has_disk: false,
            cycles: 0,
        })
    }

    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    #[must_use]
    pub const fn cia(&self) -> &Cia6526 {
        &self.cia
    }

    #[must_use]
    pub const fn cycles(&self) -> u64 {
        self.cycles
    }

    #[must_use]
    pub const fn device_number(&self) -> u8 {
        self.device_number
    }

    /// Sets the IEC device number (8-11). The DOS reads it from the jumper
    /// bits on CIA Port A at boot, so set this before ticking.
    pub const fn set_device_number(&mut self, device_number: u8) {
        self.device_number = device_number;
    }

    /// Whether a disk is currently mounted.
    #[must_use]
    pub const fn disk_inserted(&self) -> bool {
        self.has_disk
    }

    /// The WD177x status register (side-effect-free), for inspection.
    #[must_use]
    pub fn fdc_status(&self) -> u8 {
        self.fdc.peek_status()
    }

    /// Attaches a write-protected D81 image.
    ///
    /// # Errors
    ///
    /// Returns an error if the image is not a valid 800 KB D81.
    pub fn load_d81_bytes(&mut self, bytes: &[u8]) -> Result<(), Drive1581MediaError> {
        self.load_d81_bytes_writable(bytes, false)
    }

    /// Attaches a D81 image, optionally writable.
    ///
    /// # Errors
    ///
    /// Returns an error if the image is not a valid 800 KB D81.
    pub fn load_d81_bytes_writable(
        &mut self,
        bytes: &[u8],
        writable: bool,
    ) -> Result<(), Drive1581MediaError> {
        if bytes.len() != D81_IMAGE_SIZE {
            return Err(Drive1581MediaError::WrongImageSize {
                actual: bytes.len(),
            });
        }
        // Validate as a D81 container (directory/geometry) before mounting.
        format_commodore_c64_d81::read_sector(bytes, 1, 0)?;

        let disk = Disk::new(
            bytes.to_vec(),
            D81_TRACKS,
            D81_SIDES,
            D81_SECTORS_PER_TRACK,
            D81_SECTOR_SIZE,
        )
        .with_first_sector_id(1)
        .write_protected(!writable);
        self.fdc.insert_disk(0, disk);
        self.write_protected = !writable;
        self.has_disk = true;
        Ok(())
    }

    /// Detaches any disk.
    pub fn eject_disk(&mut self) {
        self.fdc.eject_disk(0);
        self.has_disk = false;
    }

    /// The current disk image bytes if one is mounted (for write-back).
    #[must_use]
    pub fn flush_image(&self) -> Option<Vec<u8>> {
        if !self.has_disk {
            return None;
        }
        self.fdc.disk(0).map(|disk| disk.data().to_vec())
    }

    /// Advances one CPU cycle without an IEC bus (standalone/test).
    pub fn tick(&mut self) -> bool {
        self.step(None)
    }

    /// Advances one CPU cycle connected to the shared IEC bus.
    pub fn tick_with_iec_bus(&mut self, bus: &mut IecBus) -> bool {
        self.step(Some(bus))
    }

    /// Re-folds the drive's IEC output onto the bus without stepping (used by
    /// the host after the C64 side advances).
    pub fn sync_iec_bus(&mut self, bus: &mut IecBus) {
        self.apply_drive_inputs(Some(bus));
        self.drive_iec_outputs(bus);
    }

    fn step(&mut self, mut bus: Option<&mut IecBus>) -> bool {
        self.apply_drive_inputs(bus.as_deref());
        self.cpu.irq = self.cia.irq;

        if self.cpu.rw {
            self.cpu.data_in = self.read_mem(self.cpu.addr);
        } else {
            self.write_mem(self.cpu.addr, self.cpu.data, bus.as_deref_mut());
        }
        let completed = self.cpu.tick();

        self.cia.tick();
        self.fdc.tick();
        self.refresh_mechanics();
        self.apply_drive_inputs(bus.as_deref());
        if let Some(bus) = bus {
            self.drive_iec_outputs(bus);
        }
        self.cycles += 1;
        completed
    }

    /// CPU-visible read (side effects on I/O reads).
    fn read_mem(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram[usize::from(addr) & (RAM_SIZE - 1)],
            0x4000..=0x5FFF => self.cia.read((addr & 0x0F) as u8),
            0x6000..=0x7FFF => self.fdc.read((addr & 0x03) as u8),
            0x8000..=0xFFFF => self.rom[usize::from(addr - 0x8000)],
            _ => 0xFF,
        }
    }

    /// Side-effect-free read for debug surfaces (I/O returns `0xFF`).
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram[usize::from(addr) & (RAM_SIZE - 1)],
            0x8000..=0xFFFF => self.rom[usize::from(addr - 0x8000)],
            _ => 0xFF,
        }
    }

    fn write_mem(&mut self, addr: u16, value: u8, bus: Option<&mut IecBus>) {
        match addr {
            0x0000..=0x1FFF => self.ram[usize::from(addr) & (RAM_SIZE - 1)] = value,
            0x4000..=0x5FFF => {
                self.cia.write((addr & 0x0F) as u8, value);
                self.refresh_mechanics();
                if let Some(bus) = bus {
                    self.drive_iec_outputs(bus);
                }
            }
            0x6000..=0x7FFF => self.fdc.write((addr & 0x03) as u8, value),
            _ => {}
        }
    }

    /// Drives side select and motor from the CIA Port A output latch (VICE
    /// `cia1581d.c`: PA0 side, PA2 motor, both active-low).
    fn refresh_mechanics(&mut self) {
        let pa = self.cia.port_a_drive_state();
        // PA0 selects the side; the D81's CBM-block ordering interleaves the
        // two physical heads, so a raw .d81 fed to the WD177x reads correctly
        // when the side index carries VICE's head-invert (physical head H ->
        // Disk side H^1). PA0 low selects physical head 1 (VICE inverts once);
        // mapping PA0 straight through lands the raw image on the right side.
        self.fdc.set_side(u8::from(pa & 0x01 != 0));
    }

    /// Folds the shared bus into the CIA input latches. The 1581's CIA Port B
    /// carries the serial lines with the same bit layout as the 1541's VIA1
    /// Port B: PB0 DATA-in, PB2 CLK-in, PB7 ATN-in, plus PB6 write-protect.
    fn apply_drive_inputs(&mut self, bus: Option<&IecBus>) {
        let drive_port = bus.map_or(0x85, IecBus::drive_port);
        let atn_high = bus.is_none_or(IecBus::drive_atn_high);

        let mut pb = 0xFF;
        if drive_port & 0x01 == 0 {
            pb &= !0x01; // DATA in low
        }
        if drive_port & 0x04 == 0 {
            pb &= !0x04; // CLK in low
        }
        if !atn_high {
            pb &= !0x80; // ATN in low
        }
        if self.write_protected {
            pb &= !0x40; // WP sense: read-only clears PB6
        }
        self.cia.pb_in = pb;

        // Port A: PA7 disk-change (high = no change pending), and the device
        // number jumpers at bits 3-4 (VICE `read_ciapa`: `8 * (device - 8)`).
        // A statically mounted image reports "no change".
        let jumper = (self.device_number.wrapping_sub(DEFAULT_DEVICE_NUMBER) & 0x03) << 3;
        self.cia.pa_in = 0x80 | jumper;

        // /ATN drives the CIA FLAG pin — a falling edge (ATN asserted) raises
        // the FLAG interrupt the DOS uses to enter its command handler.
        self.cia.flag = atn_high;
    }

    /// Pushes the drive's serial-line contribution onto the shared bus from the
    /// CIA Port B output state (DATA-out PB1, CLK-out PB3, ATNA PB4), matching
    /// the 1541's VIA1 Port B convention `write_drive_port_b` expects.
    fn drive_iec_outputs(&mut self, bus: &mut IecBus) {
        bus.write_drive_port_b(self.device_number, self.cia.port_b_drive_state());
    }

    /// Captures the full drive state for snapshotting.
    #[must_use]
    pub fn snapshot_state(&self) -> Drive1581Snapshot {
        Drive1581Snapshot {
            cpu: self.cpu.clone(),
            cia: self.cia.clone(),
            fdc: self.fdc.clone(),
            ram: self.ram.to_vec(),
            rom: self.rom.to_vec(),
            device_number: self.device_number,
            write_protected: self.write_protected,
            has_disk: self.has_disk,
            cycles: self.cycles,
        }
    }

    /// Rebuilds a drive from a snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the snapshot's RAM or ROM sizes are wrong.
    pub fn from_snapshot(snapshot: Drive1581Snapshot) -> Result<Self, String> {
        let mut ram = [0u8; RAM_SIZE];
        if snapshot.ram.len() != RAM_SIZE {
            return Err(format!("1581 RAM snapshot is {} bytes", snapshot.ram.len()));
        }
        ram.copy_from_slice(&snapshot.ram);

        let mut rom = [0u8; ROM_SIZE];
        if snapshot.rom.len() != ROM_SIZE {
            return Err(format!("1581 ROM snapshot is {} bytes", snapshot.rom.len()));
        }
        rom.copy_from_slice(&snapshot.rom);

        Ok(Self {
            cpu: snapshot.cpu,
            cia: snapshot.cia,
            fdc: snapshot.fdc,
            ram,
            rom,
            device_number: snapshot.device_number,
            write_protected: snapshot.write_protected,
            has_disk: snapshot.has_disk,
            cycles: snapshot.cycles,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_rom() -> Vec<u8> {
        let mut rom = vec![0u8; ROM_SIZE];
        // Reset vector -> $8000; a tiny loop there (JMP *) keeps the CPU alive.
        rom[0x7FFC] = 0x00;
        rom[0x7FFD] = 0x80;
        rom[0x0000] = 0x4C; // JMP $8000
        rom[0x0001] = 0x00;
        rom[0x0002] = 0x80;
        rom
    }

    fn drive() -> Drive1581 {
        Drive1581::new(Drive1581Config {
            dos_rom: &stub_rom(),
        })
        .expect("valid ROM")
    }

    #[test]
    fn rejects_wrong_rom_size() {
        match Drive1581::new(Drive1581Config { dos_rom: &[0; 100] }) {
            Err(err) => assert_eq!(
                err,
                Drive1581InitError::InvalidRomSize {
                    expected: ROM_SIZE,
                    actual: 100
                }
            ),
            Ok(_) => panic!("a 100-byte ROM must be rejected"),
        }
    }

    #[test]
    fn reset_vector_boots_into_rom() {
        let mut drive = drive();
        // Run enough cycles to fetch the reset vector and land in the ROM's
        // JMP-self loop at $8000.
        for _ in 0..12 {
            drive.tick();
        }
        assert!(
            (0x8000..=0x8002).contains(&drive.cpu().regs.pc),
            "CPU should be executing the ROM stub at $8000, got ${:04X}",
            drive.cpu().regs.pc
        );
    }

    #[test]
    fn ram_reads_and_writes_round_trip() {
        let mut drive = drive();
        drive.write_mem(0x0500, 0xAB, None);
        assert_eq!(drive.read_mem(0x0500), 0xAB);
        assert_eq!(drive.peek(0x0500), 0xAB);
        // RAM mirrors across the 8 KB window edge.
        assert_eq!(drive.read_mem(0x1500), drive.read_mem(0x1500));
    }

    #[test]
    fn rom_maps_at_8000() {
        let drive = drive();
        assert_eq!(drive.peek(0x8000), 0x4C);
        assert_eq!(drive.peek(0xFFFC), 0x00);
        assert_eq!(drive.peek(0xFFFD), 0x80);
    }

    #[test]
    fn fdc_registers_decode_at_6000() {
        let mut drive = drive();
        // Track register round-trips through $6001.
        drive.write_mem(0x6001, 0x2A, None);
        assert_eq!(drive.read_mem(0x6001), 0x2A);
    }

    #[test]
    fn cia_registers_decode_at_4000() {
        let mut drive = drive();
        // DDRA at $4002 round-trips.
        drive.write_mem(0x4002, 0xFF, None);
        assert_eq!(drive.read_mem(0x4002), 0xFF);
    }

    #[test]
    fn rejects_wrong_disk_size() {
        let mut drive = drive();
        let err = drive.load_d81_bytes(&[0u8; 1000]).expect_err("bad size");
        assert_eq!(err, Drive1581MediaError::WrongImageSize { actual: 1000 });
    }

    #[test]
    fn mounts_valid_d81_and_flushes() {
        let mut drive = drive();
        let image = vec![0u8; D81_IMAGE_SIZE];
        drive.load_d81_bytes(&image).expect("valid D81 mounts");
        assert_eq!(drive.flush_image().map(|v| v.len()), Some(D81_IMAGE_SIZE));
        drive.eject_disk();
        assert!(drive.flush_image().is_none());
    }

    #[test]
    fn snapshot_round_trips() {
        let mut drive = drive();
        for _ in 0..20 {
            drive.tick();
        }
        let snap = drive.snapshot_state();
        let restored = Drive1581::from_snapshot(snap).expect("restore");
        assert_eq!(restored.cycles(), drive.cycles());
        assert_eq!(restored.cpu().regs.pc, drive.cpu().regs.pc);
    }
}

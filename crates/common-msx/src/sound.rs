//! Sound beyond the PSG: MSX-MUSIC, the FM-PAC and Konami's SCC cartridges.
//!
//! # MSX-MUSIC and the FM-PAC
//!
//! MSX-MUSIC is a YM2413 (OPLL) written through I/O ports `$7C` (register
//! address) and `$7D` (data), with a 16 KB ROM, the FM-BIOS, at
//! `$4000-$7FFF` in some slot. Machines that have it built in carry it in a
//! slot of their own; [`MsxState::install_msx_music`](crate::MsxState::install_msx_music)
//! fits it.
//!
//! The FM-PAC (Panasonic SW-M004) is the cartridge form. Its 64 KB ROM shows
//! one 16 KB bank at a time in page 1, and it adds 8 KB of battery-backed
//! SRAM for PAC save data. In page 1:
//!
//! | Offset          | Read                 | Write                          |
//! |-----------------|----------------------|--------------------------------|
//! | `$1FFE`, `$1FFF`| the unlock bytes     | `$4D` then `$69` unlock the SRAM |
//! | `$3FF4`, `$3FF5`| ROM                  | the YM2413's address and data  |
//! | `$3FF6`         | the enable register  | bit 0 opens ports `$7C`/`$7D`; bit 4 locks the SRAM |
//! | `$3FF7`         | the bank             | the ROM bank, 0-3              |
//!
//! With the SRAM unlocked, `$0000-$1FFD` of the page is SRAM in place of
//! ROM. The memory-mapped YM2413 registers work whatever the enable bit says;
//! the I/O ports only when it is set.
//!
//! # The SCC cartridges
//!
//! A Konami SCC MegaROM switches four 8 KB banks with writes to
//! `$5000`, `$7000`, `$9000` and `$B000`. Writing a value whose low six bits
//! are all set to the `$9000` bank register also opens the SCC's registers
//! at `$9800-$9FFF`. Pages 0 and 3 mirror pages 2 and 1.
//!
//! The Sound Cartridge, which came with Snatcher and SD Snatcher, carries
//! the SCC-I and 128 KB of RAM in place of ROM, switched by the same four
//! registers. A mode register at `$BFFE`/`$BFFF` chooses the chip's SCC+
//! mode (bit 5), which moves its registers to `$B800-$BFFF` (opened by bit 7
//! of the `$B000` register) and makes banks writable: bits 0 and 1 the first
//! two, bit 2 with SCC+ mode the third, bit 4 all four.
//!
//! # Mixing
//!
//! Every source's output is summed once a T-state and averaged down to the
//! PSG's sample cadence: the same Bresenham step as the AY-3-8910's, on the
//! same clock, so the two buffers stay the same length and mix sample for
//! sample. One FM channel or one SCC channel at full volume is scaled to the
//! swing of one PSG channel at full volume; the YM2413's rhythm pin counts
//! twice its melody pin, as the YM2413 application manual has it. Both
//! levels are nominal, not measured against a board. The sum passes through
//! the same one-pole DC block the other machines apply.
//!
//! # Provenance
//!
//! The FM-PAC's register map and SRAM unlock, MSX-MUSIC's ports and the SCC
//! cartridges' mappers are facts read from openMSX (GPL-2.0, read for facts
//! only, not code), at commit `38af174e2785b490de07bb0ff891e950e64a3bcd`.

use emu198x_konami_scc::{Scc, Variant as SccVariant};
use emu198x_yamaha_ym2413::{Variant as Ym2413Variant, Ym2413};
use serde::{Deserialize, Serialize};

use crate::SlotAddress;

/// The output sample rate, the AY-3-8910's.
const SAMPLE_RATE: u32 = 48_000;
/// The AY-3-8910's clock, which paces the resampler.
const PSG_CLOCK_HZ: u32 = 1_789_773;
/// The swing of one PSG channel at full volume, which one FM or SCC
/// channel at full volume is scaled to.
const CHANNEL_SWING: f32 = 1.0 / 3.0;
/// One YM2413 channel swings ±255 on its pin.
const YM2413_SCALE: f32 = CHANNEL_SWING / 510.0;
/// One SCC channel swings from -120 to 119.
const SCC_SCALE: f32 = CHANNEL_SWING / 239.0;
/// The one-pole high-pass the other machines put on their sound chips.
const DC_BLOCK_ALPHA: f32 = 0.9952;

/// The FM-PAC's SRAM: 8 KB less the two unlock bytes.
pub const FM_PAC_SRAM_SIZE: usize = 0x1FFE;
/// The Sound Cartridge's RAM.
const SOUND_CARTRIDGE_RAM: usize = 128 * 1024;
/// The most samples a frame holds, the PSG's buffer: past it, both drop
/// samples alike, so a host that never drains them stays in step.
const MAX_SAMPLES: usize = 1024;

/// A YM2413's output, held from one of its samples to the next.
///
/// Each channel drives the pins for one internal clock in 18, so the chip's
/// sample is the pins summed over 18 clocks (72 T-states, 49.7 kHz). Holding
/// that sum, rather than averaging the pins over the 48 kHz output window,
/// keeps the 18-clock pattern from beating against the output rate.
#[derive(Default, Serialize, Deserialize)]
struct OpllOutput {
    sum: i32,
    clocks: u8,
    held: i32,
}

impl OpllOutput {
    /// After a T-state: take the pins if an internal clock ran, and return
    /// the held sample, the rhythm pin counted twice, at the PSG's level.
    fn step(&mut self, ym: &Ym2413) -> f32 {
        if ym.prescaler == 0 {
            self.sum += i32::from(ym.mo) + 2 * i32::from(ym.ro);
            self.clocks += 1;
            if self.clocks == 18 {
                self.held = self.sum;
                self.sum = 0;
                self.clocks = 0;
            }
        }
        self.held as f32 * YM2413_SCALE
    }
}

/// MSX-MUSIC built into the machine: the FM-BIOS ROM in a slot and a
/// YM2413 on ports `$7C`/`$7D`.
#[derive(Serialize, Deserialize)]
pub(crate) struct MsxMusic {
    pub(crate) slot: SlotAddress,
    rom: Vec<u8>,
    pub(crate) ym: Ym2413,
    output: OpllOutput,
}

impl MsxMusic {
    pub(crate) fn new(slot: SlotAddress, rom: Vec<u8>) -> Self {
        Self {
            slot,
            rom,
            ym: Ym2413::new(Ym2413Variant::Ym2413),
            output: OpllOutput::default(),
        }
    }

    /// The FM-BIOS at `$4000-$7FFF`, mirrored if it is smaller.
    pub(crate) fn read(&self, addr: u16) -> Option<u8> {
        if !(0x4000..0x8000).contains(&addr) || self.rom.is_empty() {
            return None;
        }
        Some(self.rom[usize::from(addr & 0x3FFF) % self.rom.len()])
    }

    pub(crate) fn tick(&mut self) -> f32 {
        self.ym.tick();
        self.output.step(&self.ym)
    }
}

/// The FM-PAC's registers, SRAM and YM2413.
#[derive(Serialize, Deserialize)]
pub(crate) struct FmPac {
    pub(crate) ym: Ym2413,
    output: OpllOutput,
    pub(crate) sram: Vec<u8>,
    enable: u8,
    bank: u8,
    unlock: [u8; 2],
}

impl FmPac {
    fn new() -> Self {
        Self {
            ym: Ym2413::new(Ym2413Variant::Ym2413),
            output: OpllOutput::default(),
            sram: vec![0; FM_PAC_SRAM_SIZE],
            enable: 0,
            bank: 0,
            unlock: [0; 2],
        }
    }

    fn sram_enabled(&self) -> bool {
        self.unlock == [0x4D, 0x69]
    }

    fn read(&self, rom: &[u8], addr: u16) -> u8 {
        if !(0x4000..0x8000).contains(&addr) {
            return 0xFF;
        }
        let offset = usize::from(addr & 0x3FFF);
        match offset {
            0x3FF6 => self.enable,
            0x3FF7 => self.bank,
            _ if self.sram_enabled() => match offset {
                0..FM_PAC_SRAM_SIZE => self.sram[offset],
                0x1FFE => self.unlock[0],
                0x1FFF => self.unlock[1],
                _ => 0xFF,
            },
            _ => rom
                .get(usize::from(self.bank) * 0x4000 + offset)
                .copied()
                .unwrap_or(0xFF),
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        if !(0x4000..0x8000).contains(&addr) {
            return;
        }
        let offset = usize::from(addr & 0x3FFF);
        match offset {
            0x1FFE | 0x1FFF => {
                if self.enable & 0x10 == 0 {
                    self.unlock[offset - 0x1FFE] = value;
                }
            }
            0x3FF4 | 0x3FF5 => self.ym.write((offset & 1) as u8, value),
            0x3FF6 => {
                self.enable = value & 0x11;
                if self.enable & 0x10 != 0 {
                    self.unlock = [0; 2];
                }
            }
            0x3FF7 => self.bank = value & 0x03,
            _ => {
                if self.sram_enabled() && offset < FM_PAC_SRAM_SIZE {
                    self.sram[offset] = value;
                }
            }
        }
    }
}

/// The Sound Cartridge's RAM and mode register around its SCC-I.
#[derive(Serialize, Deserialize)]
pub(crate) struct SoundCartridge {
    ram: Vec<u8>,
    mode: u8,
}

impl SoundCartridge {
    /// Banks 0-3 of the RAM, filled from `image` as openMSX loads one.
    fn new(image: &[u8]) -> Self {
        let mut ram = vec![0; SOUND_CARTRIDGE_RAM];
        let length = image.len().min(SOUND_CARTRIDGE_RAM);
        ram[..length].copy_from_slice(&image[..length]);
        Self { ram, mode: 0 }
    }

    fn writable(&self, region: usize) -> bool {
        if self.mode & 0x10 != 0 {
            return true;
        }
        match region {
            0 => self.mode & 0x01 != 0,
            1 => self.mode & 0x02 != 0,
            2 => self.mode & 0x24 == 0x24,
            _ => false,
        }
    }

    fn ram_offset(banks: &[u8; 4], addr: u16) -> usize {
        let region = usize::from(addr >> 13) - 2;
        usize::from(banks[region] & 0x0F) * 0x2000 + usize::from(addr & 0x1FFF)
    }
}

/// What a cartridge carries besides ROM and a mapper.
#[derive(Serialize, Deserialize)]
pub(crate) enum CartridgeSound {
    None,
    /// A Konami SCC MegaROM's chip.
    Scc(Box<Scc>),
    /// The Sound Cartridge.
    SoundCartridge(Box<Scc>, Box<SoundCartridge>),
    FmPac(Box<FmPac>),
}

impl CartridgeSound {
    pub(crate) fn konami_scc() -> Self {
        Self::Scc(Box::new(Scc::new(SccVariant::Scc)))
    }

    pub(crate) fn sound_cartridge(image: &[u8]) -> Self {
        Self::SoundCartridge(
            Box::new(Scc::new(SccVariant::SccI)),
            Box::new(SoundCartridge::new(image)),
        )
    }

    pub(crate) fn fm_pac() -> Self {
        Self::FmPac(Box::new(FmPac::new()))
    }

    /// Where the SCC's registers sit with these bank registers, if they
    /// are open: `$9800` (SCC or compatible mode) or `$B800` (SCC+ mode).
    fn scc_window(&self, banks: &[u8; 4]) -> Option<u16> {
        match self {
            Self::Scc(_) => (banks[2] & 0x3F == 0x3F).then_some(0x9800),
            Self::SoundCartridge(_, cart) => {
                if cart.mode & 0x20 != 0 {
                    (banks[3] & 0x80 != 0).then_some(0xB800)
                } else {
                    (banks[2] & 0x3F == 0x3F).then_some(0x9800)
                }
            }
            _ => None,
        }
    }

    fn scc(&self) -> Option<&Scc> {
        match self {
            Self::Scc(scc) | Self::SoundCartridge(scc, _) => Some(scc),
            _ => None,
        }
    }

    fn scc_mut(&mut self) -> Option<&mut Scc> {
        match self {
            Self::Scc(scc) | Self::SoundCartridge(scc, _) => Some(scc),
            _ => None,
        }
    }

    /// The SCC register `addr` reaches, if the window is open there.
    fn scc_register(&self, banks: &[u8; 4], addr: u16) -> Option<u8> {
        let base = self.scc_window(banks)?;
        (base..base + 0x800).contains(&addr).then_some(addr as u8)
    }

    /// A read the cartridge's own hardware answers, without side effects;
    /// `None` leaves it to the ROM.
    pub(crate) fn peek(&self, rom: &[u8], banks: &[u8; 4], addr: u16) -> Option<u8> {
        if let Some(register) = self.scc_register(banks, addr) {
            return self.scc().map(|scc| scc.peek(register));
        }
        match self {
            Self::SoundCartridge(_, cart) => Some(if (0x4000..0xC000).contains(&addr) {
                cart.ram[SoundCartridge::ram_offset(banks, addr)]
            } else {
                0xFF
            }),
            Self::FmPac(pac) => Some(pac.read(rom, addr)),
            _ => None,
        }
    }

    /// A CPU read, with the SCC's side effect on its deformation register.
    pub(crate) fn read(&mut self, rom: &[u8], banks: &[u8; 4], addr: u16) -> Option<u8> {
        if let Some(register) = self.scc_register(banks, addr) {
            return self.scc_mut().map(|scc| scc.read(register));
        }
        self.peek(rom, banks, addr)
    }

    /// A CPU write. Returns `true` when the cartridge's own hardware took
    /// it, so the mapper should not see it.
    pub(crate) fn write(&mut self, banks: &mut [u8; 4], addr: u16, value: u8) -> bool {
        if let Self::SoundCartridge(scc, cart) = self {
            if !(0x4000..0xC000).contains(&addr) {
                return true;
            }
            if addr | 1 == 0xBFFF {
                cart.mode = value;
                scc.set_plus_mode(value & 0x20 != 0);
                return true;
            }
            let region = usize::from(addr >> 13) - 2;
            if cart.writable(region) {
                let offset = SoundCartridge::ram_offset(banks, addr);
                cart.ram[offset] = value;
                return true;
            }
            if addr & 0x1800 == 0x1000 {
                banks[region] = value;
                return true;
            }
        }
        if let Some(register) = self.scc_register(banks, addr) {
            if let Some(scc) = self.scc_mut() {
                scc.write(register, value);
            }
            return true;
        }
        match self {
            Self::FmPac(pac) => {
                pac.write(addr, value);
                true
            }
            Self::SoundCartridge(..) => true,
            _ => false,
        }
    }

    /// An `OUT` to `$7C`/`$7D`: the FM-PAC's YM2413, when its enable bit
    /// opens the ports.
    pub(crate) fn write_io(&mut self, port: u8, value: u8) {
        if let Self::FmPac(pac) = self
            && pac.enable & 0x01 != 0
        {
            pac.ym.write(port & 1, value);
        }
    }

    /// One T-state; returns the output, scaled to the PSG's level.
    pub(crate) fn tick(&mut self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Scc(scc) | Self::SoundCartridge(scc, _) => {
                scc.tick();
                scc.output() as f32 * SCC_SCALE
            }
            Self::FmPac(pac) => {
                pac.ym.tick();
                pac.output.step(&pac.ym)
            }
        }
    }

    /// Drop the YM2413's own sample log, which this machine does not use,
    /// so it cannot grow without bound.
    pub(crate) fn end_frame(&mut self) {
        if let Self::FmPac(pac) = self {
            drop(pac.ym.take_samples());
        }
    }

    pub(crate) fn scc_chip(&self) -> Option<&Scc> {
        self.scc()
    }

    pub(crate) fn fm_pac_sram(&self) -> Option<&[u8]> {
        match self {
            Self::FmPac(pac) => Some(&pac.sram),
            _ => None,
        }
    }

    pub(crate) fn fm_pac_sram_mut(&mut self) -> Option<&mut [u8]> {
        match self {
            Self::FmPac(pac) => Some(&mut pac.sram),
            _ => None,
        }
    }

    pub(crate) fn ym2413(&self) -> Option<&Ym2413> {
        match self {
            Self::FmPac(pac) => Some(&pac.ym),
            _ => None,
        }
    }
}

/// Averages the summed expansion sound down to the PSG's sample cadence.
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct ExpansionMixer {
    accumulator: f32,
    tstates: u32,
    /// PSG ticks since the last sample, as the PSG counts them.
    psg_ticks: u32,
    sample_error: u32,
    samples: Vec<f32>,
    hp_in: f32,
    hp_out: f32,
}

impl ExpansionMixer {
    /// Integrate the driven click line for half a CPU T-state. Its gain is
    /// the same nominal one-channel convention as the expansion sources;
    /// per-board analog calibration is separate. The existing accumulator
    /// also preserves this charge across a mid-sample save.
    pub(crate) fn accumulate_click_halfcycle(&mut self, high: bool) {
        if high {
            self.accumulator += CHANNEL_SWING * 0.5;
        }
    }

    /// Add one T-state's output.
    pub(crate) fn accumulate(&mut self, level: f32) {
        self.accumulator += level;
        self.tstates += 1;
    }

    /// The PSG ticked: emit a sample exactly when it does.
    pub(crate) fn psg_tick(&mut self) {
        self.psg_ticks += 1;
        self.sample_error += SAMPLE_RATE;
        if self.sample_error >= PSG_CLOCK_HZ {
            self.sample_error -= PSG_CLOCK_HZ;
            self.emit();
        }
    }

    fn emit(&mut self) {
        let sample = self.accumulator / self.tstates.max(1) as f32;
        let filtered = DC_BLOCK_ALPHA * (self.hp_out + sample - self.hp_in);
        self.hp_in = sample;
        self.hp_out = filtered;
        if self.samples.len() < MAX_SAMPLES {
            self.samples.push(filtered);
        }
        self.accumulator = 0.0;
        self.tstates = 0;
        self.psg_ticks = 0;
    }

    /// Add this frame's samples into `out`, the PSG's, flushing a partial
    /// sample exactly when the PSG does.
    /// Returns how many samples the frame produced.
    pub(crate) fn end_frame(&mut self, out: &mut [f32]) -> usize {
        if self.psg_ticks > 0 {
            self.emit();
        }
        for (sample, extra) in out.iter_mut().zip(&self.samples) {
            *sample += extra;
        }
        let count = self.samples.len();
        self.samples.clear();
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mixer_emits_on_the_psg_cadence() {
        let mut mixer = ExpansionMixer::default();
        // A frame of 59,736 T-states: 29,868 PSG ticks.
        for tstate in 0..59_736u32 {
            mixer.accumulate(0.25);
            if tstate % 2 == 1 {
                mixer.psg_tick();
            }
        }
        let count = mixer.samples.len();
        assert_eq!(count, 29_868 * 48_000 / 1_789_773);
        let mut out = vec![0.0; 1024];
        assert_eq!(mixer.end_frame(&mut out), count + 1, "and a partial sample");
        assert!(out[..count].iter().all(|&s| s != 0.0));
        assert_eq!(out[count + 1], 0.0);
    }

    #[test]
    fn the_fm_pac_unlocks_its_sram_with_4d_69() {
        let rom = vec![0xAA; 0x10000];
        let mut sound = CartridgeSound::fm_pac();
        let mut banks = [0, 1, 2, 3];
        assert_eq!(sound.peek(&rom, &banks, 0x4000), Some(0xAA));
        sound.write(&mut banks, 0x4000, 0x12);
        assert_eq!(sound.peek(&rom, &banks, 0x4000), Some(0xAA), "locked");
        sound.write(&mut banks, 0x5FFE, 0x4D);
        sound.write(&mut banks, 0x5FFF, 0x69);
        assert_eq!(sound.peek(&rom, &banks, 0x4000), Some(0x00), "SRAM shows");
        sound.write(&mut banks, 0x4000, 0x12);
        assert_eq!(sound.peek(&rom, &banks, 0x4000), Some(0x12));
        assert_eq!(sound.peek(&rom, &banks, 0x5FFE), Some(0x4D));
        assert_eq!(sound.fm_pac_sram().map(|sram| sram[0]), Some(0x12));
        // Bit 4 of the enable register locks it again.
        sound.write(&mut banks, 0x7FF6, 0x10);
        assert_eq!(sound.peek(&rom, &banks, 0x4000), Some(0xAA));
        assert_eq!(sound.peek(&rom, &banks, 0x7FF6), Some(0x10));
        sound.write(&mut banks, 0x5FFE, 0x4D);
        sound.write(&mut banks, 0x5FFF, 0x69);
        assert_eq!(sound.peek(&rom, &banks, 0x4000), Some(0xAA), "still locked");
    }

    #[test]
    fn the_fm_pac_switches_rom_banks() {
        let mut rom = vec![0; 0x10000];
        rom[0x8000] = 0x22;
        let mut sound = CartridgeSound::fm_pac();
        let mut banks = [0; 4];
        sound.write(&mut banks, 0x7FF7, 0x06);
        assert_eq!(sound.peek(&rom, &banks, 0x7FF7), Some(0x02));
        assert_eq!(sound.peek(&rom, &banks, 0x4000), Some(0x22));
        assert_eq!(sound.peek(&rom, &banks, 0x8000), Some(0xFF), "page 1 only");
        sound.write(&mut banks, 0x8000, 0x00);
    }

    #[test]
    fn the_sound_cartridge_writes_ram_where_its_mode_allows() {
        let mut sound = CartridgeSound::sound_cartridge(&[0x5A; 0x2000]);
        let mut banks = [0, 1, 2, 3];
        assert_eq!(sound.peek(&[], &banks, 0x4000), Some(0x5A));
        sound.write(&mut banks, 0x4000, 0x11);
        assert_eq!(sound.peek(&[], &banks, 0x4000), Some(0x5A), "read-only");
        sound.write(&mut banks, 0xBFFE, 0x01);
        sound.write(&mut banks, 0x4000, 0x11);
        assert_eq!(
            sound.peek(&[], &banks, 0x4000),
            Some(0x11),
            "bank 0 writable"
        );
        // While bank 0 is RAM-writable, its register writes are RAM writes.
        sound.write(&mut banks, 0x5000, 0x07);
        assert_eq!(banks[0], 0);
        sound.write(&mut banks, 0xBFFF, 0x00);
        sound.write(&mut banks, 0x5000, 0x07);
        assert_eq!(banks[0], 0x07);
        assert_eq!(sound.peek(&[], &banks, 0xC000), Some(0xFF));
        assert!(sound.write(&mut banks, 0x0000, 0x00), "outside, ignored");
        // All four banks writable with bit 4.
        sound.write(&mut banks, 0xBFFE, 0x10);
        sound.write(&mut banks, 0xA000, 0x33);
        assert_eq!(sound.peek(&[], &banks, 0xA000), Some(0x33));
    }

    #[test]
    fn the_sound_cartridge_moves_the_scc_in_plus_mode() {
        let mut sound = CartridgeSound::sound_cartridge(&[]);
        let mut banks = [0, 1, 2, 3];
        sound.write(&mut banks, 0x9000, 0x3F);
        sound.write(&mut banks, 0x9800, 0x40);
        assert_eq!(
            sound.peek(&[], &banks, 0x9800),
            Some(0x40),
            "compatible mode"
        );
        sound.write(&mut banks, 0xBFFE, 0x20);
        assert_eq!(sound.peek(&[], &banks, 0x9800), Some(0x00), "now RAM");
        sound.write(&mut banks, 0xB000, 0x80);
        sound.write(&mut banks, 0xB880, 0x22);
        assert_eq!(
            sound.peek(&[], &banks, 0xB880),
            Some(0x22),
            "channel 5's wave"
        );
        assert_eq!(
            sound.scc_chip().map(Scc::mode),
            Some(emu198x_konami_scc::Mode::Plus)
        );
    }

    #[test]
    fn levels_are_scaled_to_a_psg_channel() {
        let mut sound = CartridgeSound::konami_scc();
        let mut banks = [0, 1, 0x3F, 3];
        for i in 0..32 {
            sound.write(&mut banks, 0x9800 + i, if i < 16 { 0x7F } else { 0x80 });
        }
        sound.write(&mut banks, 0x988F, 0x01);
        sound.write(&mut banks, 0x9880, 100);
        let mut high = f32::MIN;
        let mut low = f32::MAX;
        for _ in 0..(101 * 32) {
            let level = sound.tick();
            high = high.max(level);
            low = low.min(level);
        }
        assert!(((high - low) - CHANNEL_SWING).abs() < 0.01, "{high} {low}");
        assert_eq!(CartridgeSound::None.tick(), 0.0);
    }

    #[test]
    fn msx_music_maps_its_rom_into_page_1() {
        let mut music = MsxMusic::new(SlotAddress::primary(0), vec![0x42; 0x4000]);
        assert_eq!(music.read(0x4000), Some(0x42));
        assert_eq!(music.read(0x8000), None);
        let _ = music.tick();
        assert_eq!(
            MsxMusic::new(SlotAddress::primary(0), Vec::new()).read(0x4000),
            None
        );
    }
}

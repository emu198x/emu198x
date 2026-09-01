//! Runtime wrapper for the Commodore VIC-20.
//!
//! The VIC-20 needs three ROMs at construction (KERNAL, BASIC, char ROM).
//! The runtime defers construction until all three arrive via
//! `set_roms` / `from_firmware`. Each frame it drains the VIC's three-tone +
//! noise audio and pumps it into the host audio sink.

use emu198x_shell::{
    AudioPacket, CapabilitySet, ControlCommand, FirmwareSet, FramePacket, HostIo, MachineCore,
    MachineError, MachineProfile, MachineTime, MediaKind, MediaSet, PixelFormat, ResetKind,
    RunResult, StopReason,
};
use machine_commodore_vic_20::{Vic20, Vic20Model, Vic20RamExpansion};

use crate::input::apply_input_event;
use crate::profiles::{
    BASIC_FIRMWARE_ID, CHAR_FIRMWARE_ID, KERNAL_FIRMWARE_ID, Model, profile_for,
};
use crate::snapshot;
use crate::{BitBangSerial, EspAtModem, EspAtTcpBridge};
use emu198x_shell::display::Display;

const KERNAL_SIZE: usize = 8 * 1024;
const BASIC_SIZE: usize = 8 * 1024;
const CHAR_SIZE: usize = 4 * 1024;
const AUDIO_SAMPLE_RATE: u32 = 48_000;
const PRG_AUTOLOAD_FRAME: u64 = 150;

/// This machine's keyboard for the shared `press_key` / `type_string` tools:
/// the standard layout, backed by this machine's own key-name resolver so a
/// character it cannot type is refused rather than silently dropped (#1196).
/// The character each keycap carries above its own, read off the machine:
/// hold SHIFT with every key in turn and let BASIC echo the result.
///
/// Only the symbols are listed. SHIFT with `*`, `-`, `@` or `=` produces
/// PETSCII graphics rather than punctuation, and SHIFT-0 produces `0`, so
/// none of them belongs here — `type_string` refuses those characters
/// instead of typing a line drawing (#1206).
const SHIFTED_LEGENDS: &[(char, &str)] = &[
    ('!', "1"),
    ('"', "2"),
    ('#', "3"),
    ('$', "4"),
    ('%', "5"),
    ('&', "6"),
    ('\'', "7"),
    ('(', "8"),
    (')', "9"),
    ('[', ":"),
    (']', ";"),
    ('<', ","),
    ('>', "."),
    ('?', "/"),
];

static KEYBOARD: emu198x_shell::StandardKeyboard = emu198x_shell::StandardKeyboard::with_legends(
    emu198x_shell::STANDARD_KEY_TIMING,
    crate::input::knows_key_name,
    "shift",
    SHIFTED_LEGENDS,
);

pub struct Vic20Runtime {
    profile: MachineProfile,
    model: Model,
    machine: Option<Vic20>,
    kernal_bytes: Option<Vec<u8>>,
    basic_bytes: Option<Vec<u8>>,
    char_bytes: Option<Vec<u8>>,
    ram_expansion: Vic20RamExpansion,
    time: MachineTime,
    rgba_framebuffer: Vec<u8>,
    rgba_width: u32,
    rgba_height: u32,
    controller_cache: crate::input::ControllerCache,
    pending_prg: Option<Vec<u8>>,
    /// Original cartridge container retained so resets rebuild the inserted
    /// ROM before the KERNAL performs its cold-start probe.
    cartridge_image: Option<Vec<u8>>,
    user_port_serial: Option<BitBangSerial>,
    esp_at_modem: Option<EspAtModem>,
    esp_at_tcp_bridge: Option<EspAtTcpBridge>,
}

impl Vic20Runtime {
    #[must_use]
    pub fn blank(model: Model) -> Self {
        Self {
            profile: profile_for(model),
            model,
            machine: None,
            kernal_bytes: None,
            basic_bytes: None,
            char_bytes: None,
            ram_expansion: Vic20RamExpansion::NONE,
            time: MachineTime::default(),
            rgba_framebuffer: Vec::new(),
            rgba_width: 0,
            rgba_height: 0,
            controller_cache: crate::input::ControllerCache::default(),
            pending_prg: None,
            cartridge_image: None,
            user_port_serial: None,
            esp_at_modem: None,
            esp_at_tcp_bridge: None,
        }
    }

    /// Build directly from explicit KERNAL + BASIC + char ROMs.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if any ROM size is wrong.
    pub fn new(
        model: Model,
        kernal: Vec<u8>,
        basic: Vec<u8>,
        char_rom: Vec<u8>,
    ) -> Result<Self, MachineError> {
        let mut runtime = Self::blank(model);
        runtime.set_roms(kernal, basic, char_rom)?;
        Ok(runtime)
    }

    /// Build from a firmware set.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or any ROM is missing.
    pub fn from_firmware(model: Model, firmware: &FirmwareSet<'_>) -> Result<Self, MachineError> {
        let profile = profile_for(model);
        firmware.validate_for_profile(&profile)?;
        let kernal =
            firmware
                .bytes(KERNAL_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: KERNAL_FIRMWARE_ID.to_owned(),
                })?;
        let basic =
            firmware
                .bytes(BASIC_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: BASIC_FIRMWARE_ID.to_owned(),
                })?;
        let char_rom =
            firmware
                .bytes(CHAR_FIRMWARE_ID)
                .ok_or_else(|| MachineError::MissingFirmware {
                    id: CHAR_FIRMWARE_ID.to_owned(),
                })?;
        Self::new(model, kernal.to_vec(), basic.to_vec(), char_rom.to_vec())
    }

    /// Replace ROMs and rebuild.
    ///
    /// # Errors
    ///
    /// Returns `MachineError::InvalidFirmware` if any size is wrong.
    pub fn set_roms(
        &mut self,
        kernal: Vec<u8>,
        basic: Vec<u8>,
        char_rom: Vec<u8>,
    ) -> Result<(), MachineError> {
        if kernal.len() != KERNAL_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: KERNAL_FIRMWARE_ID.to_owned(),
                reason: format!("KERNAL is {} bytes; expected {KERNAL_SIZE}", kernal.len()),
            });
        }
        if basic.len() != BASIC_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: BASIC_FIRMWARE_ID.to_owned(),
                reason: format!("BASIC is {} bytes; expected {BASIC_SIZE}", basic.len()),
            });
        }
        if char_rom.len() != CHAR_SIZE {
            return Err(MachineError::InvalidFirmware {
                id: CHAR_FIRMWARE_ID.to_owned(),
                reason: format!(
                    "character ROM is {} bytes; expected {CHAR_SIZE}",
                    char_rom.len()
                ),
            });
        }
        self.kernal_bytes = Some(kernal);
        self.basic_bytes = Some(basic);
        self.char_bytes = Some(char_rom);
        self.rebuild_machine();
        Ok(())
    }

    /// Fit the named RAM expansion cartridges to the expansion port.
    pub fn set_ram_expansion(&mut self, expansion: Vic20RamExpansion) {
        self.ram_expansion = expansion;
        self.rebuild_machine();
    }

    #[must_use]
    pub fn machine(&self) -> Option<&Vic20> {
        self.machine.as_ref()
    }

    pub fn machine_mut(&mut self) -> Option<&mut Vic20> {
        self.machine.as_mut()
    }

    /// Attach a deterministic 8N1 byte stream to user-port CB2/PB0.
    pub fn attach_user_port_serial(&mut self, cycles_per_bit: u32) {
        self.user_port_serial = Some(BitBangSerial::new(cycles_per_bit));
        self.esp_at_modem = None;
        self.esp_at_tcp_bridge = None;
    }

    pub fn user_port_serial_mut(&mut self) -> Option<&mut BitBangSerial> {
        self.user_port_serial.as_mut()
    }

    /// Attach the deterministic ESP-AT subset used by Rachel.
    pub fn attach_esp_at_modem(&mut self, cycles_per_bit: u32) {
        self.esp_at_modem = Some(EspAtModem::new(cycles_per_bit));
        self.user_port_serial = None;
        self.esp_at_tcp_bridge = None;
    }

    pub fn esp_at_modem_mut(&mut self) -> Option<&mut EspAtModem> {
        self.esp_at_modem.as_mut()
    }

    /// Attach ESP-AT to a real non-blocking TCP transport. Connections are
    /// made only after the emulated client sends `AT+CIPSTART`.
    pub fn attach_esp_at_tcp_bridge(&mut self, cycles_per_bit: u32, frame_size: usize) {
        self.esp_at_tcp_bridge = Some(EspAtTcpBridge::new(cycles_per_bit, frame_size));
        self.esp_at_modem = None;
        self.user_port_serial = None;
    }

    pub fn esp_at_tcp_bridge(&self) -> Option<&EspAtTcpBridge> {
        self.esp_at_tcp_bridge.as_ref()
    }

    /// Inject a `.PRG` image into RAM and queue a launch command so it runs
    /// itself. The first two bytes are the little-endian load address; the rest
    /// is copied there. BASIC's end-of-program / start-of-variables pointers
    /// (`$2D`-`$32`) are set just past the program, then the launch command is
    /// placed in the KERNAL keyboard buffer (`$0277`, count at `$C6`) so the
    /// editor runs it once the machine is at READY: `RUN` for a BASIC program,
    /// or `SYS <load-address>` (`sys = true`) for a machine-code program (whose
    /// first byte is its entry point).
    ///
    /// The machine must already be booted to READY and configured with RAM that
    /// puts the BASIC start (`TXTTAB`) at the PRG's load address — e.g. a `$1201`
    /// program needs the `+8K` block.
    ///
    /// # Errors
    ///
    /// Returns an error when no machine is loaded or the image is too short.
    pub fn autoload_prg(&mut self, bytes: &[u8], sys: bool) -> Result<(), String> {
        let machine = self.machine.as_mut().ok_or("VIC-20 not initialised")?;
        if bytes.len() < 3 {
            return Err("PRG image too short".into());
        }
        let load = u16::from(bytes[0]) | (u16::from(bytes[1]) << 8);
        for (i, &byte) in bytes[2..].iter().enumerate() {
            let offset = u16::try_from(i).map_err(|_| "PRG larger than 64 KB")?;
            machine.poke(load.wrapping_add(offset), byte);
        }
        let body = u16::try_from(bytes.len() - 2).map_err(|_| "PRG larger than 64 KB")?;
        let end = load.wrapping_add(body);
        let [lo, hi] = end.to_le_bytes();
        // VARTAB / ARYTAB / STREND all point just past the program.
        for base in [0x2Du16, 0x2F, 0x31] {
            machine.poke(base, lo);
            machine.poke(base + 1, hi);
        }
        // Queue the launch command + RETURN (PETSCII == ASCII for these chars).
        let command = if sys {
            format!("SYS{load}\r")
        } else {
            "RUN\r".to_owned()
        };
        for (i, &byte) in command.as_bytes().iter().enumerate() {
            let offset = u16::try_from(i).map_err(|_| "launch command too long")?;
            machine.poke(0x0277 + offset, byte);
        }
        let count = u8::try_from(command.len()).map_err(|_| "launch command too long")?;
        machine.poke(0x00C6, count); // NDX: characters queued
        Ok(())
    }

    /// RAM configuration implied by the three canonical VIC-20 BASIC load
    /// addresses. An arbitrary machine-code load keeps the caller's chosen
    /// cartridges; a PRG gives no other expansion metadata.
    ///
    /// `$0401` and `$1001` name an exact machine: the 3K expander alone, and
    /// the unexpanded 5K machine. `$1201` names only a floor — BLK1 must be
    /// fitted for BASIC's program area to start there, but BLK2, BLK3 and the
    /// 3K expander may also be present, and a caller who asked for them keeps
    /// them rather than having a larger program truncated.
    fn expansion_for_prg(
        bytes: &[u8],
        current: Vic20RamExpansion,
    ) -> Result<Vic20RamExpansion, String> {
        if bytes.len() < 3 {
            return Err("PRG image too short".into());
        }
        let load = u16::from_le_bytes([bytes[0], bytes[1]]);
        Ok(match load {
            0x0401 => Vic20RamExpansion::EXP_3K,
            0x1001 => Vic20RamExpansion::NONE,
            0x1201 => Vic20RamExpansion {
                blk1: true,
                ..current
            },
            _ => current,
        })
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }

    pub(crate) fn set_time(&mut self, time: MachineTime) {
        self.time = time;
    }

    pub(crate) fn cartridge_image(&self) -> Option<&[u8]> {
        self.cartridge_image.as_deref()
    }

    pub(crate) fn set_cartridge_image(&mut self, image: Option<Vec<u8>>) {
        self.cartridge_image = image;
    }

    /// RAM expansion cartridges currently fitted, including any block added to
    /// satisfy a loaded PRG's canonical BASIC start address.
    #[must_use]
    pub fn ram_expansion(&self) -> Vic20RamExpansion {
        self.ram_expansion
    }

    /// Total installed expansion RAM in KiB.
    #[must_use]
    pub fn ram_expansion_kb(&self) -> usize {
        self.ram_expansion.total_kib()
    }

    /// Install a machine restored from a snapshot, re-deriving the host RGBA
    /// framebuffer from its live state. Replaces the cold-boot rebuild on the
    /// restore path so the resumed machine keeps its CPU/VIA/VIC-I/RAM state.
    pub(crate) fn set_machine(&mut self, machine: Option<Vic20>) {
        if let Some(machine) = &machine {
            let width = machine.framebuffer_width();
            let height = machine.framebuffer_height();
            self.rgba_width = width;
            self.rgba_height = height;
            self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        }
        self.machine = machine;
        self.update_rgba_framebuffer();
    }

    fn rebuild_machine(&mut self) {
        let (Some(kernal), Some(basic), Some(char_rom)) = (
            self.kernal_bytes.clone(),
            self.basic_bytes.clone(),
            self.char_bytes.clone(),
        ) else {
            self.machine = None;
            return;
        };
        let vic_model = match self.model.region() {
            emu198x_shell::Region::Pal => Vic20Model::Pal,
            _ => Vic20Model::Ntsc,
        };
        let mut machine = Vic20::new(kernal, basic, char_rom, vic_model, self.ram_expansion);
        if let Some(image) = self.cartridge_image.as_deref() {
            machine
                .insert_cartridge_bytes(image)
                .expect("retained cartridge was validated when loaded");
        }
        let width = machine.framebuffer_width();
        let height = machine.framebuffer_height();
        self.rgba_width = width;
        self.rgba_height = height;
        self.rgba_framebuffer = vec![0; (width * height * 4) as usize];
        self.machine = Some(machine);
        self.update_rgba_framebuffer();
    }

    fn update_rgba_framebuffer(&mut self) {
        let Some(machine) = self.machine.as_ref() else {
            self.rgba_framebuffer.fill(0);
            return;
        };
        for (index, &pixel) in machine.framebuffer().iter().enumerate() {
            let base = index * 4;
            self.rgba_framebuffer[base] = ((pixel >> 16) & 0xff) as u8;
            self.rgba_framebuffer[base + 1] = ((pixel >> 8) & 0xff) as u8;
            self.rgba_framebuffer[base + 2] = (pixel & 0xff) as u8;
            self.rgba_framebuffer[base + 3] = ((pixel >> 24) & 0xff) as u8;
        }
    }
}

impl MachineCore for Vic20Runtime {
    fn profile(&self) -> &MachineProfile {
        &self.profile
    }

    fn time(&self) -> MachineTime {
        self.time
    }

    fn reset(&mut self, _kind: ResetKind) {
        self.rebuild_machine();
        self.time = MachineTime::default();
    }

    fn load_media(&mut self, media: &MediaSet<'_>) -> Result<(), MachineError> {
        for image in &media.images {
            let slot = image.slot.as_ref();
            match image.kind {
                MediaKind::Cartridge if slot == "cartridge-1" => {
                    let machine =
                        self.machine
                            .as_mut()
                            .ok_or_else(|| MachineError::MissingFirmware {
                                id: KERNAL_FIRMWARE_ID.to_owned(),
                            })?;
                    machine
                        .insert_cartridge_bytes(image.bytes)
                        .map_err(|reason| MachineError::InvalidMedia {
                            slot: slot.to_owned(),
                            reason,
                        })?;
                    self.cartridge_image = Some(image.bytes.to_vec());
                }
                MediaKind::Cartridge => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                MediaKind::Program if slot == "program-1" => {
                    let expansion = Self::expansion_for_prg(image.bytes, self.ram_expansion)
                        .map_err(|reason| MachineError::InvalidMedia {
                            slot: slot.to_owned(),
                            reason,
                        })?;
                    if expansion != self.ram_expansion {
                        self.ram_expansion = expansion;
                        self.rebuild_machine();
                    }
                    self.pending_prg = Some(image.bytes.to_vec());
                }
                MediaKind::Program => {
                    return Err(MachineError::UnknownMediaSlot {
                        slot: slot.to_owned(),
                    });
                }
                kind => return Err(MachineError::UnsupportedMediaKind { kind }),
            }
        }
        Ok(())
    }

    fn run_until(
        &mut self,
        target: MachineTime,
        host: &mut HostIo<'_>,
    ) -> Result<RunResult, MachineError> {
        if self.machine.is_none() {
            return Ok(RunResult::new(self.time, StopReason::WaitingForInput));
        }

        for event in host.input_events {
            if let Some(machine) = self.machine.as_mut() {
                apply_input_event(machine, &mut self.controller_cache, event);
            }
        }

        while self.time < target {
            let machine = self.machine.as_mut().expect("machine checked above");
            let ticks = if let Some(bridge) = &mut self.esp_at_tcp_bridge {
                machine.run_frame_with_user_port(&mut |cb2| bridge.tick(cb2))
            } else if let Some(modem) = &mut self.esp_at_modem {
                machine.run_frame_with_user_port(&mut |cb2| modem.tick(cb2))
            } else if let Some(serial) = &mut self.user_port_serial {
                machine.run_frame_with_user_port(&mut |cb2| serial.tick(cb2))
            } else {
                machine.run_frame()
            };
            // Drain the VIC's audio for the frame just run before releasing the
            // machine borrow for the framebuffer conversion below.
            let audio = machine.take_vic_audio();
            let inject_prg =
                self.pending_prg.is_some() && machine.frame_count() >= PRG_AUTOLOAD_FRAME;
            self.time = self.time.saturating_add(ticks);
            if inject_prg {
                let bytes = self.pending_prg.take().expect("checked above");
                self.autoload_prg(&bytes, false)
                    .map_err(|reason| MachineError::InvalidMedia {
                        slot: "program-1".to_owned(),
                        reason,
                    })?;
            }
            self.update_rgba_framebuffer();

            host.frame_sink.push_frame(FramePacket {
                timestamp: self.time,
                format: PixelFormat::Rgba8888,
                width: self.rgba_width,
                height: self.rgba_height,
                palette: None,
                pixels: &self.rgba_framebuffer,
            })?;

            host.audio_sink.push_audio(AudioPacket {
                timestamp: self.time,
                sample_rate: AUDIO_SAMPLE_RATE,
                channels: 1,
                samples: &audio,
            })?;
        }

        Ok(RunResult::new(self.time, StopReason::ReachedTarget))
    }

    fn snapshot(&self) -> Result<Vec<u8>, MachineError> {
        snapshot::encode(self)
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        snapshot::decode(self, bytes)
    }

    fn command(&mut self, command: &ControlCommand) -> Result<(), MachineError> {
        Err(MachineError::UnsupportedOperation {
            operation: command.operation_name(),
        })
    }

    /// Eight pixels per machine cycle. On NTSC that is the VIC-II's clock, so a
    /// VIC-20 and a C64 share a pixel shape there and diverge on PAL, where
    /// their cycle rates differ.
    fn display(&self) -> Option<Display> {
        Display::television_for_region(
            self.profile().region,
            mos_vic_i::PAL_PIXEL_CLOCK_HZ,
            mos_vic_i::NTSC_PIXEL_CLOCK_HZ,
        )
    }

    fn capabilities(&self) -> CapabilitySet {
        self.profile.capabilities.clone()
    }

    emu198x_shell::debug_target_hooks!();
    fn keyboard_target(&self) -> Option<&dyn emu198x_shell::KeyboardTarget> {
        self.machine
            .is_some()
            .then_some(&KEYBOARD as &dyn emu198x_shell::KeyboardTarget)
    }
}

// 6502 debug target via the shared macro (lazy `machine: Option<Vic20>`).
emu198x_shell::impl_6502_debug_primitives!(Vic20Runtime);

#[cfg(test)]
mod tests {
    use super::*;
    use emu198x_shell::MediaImage;

    fn load_program(runtime: &mut Vic20Runtime, bytes: &[u8]) -> Result<(), MachineError> {
        let mut media = MediaSet::new();
        media.push(MediaImage::new("program-1", MediaKind::Program, bytes));
        runtime.load_media(&media)
    }

    #[test]
    fn program_media_selects_the_expansion_from_its_load_address() {
        for (load, expected) in [
            (0x0401u16, Vic20RamExpansion::EXP_3K),
            (0x1001, Vic20RamExpansion::NONE),
            (0x1201, Vic20RamExpansion::EXP_8K),
        ] {
            let mut runtime = Vic20Runtime::blank(Model::Vic20Ntsc);
            let [lo, hi] = load.to_le_bytes();
            load_program(&mut runtime, &[lo, hi, 0x00]).expect("valid PRG");
            assert_eq!(runtime.ram_expansion, expected, "load ${load:04X}");
            assert_eq!(runtime.pending_prg.as_deref(), Some(&[lo, hi, 0x00][..]));
        }
    }

    #[test]
    fn expanded_basic_prg_keeps_cartridges_beyond_the_one_it_needs() {
        // BLK2, BLK3 and the 3K expander all leave the BASIC start at $1201,
        // so inference must add BLK1 without evicting anything already fitted.
        for spec in ["8k", "16k", "24k", "3k+8k", "24k+8k@a000"] {
            let requested = Vic20RamExpansion::parse(spec).expect("valid spec");
            let mut runtime = Vic20Runtime::blank(Model::Vic20Ntsc);
            runtime.set_ram_expansion(requested);
            load_program(&mut runtime, &[0x01, 0x12, 0x00]).expect("valid PRG");
            assert_eq!(runtime.ram_expansion, requested, "{spec}");
        }
    }

    #[test]
    fn expanded_basic_prg_fits_blk1_when_it_is_missing() {
        for (spec, expected) in [
            ("none", "8k"),
            ("3k", "3k+8k"),
            ("8k@4000", "16k"),
            ("8k@a000", "8k+8k@a000"),
        ] {
            let mut runtime = Vic20Runtime::blank(Model::Vic20Ntsc);
            runtime.set_ram_expansion(Vic20RamExpansion::parse(spec).expect("valid spec"));
            load_program(&mut runtime, &[0x01, 0x12, 0x00]).expect("valid PRG");
            assert_eq!(runtime.ram_expansion.to_string(), expected, "from {spec}");
        }
    }

    #[test]
    fn a_3k_program_evicts_the_blocks_that_would_move_the_basic_start() {
        // $0401 is only reachable with the 3K expander and no BLK1.
        let mut runtime = Vic20Runtime::blank(Model::Vic20Ntsc);
        runtime.set_ram_expansion(Vic20RamExpansion::EXP_24K);
        load_program(&mut runtime, &[0x01, 0x04, 0x00]).expect("valid PRG");
        assert_eq!(runtime.ram_expansion, Vic20RamExpansion::EXP_3K);
    }

    #[test]
    fn short_program_media_is_rejected_at_the_slot_boundary() {
        let mut runtime = Vic20Runtime::blank(Model::Vic20Ntsc);
        let error = load_program(&mut runtime, &[0x01, 0x12]).expect_err("header only");
        assert!(
            matches!(error, MachineError::InvalidMedia { ref slot, .. } if slot == "program-1")
        );
    }
}

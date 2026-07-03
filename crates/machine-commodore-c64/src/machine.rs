//! Board-level C64 machine substrate.

use common_commodore_c64::timing::C64Timing;
use common_commodore_iec::IecBus;
use format_commodore_c64_crt::{CrtCartridge, parse as parse_crt};
use format_commodore_c64_tap::{TapParseError, TapSystem, TapVideo, encode_tap, parse_tap};
use mos_6502::M6502;
use mos_cia_6526::Cia6526;
use mos_sid_6581::{AudioControls, Sid6581, SidChannel};
use mos_vic_ii::{Vic, VicModel};

use crate::config::{C64Config, C64Model};
use crate::datasette::Datasette;
use crate::keyboard::KeyboardMatrix;
use crate::memory::{C64Memory, C64MemorySnapshot, CartBankPair, CartBanking, MemoryInitError};

const AUDIO_SAMPLE_RATE: u32 = 48_000;
const PORT_INPUT_PULLUPS: u8 = 0x37;

/// Fresh-workspace C64 machine substrate.
#[derive(Clone)]
pub struct C64 {
    model: C64Model,
    cpu: M6502,
    vic: Vic,
    cia1: Cia6526,
    cia2: Cia6526,
    sid: Sid6581,
    datasette: Datasette,
    memory: C64Memory,
    keyboard: KeyboardMatrix,
    /// RESTORE key state. RESTORE is not on the keyboard matrix — it is
    /// wired straight to the CPU `/NMI` line (in parallel with CIA #2), so
    /// pressing it pulses an NMI. Held high here while pressed; the 6502's
    /// edge-trigger fires the NMI once and not again until release+repress.
    /// Host momentary input — not part of the snapshot.
    restore_nmi: bool,
    joysticks: [JoystickState; 2],
    /// Paddle pot positions, `[port][axis]` (port 0/1, axis 0 = X, 1 = Y).
    /// Lines default open (`0xFF`) until a host axis arrives; the CIA #1 mux
    /// then routes the selected port to the SID POTX/POTY. See
    /// [`Self::refresh_paddle_pots`].
    paddles: [[u8; 2]; 2],
    /// A 1351 proportional mouse per control port (index 0 = port 1, 1 =
    /// port 2), `None` when nothing is plugged in. A plugged mouse overrides
    /// that port's paddle pots and adds its two button lines. Host momentary
    /// motion/buttons arrive through [`Self::move_mouse_1351`] /
    /// [`Self::set_mouse_1351_button`].
    mice: [Option<Mouse1351>; 2],
    phi2_cycles: u64,
    frame_count: u64,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct C64Snapshot {
    model: C64Model,
    cpu: M6502,
    vic: Vic,
    cia1: Cia6526,
    cia2: Cia6526,
    sid: Sid6581,
    datasette: Datasette,
    memory: C64MemorySnapshot,
    keyboard: KeyboardMatrix,
    joysticks: [JoystickState; 2],
    paddles: [[u8; 2]; 2],
    #[serde(default)]
    mice: [Option<Mouse1351>; 2],
    phi2_cycles: u64,
    frame_count: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct JoystickState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    fire: bool,
}

impl JoystickState {
    fn set_control(&mut self, name: &str, pressed: bool) -> bool {
        match name.to_ascii_uppercase().as_str() {
            "UP" => self.up = pressed,
            "DOWN" => self.down = pressed,
            "LEFT" => self.left = pressed,
            "RIGHT" => self.right = pressed,
            "FIRE" => self.fire = pressed,
            _ => return false,
        }
        true
    }

    const fn input_mask(self) -> u8 {
        let mut value = 0xFF;
        if self.up {
            value &= !0x01;
        }
        if self.down {
            value &= !0x02;
        }
        if self.left {
            value &= !0x04;
        }
        if self.right {
            value &= !0x08;
        }
        if self.fire {
            value &= !0x10;
        }
        value
    }
}

/// A Commodore 1351 proportional mouse plugged into one control port.
///
/// The 1351 reports movement as *deltas on the analogue POT lines*, not as
/// an absolute position: each axis is a free-running counter whose low 7 bits
/// reach the SID pot register offset by `0x40`, so a read returns `0x40..=0xBF`
/// (`mouse_get_1351_x` in VICE `mouse_1351.c`). Software reads the pot twice
/// and sign-extends the 7-bit difference to recover the movement since the
/// last read. The two buttons are digital lines shared with the joystick:
/// left → FIRE (bit 4), right → UP (bit 0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Mouse1351 {
    /// Free-running X/Y position counters. Only the low 7 bits are observable;
    /// `i16` keeps a wide accumulation range so fast host motion wraps cleanly.
    x: i16,
    y: i16,
    left: bool,
    right: bool,
}

impl Mouse1351 {
    /// The POT reading for one axis (0 = X, 1 = Y): the low 7 bits of the
    /// running counter, offset by `0x40`. Matches VICE `mouse_get_1351_x/y`.
    const fn pot(self, axis: usize) -> u8 {
        let counter = if axis == 0 { self.x } else { self.y };
        ((counter & 0x7F) as u8) + 0x40
    }

    /// Active-low digital-line mask for the two buttons, ANDed into the port:
    /// left → FIRE (bit 4), right → UP (bit 0), per VICE
    /// `mouse_1351_button_left/right`.
    const fn digital_mask(self) -> u8 {
        let mut value = 0xFF;
        if self.left {
            value &= !0x10;
        }
        if self.right {
            value &= !0x01;
        }
        value
    }
}

/// Selects the paddle pot value reaching the SID for a CIA #1 mux mask
/// (`(PRA >> 6) & 3`): 1 = control port 1, 2 = control port 2, 3 = both pots in
/// parallel, 0 = open line (`0xFF`). Adapted from VICE's `read_joyport_potx`.
fn select_paddle_pot(mask: u8, port1: u8, port2: u8) -> u8 {
    match mask {
        1 => port1,
        2 => port2,
        3 => parallel_paddle(port1, port2),
        _ => 0xFF,
    }
}

/// Combines two paddle pots wired in parallel. Following VICE's resistor model
/// (`calc_parallel_paddle_value`): a pot at `0` (tied to VCC) forces `0`; an
/// open pot (`255`) yields the other; otherwise the parallel resistance, which
/// reduces to `t1·t2 / (t1+t2)` once the common scale cancels.
fn parallel_paddle(t1: u8, t2: u8) -> u8 {
    if t1 == 0 || t2 == 0 {
        return 0;
    }
    if t1 == 255 {
        return t2;
    }
    if t2 == 255 {
        return t1;
    }
    let r = (u16::from(t1) * u16::from(t2)) / (u16::from(t1) + u16::from(t2));
    u8::try_from(r.min(255)).unwrap_or(255)
}

/// One fixed 8K cartridge bank.
type CartBank = Box<[u8; 0x2000]>;

/// Pads or truncates a ROM image slice into one fixed 8K cartridge bank.
fn to_bank(data: &[u8]) -> CartBank {
    let mut bank = Box::new([0u8; 0x2000]);
    let len = data.len().min(0x2000);
    bank[..len].copy_from_slice(&data[..len]);
    bank
}

/// Maps a generic (hardware type 0) cartridge's CHIP packets into the ROML
/// (`$8000`) and ROMH (`$A000`/`$E000`) 8K banks the base PLA exposes.
///
/// The base machine only maps bank 0, so bank-switched carts collapse to their
/// first bank. A single `$8000` chip of 16K is split across ROML and ROMH; two
/// chips at `$8000` and `$A000`/`$E000` fill the banks directly.
fn map_generic_cartridge(
    cart: &CrtCartridge,
) -> Result<(Option<CartBank>, Option<CartBank>), String> {
    let mut roml = None;
    let mut romh = None;
    for chip in cart.chips.iter().filter(|chip| chip.bank == 0) {
        match chip.load_address {
            0x8000 => {
                roml = Some(to_bank(&chip.data));
                // A 16K image carried in one $8000 chip fills ROMH too.
                if chip.data.len() > 0x2000 {
                    romh = Some(to_bank(&chip.data[0x2000..]));
                }
            }
            0xA000 | 0xE000 => romh = Some(to_bank(&chip.data)),
            other => {
                return Err(format!("unsupported cartridge load address ${other:04X}"));
            }
        }
    }

    if roml.is_none() && romh.is_none() {
        return Err("cartridge has no ROM image for the mapped windows".to_owned());
    }
    Ok((roml, romh))
}

/// The bank-switching scheme a cartridge hardware type drives, or `None` for the
/// unbanked types the generic mapper handles. Returns an error for hardware
/// types the base machine does not model.
fn banking_for_hardware_type(hardware_type: u16) -> Result<Option<CartBanking>, String> {
    match hardware_type {
        0 => Ok(None),
        5 => Ok(Some(CartBanking::Ocean)),
        19 => Ok(Some(CartBanking::MagicDesk)),
        other => Err(format!("unsupported cartridge hardware type {other}")),
    }
}

/// Maps a simple bank-switched cartridge's CHIP packets into an indexed vector
/// of 8K banks. Each CHIP's `bank` field selects the slot; `$8000` fills ROML
/// (a 16K chip also fills that bank's ROMH), `$A000`/`$E000` fill ROMH.
fn map_banked_cartridge(cart: &CrtCartridge) -> Result<Vec<CartBankPair>, String> {
    let bank_count = cart
        .chips
        .iter()
        .map(|chip| usize::from(chip.bank) + 1)
        .max()
        .unwrap_or(0);
    if bank_count == 0 {
        return Err("cartridge has no CHIP banks".to_owned());
    }

    let mut banks: Vec<CartBankPair> = (0..bank_count)
        .map(|_| CartBankPair {
            roml: None,
            romh: None,
        })
        .collect();
    for chip in &cart.chips {
        let slot = &mut banks[usize::from(chip.bank)];
        match chip.load_address {
            0x8000 => {
                slot.roml = Some(to_bank(&chip.data));
                if chip.data.len() > 0x2000 {
                    slot.romh = Some(to_bank(&chip.data[0x2000..]));
                }
            }
            0xA000 | 0xE000 => slot.romh = Some(to_bank(&chip.data)),
            other => {
                return Err(format!("unsupported cartridge load address ${other:04X}"));
            }
        }
    }
    Ok(banks)
}

impl C64 {
    /// Constructs a new C64 machine substrate from ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if any ROM size is incorrect.
    pub fn new(config: C64Config<'_>) -> Result<Self, MemoryInitError> {
        let memory = C64Memory::new(config.kernal_rom, config.basic_rom, config.character_rom)?;
        let mut cpu = M6502::new();
        cpu.reset();
        let timing = config.model.timing();

        let mut cia1 = Cia6526::new_with_tod(timing.cia_tod_divider);
        cia1.write(0x02, 0xFF);
        cia1.write(0x03, 0x00);
        cia1.write(0x00, 0xFF);

        let mut cia2 = Cia6526::new_with_tod(timing.cia_tod_divider);
        cia2.write(0x02, 0x03);
        cia2.write(0x00, 0x03);

        let vic_model = match config.model {
            C64Model::PalBreadbin | C64Model::PalC64c => VicModel::Pal6569,
            C64Model::NtscBreadbin | C64Model::NtscC64c => VicModel::Ntsc6567,
        };
        let mut vic = Vic::new(vic_model);
        vic.set_bank(0);
        let sid =
            Sid6581::new_with_model(timing.cpu_hz, AUDIO_SAMPLE_RATE, config.model.sid_model());

        let mut machine = Self {
            model: config.model,
            cpu,
            vic,
            cia1,
            cia2,
            sid,
            datasette: Datasette::new(),
            memory,
            keyboard: KeyboardMatrix::new(),
            restore_nmi: false,
            joysticks: [JoystickState::default(); 2],
            paddles: [[0xFF; 2]; 2],
            mice: [None; 2],
            phi2_cycles: 0,
            frame_count: 0,
        };
        machine.refresh_keyboard_scan();
        machine.refresh_vic_bank();
        machine.refresh_datasette_port_lines();
        Ok(machine)
    }

    /// Hardware model.
    #[must_use]
    pub const fn model(&self) -> C64Model {
        self.model
    }

    /// CPU state.
    #[must_use]
    pub fn cpu(&self) -> &M6502 {
        &self.cpu
    }

    /// Mutable CPU state, for test harnesses that seed registers / PC to jump
    /// straight into a program (matches the `cpu_mut` hook the other machine
    /// cores expose). Not used on the normal boot/run path.
    pub fn cpu_mut(&mut self) -> &mut M6502 {
        &mut self.cpu
    }

    /// Side-effect-free debugger read of CPU-visible memory.
    ///
    /// Honors PLA banking via the memory subsystem but — unlike
    /// [`cpu_read`](Self::cpu_read) — never touches live I/O registers (whose
    /// reads can clear latches and so on). The byte returned for the
    /// `$D000`–`$DFFF` window is the banked RAM / character ROM underneath.
    /// Adequate for the disassembler and memory inspector, whose addresses
    /// point into RAM or ROM, not live I/O.
    #[must_use]
    pub fn peek(&self, addr: u16) -> u8 {
        self.memory.cpu_read(addr)
    }

    /// Debugger write to CPU-visible RAM, honoring banking. The byte lands in
    /// the RAM beneath any ROM or I/O without triggering I/O side effects.
    pub fn poke(&mut self, addr: u16, value: u8) {
        self.memory.cpu_write(addr, value);
    }

    /// Runs exactly one whole CPU instruction, returning the `phi2` cycles
    /// consumed. Ticks off the current instruction boundary, then on to the
    /// next — the two-phase shape the other 6502 cores use, so a one-cycle
    /// boundary flag never over- or under-runs the step. Bounded against a
    /// wedged CPU.
    pub fn step_instruction(&mut self) -> u64 {
        let start = self.phi2_cycles;
        let cap = start + 1024;
        while self.cpu.instruction_complete() && self.phi2_cycles < cap {
            self.tick();
        }
        while !self.cpu.instruction_complete() && self.phi2_cycles < cap {
            self.tick();
        }
        self.phi2_cycles - start
    }

    /// VIC-II state.
    #[must_use]
    pub const fn vic(&self) -> &Vic {
        &self.vic
    }

    /// CIA1 state.
    #[must_use]
    pub const fn cia1(&self) -> &Cia6526 {
        &self.cia1
    }

    /// CIA2 state.
    #[must_use]
    pub const fn cia2(&self) -> &Cia6526 {
        &self.cia2
    }

    /// Timing descriptor for the current model.
    #[must_use]
    pub const fn timing(&self) -> C64Timing {
        self.model.timing()
    }

    /// Underlying memory subsystem.
    #[must_use]
    pub const fn memory(&self) -> &C64Memory {
        &self.memory
    }

    /// Mutable access to the keyboard matrix.
    #[must_use]
    pub fn keyboard_mut(&mut self) -> &mut KeyboardMatrix {
        &mut self.keyboard
    }

    /// Press or release the RESTORE key. RESTORE is wired to the CPU `/NMI`
    /// line (not the keyboard matrix), so a press pulses an NMI — held with
    /// Run/Stop down, the KERNAL NMI handler performs a warm reset.
    pub fn set_restore(&mut self, pressed: bool) {
        self.restore_nmi = pressed;
    }

    /// Sets one joystick control on controller port 1 or 2.
    ///
    /// Returns `false` when the port or control name is unknown.
    pub fn set_joystick_control(&mut self, port: u8, name: &str, pressed: bool) -> bool {
        let Some(joystick) = self.joystick_mut(port) else {
            return false;
        };
        if !joystick.set_control(name, pressed) {
            return false;
        }
        self.refresh_keyboard_scan();
        true
    }

    /// Sets a paddle pot position on controller port 1 or 2. `axis` 0 = X
    /// (POTX), 1 = Y (POTY); `value` is the 8-bit pot reading (`0..=255`).
    /// The value surfaces at the SID POTX/POTY once CIA #1 selects the port.
    /// Returns `false` for an unknown port or axis.
    pub fn set_paddle(&mut self, port: u8, axis: u8, value: u8) -> bool {
        let Some(slot) = self
            .paddles
            .get_mut(usize::from(port.wrapping_sub(1)))
            .and_then(|p| p.get_mut(usize::from(axis)))
        else {
            return false;
        };
        *slot = value;
        self.refresh_paddle_pots();
        true
    }

    /// Plugs a 1351 proportional mouse into control port 1 or 2, centred with
    /// both buttons released. Overrides that port's paddle pots while attached.
    /// Returns `false` for an unknown port.
    pub fn attach_mouse_1351(&mut self, port: u8) -> bool {
        let Some(slot) = self.mouse_slot_mut(port) else {
            return false;
        };
        *slot = Some(Mouse1351::default());
        self.refresh_paddle_pots();
        true
    }

    /// Unplugs the 1351 mouse from control port 1 or 2, returning that port's
    /// paddle pots to the POT lines. Returns `false` for an unknown port.
    pub fn detach_mouse_1351(&mut self, port: u8) -> bool {
        let Some(slot) = self.mouse_slot_mut(port) else {
            return false;
        };
        *slot = None;
        self.refresh_paddle_pots();
        true
    }

    /// Whether a 1351 mouse is plugged into control port 1 or 2.
    #[must_use]
    pub fn has_mouse_1351(&self, port: u8) -> bool {
        matches!(port, 1 | 2) && self.mice[usize::from(port - 1)].is_some()
    }

    /// Accumulates a host mouse-motion delta into the 1351 on control port 1
    /// or 2. The deltas move the free-running POT counters directly; the guest
    /// reads the pots twice and diffs. Returns `false` if no mouse is plugged
    /// into that port.
    pub fn move_mouse_1351(&mut self, port: u8, dx: i32, dy: i32) -> bool {
        let Some(Some(mouse)) = self.mouse_slot_mut(port) else {
            return false;
        };
        mouse.x = mouse.x.wrapping_add(dx as i16);
        mouse.y = mouse.y.wrapping_add(dy as i16);
        self.refresh_paddle_pots();
        true
    }

    /// Presses or releases a 1351 button (`"left"` or `"right"`) on control
    /// port 1 or 2. Returns `false` for an unknown port, button, or an empty
    /// port. The buttons surface on the joystick digital lines: left → FIRE,
    /// right → UP.
    pub fn set_mouse_1351_button(&mut self, port: u8, button: &str, pressed: bool) -> bool {
        let Some(Some(mouse)) = self.mouse_slot_mut(port) else {
            return false;
        };
        match button.to_ascii_lowercase().as_str() {
            "left" | "fire" => mouse.left = pressed,
            "right" => mouse.right = pressed,
            _ => return false,
        }
        self.refresh_keyboard_scan();
        true
    }

    fn mouse_slot_mut(&mut self, port: u8) -> Option<&mut Option<Mouse1351>> {
        match port {
            1 => Some(&mut self.mice[0]),
            2 => Some(&mut self.mice[1]),
            _ => None,
        }
    }

    /// `phi2` cycles elapsed since construction.
    #[must_use]
    pub const fn phi2_cycles(&self) -> u64 {
        self.phi2_cycles
    }

    /// Completed video frames.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Current raster line within the frame.
    #[must_use]
    pub fn raster_line(&self) -> u16 {
        self.vic.raster_line()
    }

    /// Current `phi2` cycle within the raster line.
    #[must_use]
    pub fn cycle_in_line(&self) -> u8 {
        self.vic.raster_cycle()
    }

    /// Current VIC bank selected by CIA2 port A bits 0-1, inverted.
    #[must_use]
    pub fn vic_bank(&self) -> u8 {
        self.vic.bank()
    }

    /// Current CIA1 Port B input value after keyboard scan.
    #[must_use]
    pub const fn cia1_port_b_input(&self) -> u8 {
        self.cia1.pb_in
    }

    /// Reads one live VIC-II register without side effects.
    #[must_use]
    pub fn vic_register(&self, index: u8) -> u8 {
        self.vic.peek(index)
    }

    /// Live SID state.
    #[must_use]
    pub const fn sid(&self) -> &Sid6581 {
        &self.sid
    }

    /// Returns `true` when one tape image is currently inserted.
    #[must_use]
    pub const fn tape_is_loaded(&self) -> bool {
        self.datasette.is_loaded()
    }

    /// Returns `true` when the datasette sense line is active.
    #[must_use]
    pub const fn tape_sense_active(&self) -> bool {
        self.datasette.sense_active()
    }

    /// Returns `true` when the datasette motor line is actively driving tape motion.
    #[must_use]
    pub const fn tape_motor_on(&self) -> bool {
        self.datasette.motor_on()
    }

    /// Current position within the loaded TAP pulse stream.
    #[must_use]
    pub const fn tape_pulse_index(&self) -> usize {
        self.datasette.pulse_index()
    }

    /// Total number of pulses in the loaded TAP image.
    #[must_use]
    pub fn tape_pulse_count(&self) -> usize {
        self.datasette.pulse_count()
    }

    /// Returns `true` when the datasette transport is engaged.
    #[must_use]
    pub fn tape_is_playing(&self) -> bool {
        self.datasette.is_playing()
    }

    /// Output sample rate used by the machine-local SID mixer.
    #[must_use]
    pub const fn audio_sample_rate(&self) -> u32 {
        AUDIO_SAMPLE_RATE
    }

    /// Drains the current mixed SID output buffer.
    #[must_use]
    pub fn take_audio_buffer(&mut self) -> Vec<f32> {
        self.sid.take_buffer()
    }

    /// Current host-side SID audio controls.
    #[must_use]
    pub const fn audio_controls(&self) -> AudioControls {
        self.sid.audio_controls()
    }

    /// Replace all host-side SID audio controls.
    pub fn set_audio_controls(&mut self, controls: AudioControls) {
        self.sid.set_audio_controls(controls);
    }

    /// Enable or disable one SID voice in the host mixer.
    pub fn set_audio_channel_enabled(&mut self, channel: SidChannel, enabled: bool) {
        self.sid.set_audio_channel_enabled(channel, enabled);
    }

    /// Set one SID voice's host mixer gain.
    pub fn set_audio_channel_gain(&mut self, channel: SidChannel, gain: f32) {
        self.sid.set_audio_channel_gain(channel, gain);
    }

    /// Borrow the VIC-II framebuffer.
    #[must_use]
    pub fn framebuffer(&self) -> &[u32] {
        self.vic.framebuffer()
    }

    /// Loads one Commodore TAP image into the datasette.
    ///
    /// # Errors
    ///
    /// Returns an error if the TAP header or pulse stream is invalid.
    pub fn load_tap_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        let image = parse_tap(bytes).map_err(|reason| match reason {
            TapParseError::UnsupportedVersion { version } => {
                format!("unsupported TAP version {version}")
            }
            other => other.to_string(),
        })?;

        if image.system != TapSystem::C64 {
            return Err(format!(
                "expected a C64 TAP image, found {:?}",
                image.system
            ));
        }

        self.datasette.load_tap(image);
        self.refresh_datasette_port_lines();
        Ok(())
    }

    /// Mounts a blank, writable tape so a KERNAL `SAVE` records onto it. The
    /// recorded pulse stream is retrieved as a `.tap` image with
    /// [`Self::flush_tape_image`].
    pub fn insert_blank_writable_tape(&mut self) {
        let video = match self.model {
            C64Model::PalBreadbin | C64Model::PalC64c => TapVideo::Pal,
            C64Model::NtscBreadbin | C64Model::NtscC64c => TapVideo::Ntsc,
        };
        self.datasette.insert_blank_writable_tape(video);
        self.refresh_datasette_port_lines();
    }

    /// Encodes the recorded SAVE tape to `.tap` bytes, or `None` when no writable
    /// tape is mounted.
    #[must_use]
    pub fn flush_tape_image(&self) -> Option<Vec<u8>> {
        self.datasette.recorded_tap_image().map(encode_tap)
    }

    /// Inserts one Commodore `.crt` cartridge image.
    ///
    /// Supports the generic unbanked types (hardware type 0 — plain 8K/16K and
    /// Ultimax) and the simple bank-switched types Ocean (5) and Magic Desk
    /// (19), which select one of several 8K banks through a `$DE00` register.
    /// The caller resets the machine afterwards so the KERNAL runs the
    /// cartridge's cold-start vector (or, for Ultimax, so `$FFFC` fetches from
    /// cartridge ROMH).
    ///
    /// # Errors
    ///
    /// Returns an error if the CRT image is malformed, uses an unsupported
    /// hardware type, or carries no ROM image for the `$8000`/`$A000`/`$E000`
    /// windows the base machine maps.
    pub fn insert_crt_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        let cart = parse_crt(bytes).map_err(|reason| reason.to_string())?;
        match banking_for_hardware_type(cart.hardware_type)? {
            None => {
                let (roml, romh) = map_generic_cartridge(&cart)?;
                self.memory
                    .insert_cartridge(cart.exrom, cart.game, roml, romh);
            }
            Some(banking) => {
                let banks = map_banked_cartridge(&cart)?;
                self.memory
                    .insert_banked_cartridge(cart.exrom, cart.game, banking, banks);
            }
        }
        Ok(())
    }

    /// Removes any inserted cartridge, restoring the plain RAM/ROM map.
    pub fn remove_cartridge(&mut self) {
        self.memory.remove_cartridge();
    }

    /// Attaches a GeoRAM RAM expansion of `size_kb` KiB (typically 512, 1024, or
    /// 2048), zero-filled. Accessed through the `$DE00` window + `$DFFE`/`$DFFF`
    /// bank registers.
    pub fn attach_georam(&mut self, size_kb: usize) {
        self.memory.attach_georam(size_kb);
    }

    /// Detaches any GeoRAM expansion.
    pub fn detach_georam(&mut self) {
        self.memory.detach_georam();
    }

    /// Whether a GeoRAM expansion is attached.
    #[must_use]
    pub fn has_georam(&self) -> bool {
        self.memory.has_georam()
    }

    /// Attaches a 17xx REU of `size_kb` KiB (typically 128, 256, or 512),
    /// zero-filled. The DMA controller responds at `$DF00-$DF0A`.
    pub fn attach_reu(&mut self, size_kb: usize) {
        self.memory.attach_reu(size_kb);
    }

    /// Detaches any REU.
    pub fn detach_reu(&mut self) {
        self.memory.detach_reu();
    }

    /// Whether a REU is attached.
    #[must_use]
    pub fn has_reu(&self) -> bool {
        self.memory.has_reu()
    }

    /// Presses PLAY on the currently inserted datasette image.
    pub fn play_tape(&mut self) {
        self.datasette.play();
        self.refresh_datasette_port_lines();
    }

    /// Stops the datasette transport without ejecting the image.
    pub fn stop_tape(&mut self) {
        self.datasette.stop();
        self.refresh_datasette_port_lines();
    }

    /// Advances the board by one `phi2` cycle.
    ///
    /// Returns `true` when this tick completed a frame.
    pub fn tick(&mut self) -> bool {
        self.phi2_cycles = self.phi2_cycles.saturating_add(1);
        self.vic.tick(&self.memory);
        self.cia1.flag = !self.datasette.advance_phi2_cycle();
        self.refresh_keyboard_scan();
        self.cia1.tick();
        self.cia2.tick();
        self.refresh_paddle_pots();
        self.refresh_vic_bank();
        self.cpu.irq = self.vic.irq || self.cia1.irq || self.memory.reu_irq();
        self.cpu.nmi = self.cia2.irq || self.restore_nmi;
        self.cpu.rdy = !self.vic.ba_low || !self.cpu.rw;

        if self.cpu.rdy {
            if self.cpu.rw {
                self.cpu.data_in = self.cpu_read(self.cpu.addr);
            } else {
                self.cpu_write(self.cpu.addr, self.cpu.data);
            }
            self.cpu.tick();
        }
        self.sid.tick();

        if self.vic.take_frame_complete() {
            self.frame_count = self.frame_count.saturating_add(1);
            return true;
        }

        false
    }

    /// Advances the board by one `phi2` cycle with one shared IEC bus.
    ///
    /// Returns `true` when this tick completed a frame.
    pub fn tick_with_iec_bus(&mut self, bus: &mut IecBus) -> bool {
        self.phi2_cycles = self.phi2_cycles.saturating_add(1);
        self.vic.tick(&self.memory);
        self.cia1.flag = !self.datasette.advance_phi2_cycle();
        self.refresh_keyboard_scan();
        self.sync_iec_bus(bus);
        self.cia1.tick();
        self.cia2.tick();
        self.refresh_paddle_pots();
        self.refresh_vic_bank();
        self.cpu.irq = self.vic.irq || self.cia1.irq || self.memory.reu_irq();
        self.cpu.nmi = self.cia2.irq || self.restore_nmi;
        self.cpu.rdy = !self.vic.ba_low || !self.cpu.rw;

        if self.cpu.rdy {
            if self.cpu.rw {
                self.cpu.data_in = self.cpu_read_with_iec_bus(self.cpu.addr, bus);
            } else {
                self.cpu_write_with_iec_bus(self.cpu.addr, self.cpu.data, bus);
            }
            self.cpu.tick();
        }
        self.sid.tick();

        if self.vic.take_frame_complete() {
            self.frame_count = self.frame_count.saturating_add(1);
            return true;
        }

        false
    }

    /// Advances the board by a fixed number of `phi2` cycles.
    pub fn advance_phi2_cycles(&mut self, cycles: u64) {
        for _ in 0..cycles {
            self.tick();
        }
    }

    /// Advances exactly one frame and returns the number of cycles executed.
    pub fn run_frame(&mut self) -> u32 {
        let start = self.phi2_cycles;
        while !self.tick() {}
        (self.phi2_cycles - start) as u32
    }

    /// Loads one PRG file into raw RAM and returns its load address.
    ///
    /// This is a host-side import convenience, not an emulated disk or tape
    /// path. It matches the direct-RAM effect of a completed KERNAL LOAD.
    ///
    /// # Errors
    ///
    /// Returns an error if the PRG header is malformed.
    pub fn load_prg(&mut self, data: &[u8]) -> Result<u16, String> {
        format_commodore_c64_prg::load_prg(&mut self.memory, data)
    }

    /// Captures the machine state for runtime snapshot serialization.
    #[must_use]
    pub fn snapshot_state(&self) -> C64Snapshot {
        C64Snapshot {
            model: self.model,
            cpu: self.cpu.clone(),
            vic: self.vic.clone(),
            cia1: self.cia1.clone(),
            cia2: self.cia2.clone(),
            sid: self.sid.clone(),
            datasette: self.datasette.clone(),
            memory: self.memory.snapshot_state(),
            keyboard: self.keyboard.clone(),
            joysticks: self.joysticks,
            paddles: self.paddles,
            mice: self.mice,
            phi2_cycles: self.phi2_cycles,
            frame_count: self.frame_count,
        }
    }

    /// Restores a machine state produced by [`Self::snapshot_state`].
    ///
    /// # Errors
    ///
    /// Returns an error if the snapshot belongs to a different model or any
    /// captured memory image has the wrong size.
    pub fn restore_snapshot_state(&mut self, snapshot: C64Snapshot) -> Result<(), String> {
        if snapshot.model != self.model {
            return Err(format!(
                "snapshot model {:?} does not match machine model {:?}",
                snapshot.model, self.model
            ));
        }

        self.cpu = snapshot.cpu;
        self.vic = snapshot.vic;
        self.cia1 = snapshot.cia1;
        self.cia2 = snapshot.cia2;
        self.sid = snapshot.sid;
        self.datasette = snapshot.datasette;
        self.memory = C64Memory::from_snapshot(snapshot.memory)?;
        self.keyboard = snapshot.keyboard;
        self.joysticks = snapshot.joysticks;
        self.paddles = snapshot.paddles;
        self.mice = snapshot.mice;
        self.phi2_cycles = snapshot.phi2_cycles;
        self.frame_count = snapshot.frame_count;
        self.refresh_keyboard_scan();
        self.refresh_vic_bank();
        self.refresh_datasette_port_lines();
        Ok(())
    }

    /// CPU-visible read through banked memory and the current board I/O state.
    pub fn cpu_read(&mut self, addr: u16) -> u8 {
        if addr == 0x0001 {
            return self.cpu_port_read();
        }
        if (0xD000..=0xDFFF).contains(&addr) && self.memory.is_io_visible() {
            return self.io_read(addr);
        }
        self.memory.cpu_read(addr)
    }

    /// CPU-visible read through banked memory and one shared IEC bus.
    pub fn cpu_read_with_iec_bus(&mut self, addr: u16, bus: &mut IecBus) -> u8 {
        if addr == 0x0001 {
            return self.cpu_port_read();
        }
        if (0xD000..=0xDFFF).contains(&addr) && self.memory.is_io_visible() {
            if (0xDD00..=0xDDFF).contains(&addr) {
                self.sync_iec_bus(bus);
            }
            return self.io_read(addr);
        }
        self.memory.cpu_read(addr)
    }

    /// CPU-visible write through banked memory and the current board I/O state.
    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        self.memory.cpu_write(addr, value);
        if matches!(addr, 0x0000 | 0x0001) {
            self.refresh_datasette_port_lines();
        }
        if addr == 0xFF00 {
            self.memory.reu_ff00_write();
        }
        if (0xD000..=0xDFFF).contains(&addr) && self.memory.is_io_visible() {
            self.io_write(addr, value);
        }
    }

    /// CPU-visible write through banked memory and one shared IEC bus.
    pub fn cpu_write_with_iec_bus(&mut self, addr: u16, value: u8, bus: &mut IecBus) {
        self.memory.cpu_write(addr, value);
        if matches!(addr, 0x0000 | 0x0001) {
            self.refresh_datasette_port_lines();
        }
        if addr == 0xFF00 {
            self.memory.reu_ff00_write();
        }
        if (0xD000..=0xDFFF).contains(&addr) && self.memory.is_io_visible() {
            self.io_write(addr, value);
            if (0xDD00..=0xDDFF).contains(&addr) {
                self.drive_iec_outputs(bus);
                self.sync_iec_bus(bus);
            }
        }
    }

    /// Reads the current VIC-visible byte from the active bank.
    #[must_use]
    pub fn vic_read(&self, offset: u16) -> u8 {
        self.memory.vic_read(self.vic.bank(), offset)
    }

    fn io_read(&mut self, addr: u16) -> u8 {
        match addr {
            0xD000..=0xD3FF => self.vic.read((addr & 0x3F) as u8),
            0xD400..=0xD7FF => {
                let reg = (addr & 0x1F) as u8;
                // The paddle pots reflect the live CIA #1 mux selection at read
                // time, so refresh them before returning POTX/POTY.
                if reg == 0x19 || reg == 0x1A {
                    self.refresh_paddle_pots();
                }
                self.sid.read(reg)
            }
            0xD800..=0xDBFF => self.memory.colour_ram_read(addr - 0xD800),
            0xDC00..=0xDCFF => {
                self.refresh_keyboard_scan();
                match addr & 0x0F {
                    0x00 => self.cia1_port_a_read(),
                    0x01 => self.cia1_port_b_read(),
                    reg => self.cia1.read(reg as u8),
                }
            }
            0xDD00..=0xDDFF => self.cia2.read((addr & 0x0F) as u8),
            0xDE00..=0xDFFF => self
                .memory
                .reu_read(addr)
                .or_else(|| self.memory.georam_read(addr))
                .unwrap_or(0xFF),
            _ => 0xFF,
        }
    }

    fn io_write(&mut self, addr: u16, value: u8) {
        match addr {
            0xD000..=0xD3FF => self.vic.write((addr & 0x3F) as u8, value),
            0xD400..=0xD7FF => self.sid.write((addr & 0x1F) as u8, value),
            0xD800..=0xDBFF => self.memory.colour_ram_write(addr - 0xD800, value),
            0xDC00..=0xDCFF => {
                self.cia1.write((addr & 0x0F) as u8, value);
                self.refresh_keyboard_scan();
            }
            0xDD00..=0xDDFF => {
                self.cia2.write((addr & 0x0F) as u8, value);
                self.refresh_vic_bank();
            }
            0xDE00..=0xDFFF => self.memory.expansion_io_write(addr, value),
            _ => {}
        }
    }

    fn refresh_keyboard_scan(&mut self) {
        self.cia1.pa_in = self.joystick_input(2);
        self.cia1.pb_in = self.keyboard.scan(self.cia1.pa) & self.joystick_input(1);
    }

    /// Drives the SID pot inputs from the paddle multiplexer. The C64 wires both
    /// control ports' paddle pots to the SID's single POTX/POTY pair through a
    /// 4066 analogue switch selected by CIA #1 port A bits 6-7. See
    /// [`select_paddle_pot`].
    fn refresh_paddle_pots(&mut self) {
        let mask = (self.cia1.port_a_drive_state() >> 6) & 0x03;
        self.sid.potx = select_paddle_pot(mask, self.port_pot(0, 0), self.port_pot(1, 0));
        self.sid.poty = select_paddle_pot(mask, self.port_pot(0, 1), self.port_pot(1, 1));
    }

    /// The POT reading a control port drives onto the SID line, before the
    /// CIA #1 mux selects it. A plugged 1351 mouse overrides the paddle pot;
    /// otherwise the paddle position stands (open line `0xFF` by default).
    fn port_pot(&self, port_idx: usize, axis: usize) -> u8 {
        match self.mice[port_idx] {
            Some(mouse) => mouse.pot(axis),
            None => self.paddles[port_idx][axis],
        }
    }

    fn refresh_vic_bank(&mut self) {
        self.vic.set_bank((!self.cia2.pa) & 0x03);
    }

    pub fn sync_iec_bus(&mut self, bus: &mut IecBus) {
        self.cia2.pa_in = (self.cia2.pa_in & 0x3F) | (bus.cpu_port() & 0xC0);
    }

    fn drive_iec_outputs(&self, bus: &mut IecBus) {
        // The C64 serial bus lines on CIA2 Port A are active-low. VICE feeds
        // the IEC layer with `~(PRA | ~DDRA)`, not the mixed port-drive state
        // directly.
        bus.write_cpu_port_a(!self.cia2.port_a_drive_state());
    }

    fn cpu_port_read(&self) -> u8 {
        let ddr = self.memory.port_ddr();
        let data = self.memory.port_data();
        let mut value = (data & ddr) | (PORT_INPUT_PULLUPS & !ddr);

        if self.datasette.sense_active() && (ddr & 0x10) == 0 {
            value &= !0x10;
        }

        if self.datasette.write_input_active() && (ddr & 0x08) == 0 {
            value &= !0x08;
        }

        value
    }

    fn refresh_datasette_port_lines(&mut self) {
        let ddr = self.memory.port_ddr();
        let data = self.memory.port_data();
        let motor_on = (ddr & 0x20) != 0 && (data & 0x20) == 0;
        self.datasette.set_motor_on(motor_on);
        // Cassette WRITE line = 6510 port bit 3 (the SAVE routine toggles it).
        self.datasette
            .set_write_line((self.memory.effective_port() & 0x08) != 0);
    }

    fn joystick_mut(&mut self, port: u8) -> Option<&mut JoystickState> {
        match port {
            1 => Some(&mut self.joysticks[0]),
            2 => Some(&mut self.joysticks[1]),
            _ => None,
        }
    }

    fn joystick_input(&self, port: u8) -> u8 {
        let idx = match port {
            1 => 0,
            2 => 1,
            _ => return 0xFF,
        };
        // A 1351's buttons share the joystick digital lines, so fold its mask
        // in alongside any joystick plugged into the same port.
        let mouse_mask = self.mice[idx].map_or(0xFF, Mouse1351::digital_mask);
        self.joysticks[idx].input_mask() & mouse_mask
    }

    fn cia1_port_a_read(&self) -> u8 {
        self.cia1.port_a_drive_state() & self.joystick_input(2)
    }

    fn cia1_port_b_read(&self) -> u8 {
        self.cia1.port_b_drive_state() & self.keyboard.scan(self.cia1.pa) & self.joystick_input(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datasette::MOTOR_DELAY_CYCLES;
    use common_commodore_iec::IecBus;
    use machine_commodore_1541::{Drive1541, Drive1541Config};
    use std::fs;
    use std::path::PathBuf;

    fn stub_machine(model: C64Model) -> C64 {
        stub_machine_with_reset_vector(model, 0xE000)
    }

    fn stub_machine_with_reset_vector(model: C64Model, start_pc: u16) -> C64 {
        let mut kernal = [0xEA; 0x2000];
        kernal[0x1FFC] = start_pc as u8;
        kernal[0x1FFD] = (start_pc >> 8) as u8;
        C64::new(C64Config {
            model,
            kernal_rom: &kernal,
            basic_rom: &[0xBB; 0x2000],
            character_rom: &[0xCC; 0x1000],
        })
        .expect("stub ROM sizes should be valid")
    }

    fn c64_rom_dir() -> PathBuf {
        PathBuf::from(
            std::env::var("HOME").expect("HOME should be available for ROM-backed C64 tests"),
        )
        .join(".emu198x/roms/commodore-c64")
    }

    #[test]
    fn constructs_with_expected_initial_state() {
        let machine = stub_machine(C64Model::PalBreadbin);
        assert_eq!(machine.phi2_cycles(), 0);
        assert_eq!(machine.frame_count(), 0);
        assert_eq!(machine.raster_line(), 0);
        assert_eq!(machine.cycle_in_line(), 0);
        assert_eq!(machine.vic_bank(), 0);
        // Post-reset the CPU is in the 7-cycle reset sequence; the
        // first five cycles are phantom stack reads, so addr is
        // SP-relative, not yet on the reset vector. Only rw=read and
        // !sync are observable right away.
        assert_eq!(machine.cpu().reset_phase, 7);
        assert!(machine.cpu().rw);
        assert!(!machine.cpu().sync);
    }

    fn drive_rom() -> [u8; 0x4000] {
        let mut rom = [0xEA; 0x4000];
        rom[0x3FFC] = 0x00;
        rom[0x3FFD] = 0xC0;
        rom
    }

    /// Build a minimal generic CRT image: 64-byte header + one CHIP packet.
    fn build_crt(exrom: u8, game: u8, load: u16, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"C64 CARTRIDGE   ");
        v.extend_from_slice(&0x40u32.to_be_bytes()); // header length
        v.extend_from_slice(&0x0100u16.to_be_bytes()); // version
        v.extend_from_slice(&0u16.to_be_bytes()); // hardware type 0 (generic)
        v.push(exrom);
        v.push(game);
        v.extend_from_slice(&[0u8; 6]); // reserved
        v.extend_from_slice(&[0u8; 32]); // name
        // CHIP packet.
        v.extend_from_slice(b"CHIP");
        v.extend_from_slice(&((0x10 + data.len()) as u32).to_be_bytes());
        v.extend_from_slice(&0u16.to_be_bytes()); // ROM
        v.extend_from_slice(&0u16.to_be_bytes()); // bank 0
        v.extend_from_slice(&load.to_be_bytes());
        v.extend_from_slice(&(data.len() as u16).to_be_bytes());
        v.extend_from_slice(data);
        v
    }

    /// Build a bank-switched CRT: header with `hardware_type` + one `$8000`
    /// CHIP packet per `(bank, data)` entry.
    fn build_banked_crt(
        hardware_type: u16,
        exrom: u8,
        game: u8,
        banks: &[(u16, &[u8])],
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"C64 CARTRIDGE   ");
        v.extend_from_slice(&0x40u32.to_be_bytes());
        v.extend_from_slice(&0x0100u16.to_be_bytes());
        v.extend_from_slice(&hardware_type.to_be_bytes());
        v.push(exrom);
        v.push(game);
        v.extend_from_slice(&[0u8; 6]);
        v.extend_from_slice(&[0u8; 32]);
        for &(bank, data) in banks {
            v.extend_from_slice(b"CHIP");
            v.extend_from_slice(&((0x10 + data.len()) as u32).to_be_bytes());
            v.extend_from_slice(&0u16.to_be_bytes()); // ROM
            v.extend_from_slice(&bank.to_be_bytes());
            v.extend_from_slice(&0x8000u16.to_be_bytes());
            v.extend_from_slice(&(data.len() as u16).to_be_bytes());
            v.extend_from_slice(data);
        }
        v
    }

    #[test]
    fn ocean_cart_switches_banks_via_de00() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        // Ocean type 5: two 8K banks, distinguished by their fill byte.
        let crt = build_banked_crt(5, 0, 1, &[(0, &[0xB0; 0x2000]), (1, &[0xB1; 0x2000])]);
        machine.insert_crt_bytes(&crt).expect("valid Ocean CRT");

        // Power-on bank 0 is visible at $8000.
        assert_eq!(machine.cpu_read(0x8000), 0xB0);
        // Writing the bank number to $DE00 selects bank 1.
        machine.cpu_write(0xDE00, 0x01);
        assert_eq!(machine.cpu_read(0x8000), 0xB1);
        // Back to bank 0.
        machine.cpu_write(0xDE00, 0x00);
        assert_eq!(machine.cpu_read(0x8000), 0xB0);
    }

    #[test]
    fn banked_cart_bank_survives_snapshot_roundtrip() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let crt = build_banked_crt(5, 0, 1, &[(0, &[0xB0; 0x2000]), (1, &[0xB1; 0x2000])]);
        machine.insert_crt_bytes(&crt).expect("valid Ocean CRT");
        machine.cpu_write(0xDE00, 0x01); // select bank 1
        assert_eq!(machine.cpu_read(0x8000), 0xB1);

        let snapshot = machine.snapshot_state();
        let mut restored = stub_machine(C64Model::PalBreadbin);
        restored
            .restore_snapshot_state(snapshot)
            .expect("snapshot should restore");

        // The selected bank and both banks survive the round-trip.
        assert_eq!(restored.cpu_read(0x8000), 0xB1);
        restored.cpu_write(0xDE00, 0x00);
        assert_eq!(restored.cpu_read(0x8000), 0xB0);
    }

    #[test]
    fn magic_desk_cart_disables_via_de00_bit7() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let crt = build_banked_crt(19, 0, 1, &[(0, &[0xD0; 0x2000]), (1, &[0xD1; 0x2000])]);
        machine
            .insert_crt_bytes(&crt)
            .expect("valid Magic Desk CRT");

        assert_eq!(machine.cpu_read(0x8000), 0xD0);
        machine.cpu_write(0xDE00, 0x01); // select bank 1
        assert_eq!(machine.cpu_read(0x8000), 0xD1);
        // Bit 7 set disables the cart: $8000 shows RAM (0x00 by default).
        machine.cpu_write(0xDE00, 0x80);
        assert_eq!(machine.cpu_read(0x8000), 0x00);
        // Clearing bit 7 re-enables it at the written bank (0).
        machine.cpu_write(0xDE00, 0x00);
        assert_eq!(machine.cpu_read(0x8000), 0xD0);
    }

    #[test]
    fn georam_window_reads_and_writes_paged_ram() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.attach_georam(512);
        assert!(machine.has_georam());

        // Page 0, block 0: write through the $DE00 window, read it back.
        machine.cpu_write(0xDE00, 0xAA);
        machine.cpu_write(0xDE10, 0xBB);
        assert_eq!(machine.cpu_read(0xDE00), 0xAA);
        assert_eq!(machine.cpu_read(0xDE10), 0xBB);

        // Select page 1 ($DFFE): a different 256-byte page, initially zero.
        machine.cpu_write(0xDFFE, 0x01);
        assert_eq!(machine.cpu_read(0xDE00), 0x00);
        machine.cpu_write(0xDE00, 0xCC);
        assert_eq!(machine.cpu_read(0xDE00), 0xCC);

        // Select block 1 ($DFFF), page 0: another distinct region.
        machine.cpu_write(0xDFFF, 0x01);
        machine.cpu_write(0xDFFE, 0x00);
        assert_eq!(machine.cpu_read(0xDE00), 0x00);
        machine.cpu_write(0xDE00, 0xDD);
        assert_eq!(machine.cpu_read(0xDE00), 0xDD);

        // Back to page 0 / block 0: the original byte survives.
        machine.cpu_write(0xDFFF, 0x00);
        assert_eq!(machine.cpu_read(0xDE00), 0xAA);
    }

    #[test]
    fn georam_block_register_masks_to_size() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.attach_georam(512); // 32 blocks → 5-bit block register
        machine.cpu_write(0xDE00, 0x11); // block 0, page 0
        // Writing block 0x20 wraps to 0 under the 0x1F mask, so the same cell.
        machine.cpu_write(0xDFFF, 0x20);
        assert_eq!(machine.cpu_read(0xDE00), 0x11);
    }

    #[test]
    fn georam_survives_snapshot_roundtrip() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.attach_georam(512);
        machine.cpu_write(0xDFFF, 0x02); // block 2
        machine.cpu_write(0xDFFE, 0x03); // page 3
        machine.cpu_write(0xDE05, 0x5A);

        let snapshot = machine.snapshot_state();
        let mut restored = stub_machine(C64Model::PalBreadbin);
        restored
            .restore_snapshot_state(snapshot)
            .expect("snapshot should restore");

        assert!(restored.has_georam());
        // The selected block/page and the stored byte all survive.
        assert_eq!(restored.cpu_read(0xDE05), 0x5A);
    }

    #[test]
    fn georam_detach_restores_open_bus() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.attach_georam(512);
        machine.cpu_write(0xDE00, 0x42);
        machine.detach_georam();
        assert!(!machine.has_georam());
        // With no expansion, the I/O area reads back as open bus.
        assert_eq!(machine.cpu_read(0xDE00), 0xFF);
    }

    /// Program the REU transfer registers: C64 address, REU address, length.
    fn program_reu(machine: &mut C64, c64: u16, reu: u32, len: u16) {
        machine.cpu_write(0xDF02, (c64 & 0xFF) as u8);
        machine.cpu_write(0xDF03, (c64 >> 8) as u8);
        machine.cpu_write(0xDF04, (reu & 0xFF) as u8);
        machine.cpu_write(0xDF05, ((reu >> 8) & 0xFF) as u8);
        machine.cpu_write(0xDF06, ((reu >> 16) & 0xFF) as u8);
        machine.cpu_write(0xDF07, (len & 0xFF) as u8);
        machine.cpu_write(0xDF08, (len >> 8) as u8);
    }

    #[test]
    fn reu_stash_and_fetch_round_trips_c64_ram() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.attach_reu(128);
        assert!(machine.has_reu());

        let payload = [0x11u8, 0x22, 0x33, 0x44];
        for (i, &b) in payload.iter().enumerate() {
            machine.cpu_write(0xC000 + i as u16, b);
        }

        // Stash C64 $C000..$C003 → REU $000000 (execute now, type 0).
        program_reu(&mut machine, 0xC000, 0x000000, 4);
        machine.cpu_write(0xDF01, 0x90);

        // Wipe the C64 copy, then fetch it back from the REU (type 1).
        for i in 0..4 {
            machine.cpu_write(0xC000 + i, 0);
        }
        program_reu(&mut machine, 0xC000, 0x000000, 4);
        machine.cpu_write(0xDF01, 0x91);

        for (i, &b) in payload.iter().enumerate() {
            assert_eq!(machine.cpu_read(0xC000 + i as u16), b);
        }
    }

    #[test]
    fn reu_transfer_waits_for_ff00_trigger() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.attach_reu(128);
        machine.cpu_write(0xC000, 0xAB);

        // Arm a stash with the FF00 trigger enabled (command bit4 clear).
        program_reu(&mut machine, 0xC000, 0x000000, 1);
        machine.cpu_write(0xDF01, 0x80);
        // Nothing has transferred: end-of-block is clear.
        assert_eq!(machine.cpu_read(0xDF00) & 0x40, 0);

        // A write to $FF00 fires it; end-of-block is now set.
        machine.cpu_write(0xFF00, 0x00);
        assert_ne!(machine.cpu_read(0xDF00) & 0x40, 0);

        // And the byte reached the REU: fetch it back.
        machine.cpu_write(0xC000, 0);
        program_reu(&mut machine, 0xC000, 0x000000, 1);
        machine.cpu_write(0xDF01, 0x91);
        assert_eq!(machine.cpu_read(0xC000), 0xAB);
    }

    #[test]
    fn reu_autoload_restores_base_registers() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.attach_reu(128);
        program_reu(&mut machine, 0xC000, 0x000005, 4);
        // Execute now + autoload (bit5) + type 0.
        machine.cpu_write(0xDF01, 0xB0);
        // Autoload restores the programmed REU base (5), not the end value (9).
        assert_eq!(machine.cpu_read(0xDF04), 0x05);
    }

    #[test]
    fn reu_fixed_c64_address_reads_one_cell() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.attach_reu(128);
        machine.cpu_write(0xC000, 0x5A);
        // Fix the C64 address so every byte stashes from $C000.
        machine.cpu_write(0xDF0A, 0x80);
        program_reu(&mut machine, 0xC000, 0x000000, 4);
        machine.cpu_write(0xDF01, 0x90);

        // Fetch REU $0..$3 back into $C010.. and confirm all four are $5A.
        machine.cpu_write(0xDF0A, 0x00);
        program_reu(&mut machine, 0xC010, 0x000000, 4);
        machine.cpu_write(0xDF01, 0x91);
        for i in 0..4 {
            assert_eq!(machine.cpu_read(0xC010 + i), 0x5A);
        }
    }

    #[test]
    fn reu_verify_flags_a_mismatch() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.attach_reu(128);
        machine.cpu_write(0xC000, 0x01);
        program_reu(&mut machine, 0xC000, 0x000000, 1);
        machine.cpu_write(0xDF01, 0x90); // stash 0x01 to REU

        // Change the C64 byte, then verify (type 3): mismatch sets the fault bit.
        machine.cpu_write(0xC000, 0x02);
        program_reu(&mut machine, 0xC000, 0x000000, 1);
        machine.cpu_write(0xDF01, 0x93);
        assert_ne!(machine.cpu_read(0xDF00) & 0x20, 0, "verify fault expected");
    }

    #[test]
    fn reu_raises_irq_when_enabled_and_clears_on_status_read() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.attach_reu(128);
        machine.cpu_write(0xC000, 0x77);
        // Enable IRQ on end-of-block (mask bit7 + bit6).
        machine.cpu_write(0xDF09, 0xC0);
        program_reu(&mut machine, 0xC000, 0x000000, 1);
        machine.cpu_write(0xDF01, 0x90);

        // Status bit7 (IRQ pending) is set; reading the status clears it.
        assert_ne!(machine.cpu_read(0xDF00) & 0x80, 0);
        assert_eq!(machine.cpu_read(0xDF00) & 0x80, 0);
    }

    #[test]
    fn reu_survives_snapshot_roundtrip() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.attach_reu(128);
        for i in 0..4u16 {
            machine.cpu_write(0xC000 + i, 0xE0 + i as u8);
        }
        program_reu(&mut machine, 0xC000, 0x001000, 4);
        machine.cpu_write(0xDF01, 0x90); // stash into REU

        let snapshot = machine.snapshot_state();
        let mut restored = stub_machine(C64Model::PalBreadbin);
        restored
            .restore_snapshot_state(snapshot)
            .expect("snapshot should restore");
        assert!(restored.has_reu());

        // Fetch the stashed block back from the restored REU RAM.
        for i in 0..4 {
            restored.cpu_write(0xC000 + i, 0);
        }
        program_reu(&mut restored, 0xC000, 0x001000, 4);
        restored.cpu_write(0xDF01, 0x91);
        for i in 0..4u16 {
            assert_eq!(restored.cpu_read(0xC000 + i), 0xE0 + i as u8);
        }
    }

    #[test]
    fn c64c_model_constructs_with_the_8580_sid() {
        use mos_sid_6581::SidModel;
        let breadbin = stub_machine(C64Model::PalBreadbin);
        assert_eq!(breadbin.sid().model, SidModel::Mos6581);

        let mut kernal = [0xEA; 0x2000];
        kernal[0x1FFC] = 0x00;
        kernal[0x1FFD] = 0xE0;
        let c64c = C64::new(C64Config {
            model: C64Model::PalC64c,
            kernal_rom: &kernal,
            basic_rom: &[0xBB; 0x2000],
            character_rom: &[0xCC; 0x1000],
        })
        .expect("C64C stub ROM sizes should be valid");
        assert_eq!(c64c.sid().model, SidModel::Mos8580);
    }

    #[test]
    fn insert_8k_crt_maps_roml_at_8000() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let crt = build_crt(0, 1, 0x8000, &[0xA1; 0x2000]);
        machine.insert_crt_bytes(&crt).expect("valid 8K CRT");
        assert_eq!(machine.cpu_read(0x8000), 0xA1);
        assert_eq!(machine.cpu_read(0x9FFF), 0xA1);
        // BASIC ROM still visible above the 8K window.
        assert_eq!(machine.cpu_read(0xA000), 0xBB);
    }

    #[test]
    fn insert_16k_crt_splits_single_chip_across_roml_and_romh() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let mut image = vec![0xA1; 0x2000];
        image.extend_from_slice(&[0xB2; 0x2000]);
        let crt = build_crt(0, 0, 0x8000, &image);
        machine.insert_crt_bytes(&crt).expect("valid 16K CRT");
        assert_eq!(machine.cpu_read(0x8000), 0xA1);
        // ROMH replaces BASIC at $A000 for a 16K cartridge.
        assert_eq!(machine.cpu_read(0xA000), 0xB2);
        assert_eq!(machine.cpu_read(0xBFFF), 0xB2);
    }

    #[test]
    fn insert_ultimax_crt_maps_romh_at_e000_and_hides_kernal() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let mut crt = build_crt(1, 0, 0x8000, &[0xA1; 0x2000]);
        // Add a second CHIP packet at $E000 for ROMH.
        crt.extend_from_slice(b"CHIP");
        crt.extend_from_slice(&((0x10 + 0x2000) as u32).to_be_bytes());
        crt.extend_from_slice(&0u16.to_be_bytes());
        crt.extend_from_slice(&0u16.to_be_bytes());
        crt.extend_from_slice(&0xE000u16.to_be_bytes());
        crt.extend_from_slice(&0x2000u16.to_be_bytes());
        crt.extend_from_slice(&[0xE7; 0x2000]);
        machine.insert_crt_bytes(&crt).expect("valid Ultimax CRT");
        assert_eq!(machine.cpu_read(0x8000), 0xA1);
        // ROMH replaces the KERNAL at $E000 under Ultimax.
        assert_eq!(machine.cpu_read(0xE000), 0xE7);
        assert_eq!(machine.cpu_read(0xFFFE), 0xE7);
    }

    #[test]
    fn remove_cartridge_restores_plain_map() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let crt = build_crt(0, 1, 0x8000, &[0xA1; 0x2000]);
        machine.insert_crt_bytes(&crt).expect("valid 8K CRT");
        assert_eq!(machine.cpu_read(0x8000), 0xA1);
        machine.remove_cartridge();
        // With no cartridge and the default port, $8000 falls through to RAM.
        assert_eq!(machine.cpu_read(0x8000), 0x00);
    }

    #[test]
    fn insert_crt_rejects_non_generic_hardware_type() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let mut crt = build_crt(0, 1, 0x8000, &[0xA1; 0x2000]);
        crt[0x16] = 0x00;
        crt[0x17] = 0x20; // hardware type 32 (EasyFlash)
        assert!(machine.insert_crt_bytes(&crt).is_err());
    }

    #[test]
    fn one_tick_advances_cycle_position() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        assert!(!machine.tick());
        assert_eq!(machine.phi2_cycles(), 1);
        assert_eq!(machine.cycle_in_line(), 1);
        assert_eq!(machine.raster_line(), 0);
    }

    #[test]
    fn pal_line_wraps_after_sixty_three_cycles() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.advance_phi2_cycles(63);
        assert_eq!(machine.cycle_in_line(), 0);
        assert_eq!(machine.raster_line(), 1);
    }

    #[test]
    fn run_frame_matches_pal_geometry() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let cycles = machine.run_frame();
        assert_eq!(cycles, 19_656);
        assert_eq!(machine.frame_count(), 1);
        assert_eq!(machine.raster_line(), 0);
        assert_eq!(machine.cycle_in_line(), 0);
    }

    #[test]
    fn ntsc_frame_geometry_is_honoured() {
        let mut machine = stub_machine(C64Model::NtscBreadbin);
        machine.advance_phi2_cycles(17_095);
        assert_eq!(machine.frame_count(), 1);
        assert_eq!(machine.raster_line(), 0);
        assert_eq!(machine.cycle_in_line(), 0);
    }

    #[test]
    fn cpu_reset_bootstrap_reaches_first_opcode_fetch() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        // 5 phantom cycles + vector-lo + vector-hi = 7 cycles total,
        // then the first opcode fetch lands on tick 7.
        for _ in 0..7 {
            if machine.tick() && machine.cpu().instruction_complete() {
                break;
            }
        }
        assert!(machine.cpu().sync);
        assert_eq!(machine.cpu().addr, 0xE000);
        assert_eq!(machine.cpu().regs.pc, 0xE000);
        assert!(machine.cpu().instruction_complete());
    }

    #[test]
    fn cpu_can_execute_load_and_store_through_board_bus() {
        let mut machine = stub_machine_with_reset_vector(C64Model::PalBreadbin, 0x0400);
        machine.memory.ram_write(0x0400, 0xA9);
        machine.memory.ram_write(0x0401, 0x42);
        machine.memory.ram_write(0x0402, 0x8D);
        machine.memory.ram_write(0x0403, 0x00);
        machine.memory.ram_write(0x0404, 0x02);

        // 7 reset + 2 LDA#imm + 4 STA abs = 13 cycles.
        for _ in 0..13 {
            machine.tick();
        }

        assert_eq!(machine.cpu().regs.a, 0x42);
        assert_eq!(machine.memory().ram_read(0x0200), 0x42);
        assert_eq!(machine.cpu().regs.pc, 0x0405);
    }

    #[test]
    fn keyboard_scan_flows_through_dc01() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.keyboard_mut().set_key(0, 1, true);
        machine.cpu_write(0xDC00, 0xFE);
        assert_eq!(machine.cpu_read(0xDC01) & 0x02, 0x00);
        assert_eq!(machine.cia1_port_b_input() & 0x02, 0x00);
    }

    #[test]
    fn keyboard_scan_does_not_transpose_row_and_column() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.keyboard_mut().set_key(0, 1, true);

        machine.cpu_write(0xDC00, 0xFD);
        assert_eq!(machine.cpu_read(0xDC01), 0xFF);

        machine.cpu_write(0xDC00, 0xFE);
        assert_eq!(machine.cpu_read(0xDC01) & 0x02, 0x00);
    }

    #[test]
    fn joystick_port_2_fire_pulls_dc00_low() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDC02, 0xFF);
        machine.cpu_write(0xDC00, 0xFF);
        assert!(machine.set_joystick_control(2, "fire", true));

        assert_eq!(machine.cpu_read(0xDC00) & 0x10, 0x00);
    }

    #[test]
    fn joystick_port_1_fire_pulls_dc01_low() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDC02, 0xFF);
        machine.cpu_write(0xDC00, 0xFF);
        assert!(machine.set_joystick_control(1, "fire", true));

        assert_eq!(machine.cpu_read(0xDC01) & 0x10, 0x00);
    }

    #[test]
    fn paddles_read_through_the_sid_pots_with_cia_port_select() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDC02, 0xFF); // CIA1 DDRA: PA6/PA7 are outputs

        machine.set_paddle(1, 0, 0xC8); // port 1 X
        machine.set_paddle(1, 1, 0x20); // port 1 Y
        machine.set_paddle(2, 0, 0x90); // port 2 X
        machine.set_paddle(2, 1, 0x44); // port 2 Y

        // Select control port 1 (PA6 = 1 → mask 1): SID pots read port 1.
        machine.cpu_write(0xDC00, 0x40);
        assert_eq!(machine.cpu_read(0xD419), 0xC8, "POTX = port 1 X");
        assert_eq!(machine.cpu_read(0xD41A), 0x20, "POTY = port 1 Y");

        // Select control port 2 (PA7 = 1 → mask 2): SID pots read port 2.
        machine.cpu_write(0xDC00, 0x80);
        assert_eq!(machine.cpu_read(0xD419), 0x90, "POTX = port 2 X");
        assert_eq!(machine.cpu_read(0xD41A), 0x44, "POTY = port 2 Y");

        // Neither selected (mask 0) → lines float open (0xFF).
        machine.cpu_write(0xDC00, 0x00);
        machine.set_paddle(1, 0, 0xC8); // re-trigger a pot refresh
        assert_eq!(machine.cpu_read(0xD419), 0xFF, "no port selected → open");

        // An unknown port / axis is rejected.
        assert!(!machine.set_paddle(0, 0, 0x10));
        assert!(!machine.set_paddle(1, 2, 0x10));
    }

    #[test]
    fn mouse_1351_pots_report_low_seven_bits_offset_by_0x40() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDC02, 0xFF); // CIA1 DDRA: PA6/PA7 outputs
        machine.cpu_write(0xDC00, 0x80); // select control port 2

        assert!(machine.attach_mouse_1351(2));
        // Centred at zero → (0 & 0x7f) + 0x40 = 0x40 on both axes.
        assert_eq!(machine.cpu_read(0xD419), 0x40, "centred POTX");
        assert_eq!(machine.cpu_read(0xD41A), 0x40, "centred POTY");

        // A positive delta moves the running counter; only the low 7 bits
        // reach the pot, offset by 0x40 (VICE mouse_get_1351_x).
        assert!(machine.move_mouse_1351(2, 5, -3));
        assert_eq!(machine.cpu_read(0xD419), 0x45, "POTX = (5 & 0x7f) + 0x40");
        assert_eq!(
            machine.cpu_read(0xD41A),
            (((-3i16) & 0x7F) as u8) + 0x40,
            "POTY wraps the signed counter into 7 bits"
        );

        // Past 7 bits the counter wraps, exactly as the guest's diff logic
        // expects: 0x40 + 3 units of movement past a 128-count boundary.
        assert!(machine.move_mouse_1351(2, 0x80 - 5, 0));
        assert_eq!(machine.cpu_read(0xD419), 0x40, "POTX wraps at 128 counts");
    }

    #[test]
    fn mouse_1351_overrides_only_its_own_ports_paddle() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDC02, 0xFF);

        machine.set_paddle(1, 0, 0xC8); // port 1 keeps its paddle
        machine.set_paddle(2, 0, 0x90);
        assert!(machine.attach_mouse_1351(2)); // port 2 gets a mouse

        machine.cpu_write(0xDC00, 0x40); // select port 1 → paddle stands
        assert_eq!(machine.cpu_read(0xD419), 0xC8, "port 1 paddle unaffected");

        machine.cpu_write(0xDC00, 0x80); // select port 2 → mouse pot wins
        assert_eq!(machine.cpu_read(0xD419), 0x40, "port 2 mouse centred");

        // Unplugging restores the port-2 paddle position.
        assert!(machine.detach_mouse_1351(2));
        assert_eq!(machine.cpu_read(0xD419), 0x90, "port 2 paddle restored");
    }

    #[test]
    fn mouse_1351_buttons_land_on_the_joystick_lines() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDC02, 0xFF);
        machine.cpu_write(0xDC00, 0xFF);
        assert!(machine.attach_mouse_1351(2)); // control port 2 = CIA1 PA

        // Left button → FIRE (bit 4) low on the main gameport.
        assert!(machine.set_mouse_1351_button(2, "left", true));
        assert_eq!(machine.cpu_read(0xDC00) & 0x10, 0x00, "left → FIRE low");
        // Right button → UP (bit 0) low.
        assert!(machine.set_mouse_1351_button(2, "right", true));
        assert_eq!(machine.cpu_read(0xDC00) & 0x01, 0x00, "right → UP low");

        machine.set_mouse_1351_button(2, "left", false);
        assert_eq!(machine.cpu_read(0xDC00) & 0x10, 0x10, "FIRE released");
    }

    #[test]
    fn mouse_1351_rejects_unknown_ports_and_empty_slots() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        assert!(!machine.attach_mouse_1351(0));
        assert!(!machine.has_mouse_1351(2));
        // No mouse plugged in → motion and buttons are rejected.
        assert!(!machine.move_mouse_1351(1, 4, 4));
        assert!(!machine.set_mouse_1351_button(1, "left", true));

        assert!(machine.attach_mouse_1351(1));
        assert!(machine.has_mouse_1351(1));
        assert!(!machine.set_mouse_1351_button(1, "middle", true));
    }

    #[test]
    fn cia2_port_a_selects_vic_bank() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDD02, 0x03);
        machine.cpu_write(0xDD00, 0x01);
        assert_eq!(machine.vic_bank(), 2);
    }

    #[test]
    fn cia2_iec_outputs_use_port_drive_state() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let mut bus = IecBus::new();

        machine.cpu_write(0xDD02, 0x3F);
        machine.cpu_write(0xDD00, 0x00);
        machine.sync_iec_bus(&mut bus);
        machine.drive_iec_outputs(&mut bus);

        assert_eq!(machine.cia2.port_a_drive_state(), 0xC0);
        assert_eq!(bus.drive_port() & 0x85, 0x85);
    }

    fn make_tap(payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 20];
        bytes[..12].copy_from_slice(b"C64-TAPE-RAW");
        bytes[12] = 1;
        bytes[13] = 0;
        bytes[14] = 0;
        bytes[16..20].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn tape_start_pulls_sense_low_on_cpu_port() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine
            .load_tap_bytes(&make_tap(&[0x24]))
            .expect("synthetic TAP should load");

        assert_ne!(machine.cpu_read(0x0001) & 0x10, 0);
        machine.play_tape();
        assert_eq!(machine.cpu_read(0x0001) & 0x10, 0);
    }

    #[test]
    fn tape_start_latches_sense_before_motor_runs() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine
            .load_tap_bytes(&make_tap(&[0x24]))
            .expect("synthetic TAP should load");

        machine.play_tape();

        assert_eq!(machine.cpu_read(0x0001) & 0x10, 0);
        assert!(!machine.tape_is_playing());
    }

    #[test]
    fn tape_pulses_raise_cia1_flag_when_motor_runs() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine
            .load_tap_bytes(&make_tap(&[0x01]))
            .expect("synthetic TAP should load");
        machine.play_tape();

        for _ in 0..7 {
            machine.tick();
        }
        assert_eq!(machine.cia1.read(0x0D) & 0x10, 0x00);

        machine.cpu_write(0x0001, machine.memory().port_data() & !0x20);
        for _ in 0..MOTOR_DELAY_CYCLES {
            machine.tick();
        }

        assert!(machine.tape_motor_on());
        for _ in 0..8 {
            machine.tick();
        }

        assert_eq!(machine.cia1.read(0x0D) & 0x10, 0x10);
        assert!(!machine.tape_is_playing());
    }

    #[test]
    fn tape_end_keeps_sense_low_until_stop() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine
            .load_tap_bytes(&make_tap(&[0x01]))
            .expect("synthetic TAP should load");
        machine.play_tape();
        machine.cpu_write(0x0001, machine.memory().port_data() & !0x20);

        for _ in 0..MOTOR_DELAY_CYCLES {
            machine.tick();
        }
        for _ in 0..8 {
            machine.tick();
        }

        assert_eq!(machine.cpu_read(0x0001) & 0x10, 0x00);
        assert!(!machine.tape_is_playing());

        machine.stop_tape();
        assert_ne!(machine.cpu_read(0x0001) & 0x10, 0x00);
    }

    #[test]
    fn tape_motor_line_has_real_spin_up_delay() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine
            .load_tap_bytes(&make_tap(&[0x01]))
            .expect("synthetic TAP should load");
        machine.play_tape();
        machine.cpu_write(0x0001, machine.memory().port_data() & !0x20);

        for _ in 0..(MOTOR_DELAY_CYCLES - 1) {
            machine.tick();
        }
        assert!(!machine.tape_motor_on());
        assert!(!machine.tape_is_playing());

        machine.tick();
        assert!(machine.tape_motor_on());
        assert!(machine.tape_is_playing());
    }

    #[test]
    fn motor_stop_is_delayed_after_cpu_drops_line() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine
            .load_tap_bytes(&make_tap(&[0x01, 0x01, 0x01]))
            .expect("synthetic TAP should load");
        machine.play_tape();
        machine.cpu_write(0x0001, machine.memory().port_data() & !0x20);

        for _ in 0..MOTOR_DELAY_CYCLES {
            machine.tick();
        }
        assert!(machine.tape_motor_on());

        machine.cpu_write(0x0001, machine.memory().port_data() | 0x20);
        for _ in 0..(MOTOR_DELAY_CYCLES - 1) {
            machine.tick();
            assert!(machine.tape_motor_on());
        }

        machine.tick();
        assert!(!machine.tape_motor_on());
        assert!(!machine.tape_is_playing());
    }

    #[test]
    fn visible_io_writes_hit_vic_and_underlying_ram() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xD020, 0x06);
        assert_eq!(machine.vic_register(0x20) & 0x0F, 0x06);
        assert_eq!(machine.memory().ram_read(0xD020), 0x06);
    }

    #[test]
    fn visible_sid_io_writes_reach_live_sid_and_underlying_ram() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xD400, 0x34);
        machine.cpu_write(0xD401, 0x12);
        machine.cpu_write(0xD418, 0x0F);

        assert_eq!(machine.sid().voices[0].frequency, 0x1234);
        assert_eq!(machine.sid().volume, 0x0F);
        assert_eq!(machine.memory().ram_read(0xD400), 0x34);
        assert_eq!(machine.memory().ram_read(0xD401), 0x12);
        assert_eq!(machine.memory().ram_read(0xD418), 0x0F);
    }

    #[test]
    fn hidden_io_reads_and_writes_fall_back_to_ram() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0x0000, 0xFF);
        machine.cpu_write(0x0001, 0x00);
        machine.cpu_write(0xD020, 0x44);
        assert_eq!(machine.memory().ram_read(0xD020), 0x44);
        assert_eq!(machine.vic_register(0x20), 0x00);
        assert_eq!(machine.cpu_read(0xD020), 0x44);
    }

    #[test]
    fn active_vic_bank_feeds_vic_reads() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.memory.ram_write(0x5000, 0xAA);
        machine.cpu_write(0xDD02, 0x03);
        machine.cpu_write(0xDD00, 0x02);
        assert_eq!(machine.vic_bank(), 1);
        assert_eq!(machine.vic_read(0x1000), 0xAA);
    }

    #[test]
    fn sid_generates_audio_samples_after_voice_programming() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xD400, 0x37);
        machine.cpu_write(0xD401, 0x1D);
        machine.cpu_write(0xD404, 0x21);
        machine.cpu_write(0xD405, 0x00);
        machine.cpu_write(0xD406, 0xF0);
        machine.cpu_write(0xD418, 0x0F);

        machine.advance_phi2_cycles(19_656);
        let audio = machine.take_audio_buffer();

        assert!(!audio.is_empty(), "SID should emit mixed audio samples");
        assert!(audio.iter().any(|sample| sample.abs() > 0.001));
        assert_eq!(machine.audio_sample_rate(), AUDIO_SAMPLE_RATE);
    }

    #[test]
    fn cia1_timer_irq_reaches_cpu_irq_line() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDC04, 0x00);
        machine.cpu_write(0xDC05, 0x00);
        machine.cpu_write(0xDC0D, 0x81);
        machine.cpu_write(0xDC0E, 0x01);
        assert!(!machine.cpu().irq);
        machine.tick();
        assert!(machine.cia1().irq);
        assert!(machine.cpu().irq);
    }

    #[test]
    fn cia2_timer_irq_reaches_cpu_nmi_line() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xDD04, 0x00);
        machine.cpu_write(0xDD05, 0x00);
        machine.cpu_write(0xDD0D, 0x81);
        machine.cpu_write(0xDD0E, 0x01);
        assert!(!machine.cpu().nmi);
        machine.tick();
        assert!(machine.cia2().irq);
        assert!(machine.cpu().nmi);
    }

    #[test]
    fn restore_key_pulses_the_cpu_nmi_line() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        // Idle: no NMI source asserted.
        machine.tick();
        assert!(!machine.cpu().nmi, "no NMI source idle");
        // RESTORE is wired to /NMI, so a press drives the line.
        machine.set_restore(true);
        machine.tick();
        assert!(machine.cpu().nmi, "RESTORE asserts the CPU /NMI line");
        // Releasing it drops the line back to the CIA #2 source (idle = low).
        machine.set_restore(false);
        machine.tick();
        assert!(
            !machine.cpu().nmi,
            "release returns /NMI to the CIA #2 source"
        );
    }

    #[test]
    fn vic_raster_irq_reaches_cpu_irq_line() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        machine.cpu_write(0xD012, 0x01);
        machine.cpu_write(0xD01A, 0x01);
        for _ in 0..63 {
            machine.tick();
        }
        assert!(machine.vic().irq);
        assert!(machine.cpu().irq);
    }

    #[test]
    fn badline_ba_stalls_cpu_reads() {
        let mut machine = stub_machine_with_reset_vector(C64Model::PalBreadbin, 0x0400);
        machine.memory.ram_write(0x0400, 0xEA);
        machine.memory.ram_write(0x0401, 0xEA);
        machine.memory.ram_write(0x0402, 0xEA);
        machine.cpu_write(0xD011, 0x1B);

        machine.tick();
        machine.tick();
        let target_cycles = (0x33u64 * 63) + 13;
        while machine.phi2_cycles() < target_cycles {
            machine.tick();
        }

        let pc_before = machine.cpu().regs.pc;
        assert!(machine.vic().ba_low);
        assert!(machine.cpu().rw);
        machine.tick();
        assert_eq!(machine.cpu().regs.pc, pc_before);
    }

    #[test]
    #[ignore = "requires real C64 BASIC/KERNAL/CHARGEN ROMs at ~/.emu198x/roms/commodore-c64"]
    fn boots_kernal_to_ready_prompt() {
        let rom_dir = c64_rom_dir();
        let kernal = fs::read(rom_dir.join("kernal.rom")).expect("KERNAL ROM");
        let basic = fs::read(rom_dir.join("basic.rom")).expect("BASIC ROM");
        let chargen = fs::read(rom_dir.join("chargen.rom")).expect("character ROM");

        let mut machine = C64::new(C64Config {
            model: C64Model::PalBreadbin,
            kernal_rom: &kernal,
            basic_rom: &basic,
            character_rom: &chargen,
        })
        .expect("real C64 ROM set should construct a machine");

        // Screen codes for READY.
        const READY: [u8; 6] = [18, 5, 1, 4, 25, 46];

        let mut found = None;
        for frame in 0..200u32 {
            machine.run_frame();

            for offset in 0..=(0x07E8u16 - 0x0400 - READY.len() as u16) {
                let mut matched = true;
                for (i, &expected) in READY.iter().enumerate() {
                    if machine.memory().ram_read(0x0400 + offset + i as u16) != expected {
                        matched = false;
                        break;
                    }
                }
                if matched {
                    found = Some((frame + 1, offset));
                    break;
                }
            }

            if found.is_some() {
                break;
            }
        }

        assert!(
            found.is_some(),
            "C64 did not reach READY. prompt within 200 frames"
        );
    }

    #[test]
    fn snapshot_round_trip_restores_machine_mid_instruction() {
        let mut machine = stub_machine_with_reset_vector(C64Model::PalBreadbin, 0x0400);
        machine.memory.ram_write(0x0400, 0xAD);
        machine.memory.ram_write(0x0401, 0x00);
        machine.memory.ram_write(0x0402, 0x20);
        machine.memory.ram_write(0x2000, 0x42);
        machine.keyboard_mut().set_key(2, 3, true);

        machine.tick();
        machine.tick();
        machine.tick();

        let snapshot = machine.snapshot_state();
        let mut expected = machine.clone();
        machine.tick();
        machine.tick();
        machine
            .restore_snapshot_state(snapshot)
            .expect("snapshot restore should succeed");

        assert_eq!(machine.cpu().regs.pc, expected.cpu().regs.pc);
        assert_eq!(machine.cpu().addr, expected.cpu().addr);
        assert_eq!(machine.cpu().rw, expected.cpu().rw);
        assert_eq!(machine.vic_bank(), expected.vic_bank());
        assert_eq!(machine.cia1_port_b_input(), expected.cia1_port_b_input());
        assert_eq!(machine.memory().ram(), expected.memory().ram());
        assert_eq!(
            machine.memory().colour_ram(),
            expected.memory().colour_ram()
        );

        for _ in 0..8 {
            let expected_frame_complete = expected.tick();
            let restored_frame_complete = machine.tick();
            assert_eq!(restored_frame_complete, expected_frame_complete);
            assert_eq!(machine.cpu().regs, expected.cpu().regs);
            assert_eq!(machine.cpu().addr, expected.cpu().addr);
            assert_eq!(machine.cpu().rw, expected.cpu().rw);
            assert_eq!(machine.cpu().sync, expected.cpu().sync);
            assert_eq!(machine.cpu().total_cycles, expected.cpu().total_cycles);
            assert_eq!(machine.raster_line(), expected.raster_line());
            assert_eq!(machine.cycle_in_line(), expected.cycle_in_line());
            assert_eq!(machine.vic().irq, expected.vic().irq);
            assert_eq!(machine.vic().ba_low, expected.vic().ba_low);
            assert_eq!(machine.framebuffer(), expected.framebuffer());
        }
    }

    #[test]
    fn load_prg_imports_basic_program_and_updates_vartab() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let prg = [0x01, 0x08, 0x07, 0x08, 0x0A, 0x00, 0x80, 0x00, 0x00, 0x00];

        let load_addr = machine.load_prg(&prg).expect("PRG should load");

        assert_eq!(load_addr, 0x0801);
        assert_eq!(machine.memory().ram_read(0x0801), 0x07);
        assert_eq!(machine.memory().ram_read(0x0802), 0x08);
        let vartab = u16::from(machine.memory().ram_read(0x2D))
            | (u16::from(machine.memory().ram_read(0x2E)) << 8);
        assert_eq!(vartab, 0x0809);
    }

    #[test]
    fn iec_bus_updates_cia2_input_bits() {
        let mut machine = stub_machine(C64Model::PalBreadbin);
        let mut bus = IecBus::new();

        machine.cpu_write_with_iec_bus(0xDD02, 0x3F, &mut bus);
        machine.cpu_write_with_iec_bus(0xDD00, 0x00, &mut bus);

        assert_eq!(bus.drive_port() & 0x85, 0x85);
    }

    #[test]
    fn audio_controls_proxy_to_sid() {
        let mut machine = stub_machine(C64Model::PalBreadbin);

        machine.set_audio_channel_enabled(SidChannel::Voice2, false);
        machine.set_audio_channel_gain(SidChannel::Voice3, 0.5);

        assert!(
            !machine
                .audio_controls()
                .channel(SidChannel::Voice2)
                .enabled()
        );
        assert_eq!(
            machine.audio_controls().channel(SidChannel::Voice3).gain(),
            0.5
        );
    }

    #[test]
    fn c64_and_1541_share_iec_data_and_atn_lines() {
        let mut c64 = stub_machine(C64Model::PalBreadbin);
        let mut drive = Drive1541::new(Drive1541Config {
            dos_rom: &drive_rom(),
        })
        .expect("1541 ROM should be valid");
        let mut bus = IecBus::new();

        drive.write_with_iec_bus(0x1802, 0xFF, &mut bus);
        drive.write_with_iec_bus(0x1800, 0xF7, &mut bus);
        c64.sync_iec_bus(&mut bus);

        assert_eq!(c64.cpu_read_with_iec_bus(0xDD00, &mut bus) & 0x80, 0x00);

        c64.cpu_write_with_iec_bus(0xDD02, 0x3F, &mut bus);
        c64.cpu_write_with_iec_bus(0xDD00, 0x08, &mut bus);
        drive.sync_iec_bus(&mut bus);

        assert!(!bus.drive_atn_high());
        assert_eq!(drive.read_with_iec_bus(0x1800, &bus) & 0x80, 0x80);
    }
}

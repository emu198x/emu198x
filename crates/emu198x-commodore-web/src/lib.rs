//! Firmware-free, canvas-free C64/A500 prototype. The same binding runs natively
//! for comparison; the browser owns file selection, presentation and workers.
use emu198x_shell::{InputEvent, MediaKind};
use emu198x_web::WebMachine;
use machine_commodore_amiga_ocs::ExtendedRomWindow;
use runtime_commodore_amiga::{AmigaOcsRuntime, AmigaRuntimeKind, Model as AmigaModel};
use runtime_commodore_c64::{C64Runtime, Model as C64Model};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn report_panic(message: &str);
}

/// Reports a useful panic message before a WASM trap reaches the worker.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| report_panic(&info.to_string())));
}

enum Machine {
    C64(Box<WebMachine<C64Runtime>>),
    Amiga(Box<WebMachine<AmigaRuntimeKind>>),
}

// Only the binding dispatches here; emulation, pacing, conversion and buffering
// remain in WebMachine. Neither machine needs another FamilyRuntime adapter.
macro_rules! with_machine {
    ($this:expr, $machine:ident, $body:expr) => {
        match $this {
            Machine::C64($machine) => $body,
            Machine::Amiga($machine) => $body,
        }
    };
}

/// One PAL breadbin C64 or PAL A500 + A501, with user-selected firmware.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Commodore {
    machine: Machine,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Commodore {
    /// Creates a PAL C64. An empty drive image means no 1541 is attached.
    /// # Errors
    /// Rejects firmware with invalid lengths.
    pub fn c64(kernal: &[u8], basic: &[u8], chargen: &[u8], drive: &[u8]) -> Result<Self, String> {
        let runtime = C64Runtime::new(
            C64Model::C64PalBreadbin,
            kernal.to_vec(),
            basic.to_vec(),
            chargen.to_vec(),
            (!drive.is_empty()).then(|| drive.to_vec()),
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            machine: Machine::C64(Box::new(WebMachine::new(runtime))),
        })
    }

    /// Creates a PAL A500 + A501: 512 KiB chip, 512 KiB slow RAM and RTC.
    /// # Errors
    /// Rejects firmware with an invalid length.
    pub fn amiga(kickstart: &[u8]) -> Result<Self, String> {
        Self::build_amiga(kickstart, None)
    }

    /// Creates the same A500 + A501 with both AROS m68k ROM windows fitted.
    /// The main ROM maps at $F80000 and the extension at $E00000, following
    /// the existing machine crate's AROS boot test. Restart recreates both.
    /// # Errors
    /// Rejects anything other than two 512 KiB ROM images.
    pub fn amiga_aros(main_rom: &[u8], extended_rom: &[u8]) -> Result<Self, String> {
        if main_rom.len() != 512 * 1024 || extended_rom.len() != 512 * 1024 {
            return Err("AROS requires the matching 512 KiB main and extended ROM images".into());
        }
        Self::build_amiga(main_rom, Some(extended_rom))
    }

    /// Exports machine state and modified media.
    /// # Errors
    /// Returns runtime serialisation errors.
    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        with_machine!(&self.machine, m, m.save_state()).map_err(|e| e.to_string())
    }
    /// Restores a compatible saved machine.
    /// # Errors
    /// Rejects invalid or incompatible state.
    pub fn restore_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        with_machine!(&mut self.machine, m, m.restore_state(bytes)).map_err(|e| e.to_string())
    }
    /// Exports a working disk image to a new downloadable file.
    /// # Errors
    /// Requires a loaded disk in the specified slot.
    pub fn export_media(&self, slot: &str) -> Result<Vec<u8>, String> {
        use runtime_commodore_amiga::AmigaMachine;
        let bytes = match (&self.machine, slot) {
            (Machine::C64(m), "drive-8") => m.runtime().flush_drive8_image(),
            (Machine::Amiga(m), "floppy-0") => match m.runtime() {
                AmigaRuntimeKind::Ocs(r) => r.machine().floppy0_image_bytes(),
                AmigaRuntimeKind::Ecs(r) => r.machine().floppy0_image_bytes(),
                AmigaRuntimeKind::Aga(r) => r.machine().floppy0_image_bytes(),
            },
            _ => None,
        };
        bytes.ok_or_else(|| "No exportable disk in this slot".into())
    }
    /// Advances using the shared bounded wall-clock pacer.
    /// # Errors
    /// Returns a runtime execution failure.
    pub fn advance(&mut self, elapsed_ms: f64) -> Result<u32, String> {
        with_machine!(&mut self.machine, m, m.advance(elapsed_ms)).map_err(|e| e.to_string())
    }

    /// Runs exactly one frame for measurement and native/WASM comparison.
    /// # Errors
    /// Returns a runtime execution failure.
    pub fn step(&mut self) -> Result<(), String> {
        with_machine!(&mut self.machine, m, m.run_one_frame()).map_err(|e| e.to_string())
    }

    /// Queues a machine key name, returning whether it is recognised.
    pub fn key(&mut self, name: &str, pressed: bool) -> bool {
        with_machine!(&mut self.machine, m, m.queue_key(name.to_owned(), pressed))
    }

    /// Queues a direction or fire on control port 2.
    /// # Errors
    /// Rejects unknown controls.
    pub fn joystick(&mut self, name: &str, pressed: bool) -> Result<(), String> {
        if !matches!(name, "up" | "down" | "left" | "right" | "fire") {
            return Err(format!("unknown joystick control: {name}"));
        }
        let event = InputEvent::Button {
            port: 2,
            name: name.to_owned().into(),
            pressed,
        };
        with_machine!(&mut self.machine, m, m.queue_input(event));
        Ok(())
    }

    /// Moves the Amiga mouse; returns false on the C64.
    pub fn mouse_move(&mut self, dx: i32, dy: i32) -> bool {
        let Machine::Amiga(m) = &mut self.machine else {
            return false;
        };
        m.queue_input(InputEvent::PointerMotion {
            device: "mouse-1".into(),
            dx,
            dy,
        });
        true
    }

    /// Changes an Amiga mouse button; rejects unknown buttons and C64 use.
    pub fn mouse_button(&mut self, button: &str, pressed: bool) -> bool {
        if !matches!(button, "left" | "right" | "middle") {
            return false;
        }
        let Machine::Amiga(m) = &mut self.machine else {
            return false;
        };
        m.queue_input(InputEvent::PointerButton {
            device: "mouse-1".into(),
            button: button.to_owned().into(),
            pressed,
        });
        true
    }

    /// Mounts a C64 D64/G64 or an Amiga ADF. C64 PRG imports use the existing
    /// direct RAM loader, after boot; type RUN or SYS afterwards as appropriate.
    /// # Errors
    /// Rejects unsupported formats, malformed media and absent drive firmware.
    pub fn load(&mut self, format: &str, bytes: &[u8]) -> Result<(), String> {
        match (&mut self.machine, format) {
            (Machine::C64(m), "prg") => m.runtime_mut().load_prg_bytes(bytes).map(|_| ()),
            (Machine::C64(m), "d64" | "g64") => m
                .load_media_copy("drive-8", MediaKind::Disk, bytes)
                .map_err(|e| e.to_string()),
            (Machine::Amiga(m), "adf") => m
                .load_media_copy("floppy-0", MediaKind::Disk, bytes)
                .map_err(|e| e.to_string()),
            _ => Err(format!(
                "unsupported media format for this machine: {format}"
            )),
        }
    }

    /// Configures interleaved stereo output at the browser's audio rate.
    /// # Errors
    /// Rejects unreasonable rates before allocating a buffer.
    pub fn configure_audio(&mut self, rate: u32) -> Result<(), String> {
        if !(8_000..=192_000).contains(&rate) {
            return Err("invalid audio sample rate".into());
        }
        with_machine!(
            &mut self.machine,
            m,
            m.configure_audio(rate, 2, rate as usize / 2)
        );
        Ok(())
    }

    /// Drains interleaved stereo samples; call configure_audio before playback.
    pub fn audio(&mut self) -> Vec<f32> {
        with_machine!(&mut self.machine, m, m.audio_drain())
    }

    /// Copies the current RGBA frame across the WASM boundary.
    pub fn pixels(&self) -> Vec<u8> {
        with_machine!(&self.machine, m, m.frame_rgba().to_vec())
    }
    /// Current frame width; zero until the first frame arrives.
    pub fn width(&self) -> u32 {
        with_machine!(&self.machine, m, m.frame_size().0)
    }
    /// Current frame height; zero until the first frame arrives.
    pub fn height(&self) -> u32 {
        with_machine!(&self.machine, m, m.frame_size().1)
    }
    /// Native frame duration derived from the selected profile.
    pub fn frame_ms(&self) -> f64 {
        with_machine!(&self.machine, m, m.frame_ms())
    }
}

impl Commodore {
    fn build_amiga(firmware: &[u8], extended: Option<&[u8]>) -> Result<Self, String> {
        let mut runtime = AmigaOcsRuntime::new(AmigaModel::A500OcsPalA501, firmware.to_vec())
            .map_err(|e| e.to_string())?;
        if let Some(image) = extended {
            runtime
                .machine_mut()
                .install_extended_rom(ExtendedRomWindow::E00000, image.to_vec());
        }
        Ok(Self {
            machine: Machine::Amiga(Box::new(WebMachine::new(AmigaRuntimeKind::Ocs(runtime)))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_firmware_is_rejected() {
        assert!(Commodore::c64(&[], &[], &[], &[]).is_err());
        assert!(Commodore::amiga(&[0; 16]).is_err());
        assert!(Commodore::amiga_aros(&vec![0; 512 * 1024], &[]).is_err());
    }

    #[test]
    fn browser_default_fits_a501_ram_and_decodes_the_clock() {
        let machine = Commodore::amiga(&vec![0; 256 * 1024]).expect("valid firmware size");
        let Machine::Amiga(web) = machine.machine else {
            panic!("Amiga binding");
        };
        let AmigaRuntimeKind::Ocs(runtime) = web.runtime() else {
            panic!("OCS model");
        };
        assert_eq!(runtime.ram_config().chip_kb, 512);
        assert_eq!(runtime.ram_config().slow_kb, 512);
        assert!(runtime.machine().gary().rtc_present());
        assert_eq!(
            runtime.machine().gary().decode(0xDC0000),
            machine_commodore_amiga_ocs::ChipSelect::Rtc
        );
    }

    #[test]
    fn aros_constructor_maps_the_extended_rom_and_keeps_the_clock() {
        let main = vec![0; 512 * 1024];
        let ext = vec![0x5a; 512 * 1024];
        let machine = Commodore::amiga_aros(&main, &ext).expect("matched image sizes");
        let Machine::Amiga(web) = machine.machine else {
            panic!("Amiga binding");
        };
        let AmigaRuntimeKind::Ocs(runtime) = web.runtime() else {
            panic!("OCS model");
        };
        assert_eq!(runtime.machine().read_word(0xE00000), 0x5a5a);
        assert!(runtime.machine().gary().rtc_present());
    }
}

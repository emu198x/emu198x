//! Browser-only host boundary over the existing family runtime catalogue.
//! A feature selects each downloadable family; native tools can enable all.
use emu198x_shell::{
    ControlCommand, FamilyRuntime, FirmwareImage, FirmwareSet, InputEvent, MediaKind,
    MediaTransportAction, MediaTransportCommand, NoAdditionalQueries, SessionQueryProvider,
};
use emu198x_web::WebMachine;
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
mod factory;

trait Host {
    fn step(&mut self) -> Result<(), String>;
    fn advance(&mut self, elapsed: f64) -> Result<u32, String>;
    fn pixels(&self) -> Vec<u8>;
    fn size(&self) -> (u32, u32);
    fn audio(&mut self) -> Vec<f32>;
    fn rate(&mut self, rate: u32);
    fn frame_ms(&self) -> f64;
    fn input(&mut self, event: InputEvent);
    fn load(&mut self, slot: &str, bytes: &[u8]) -> Result<(), String>;
    fn blank_tape(&mut self, slot: &str) -> Result<(), String>;
    fn transport(&mut self, slot: &str, playing: bool) -> Result<(), String>;
    fn save_state(&self) -> Result<Vec<u8>, String>;
    fn restore_state(&mut self, bytes: &[u8]) -> Result<(), String>;
    fn export_media(&self, slot: &str) -> Result<Vec<u8>, String>;
    fn keymap(&self) -> Value;
    fn pixel_aspect_ratio(&self) -> f32;
    fn autoload_tape(&mut self, max_boot_frames: u32) -> Result<u32, String>;
    fn query(&self, path: &str) -> Result<Value, String>;
}
/// `Q` answers the family's own query paths. Most families need none; the
/// Spectrum's tape autoload waits on `boot.detected` and `tape.loaded`.
struct RuntimeHost<R: FamilyRuntime, Q = NoAdditionalQueries> {
    web: WebMachine<R, Q>,
}
impl<R: FamilyRuntime + 'static, Q: SessionQueryProvider<R> + 'static> Host for RuntimeHost<R, Q> {
    fn step(&mut self) -> Result<(), String> {
        self.web.run_one_frame().map_err(|e| e.to_string())
    }
    fn advance(&mut self, elapsed: f64) -> Result<u32, String> {
        self.web.advance(elapsed).map_err(|e| e.to_string())
    }
    fn pixels(&self) -> Vec<u8> {
        self.web.frame_rgba().to_vec()
    }
    fn size(&self) -> (u32, u32) {
        self.web.frame_size()
    }
    fn audio(&mut self) -> Vec<f32> {
        self.web.audio_drain()
    }
    fn rate(&mut self, rate: u32) {
        self.web.configure_audio(rate, 2, rate as usize / 2);
        self.web.set_audio_enabled(true);
    }
    fn frame_ms(&self) -> f64 {
        self.web.frame_ms()
    }
    fn autoload_tape(&mut self, max_boot_frames: u32) -> Result<u32, String> {
        #[cfg(feature = "sinclair-zx-spectrum")]
        if let Some(machine) = (&mut self.web as &mut dyn std::any::Any)
            .downcast_mut::<emu198x_spectrum_web::SpectrumWebMachine>()
        {
            return emu198x_spectrum_web::autoload_tape(machine, max_boot_frames);
        }
        #[cfg(feature = "commodore-c64")]
        if let Some(machine) = (&mut self.web as &mut dyn std::any::Any).downcast_mut::<WebMachine<
            runtime_commodore_c64::C64Runtime,
            runtime_commodore_c64::C64SessionQueryProvider,
        >>() {
            return runtime_commodore_c64::autoload_basic_tape_on(
                machine,
                runtime_commodore_c64::DEFAULT_TAPE_AUTOLOAD_SLOT,
                max_boot_frames,
                runtime_commodore_c64::DEFAULT_TAPE_AUTOLOAD_WAIT_FRAMES,
            )
            .map(|result| result.boot.frames)
            .map_err(|error| error.to_string());
        }
        let _ = max_boot_frames;
        Err("tape autoload is available only on the Spectrum and the C64".into())
    }
    fn query(&self, path: &str) -> Result<Value, String> {
        self.web
            .query(path)
            .map(|result| result.value)
            .map_err(|e| e.to_string())
    }
    fn pixel_aspect_ratio(&self) -> f32 {
        let (width, height) = self.web.frame_size();
        self.web
            .runtime()
            .display()
            .map_or(1.0, |display| display.pixel_aspect_ratio(width, height))
    }
    fn input(&mut self, event: InputEvent) {
        self.web.queue_input(event);
    }
    fn load(&mut self, slot: &str, bytes: &[u8]) -> Result<(), String> {
        #[cfg(feature = "commodore-c64")]
        if slot == "prg"
            && let Some(runtime) = (self.web.runtime_mut() as &mut dyn std::any::Any)
                .downcast_mut::<runtime_commodore_c64::C64Runtime>()
        {
            return runtime.load_prg_bytes(bytes).map(|_| ());
        }
        #[cfg(feature = "sinclair-zx-spectrum")]
        if matches!(slot, "sna" | "z80") {
            use runtime_sinclair_zx_spectrum::SpectrumLiveAccess;
            if let Some(runtime) = (self.web.runtime_mut() as &mut dyn std::any::Any)
                .downcast_mut::<runtime_sinclair_zx_spectrum::SpectrumRuntimeKind>(
            ) {
                let snapshot = emu198x_spectrum_web::parse_snapshot(bytes, slot)?;
                runtime.apply_snapshot(&snapshot);
                return Ok(());
            }
        }
        let kind = self
            .web
            .runtime()
            .profile()
            .media_slots
            .iter()
            .find(|entry| entry.id == slot)
            .map(|entry| entry.kind)
            .ok_or_else(|| format!("unknown media slot: {slot}"))?;
        // A tape handed in is one to play, so it goes in write-protected: a
        // writable tape is a blank work tape to SAVE onto, and the C64 drops
        // the bytes of one (knowledge/decisions/disk-save-write-back.md).
        // Disks stay writable in-memory copies, so a SAVE to them works.
        if kind == MediaKind::Tape {
            self.web.load_media_bytes(slot, kind, bytes)
        } else {
            self.web.load_media_copy(slot, kind, bytes)
        }
        .map_err(|e| e.to_string())
    }
    fn blank_tape(&mut self, slot: &str) -> Result<(), String> {
        let is_tape = self
            .web
            .runtime()
            .profile()
            .media_slots
            .iter()
            .any(|entry| entry.id == slot && entry.kind == MediaKind::Tape);
        if !is_tape {
            return Err(format!("not a tape slot: {slot}"));
        }
        self.web
            .load_media_copy(slot, MediaKind::Tape, &[])
            .map_err(|e| e.to_string())
    }
    fn transport(&mut self, slot: &str, playing: bool) -> Result<(), String> {
        self.web
            .runtime_mut()
            .command(&ControlCommand::MediaTransport(MediaTransportCommand::new(
                slot.to_owned(),
                if playing {
                    MediaTransportAction::Start
                } else {
                    MediaTransportAction::Stop
                },
            )))
            .map_err(|e| e.to_string())
    }
    fn save_state(&self) -> Result<Vec<u8>, String> {
        self.web.save_state().map_err(|e| e.to_string())
    }
    fn restore_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.web.restore_state(bytes).map_err(|e| e.to_string())
    }
    fn export_media(&self, slot: &str) -> Result<Vec<u8>, String> {
        let runtime = self.web.runtime() as &dyn std::any::Any;
        #[cfg(feature = "commodore-c64")]
        if let Some(r) = runtime.downcast_ref::<runtime_commodore_c64::C64Runtime>() {
            return match slot {
                "drive-8" => r.flush_drive8_image(),
                "drive-9" => r.flush_drive_1581_image(),
                // What has been SAVEd onto a blank tape.
                "tape-1" => r.flush_tape_image(),
                _ => None,
            }
            .ok_or_else(|| "Nothing to export from this slot".into());
        }
        #[cfg(feature = "sinclair-zx-spectrum")]
        if let Some(r) = runtime.downcast_ref::<runtime_sinclair_zx_spectrum::SpectrumRuntimeKind>()
            && slot == "tape-1"
        {
            // The Spectrum records a SAVE whatever tape is in the deck.
            return r
                .flush_tape_image()
                .ok_or_else(|| "Nothing has been saved to tape".into());
        }
        #[cfg(feature = "commodore-amiga")]
        if let Some(r) = runtime.downcast_ref::<runtime_commodore_amiga::AmigaRuntimeKind>() {
            use runtime_commodore_amiga::{AmigaMachine, AmigaRuntimeKind};
            if slot != "floppy-0" {
                return Err("Unknown disk slot".into());
            }
            return match r {
                AmigaRuntimeKind::Ocs(r) => r.machine().floppy0_image_bytes(),
                AmigaRuntimeKind::Ecs(r) => r.machine().floppy0_image_bytes(),
                AmigaRuntimeKind::Aga(r) => r.machine().floppy0_image_bytes(),
            }
            .ok_or_else(|| "No disk in this slot".into());
        }
        #[cfg(feature = "dragon")]
        if let Some(r) = runtime.downcast_ref::<runtime_dragon::DragonRuntime>()
            && slot == "drive-1"
        {
            return r
                .export_drive_vdk(0)
                .ok_or_else(|| "No disk in this slot".into());
        }
        let _ = (runtime, slot);
        Err(
            "Disk image export is unavailable for this machine; export a complete save instead"
                .into(),
        )
    }
    fn keymap(&self) -> Value {
        let mut map = serde_json::Map::new();
        let mut candidates: Vec<(String, Vec<String>)> = ('A'..='Z')
            .map(|c| (format!("Key{c}"), vec![c.to_string()]))
            .chain(('0'..='9').map(|c| (format!("Digit{c}"), vec![c.to_string()])))
            .collect();
        for (code, names) in [
            ("Enter", "enter,return"),
            ("Space", "space"),
            ("Backspace", "backspace,delete,rubout"),
            ("ArrowUp", "up"),
            ("ArrowDown", "down"),
            ("ArrowLeft", "left"),
            ("ArrowRight", "right"),
            ("ShiftLeft", "shift,lshift,capsshift"),
            ("ShiftRight", "shift,rshift,capsshift"),
            ("ControlLeft", "ctrl,control,symbolshift"),
            ("ControlRight", "ctrl,control,symbolshift"),
            ("AltLeft", "alt,lalt,symbolshift,graph,commodore"),
            ("AltRight", "alt,ralt,symbolshift,graph,commodore"),
            ("Escape", "escape,esc,break,runstop"),
            ("Home", "home,clear"),
            ("Delete", "delete"),
            ("Comma", "comma,,"),
            ("Period", "period,."),
            ("Slash", "slash,/"),
            ("Minus", "minus,-"),
            ("Equal", "equals,="),
            ("Semicolon", "semicolon,;"),
            ("Quote", "apostrophe,quote,colon"),
            ("BracketLeft", "lbracket,at"),
            ("BracketRight", "rbracket,asterisk"),
            ("Backslash", "backslash"),
            ("Backquote", "backquote,leftarrow"),
            ("MetaLeft", "lamiga"),
            ("MetaRight", "ramiga"),
        ] {
            candidates.push((
                code.into(),
                names
                    .split(',')
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .collect(),
            ));
        }
        for n in 1..=12 {
            candidates.push((format!("F{n}"), vec![format!("f{n}")]));
        }
        for (code, names) in candidates {
            if let Some(name) = names.into_iter().find(|name| self.web.accepts_key(name)) {
                let chord = self
                    .web
                    .runtime()
                    .keyboard_target()
                    .and_then(|k| k.expand_named_key(&name))
                    .unwrap_or_else(|| vec![name]);
                map.insert(code, json!(chord));
            }
        }
        Value::Object(map)
    }
}
fn build<R: FamilyRuntime + 'static>(
    variant: &str,
    firmware: &FirmwareSet<'_>,
) -> Result<Box<dyn Host>, String> {
    build_with_queries::<R, _>(variant, firmware, NoAdditionalQueries)
}
fn build_with_queries<R: FamilyRuntime + 'static, Q: SessionQueryProvider<R> + 'static>(
    variant: &str,
    firmware: &FirmwareSet<'_>,
    queries: Q,
) -> Result<Box<dyn Host>, String> {
    let model = R::model_from_id(variant).ok_or_else(|| format!("unknown variant: {variant}"))?;
    let runtime = R::from_firmware(model, firmware).map_err(|e| e.to_string())?;
    Ok(Box::new(RuntimeHost {
        web: WebMachine::new_with_query_provider(runtime, queries),
    }))
}
/// Builds `variant` only when its profile names `machine`. One runtime can
/// host two machines (the MSX1 and the MSX2), and each fleet family is one
/// machine, so a family refuses the other machine's models.
#[cfg(any(feature = "msx", feature = "msx2"))]
fn build_machine<R: FamilyRuntime + 'static>(
    machine: &str,
    variant: &str,
    firmware: &FirmwareSet<'_>,
) -> Result<Box<dyn Host>, String> {
    R::model_from_id(variant)
        .filter(|model| R::profile_for(*model).machine_id.as_str() == machine)
        .ok_or_else(|| format!("unknown variant: {variant}"))?;
    build::<R>(variant, firmware)
}
/// The catalogue entries of `R` whose profile names `machine`.
#[cfg(any(feature = "msx", feature = "msx2"))]
fn machine_profiles<R: FamilyRuntime>(family: &str, machine: &str) -> Vec<Value> {
    profiles::<R>(family)
        .into_iter()
        .filter(|entry| entry["machineId"] == machine)
        .collect()
}
fn profiles<R: FamilyRuntime>(family: &str) -> Vec<Value> {
    R::variant_ids().iter().filter_map(|id|R::model_from_id(id)).map(|model| {
        let profile=R::profile_for(model);
        let sources=R::firmware_sources(model);
        json!({"family":family,"id":R::variant_id(model),"profileId":profile.profile_id,"machineId":profile.machine_id,
            "name":profile.display_name,"firmware":profile.firmware,"slots":profile.media_slots,
            "sources":sources.iter().map(|source|json!({"id":source.id,"candidates":source.candidates,"optional":source.optional})).collect::<Vec<_>>(),
            "romDirs":R::rom_convention().dirs})
    }).collect()
}
/// Runtime-owned variant, firmware and media metadata for the compiled families.
#[must_use]
pub fn catalogue() -> Vec<Value> {
    factory::catalogue()
}

/// Media given before boot.
enum PendingMedia {
    Image(Vec<u8>),
    BlankTape,
}

/// Builds a machine from explicitly supplied firmware, then drives it in memory.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Fleet {
    family: String,
    variant: String,
    firmware: BTreeMap<String, Vec<u8>>,
    /// Media given before boot, inserted as the machine is built and before
    /// it runs a frame: an Amiga counts its drives as Kickstart starts.
    media: Vec<(String, PendingMedia)>,
    host: Option<Box<dyn Host>>,
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Fleet {
    /// Creates an unbooted machine. Call `firmware` for each image, then `boot`.
    /// # Errors
    /// Rejects families or variants absent from this artifact.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(family: &str, variant: &str) -> Result<Self, String> {
        if !catalogue()
            .iter()
            .any(|entry| entry["family"] == family && entry["id"] == variant)
        {
            return Err(format!("unknown compiled variant: {family}/{variant}"));
        }
        Ok(Self {
            family: family.into(),
            variant: variant.into(),
            firmware: BTreeMap::new(),
            media: Vec::new(),
            host: None,
        })
    }
    /// Supplies a named image before boot. The runtime validates its contents.
    /// # Errors
    /// Rejects changes after boot and oversized inputs.
    pub fn firmware(&mut self, id: &str, bytes: &[u8]) -> Result<(), String> {
        if self.host.is_some() || bytes.len() > 4 * 1024 * 1024 {
            return Err("firmware cannot be changed after boot, or image too large".into());
        }
        self.firmware.insert(id.into(), bytes.to_vec());
        Ok(())
    }
    /// Builds the selected runtime using only the supplied images.
    /// # Errors
    /// Reports missing or invalid firmware.
    pub fn boot(&mut self) -> Result<(), String> {
        let mut firmware = FirmwareSet::new();
        for (id, bytes) in &self.firmware {
            firmware.push(FirmwareImage::new(id.clone(), bytes));
        }
        let mut host = factory::create(&self.family, &self.variant, &firmware)?;
        for (slot, media) in &self.media {
            match media {
                PendingMedia::Image(bytes) => host.load(slot, bytes)?,
                PendingMedia::BlankTape => host.blank_tape(slot)?,
            }
        }
        self.media.clear();
        self.host = Some(host);
        Ok(())
    }
    /// Advances wall-clock time.
    ///
    /// # Errors
    /// fails if unbooted or execution fails.
    pub fn advance(&mut self, elapsed: f64) -> Result<u32, String> {
        self.host_mut()?.advance(elapsed)
    }
    /// Steps one frame.
    ///
    /// # Errors
    /// fails if unbooted or execution fails.
    pub fn step(&mut self) -> Result<(), String> {
        self.host_mut()?.step()
    }
    /// Loads a declared media slot. Before boot the media waits and goes in
    /// as the machine is built, before its first frame.
    ///
    /// # Errors
    /// rejects unsupported or malformed media, at boot for media given
    /// before it.
    pub fn load(&mut self, slot: &str, bytes: &[u8]) -> Result<(), String> {
        if self.host.is_none() {
            self.media
                .push((slot.into(), PendingMedia::Image(bytes.to_vec())));
            return Ok(());
        }
        self.host_mut()?.load(slot, bytes)
    }
    /// Puts a blank tape in `slot` to SAVE onto; `export_media` then gives
    /// what was saved. A tape given to `load` is write-protected, for
    /// playing (knowledge/decisions/disk-save-write-back.md). Before boot
    /// it waits, as `load` does.
    ///
    /// # Errors
    /// rejects a slot that is not a tape deck, at boot when given before it.
    pub fn blank_tape(&mut self, slot: &str) -> Result<(), String> {
        if self.host.is_none() {
            self.media.push((slot.into(), PendingMedia::BlankTape));
            return Ok(());
        }
        self.host_mut()?.blank_tape(slot)
    }
    /// Queues a physical machine key.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn key(&mut self, name: &str, pressed: bool) -> Result<(), String> {
        self.host_mut()?.input(InputEvent::Key {
            name: name.to_owned().into(),
            pressed,
        });
        Ok(())
    }
    /// Queues controller-one input.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn button(&mut self, name: &str, pressed: bool) -> Result<(), String> {
        self.button_on_port(1, name, pressed)
    }
    /// Queues a control on a numbered input port. Ports are the numbers
    /// printed on the machine, and 0 is its primary joystick (Kempston on a
    /// Spectrum), per `knowledge/decisions/joystick-port-numbering.md`.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn button_on_port(&mut self, port: u8, name: &str, pressed: bool) -> Result<(), String> {
        self.host_mut()?.input(InputEvent::Button {
            port,
            name: name.to_owned().into(),
            pressed,
        });
        Ok(())
    }
    /// Moves the guest mouse.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn mouse_move(&mut self, dx: i32, dy: i32) -> Result<(), String> {
        self.host_mut()?.input(InputEvent::PointerMotion {
            device: "mouse-1".into(),
            dx,
            dy,
        });
        Ok(())
    }
    /// Changes a mouse button.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn mouse_button(&mut self, button: &str, pressed: bool) -> Result<(), String> {
        self.host_mut()?.input(InputEvent::PointerButton {
            device: "mouse-1".into(),
            button: button.to_owned().into(),
            pressed,
        });
        Ok(())
    }
    /// Configures stereo sound.
    ///
    /// # Errors
    /// rejects unreasonable rates or unbooted use.
    pub fn configure_audio(&mut self, rate: u32) -> Result<(), String> {
        if !(8000..=192000).contains(&rate) {
            return Err("invalid audio sample rate".into());
        }
        self.host_mut()?.rate(rate);
        Ok(())
    }
    /// Drains interleaved stereo audio.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn audio(&mut self) -> Result<Vec<f32>, String> {
        Ok(self.host_mut()?.audio())
    }
    /// Copies RGBA pixels.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn pixels(&self) -> Result<Vec<u8>, String> {
        Ok(self.host_ref()?.pixels())
    }
    /// Picture width.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn width(&self) -> Result<u32, String> {
        Ok(self.host_ref()?.size().0)
    }
    /// Picture height.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn height(&self) -> Result<u32, String> {
        Ok(self.host_ref()?.size().1)
    }
    /// Frame period.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn frame_ms(&self) -> Result<f64, String> {
        Ok(self.host_ref()?.frame_ms())
    }
    /// Width of one picture pixel relative to its height, from the runtime's
    /// own statement of its display. Machines that state none report 1.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn pixel_aspect_ratio(&self) -> Result<f32, String> {
        Ok(self.host_ref()?.pixel_aspect_ratio())
    }
    /// Loads the inserted tape the way a person would from power-on: reset,
    /// wait for the prompt, type `LOAD ""` and press play. Returns the frames
    /// spent booting. The runtime's own autoload does the work, as it does
    /// for the browser player and `--autoload-tape`.
    ///
    /// # Errors
    /// Rejects machines without the helper (only the Spectrum has it), a
    /// missing tape, or a machine that never reaches its prompt.
    pub fn autoload_tape(&mut self, max_boot_frames: u32) -> Result<u32, String> {
        self.host_mut()?.autoload_tape(max_boot_frames)
    }
    /// Starts or stops a tape deck after the guest's load command.
    ///
    /// # Errors
    /// Requires a booted runtime with a matching transport.
    pub fn transport(&mut self, slot: &str, playing: bool) -> Result<(), String> {
        self.host_mut()?.transport(slot, playing)
    }
    /// Exports machine state, including in-memory media changes.
    /// # Errors
    /// Requires a booted runtime supporting snapshots.
    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        self.host_ref()?.save_state()
    }
    /// Restores compatible machine state.
    /// # Errors
    /// Rejects invalid state or unbooted use.
    pub fn restore_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.host_mut()?.restore_state(bytes)
    }
    /// Exports a current disk image, or what has been saved to tape.
    /// # Errors
    /// Requires a supported slot with mounted media, or a recording.
    pub fn export_media(&self, slot: &str) -> Result<Vec<u8>, String> {
        self.host_ref()?.export_media(slot)
    }
    /// DOM-code to machine-key chords.
    ///
    /// # Errors
    /// requires a booted machine.
    pub fn keymap(&self) -> Result<String, String> {
        Ok(self.host_ref()?.keymap().to_string())
    }
    /// Answers a runtime query path, such as `tape.playing`, as JSON text.
    ///
    /// # Errors
    /// requires a booted machine and a path the runtime answers.
    pub fn query(&self, path: &str) -> Result<String, String> {
        Ok(self.host_ref()?.query(path)?.to_string())
    }
}
impl Fleet {
    fn host_mut(&mut self) -> Result<&mut (dyn Host + '_), String> {
        match self.host.as_mut() {
            Some(host) => Ok(host.as_mut()),
            None => Err("boot the machine first".into()),
        }
    }
    fn host_ref(&self) -> Result<&dyn Host, String> {
        self.host
            .as_deref()
            .ok_or_else(|| "boot the machine first".into())
    }
}

#[cfg(all(test, feature = "atari-800xl"))]
mod tests {
    use super::*;
    #[test]
    fn atari_browser_pacing_uses_colour_clocks() {
        for (variant, expected_ms) in [("atari-800xl-ntsc", 16.6882), ("atari-800xl-pal", 20.0557)]
        {
            let mut machine = Fleet::new("atari-800xl", variant).expect("variant");
            machine.boot().expect("optional firmware");
            assert!((machine.frame_ms().expect("booted") - expected_ms).abs() < 0.001);
            assert_eq!(machine.advance(10.0).expect("advance"), 0);
            assert_eq!(machine.advance(11.0).expect("advance"), 1);
        }
    }
}
#[cfg(all(test, feature = "nintendo-game-boy"))]
mod media_before_boot_tests {
    use super::*;

    const CARTRIDGE: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-data/synthetic-cartridges/nintendo-game-boy-logo.gb"
    ));

    /// `Some(true)` loads the cartridge before boot, `Some(false)` after,
    /// `None` not at all.
    fn frames(before_boot: Option<bool>) -> Vec<Vec<u8>> {
        let mut machine = Fleet::new("nintendo-game-boy", "dmg").expect("compiled variant");
        if before_boot == Some(true) {
            machine
                .load("cartridge", CARTRIDGE)
                .expect("held until boot");
        }
        machine.boot().expect("needs no firmware");
        if before_boot == Some(false) {
            machine.load("cartridge", CARTRIDGE).expect("cartridge");
        }
        (0..60)
            .map(|_| {
                machine.step().expect("step");
                machine.pixels().expect("pixels")
            })
            .collect()
    }

    /// Media given before boot is in the machine before its first frame,
    /// so the run matches one loaded straight after boot.
    #[test]
    fn media_given_before_boot_is_in_place_for_the_first_frame() {
        let held = frames(Some(true));
        assert_eq!(held, frames(Some(false)));
        assert_ne!(
            held,
            frames(None),
            "the cartridge should change the picture"
        );
    }

    /// A slot the machine does not have is reported when boot inserts it.
    #[test]
    fn media_given_before_boot_is_checked_at_boot() {
        let mut machine = Fleet::new("nintendo-game-boy", "dmg").expect("compiled variant");
        machine
            .load("no-such-slot", CARTRIDGE)
            .expect("held until boot");
        assert!(machine.boot().is_err());
        assert!(machine.step().is_err(), "a failed boot leaves no machine");
    }
}

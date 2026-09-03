//! The DOM boundary: the `Spectrum` class JavaScript sees.
//!
//! Compiled only for `wasm32`. Everything here names a browser type, so on a
//! native build there is nothing to compile — but the key mapping in the
//! parent module stays target-independent so its tests run everywhere.
//!
//! Presentation is a 2-D canvas blit. The GPU path renders through the same
//! WGSL shader as the native app and would bring the CRT and LCD filters with
//! it, but it currently attaches to a canvas and draws nothing (#1436), and it
//! costs about 2.5 MB of wasm. This path works, is pixel-exact, and is the one
//! a lesson page can afford.

use emu198x_shell::{FamilyRuntime, FirmwareImage, FirmwareSet, MediaKind};
use emu198x_web::WebMachine;
use runtime_sinclair_zx_spectrum::{
    Model, SpectrumLiveAccess, SpectrumRuntimeKind, SpectrumSessionQueryProvider,
    autoload_basic_tape,
};
use wasm_bindgen::{Clamped, prelude::*};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

use crate::{parse_snapshot, spectrum_key_name};

/// Firmware id the 48K runtime expects for its ROM image.
const ROM_ID: &str = "sinclair-zx-spectrum-48k-rom";

/// A ZX Spectrum attached to a canvas.
#[wasm_bindgen]
pub struct Spectrum {
    machine: WebMachine<SpectrumRuntimeKind, SpectrumSessionQueryProvider>,
    canvas: HtmlCanvasElement,
    context: CanvasRenderingContext2d,
}

#[wasm_bindgen]
impl Spectrum {
    /// Builds a 48K attached to `canvas`, from ROM bytes the page supplies.
    ///
    /// Async even though nothing here awaits: restoring the GPU path (#1436)
    /// needs an adapter, and acquiring one is async. Shipping this synchronous
    /// would make that a breaking change for every consumer.
    ///
    /// The canvas's drawing buffer is resized to the machine's picture and the
    /// page keeps control of the displayed size through CSS. Pair it with
    /// `image-rendering: pixelated` or the browser will blur the pixels.
    ///
    /// # Errors
    ///
    /// Returns a JavaScript error if the ROM is not a valid 48K image or the
    /// canvas has no 2-D context.
    #[allow(clippy::unused_async)]
    pub async fn create(canvas: HtmlCanvasElement, rom: Vec<u8>) -> Result<Spectrum, JsError> {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(ROM_ID, &rom));
        let runtime = SpectrumRuntimeKind::from_firmware(Model::Spectrum48KPal, &firmware)
            .map_err(|error| JsError::new(&format!("building the 48K: {error}")))?;

        let context = canvas
            .get_context("2d")
            .map_err(|_| JsError::new("the canvas refused a 2-D context"))?
            .ok_or_else(|| JsError::new("the canvas has no 2-D context"))?
            .dyn_into::<CanvasRenderingContext2d>()
            .map_err(|_| JsError::new("the canvas context is not a 2-D context"))?;

        Ok(Spectrum {
            machine: WebMachine::new_with_query_provider(runtime, SpectrumSessionQueryProvider),
            canvas,
            context,
        })
    }

    /// Runs the machine for `elapsed_ms` of real time and draws the result.
    ///
    /// Returns the number of machine frames that ran, which is often zero: a
    /// 60 Hz display driving a 50 Hz machine has nothing to do on roughly one
    /// callback in six.
    ///
    /// While a tape is playing the machine runs ahead of the clock instead,
    /// and the count is correspondingly larger. That is not a setting a page
    /// has to find: a tape takes as long to load as it did in 1982, and a
    /// reader waiting on a lesson has no reason to sit through it.
    ///
    /// # Errors
    ///
    /// Returns a JavaScript error if the machine fails or the canvas rejects
    /// the frame.
    pub fn tick(&mut self, elapsed_ms: f64) -> Result<u32, JsError> {
        self.machine
            .set_turbo(self.machine.runtime().tape_is_playing());
        let ran = self
            .machine
            .advance(elapsed_ms)
            .map_err(|error| JsError::new(&format!("running the machine: {error}")))?;

        if ran > 0 {
            self.draw()?;
        }
        Ok(ran)
    }

    /// Loads a program into a media slot from bytes.
    ///
    /// `kind` is one of `tape`, `disk`, `snapshot`, `cartridge` or `program`.
    ///
    /// # Errors
    ///
    /// Returns a JavaScript error for an unknown slot or kind, or if the
    /// machine rejects the image.
    pub fn load(&mut self, slot: &str, kind: &str, bytes: &[u8]) -> Result<(), JsError> {
        let kind = match kind {
            "tape" => MediaKind::Tape,
            "disk" => MediaKind::Disk,
            "snapshot" => MediaKind::Snapshot,
            "cartridge" => MediaKind::Cartridge,
            "program" => MediaKind::Program,
            other => {
                return Err(JsError::new(&format!(
                    "unknown media kind {other:?}; expected tape, disk, snapshot, \
                     cartridge or program"
                )));
            }
        };
        self.machine
            .load_media_bytes(slot, kind, bytes)
            .map_err(|error| JsError::new(&format!("loading into {slot:?}: {error}")))
    }

    /// Waits for the boot prompt, types `LOAD ""`, and starts the tape.
    ///
    /// The way a lesson runs a program a learner just assembled. Loading
    /// through the real ROM matters beyond authenticity: the firmware
    /// initialises the machine as it goes, so a program can call ROM routines
    /// afterwards. A snapshot built by an assembler cannot offer that, because
    /// nobody has yet written down what a booted 48K holds in RAM.
    ///
    /// Drives the ROM keyboard editor rather than patching the ROM or
    /// short-circuiting the loader, and is the same code path the native
    /// binary's `--autoload-tape` takes — including its two hard-won waits, for
    /// the editor prompt to be repainted before it is read, and for the 128K
    /// family's loader to be listening before the tape rolls.
    ///
    /// Returns the number of frames spent waiting for boot. Load a tape first.
    ///
    /// # Errors
    ///
    /// Returns a JavaScript error if no tape is loaded, if the machine does
    /// not reach a boot prompt within `max_boot_frames`, or if the prompt
    /// never becomes ready for keyword entry.
    pub fn autoload(&mut self, max_boot_frames: u32) -> Result<u32, JsError> {
        let result = autoload_basic_tape(&mut self.machine, "tape-1", max_boot_frames)
            .map_err(|error| JsError::new(&format!("autoloading the tape: {error}")))?;
        Ok(result.boot.frames)
    }

    /// Builds a 48K on the ROM embedded in this package.
    ///
    /// The ordinary entry point for a page: the firmware travels with the
    /// emulator, so a lesson embed needs no ROM of its own and no file
    /// picker in front of the first thing a learner sees.
    ///
    /// Present only in a build made with the `bundled-rom` feature, which is
    /// how the npm package is published. A build without it uses
    /// [`create`](Self::create) and supplies its own image.
    ///
    /// # Errors
    ///
    /// Returns a JavaScript error if the canvas has no 2-D context.
    #[cfg(feature = "bundled-rom")]
    #[wasm_bindgen(js_name = createBundled)]
    pub async fn create_bundled(canvas: HtmlCanvasElement) -> Result<Spectrum, JsError> {
        Self::create(canvas, crate::BUNDLED_ROM.to_vec()).await
    }

    /// Loads a portable snapshot — `.sna` or `.z80` — from bytes.
    ///
    /// This is how a lesson runs the program it ships: the curriculum's
    /// capture pipeline builds `.sna` files, and a snapshot is applied to the
    /// machine rather than mounted in a slot, so it does not go through
    /// [`load`](Self::load).
    ///
    /// # Errors
    ///
    /// Returns a JavaScript error for an unknown format or bytes that do not
    /// parse.
    #[wasm_bindgen(js_name = loadSnapshot)]
    pub fn load_snapshot(&mut self, bytes: &[u8], format: &str) -> Result<(), JsError> {
        let snapshot = parse_snapshot(bytes, format)
            .map_err(|error| JsError::new(&format!("loading a snapshot: {error}")))?;
        self.machine.runtime_mut().apply_snapshot(&snapshot);
        Ok(())
    }

    /// The machine's media slots, for a page that wants to name one.
    /// Asks the machine a question, and hands back the answer as JSON.
    ///
    /// The same query surface the headless session and the MCP server use, so
    /// a page sees what a script sees rather than a browser-only subset. The
    /// paths a Spectrum answers include `cpu.pc`, `cpu.halted`, `cpu.iff1`,
    /// `cpu.instructions_retired`, `screen.text.lines`, `tape.playing` and
    /// `boot.detected`.
    ///
    /// This is what lets a lesson say *why* a machine stopped rather than
    /// offering a reset and moving on: a program that ran past its own last
    /// instruction has a `cpu.pc` outside the bytes it was assembled into, and
    /// one that halted with interrupts disabled is `cpu.halted` with
    /// `cpu.iff1` false. Both are mistakes a unit is teaching against.
    ///
    /// JSON rather than a native value: the answers are already JSON inside
    /// the query layer, and a page parses one string more cheaply than this
    /// crate grows a serialisation dependency.
    ///
    /// # Errors
    ///
    /// Returns a JavaScript error if the machine does not know the path.
    pub fn query(&self, path: &str) -> Result<String, JsError> {
        let result = self
            .machine
            .query(path)
            .map_err(|error| JsError::new(&format!("query {path:?}: {error}")))?;
        serde_json::to_string(&result.value)
            .map_err(|error| JsError::new(&format!("query {path:?} did not serialise: {error}")))
    }

    #[wasm_bindgen(js_name = mediaSlots)]
    #[must_use]
    pub fn media_slots(&self) -> Vec<String> {
        self.machine
            .media_slots()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect()
    }

    /// Presses a key, from a DOM `KeyboardEvent.code`.
    ///
    /// Returns `false` when the Spectrum has no such key, so the page can let
    /// the browser keep the keystroke instead of swallowing it.
    #[wasm_bindgen(js_name = keyDown)]
    pub fn key_down(&mut self, code: &str) -> bool {
        self.key(code, true)
    }

    /// Releases a key, from a DOM `KeyboardEvent.code`.
    #[wasm_bindgen(js_name = keyUp)]
    pub fn key_up(&mut self, code: &str) -> bool {
        self.key(code, false)
    }

    /// Starts or stops machine audio.
    #[wasm_bindgen(js_name = setAudioEnabled)]
    pub fn set_audio_enabled(&mut self, enabled: bool) {
        self.machine.set_audio_enabled(enabled);
    }

    /// Matches the audio buffer to the page's `AudioContext`.
    #[wasm_bindgen(js_name = configureAudio)]
    pub fn configure_audio(&mut self, sample_rate: u32, channels: u16, capacity: usize) {
        self.machine
            .configure_audio(sample_rate, channels, capacity);
    }

    /// Takes the buffered audio for the page to feed its worklet.
    #[wasm_bindgen(js_name = audioDrain)]
    #[must_use]
    pub fn audio_drain(&mut self) -> Vec<f32> {
        self.machine.audio_drain()
    }

    /// The machine's picture as RGBA bytes, for a page that wants to present
    /// it itself.
    #[wasm_bindgen(js_name = frameRgba)]
    #[must_use]
    pub fn frame_rgba(&self) -> Vec<u8> {
        self.machine.frame_rgba().to_vec()
    }

    /// Width and height of the machine's picture, as `[width, height]`.
    #[wasm_bindgen(js_name = frameSize)]
    #[must_use]
    pub fn frame_size(&self) -> Vec<u32> {
        let (width, height) = self.machine.frame_size();
        vec![width, height]
    }
}

impl Spectrum {
    /// Blits the current frame to the canvas.
    fn draw(&mut self) -> Result<(), JsError> {
        let (width, height) = self.machine.frame_size();
        if width == 0 || height == 0 {
            return Ok(());
        }

        // The machine's picture size is the drawing buffer. Setting it every
        // frame would reset the context, so only when it actually changes —
        // which it does when a Spectrum variant changes its border timing.
        if self.canvas.width() != width || self.canvas.height() != height {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
        }

        let pixels = self.machine.frame_rgba();
        if pixels.len() != (width as usize) * (height as usize) * 4 {
            return Ok(());
        }

        let image = ImageData::new_with_u8_clamped_array_and_sh(Clamped(pixels), width, height)
            .map_err(|_| JsError::new("the frame is not a valid image"))?;
        self.context
            .put_image_data(&image, 0.0, 0.0)
            .map_err(|_| JsError::new("the canvas rejected the frame"))
    }

    /// Maps a DOM code, falling back to the Spectrum's own names for the keys
    /// the generic mapping deliberately leaves alone.
    fn key(&mut self, code: &str, pressed: bool) -> bool {
        if let Some(name) = spectrum_key_name(code) {
            return self.machine.queue_key(name, pressed);
        }
        self.machine.key_event(code, pressed)
    }
}

//! The browser boundary: the `Spectrum` class JavaScript sees.
//!
//! Compiled only for `wasm32`. Everything here names a browser type, so on a
//! native build there is nothing to compile. Canvas-free constructors also
//! work in a Web Worker; canvas presentation stays optional. The key mapping in the
//! parent module stays target-independent so its tests run everywhere.
//!
//! Presentation is a 2-D canvas blit. The GPU path renders through the same
//! WGSL shader as the native app and would bring the CRT and LCD filters with
//! it, but it currently attaches to a canvas and draws nothing (#1436), and it
//! costs about 2.5 MB of wasm. This path works, is pixel-exact, and is the one
//! a lesson page can afford.

use emu198x_shell::{
    DebugPrimitives, FamilyRuntime, FirmwareImage, FirmwareSet, MediaKind, SessionDriver,
};
use emu198x_web::WebMachine;
use runtime_sinclair_zx_spectrum::{
    Model, SpectrumLiveAccess, SpectrumRuntimeKind, SpectrumSessionQueryProvider,
    autoload_basic_tape, load_basic_program_with_writer, tap_key,
};
use wasm_bindgen::{Clamped, prelude::*};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

use crate::{parse_snapshot, spectrum_key_name};

/// Firmware id the 48K runtime expects for its ROM image.
const ROM_ID: &str = "sinclair-zx-spectrum-48k-rom";

/// A ZX Spectrum with optional canvas presentation.
#[wasm_bindgen]
pub struct Spectrum {
    machine: WebMachine<SpectrumRuntimeKind, SpectrumSessionQueryProvider>,
    presentation: Option<CanvasPresentation>,
    trace_screen_writes: Option<(u16, u16)>,
    routine_stop: Option<u16>,
    routine_trace: serde_json::Value,
}

struct CanvasPresentation {
    canvas: HtmlCanvasElement,
    context: CanvasRenderingContext2d,
}

#[wasm_bindgen]
impl Spectrum {
    /// Exports the current machine state.
    pub fn save_state(&self) -> Result<Vec<u8>, JsError> {
        self.machine
            .save_state()
            .map_err(|e| JsError::new(&e.to_string()))
    }
    /// Restores compatible machine state, clearing pending host input/audio.
    pub fn restore_state(&mut self, bytes: &[u8]) -> Result<(), JsError> {
        self.machine
            .restore_state(bytes)
            .map_err(|e| JsError::new(&e.to_string()))
    }
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
        let mut spectrum = Self::create_headless(rom)?;

        let context = canvas
            .get_context("2d")
            .map_err(|_| JsError::new("the canvas refused a 2-D context"))?
            .ok_or_else(|| JsError::new("the canvas has no 2-D context"))?
            .dyn_into::<CanvasRenderingContext2d>()
            .map_err(|_| JsError::new("the canvas context is not a 2-D context"))?;

        spectrum.presentation = Some(CanvasPresentation { canvas, context });
        Ok(spectrum)
    }

    /// Builds a canvas-free 48K suitable for a Web Worker.
    ///
    /// Use `tick`, `frameSize` and `frameRgba` to run the same machine and
    /// transfer completed frames to a presenter. No DOM object is accessed.
    ///
    /// # Errors
    ///
    /// Returns an error if the supplied firmware cannot build a 48K machine.
    #[wasm_bindgen(js_name = createHeadless)]
    pub fn create_headless(rom: Vec<u8>) -> Result<Spectrum, JsError> {
        let mut firmware = FirmwareSet::new();
        firmware.push(FirmwareImage::new(ROM_ID, &rom));
        let runtime = SpectrumRuntimeKind::from_firmware(Model::Spectrum48KPal, &firmware)
            .map_err(|error| JsError::new(&format!("building the 48K: {error}")))?;

        Ok(Spectrum {
            machine: WebMachine::new_with_query_provider(runtime, SpectrumSessionQueryProvider),
            presentation: None,
            trace_screen_writes: None,
            routine_stop: None,
            routine_trace: serde_json::json!({"events": [], "complete": false}),
        })
    }

    /// Builds a canvas-free 48K with the package's bundled ROM.
    ///
    /// # Errors
    ///
    /// Returns an error if the bundled firmware cannot build the machine.
    #[cfg(feature = "bundled-rom")]
    #[wasm_bindgen(js_name = createHeadlessBundled)]
    pub fn create_headless_bundled() -> Result<Spectrum, JsError> {
        Self::create_headless(crate::BUNDLED_ROM.to_vec())
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

    /// Advances exactly one frame without drawing to a canvas.
    /// # Errors
    /// Returns the runtime's execution error.
    pub fn step(&mut self) -> Result<(), JsError> {
        self.machine
            .run_one_frame()
            .map_err(|e| JsError::new(&e.to_string()))
    }
    /// Frame period derived from the machine profile.
    pub fn frame_ms(&self) -> f64 {
        self.machine.frame_ms()
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

    /// Installs a numbered BASIC listing directly into RAM and asks the ROM
    /// to RUN it. Call on a fresh 48K; no tape is mounted or played.
    ///
    /// # Errors
    /// Returns conversion, boot or editor-prompt errors.
    #[wasm_bindgen(js_name = runBasic)]
    pub fn run_basic(&mut self, source: &str) -> Result<(), JsError> {
        let program = format_sinclair_zx_spectrum_bas::tokenise_listing(source)
            .map_err(|error| JsError::new(&error))?;
        load_basic_program_with_writer(
            &mut self.machine,
            &program,
            true,
            400,
            |host, addr, byte| host.runtime_mut().write_byte(addr, byte),
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(())
    }

    /// Installs machine code in a fresh 48K and calls it through the ROM's
    /// RANDOMIZE USR, with CLEAR reserving its RAM and a valid return stack.
    ///
    /// # Errors
    /// Rejects empty code, overlap with BASIC/system RAM, overflow, entry
    /// outside the code, and ROM boot or prompt failures.
    #[wasm_bindgen(js_name = runCode)]
    pub fn run_code(&mut self, bytes: &[u8], origin: u32, entry: u32) -> Result<(), JsError> {
        let source = crate::code_launcher(bytes.len(), origin, entry)
            .map_err(|error| JsError::new(&error))?;
        let program = format_sinclair_zx_spectrum_bas::tokenise_listing(&source)
            .map_err(|error| JsError::new(&error))?;
        load_basic_program_with_writer(
            &mut self.machine,
            &program,
            false,
            400,
            |host, addr, byte| host.runtime_mut().write_byte(addr, byte),
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
        for (i, byte) in bytes.iter().enumerate() {
            self.machine
                .runtime_mut()
                .write_byte((origin as usize + i) as u16, *byte);
        }
        if let Some(stop) = self.routine_stop {
            tap_key(&mut self.machine, "r").map_err(|error| JsError::new(&error.to_string()))?;
            self.machine.queue_key("Enter", true);
            self.machine
                .apply_pending_input()
                .map_err(|error| JsError::new(&error.to_string()))?;
            let (reached, _, _) = self
                .machine
                .runtime_mut()
                .run_until_pc(entry as u16, 14_000_000);
            self.machine.queue_key("Enter", false);
            self.machine
                .apply_pending_input()
                .map_err(|error| JsError::new(&error.to_string()))?;
            if !reached {
                return Err(JsError::new("The ROM did not reach the program entry."));
            }
            let runtime = self.machine.runtime_mut();
            runtime
                .start_memory_write_watch(0x4000, 0x800)
                .map_err(JsError::new)?;
            let mut events = Vec::new();
            let mut complete = false;
            for _ in 0..4096 {
                let before = runtime.z80_registers().clone();
                if before.pc == stop {
                    complete = true;
                    break;
                }
                if u32::from(before.pc) < origin
                    || u32::from(before.pc) >= origin + bytes.len() as u32
                {
                    break;
                }
                let opcode = runtime.read_byte(before.pc);
                let target = u16::from_le_bytes([
                    runtime.read_byte(before.pc.wrapping_add(1)),
                    runtime.read_byte(before.pc.wrapping_add(2)),
                ]);
                let stack_target = u16::from_le_bytes([
                    runtime.read_byte(before.sp),
                    runtime.read_byte(before.sp.wrapping_add(1)),
                ]);
                runtime.clear_memory_write_watch_records();
                runtime.step_instructions(1);
                let after = runtime.z80_registers().clone();
                let before_state = serde_json::json!({"pc":before.pc,"sp":before.sp,"a":before.a(),"b":before.b(),"hl":before.hl,"de":before.de});
                let after_state = serde_json::json!({"pc":after.pc,"sp":after.sp,"a":after.a(),"b":after.b(),"hl":after.hl,"de":after.de});
                if opcode == 0xcd && after.sp == before.sp.wrapping_sub(2) && after.pc == target {
                    let return_address = u16::from_le_bytes([
                        runtime.read_byte(after.sp),
                        runtime.read_byte(after.sp.wrapping_add(1)),
                    ]);
                    events.push(serde_json::json!({"kind":"call","pc":before.pc,"target":after.pc,"returnAddress":return_address,"before":before_state,"after":after_state}));
                } else if opcode == 0xc9
                    && after.sp == before.sp.wrapping_add(2)
                    && after.pc == stack_target
                {
                    events.push(serde_json::json!({"kind":"return","pc":before.pc,"target":after.pc,"before":before_state,"after":after_state}));
                }
                for write in runtime.memory_write_watch_records().unwrap_or(&[]) {
                    events.push(serde_json::json!({"kind":"write","pc":before.pc,"addr":write.addr,"value":write.value,"before":before_state,"after":after_state}));
                }
            }
            runtime.stop_memory_write_watch();
            self.routine_trace =
                serde_json::json!({"events":events,"complete":complete,"stop":stop});
            self.machine
                .run_frames(2)
                .map_err(|error| JsError::new(&error.to_string()))?;
            return Ok(());
        }
        if let Some((address, length)) = self.trace_screen_writes {
            self.machine
                .runtime_mut()
                .start_memory_write_watch(address, length)
                .map_err(JsError::new)?;
        }
        tap_key(&mut self.machine, "r")
            .and_then(|()| tap_key(&mut self.machine, "enter"))
            .map_err(|error| JsError::new(&error.to_string()))?;
        self.machine
            .run_frames(30)
            .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(())
    }

    /// Enables a bounded debugger recording of CALL nn, RET and bitmap writes,
    /// stopping before the named hold address. Use on a fresh machine.
    #[wasm_bindgen(js_name = enableRoutineTrace)]
    pub fn enable_routine_trace(&mut self, stop: u16) {
        self.routine_stop = Some(stop);
    }

    /// Returns the executed routine events and whether the stop was reached.
    ///
    /// # Errors
    /// Returns a JSON serialisation error if the recording cannot be encoded.
    #[wasm_bindgen(js_name = routineTrace)]
    pub fn routine_trace(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.routine_trace).map_err(|error| JsError::new(&error.to_string()))
    }

    /// Records writes in a selected bitmap range during the next direct code run.
    /// Call on a fresh machine. Narrow ranges leave room for the program's writes
    /// after the ROM clears the display.
    ///
    /// # Errors
    /// Rejects empty ranges or ranges outside bitmap RAM ($4000..$5800).
    #[wasm_bindgen(js_name = enableScreenWriteTrace)]
    pub fn enable_screen_write_trace(&mut self, address: u32, length: u32) -> Result<(), JsError> {
        if !(0x4000..0x5800).contains(&address) || length == 0 || length > 0x5800 - address {
            return Err(JsError::new(
                "Trace range must fit inside bitmap RAM ($4000..$5800).",
            ));
        }
        self.trace_screen_writes = Some((address as u16, length as u16));
        Ok(())
    }

    /// Returns captured writes (including ROM writes) and saturation status.
    /// Consumers can select the program's PC range without inventing a trace.
    ///
    /// # Errors
    /// Returns a serialisation error if the capture cannot be encoded.
    #[wasm_bindgen(js_name = screenWriteTrace)]
    pub fn screen_write_trace(&self) -> Result<String, JsError> {
        let records = self
            .machine
            .runtime()
            .memory_write_watch_records()
            .unwrap_or(&[]);
        // The shared Spectrum tracer's documented default cap is 8192 records.
        serde_json::to_string(
            &serde_json::json!({"writes": records, "full": records.len() >= 8192}),
        )
        .map_err(|error| JsError::new(&error.to_string()))
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

    /// Inspect shared debugger registers and disassembly at PC.
    ///
    /// # Errors
    /// Returns an error if state cannot be serialised.
    #[wasm_bindgen(js_name = debugState)]
    pub fn debug_state(&self) -> Result<String, JsError> {
        let runtime = self.machine.runtime();
        let pc = runtime.dbg_pc();
        let instruction = runtime.dbg_disassemble(pc).map(|(text, length)| {
            let bytes: Vec<u8> = (0..u32::from(length))
                .map(|offset| runtime.dbg_peek((pc + offset) & 0xffff))
                .collect();
            serde_json::json!({"text": text, "bytes": bytes})
        });
        serde_json::to_string(&serde_json::json!({
            "cpu": runtime.dbg_cpu_state(), "instruction": instruction
        }))
        .map_err(|error| JsError::new(&error.to_string()))
    }

    /// Use the existing bounded native step; suspend the host frame loop first.
    ///
    /// # Errors
    /// Returns an error if pending input cannot be delivered.
    #[wasm_bindgen(js_name = debugStep)]
    pub fn debug_step(&mut self) -> Result<u64, JsError> {
        self.machine
            .apply_pending_input()
            .map_err(|error| JsError::new(&error.to_string()))?;
        Ok(self.machine.runtime_mut().dbg_step())
    }

    /// Run to an instruction boundary, bounded to 14 million half-cycles.
    /// Suspend the host frame loop before calling this method.
    ///
    /// # Errors
    /// Rejects invalid addresses and input delivery errors.
    #[wasm_bindgen(js_name = debugRunTo)]
    pub fn debug_run_to(&mut self, address: u32) -> Result<bool, JsError> {
        let target = u16::try_from(address)
            .map_err(|_| JsError::new("Breakpoint address must fit within 0..65535."))?;
        self.machine
            .apply_pending_input()
            .map_err(|error| JsError::new(&error.to_string()))?;
        let (reached, _, _) = self.machine.runtime_mut().run_until_pc(target, 14_000_000);
        Ok(reached)
    }

    /// Reads visible memory for a lesson's live inspection panel.
    ///
    /// # Errors
    /// Rejects ranges extending beyond the 64 KiB address space.
    #[wasm_bindgen(js_name = readMemory)]
    pub fn read_memory(&self, address: u32, length: u32) -> Result<Vec<u8>, JsError> {
        if address > 0xffff || length > 0x10000 - address {
            return Err(JsError::new("Memory range must fit within 0..65536."));
        }
        Ok((address..address + length)
            .map(|addr| self.machine.runtime().read_byte(addr as u16))
            .collect())
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
        let Some(presentation) = &self.presentation else {
            return Ok(());
        };
        let (width, height) = self.machine.frame_size();
        if width == 0 || height == 0 {
            return Ok(());
        }

        // The machine's picture size is the drawing buffer. Setting it every
        // frame would reset the context, so only when it actually changes —
        // which it does when a Spectrum variant changes its border timing.
        if presentation.canvas.width() != width || presentation.canvas.height() != height {
            presentation.canvas.set_width(width);
            presentation.canvas.set_height(height);
        }

        let pixels = self.machine.frame_rgba();
        if pixels.len() != (width as usize) * (height as usize) * 4 {
            return Ok(());
        }

        let image = ImageData::new_with_u8_clamped_array_and_sh(Clamped(pixels), width, height)
            .map_err(|_| JsError::new("the frame is not a valid image"))?;
        presentation
            .context
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

/// Tokenise source and create an auto-starting BASIC TAP, without a machine.
///
/// # Errors
/// Returns a JavaScript error for unsupported or malformed listing input.
#[wasm_bindgen(js_name = basicTape)]
pub fn basic_tape(source: &str, name: &str) -> Result<Vec<u8>, JsError> {
    crate::basic_tape(source, name).map_err(|error| JsError::new(&error))
}

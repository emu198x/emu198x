# Adding a New System to Emu198x

> Archived document. Do not treat status claims here as current. Current state lives in `../../status/` and binding rules/decisions.


A step-by-step guide for adding a new emulated system. Follow this
checklist to get a system from zero to fully integrated with all
features: unified app, debugger, save states, rewind, audio
visualiser, chip inspectors, WASM build, and compatibility testing.

---

## 1. Name the System

Pick the `{manufacturer}-{system}` identifier. This name propagates
everywhere — crate directories, config keys, ROM paths, system IDs.

| System | Identifier |
|--------|-----------|
| Amstrad CPC | `amstrad-cpc` |
| Game Boy | `nintendo-game-boy` |
| Mega Drive | `sega-mega-drive` |

MSX is the exception (multi-vendor, no manufacturer prefix).

---

## 2. Create Chip Crates

Each IC gets its own crate: `{manufacturer}-{chipname}`.

```
crates/
  yamaha-ym2149/       # Sound chip
  sharp-lr35902/       # Game Boy CPU
  motorola-6809/       # CPC/Dragon CPU
```

**Cargo.toml:**
```toml
[package]
name = "yamaha-ym2149"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
emu-core = { path = "../emu-core" }

[lints]
workspace = true
```

**Requirements per chip:**
- `tick()` method at the chip's native clock rate
- All state in the struct (no globals)
- `Send` — required for the background emulation thread
- Per-channel audio: `take_buffer()` for mixed output,
  `take_channel_buffers()` for per-channel visualiser data
- `save_state(&self, &mut Vec<u8>)` and
  `load_state(&mut self, &[u8]) -> Result<usize, String>` for
  save/rewind support

**Shared chips:** If the system uses a chip that already exists
(Z80, 6502, AY-3-8910, SN76489, TMS9918), depend on the existing
crate. Don't duplicate.

---

## 3. Create the Machine Crate

`machine-{manufacturer}-{system}` — the platform-independent emulation
library. No windowing, audio output, or rendering dependencies.

```
crates/machine-amstrad-cpc/
  Cargo.toml
  src/
    lib.rs          # Module declarations + pub re-exports
    cpc.rs          # System struct + Machine + EmulatedSystem impls
    bus.rs          # Address bus decoding
    config.rs       # CpcConfig, CpcModel, CpcRegion enums
    input.rs        # Key/joystick types
    memory.rs       # RAM/ROM banking (if complex)
```

**Cargo.toml:**
```toml
[package]
name = "machine-amstrad-cpc"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
emu-core = { path = "../emu-core" }
zilog-z80 = { path = "../zilog-z80" }
yamaha-ym2149 = { path = "../yamaha-ym2149" }
# ... other chip deps, NO native deps (no winit, wgpu, cpal)

[lib]
name = "machine_amstrad_cpc"
path = "src/lib.rs"

[lints]
workspace = true
```

### 3a. Implement `Machine` trait

```rust
impl Machine for Cpc {
    fn run_frame(&mut self) { /* tick until frame complete */ }
    fn framebuffer(&self) -> &[u32] { /* ARGB32 pixels */ }
    fn framebuffer_width(&self) -> u32 { 768 }
    fn framebuffer_height(&self) -> u32 { 272 }
    fn take_audio_buffer(&mut self) -> Vec<AudioFrame> { /* stereo pairs */ }
    fn frame_count(&self) -> u64 { self.frame_count }
    fn reset(&mut self) { /* CPU reset pin */ }
}
```

### 3b. Implement `EmulatedSystem` trait

Every method has a default, but implement them all for full
integration. Group by feature area:

**Identity (required):**
```rust
fn system_info(&self) -> &SystemInfo { /* id, name, manufacturer, year, extensions, config_options */ }
fn display_info(&self) -> DisplayInfo { /* PAR, scale, frame_duration */ }
```

**Input (required):**
```rust
fn input_ports(&self) -> Vec<InputPort> { /* keyboard, joystick ports with default bindings */ }
fn handle_key(&mut self, port: usize, key: &str, pressed: bool) { /* route DOM key to hardware */ }
```

**Media (if the system loads files at runtime):**
```rust
fn load_media(&mut self, path: &Path) -> Result<(), String> {
    let (data, ext) = emu_core::zip_support::read_media(path, &["dsk", "sna", "tap"])?;
    match ext.as_str() { /* parse and load */ }
}
```

**Audio visualiser:**
```rust
fn audio_channels(&self) -> Vec<AudioChannelInfo> { /* one per voice/channel */ }
fn take_channel_audio(&mut self) -> Vec<Vec<f32>> { /* per-channel samples */ }
fn channel_frequencies(&self) -> Vec<Option<(f32, f32)>> { /* (freq_hz, volume) for piano roll */ }
```

**Debugger:**
```rust
fn cpu_count(&self) -> usize { 1 }
fn cpu_name(&self, index: usize) -> &'static str { "Z80" }
fn cpu_registers(&self, index: usize) -> Vec<(&str, Value)> { /* register snapshot */ }
fn debug_read(&self, cpu_index: usize, addr: u32) -> Option<u8> { /* peek memory */ }
fn debug_write(&mut self, cpu_index: usize, addr: u32, value: u8) -> bool { /* poke memory */ }
fn disassemble(&self, cpu_index: usize, addr: u32) -> Option<(String, u8)> {
    let read = |a: u16| self.peek(a).unwrap_or(0);
    let (s, len) = zilog_z80::disasm::disassemble(addr as u16, read);
    Some((s, len))
}
fn step_instruction(&mut self, cpu_index: usize) -> u64 {
    let mut ticks = 0u64;
    loop {
        self.tick();
        ticks += 1;
        if self.cpu.is_instruction_complete() { break; }
        if ticks > 10_000 { break; }
    }
    ticks
}
```

**Chip inspectors:**
```rust
fn palette_info(&self) -> Option<PaletteInfo> { /* current palette */ }
fn sprite_info(&self) -> Vec<SpriteEntry> { /* hardware sprites if any */ }
fn pattern_table(&self) -> Vec<PatternTable> { /* tile/character data */ }
fn memory_map(&self) -> Vec<MemoryRegion> { /* ROM/RAM/IO regions */ }
fn input_state(&self) -> Vec<InputState> { /* current button/key state */ }
```

**Tape/peripherals (if applicable):**
```rust
fn tape_status(&self) -> Option<TapeStatus> { /* position, playing, label */ }
fn tape_command(&mut self, action: TapeAction) -> bool { /* transport controls */ }
fn peripheral_status(&self) -> Vec<PeripheralIndicator> { /* LEDs, counters */ }
fn media_label(&self) -> Option<&str> { /* loaded disk/cart name */ }
```

**Mouse (if applicable):**
```rust
fn has_mouse(&self) -> bool { true }
fn handle_mouse_move(&mut self, x: f32, y: f32) { /* update mouse state */ }
fn handle_mouse_button(&mut self, button: u8, pressed: bool) { /* LMB/RMB */ }
```

**Printer (if applicable):**
```rust
fn has_printer(&self) -> bool { true }
fn printer_output(&mut self) -> Vec<u8> { /* drain bytes sent to printer port */ }
```

**Save states (required for rewind):**
```rust
fn save_state(&self) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    buf.push(1); // version
    buf.extend_from_slice(&self.master_clock.to_le_bytes());
    buf.extend_from_slice(&self.frame_count.to_le_bytes());
    // ... CPU regs, RAM, chip state
    Some(buf)
}
fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
    if data.is_empty() || data[0] != 1 { return Err("Bad version".into()); }
    let mut pos = 1;
    // ... restore in the same order
    Ok(())
}
```

---

## 4. Create the Emu Crate (Runner)

`emu-{manufacturer}-{system}` — thin wrapper with CLI, native modules,
and the `register()` factory function.

```
crates/emu-amstrad-cpc/
  Cargo.toml
  src/
    lib.rs       # pub use machine_amstrad_cpc::*; + register() + native modules
    main.rs      # CLI argument parsing + Runner
    capture.rs   # Screenshot/recording (native-only)
```

**lib.rs:**
```rust
pub use machine_amstrad_cpc::*;

pub fn register() -> emu_core::SystemEntry {
    use emu_core::{ConfigOption, ConfigValues, SystemEntry, SystemInfo};

    let info = SystemInfo {
        id: "amstrad-cpc",
        name: "Amstrad CPC",
        manufacturer: "Amstrad",
        year: 1984,
        file_extensions: &["dsk", "sna", "tap", "cdt"],
        config_options: vec![
            ConfigOption::Choice {
                id: "model",
                name: "Model",
                choices: &[("464", "CPC 464"), ("6128", "CPC 6128")],
                default: "464",
            },
        ],
    };

    fn factory(config: &ConfigValues) -> Result<Box<dyn emu_core::EmulatedSystem>, String> {
        // Read config, discover ROMs from ~/.emu198x/roms/amstrad-cpc/,
        // construct the system
    }

    SystemEntry { info, factory }
}
```

**Cargo.toml features:**
```toml
[features]
default = ["native"]
native = ["emu-core/renderer", "dep:winit", ...]

[dependencies]
machine-amstrad-cpc = { path = "../machine-amstrad-cpc" }
emu-core = { path = "../emu-core" }
```

---

## 5. Create the WASM Crate

`emu-{manufacturer}-{system}-wasm` — browser build.

```
crates/emu-amstrad-cpc-wasm/
  Cargo.toml
  src/lib.rs
```

**Cargo.toml:**
```toml
[package]
name = "emu-amstrad-cpc-wasm"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
machine-amstrad-cpc = { path = "../machine-amstrad-cpc" }
emu-core = { path = "../emu-core" }
wasm-bindgen = "0.2"
```

**src/lib.rs** — `#[wasm_bindgen]` wrapper with:
- `new()` constructor
- `run_frame()`, `framebuffer_rgba_ptr()`, `width()`, `height()`
- `audio_buffer_ptr()`, `audio_buffer_len()`
- `key_down(code)`, `key_up(code)`
- `load_file(data, ext)`, `reset()`, `frame_count()`

Build with: `wasm-pack build --target web --release`

---

## 6. Register in the Unified App

**emu198x-app/Cargo.toml** — add dependency:
```toml
machine-amstrad-cpc = { path = "../machine-amstrad-cpc", default-features = false }
```

Note: depend on `machine-*` (not `emu-*`) since the app doesn't need
native runner features. But you need `register()` which lives in
`emu-*`. If `register()` uses embedded ROMs or native-only features,
keep the `emu-*` dep. Otherwise move `register()` to the machine
crate.

**emu198x-app/src/catalogue.rs** — add one line:
```rust
machine_amstrad_cpc::register(),
```

**emu198x-compat/Cargo.toml + src/main.rs** — same pattern for the
compatibility harness.

**emu198x-compat/src/detect.rs** — add file extensions:
```rust
"dsk" | "cdt" => Some("amstrad-cpc"),
```

---

## 7. Format Crates (if needed)

Each media format gets its own crate:
`format-{manufacturer}-{system}-{format}`

```
crates/format-amstrad-cpc-dsk/    # DSK disk image parser
crates/format-amstrad-cpc-cdt/    # CDT tape image parser
crates/format-amstrad-cpc-sna/    # SNA snapshot parser
```

Format crates are pure parsing — no hardware dependencies.

---

## 8. ROM Discovery

ROM-dependent systems search `~/.emu198x/roms/{system-id}/`.
The factory function in `register()` reads ROMs from there:

```rust
let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
let dir = PathBuf::from(home).join(".emu198x").join("roms").join("amstrad-cpc");
let rom = std::fs::read(dir.join("cpc464.rom"))
    .map_err(|e| format!("ROM not found: {e}"))?;
```

Cartridge-only systems (no system ROM required) can create a minimal
idle system in the factory and let the user load media via File > Open.

---

## 9. Add Tests to emu-test-suite

Every new system MUST have tests in `crates/emu-test-suite/src/lib.rs`.
These are self-contained (no external ROMs) and run in CI.

### Required tests per system

**a) CPU execution test** — embed a small test program in a ROM that:
- Writes signature bytes ($42) to RAM using arithmetic
- Enters an idle loop at a known address
- Verify: `peek(ram_addr) == 0x42`, PC at idle loop

For Z80 systems, use `z80_test_rom(ram_base)` with the system's RAM
address. For 6502 systems, use `m6502_test_code()` with the correct
reset vector and JMP target.

**b) Save/load roundtrip** — call `save_load_roundtrip(&mut system)`:
saves state, runs 10 frames, restores, verifies PC matches.

**c) Trait completeness** — call `check_trait_completeness(name, &system)`:
verifies system_info, cpu_registers, palette_info, memory_map all
return non-empty data with valid values.

**d) Video output** — write to video RAM via `debug_write()`, run a
frame, verify the framebuffer has changed. At minimum, check that
`framebuffer_has_content()` returns true.

**e) Audio output** — if the system has a sound chip, configure it
(write to PSG/SID/AY registers via the test ROM), run frames, verify
`take_audio_buffer()` returns non-empty data with samples in -1..1.

**f) Interrupt test** — embed an interrupt handler in the test ROM
(at $0038 for Z80 IM 1, or via NMI/IRQ vectors for 6502). Enable
interrupts, HALT/wait, verify the handler wrote its signature byte.

**g) Flag verification** — run arithmetic operations and verify CPU
flags are correct. Z80: save flags via PUSH AF / POP BC / store.
6502: save flags via PHP / PLA / STA.

**h) Frame timing** — verify `frame_count()` increments correctly
after `run_frame()`.

### Example: adding tests for Amstrad CPC

```rust
// In z80_tests module:
#[test]
fn cpc_cpu_test() {
    let mut rom = z80_test_rom(0xC000);
    rom.resize(16384, 0); // 16KB ROM
    let system = machine_amstrad_cpc::Cpc::new(
        rom,
        machine_amstrad_cpc::CpcModel::Cpc464,
    );
    run_z80_test("CPC", Box::new(system), 0xC000, 5);
}

// In trait_tests module:
#[test]
fn cpc_trait_complete() {
    let rom = vec![0; 16384];
    let system = machine_amstrad_cpc::Cpc::new(
        rom,
        machine_amstrad_cpc::CpcModel::Cpc464,
    );
    check_trait_completeness("CPC", &system);
}

// In audio_tests module:
#[test]
fn cpc_ay_audio_output() {
    // ROM that programs AY-3-8910 to produce a tone
    let mut rom = vec![0; 16384];
    // ... OUT instructions to set AY frequency + volume ...
    let mut system = machine_amstrad_cpc::Cpc::new(rom, ...);
    run_frames(&mut system, 10);
    let audio = system.take_audio_buffer();
    assert!(!audio.is_empty(), "CPC: no audio output");
}
```

### Running the test suite

```bash
cargo test -p emu-test-suite --lib          # all tests
cargo test -p emu-test-suite --lib z80      # Z80 systems only
cargo test -p emu-test-suite --lib video    # video tests only
cargo test -p emu-test-suite -- --nocapture # show eprintln output
```

---

## 10. Verification Checklist

After implementing, verify each feature works:

- [ ] `cargo check -p machine-{system}` — compiles
- [ ] `cargo check -p emu-{system}` — compiles
- [ ] `cargo check -p emu198x-app` — unified app compiles
- [ ] `cargo check -p emu198x-compat` — compat harness compiles
- [ ] `cargo check -p emu-{system}-wasm --target wasm32-unknown-unknown` — WASM compiles
- [ ] `cargo test -p machine-{system}` — crate tests pass
- [ ] `cargo test -p emu-test-suite` — all suite tests pass (including new system)
- [ ] Launcher shows the system with correct name/year/manufacturer
- [ ] Config options render as dropdowns/checkboxes
- [ ] System launches and shows video output
- [ ] Keyboard input works
- [ ] Audio plays
- [ ] File > Open loads media (including from ZIP)
- [ ] Debugger: registers show values, disassembly shows mnemonics
- [ ] Debugger: step instruction advances PC
- [ ] Debugger: step back (F9) rewinds to previous snapshot
- [ ] Palette viewer shows colours
- [ ] Memory map shows regions
- [ ] Input overlay shows button state
- [ ] Audio waveforms show per-channel traces
- [ ] Piano roll shows note frequencies
- [ ] Save state: save to slot, load back, screen matches
- [ ] Rewind: "RW: N" appears in status bar, F9 steps back
- [ ] Screenshot saves a valid PNG
- [ ] Compat harness: system detected from file extension

---

## 11. What Not To Do

- **Don't put UI code in the machine crate.** No winit, egui, wgpu,
  cpal, rfd, or muda. The machine crate must compile for WASM.
- **Don't skip `Send`.** Every type in the machine crate must be Send.
  Use `Box<dyn Trait + Send>` for trait objects.
- **Don't hardcode paths.** ROM discovery uses `$HOME/.emu198x/roms/`.
  Embedded ROMs (via `include_bytes!`) go in the emu-* crate, not
  the machine crate (paths are relative to the crate location).
- **Don't use floating point for timing.** Derive all component
  timing from integer ratios of the master crystal frequency.
- **Don't step by instruction.** Tick at crystal frequency. All
  component timing derives from this.

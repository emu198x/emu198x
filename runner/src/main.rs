mod audio;
mod input;
mod video;

use audio::{AudioOutput, SAMPLES_PER_FRAME};
use gilrs::{Event, Gilrs};
use machine_spectrum::Spectrum48K;
use minifb::{Key, Window, WindowOptions};
use std::fs;
use video::{HEIGHT, WIDTH};

fn main() {
    let mut spec = Spectrum48K::new();

    // Initialize gamepad support
    let mut gilrs = Gilrs::new().expect("Failed to initialize gamepad support");
    let mut active_gamepad = None;

    // Load the ROM
    let rom = fs::read("roms/48.rom").expect("Failed to load ROM");
    spec.load_rom(&rom);

    // Load a file if provided (supports .tap and .sna)
    if let Some(file_path) = std::env::args().nth(1) {
        let data = fs::read(&file_path).expect("Failed to load file");
        let lower = file_path.to_lowercase();

        if lower.ends_with(".sna") {
            spec.load_sna(&data).expect("Failed to load .SNA snapshot");
            println!("Loaded snapshot: {}", file_path);
        } else if lower.ends_with(".tap") {
            spec.load_tape(data);
            println!("Loaded tape: {}", file_path);
        } else {
            eprintln!("Unknown file type: {}", file_path);
            eprintln!("Supported formats: .tap, .sna");
            std::process::exit(1);
        }
    }

    let mut window = Window::new("Spectrum", WIDTH, HEIGHT, WindowOptions::default())
        .expect("Failed to create window");

    // Audio pacing controls emulation speed via blocking push.
    // Set a high ceiling to prevent spinning when audio buffer is filling.
    window.set_target_fps(200);

    let mut buffer = vec![0u32; WIDTH * HEIGHT];

    // Initialize audio output
    let mut audio_output = AudioOutput::new();
    if audio_output.is_none() {
        eprintln!("Warning: No audio device available, sound disabled");
    }
    let mut audio_samples = [0.0f32; SAMPLES_PER_FRAME];

    // Track beeper level from previous frame for audio continuity
    let mut prev_beeper_level = false;

    // Frame counter for FLASH attribute (toggles every 16 frames)
    let mut frame_count: u32 = 0;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Poll gamepad events to track active gamepad
        while let Some(Event { id, .. }) = gilrs.next_event() {
            active_gamepad = Some(id);
        }

        // Handle input
        let keys = window.get_keys();
        spec.reset_keyboard();
        for key in &keys {
            for &(row, bit) in input::keyboard::map_key(*key) {
                spec.key_down(row, bit);
            }
        }

        // Combine keyboard and gamepad input for Kempston joystick
        let kempston = input::joystick::map_keyboard(&keys)
            | input::joystick::map_gamepad(&gilrs, active_gamepad);
        spec.set_kempston(kempston);

        spec.run_frame();

        // Generate audio from beeper transitions
        if let Some(ref mut audio) = audio_output {
            audio::generate_frame_samples(
                spec.beeper_transitions(),
                prev_beeper_level,
                &mut audio_samples,
            );
            audio.push_samples(&audio_samples);
        }
        prev_beeper_level = spec.beeper_level();

        // Render display
        let flash_swap = (frame_count / 16) % 2 == 1;
        video::render_screen(spec.screen(), spec.border(), flash_swap, &mut buffer);
        window.update_with_buffer(&buffer, WIDTH, HEIGHT).unwrap();
        frame_count = frame_count.wrapping_add(1);
    }
}

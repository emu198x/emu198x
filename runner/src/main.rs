mod audio;
mod input;
mod video;

use audio::{AudioOutput, SAMPLES_PER_FRAME};
use machine_spectrum::Spectrum48K;
use minifb::{Key, Window, WindowOptions};
use std::fs;
use video::{HEIGHT, WIDTH};

fn main() {
    let mut spec = Spectrum48K::new();

    // Load the ROM
    let rom = fs::read("roms/48.rom").expect("Failed to load ROM");
    spec.load_rom(&rom);

    // Load a tape if provided
    if let Some(tap_path) = std::env::args().nth(1) {
        let tape = fs::read(&tap_path).expect("Failed to load tape");
        spec.load_tape(tape);
        println!("Loaded tape: {}", tap_path);
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
        // Handle keyboard
        spec.reset_keyboard();
        for key in window.get_keys() {
            for &(row, bit) in input::map_key(key) {
                spec.key_down(row, bit);
            }
        }

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

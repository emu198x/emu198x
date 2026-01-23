mod audio;

use audio::{AudioOutput, SAMPLES_PER_FRAME};
use machine_spectrum::Spectrum48K;
use minifb::{Key, Window, WindowOptions};
use std::fs;

// Native Spectrum resolution
const NATIVE_WIDTH: usize = 320; // 256 + 32 left + 32 right
const NATIVE_HEIGHT: usize = 256; // 192 + 32 top + 32 bottom
const BORDER: usize = 32;

// Integer scale factor for sharp pixels
const SCALE: usize = 3;

// Window dimensions
const WIDTH: usize = NATIVE_WIDTH * SCALE;
const HEIGHT: usize = NATIVE_HEIGHT * SCALE;

const COLOURS: [u32; 16] = [
    // Normal
    0xFF000000, // 0: black
    0xFF0000D7, // 1: blue
    0xFFD70000, // 2: red
    0xFFD700D7, // 3: magenta
    0xFF00D700, // 4: green
    0xFF00D7D7, // 5: cyan
    0xFFD7D700, // 6: yellow
    0xFFD7D7D7, // 7: white
    // Bright
    0xFF000000, // 8: black (same)
    0xFF0000FF, // 9: bright blue
    0xFFFF0000, // 10: bright red
    0xFFFF00FF, // 11: bright magenta
    0xFF00FF00, // 12: bright green
    0xFF00FFFF, // 13: bright cyan
    0xFFFFFF00, // 14: bright yellow
    0xFFFFFFFF, // 15: bright white
];

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
        let keys = window.get_keys();
        spec.reset_keyboard();
        for key in keys {
            for &(row, bit) in map_key(key) {
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

        let flash_swap = (frame_count / 16) % 2 == 1;
        render_screen(spec.screen(), spec.border(), flash_swap, &mut buffer);
        window.update_with_buffer(&buffer, WIDTH, HEIGHT).unwrap();
        frame_count = frame_count.wrapping_add(1);
    }
}

fn map_key(key: Key) -> &'static [(usize, u8)] {
    // Returns slice of (row, bit) pairs - most keys map to one, some to two
    match key {
        // Row 0: SHIFT, Z, X, C, V
        Key::LeftShift | Key::RightShift => &[(0, 0)],
        Key::Z => &[(0, 1)],
        Key::X => &[(0, 2)],
        Key::C => &[(0, 3)],
        Key::V => &[(0, 4)],

        // Row 1: A, S, D, F, G
        Key::A => &[(1, 0)],
        Key::S => &[(1, 1)],
        Key::D => &[(1, 2)],
        Key::F => &[(1, 3)],
        Key::G => &[(1, 4)],

        // Row 2: Q, W, E, R, T
        Key::Q => &[(2, 0)],
        Key::W => &[(2, 1)],
        Key::E => &[(2, 2)],
        Key::R => &[(2, 3)],
        Key::T => &[(2, 4)],

        // Row 3: 1, 2, 3, 4, 5
        Key::Key1 => &[(3, 0)],
        Key::Key2 => &[(3, 1)],
        Key::Key3 => &[(3, 2)],
        Key::Key4 => &[(3, 3)],
        Key::Key5 => &[(3, 4)],

        // Row 4: 0, 9, 8, 7, 6
        Key::Key0 => &[(4, 0)],
        Key::Key9 => &[(4, 1)],
        Key::Key8 => &[(4, 2)],
        Key::Key7 => &[(4, 3)],
        Key::Key6 => &[(4, 4)],

        // Row 5: P, O, I, U, Y
        Key::P => &[(5, 0)],
        Key::O => &[(5, 1)],
        Key::I => &[(5, 2)],
        Key::U => &[(5, 3)],
        Key::Y => &[(5, 4)],

        // Row 6: ENTER, L, K, J, H
        Key::Enter => &[(6, 0)],
        Key::L => &[(6, 1)],
        Key::K => &[(6, 2)],
        Key::J => &[(6, 3)],
        Key::H => &[(6, 4)],

        // Row 7: SPACE, SYM, M, N, B
        Key::Space => &[(7, 0)],
        Key::LeftCtrl | Key::RightCtrl => &[(7, 1)], // Symbol shift
        Key::M => &[(7, 2)],
        Key::N => &[(7, 3)],
        Key::B => &[(7, 4)],

        // Compound keys
        Key::Backspace => &[(0, 0), (4, 0)], // CAPS SHIFT + 0 = DELETE
        Key::Left => &[(0, 0), (3, 4)],      // CAPS SHIFT + 5 = Left
        Key::Down => &[(0, 0), (4, 4)],      // CAPS SHIFT + 6 = Down
        Key::Up => &[(0, 0), (4, 3)],        // CAPS SHIFT + 7 = Up
        Key::Right => &[(0, 0), (4, 2)],     // CAPS SHIFT + 8 = Right

        _ => &[],
    }
}

fn render_screen(screen: &[u8], border: u8, flash_swap: bool, buffer: &mut [u32]) {
    let border_colour = COLOURS[border as usize];

    // Render at native resolution, then scale
    for native_y in 0..NATIVE_HEIGHT {
        for native_x in 0..NATIVE_WIDTH {
            let pixel = if native_y < BORDER
                || native_y >= BORDER + 192
                || native_x < BORDER
                || native_x >= BORDER + 256
            {
                border_colour
            } else {
                let screen_y = native_y - BORDER;
                let screen_x = native_x - BORDER;
                let x_byte = screen_x / 8;
                let bit = screen_x % 8;

                let bitmap_addr = ((screen_y & 0xC0) << 5)
                    | ((screen_y & 0x07) << 8)
                    | ((screen_y & 0x38) << 2)
                    | x_byte;

                let attr_addr = 0x1800 + (screen_y / 8) * 32 + x_byte;

                let byte = screen[bitmap_addr];
                let attr = screen[attr_addr];

                let flash = attr & 0x80 != 0;
                let bright = if attr & 0x40 != 0 { 8 } else { 0 };
                let mut ink = (attr & 0x07) as usize + bright;
                let mut paper = ((attr >> 3) & 0x07) as usize + bright;

                // FLASH attribute swaps ink and paper every 16 frames
                if flash && flash_swap {
                    std::mem::swap(&mut ink, &mut paper);
                }

                if byte & (0x80 >> bit) != 0 {
                    COLOURS[ink]
                } else {
                    COLOURS[paper]
                }
            };

            // Write scaled pixel block
            for sy in 0..SCALE {
                for sx in 0..SCALE {
                    let dest_x = native_x * SCALE + sx;
                    let dest_y = native_y * SCALE + sy;
                    buffer[dest_y * WIDTH + dest_x] = pixel;
                }
            }
        }
    }
}

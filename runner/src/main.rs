use machine_spectrum::Spectrum48K;
use minifb::{Key, Window, WindowOptions};
use std::fs;

const WIDTH: usize = 320; // 256 + 32 left + 32 right
const HEIGHT: usize = 256; // 192 + 32 top + 32 bottom
const BORDER: usize = 32;

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

    let mut window = Window::new("Spectrum", WIDTH, HEIGHT, WindowOptions::default())
        .expect("Failed to create window");

    // Limit to roughly 50fps
    window.set_target_fps(50);

    let mut buffer = vec![0u32; WIDTH * HEIGHT];

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Handle keyboard
        let keys = window.get_keys();
        spec.reset_keyboard();
        for key in keys {
            if let Some((row, bit)) = map_key(key) {
                spec.key_down(row, bit);
            }
        }

        spec.run_frame();
        render_screen(spec.screen(), spec.border(), &mut buffer);
        window.update_with_buffer(&buffer, WIDTH, HEIGHT).unwrap();
    }
}

fn map_key(key: Key) -> Option<(usize, u8)> {
    // Returns (row, bit)
    match key {
        // Row 0: SHIFT, Z, X, C, V
        Key::LeftShift | Key::RightShift => Some((0, 0)),
        Key::Z => Some((0, 1)),
        Key::X => Some((0, 2)),
        Key::C => Some((0, 3)),
        Key::V => Some((0, 4)),

        // Row 1: A, S, D, F, G
        Key::A => Some((1, 0)),
        Key::S => Some((1, 1)),
        Key::D => Some((1, 2)),
        Key::F => Some((1, 3)),
        Key::G => Some((1, 4)),

        // Row 2: Q, W, E, R, T
        Key::Q => Some((2, 0)),
        Key::W => Some((2, 1)),
        Key::E => Some((2, 2)),
        Key::R => Some((2, 3)),
        Key::T => Some((2, 4)),

        // Row 3: 1, 2, 3, 4, 5
        Key::Key1 => Some((3, 0)),
        Key::Key2 => Some((3, 1)),
        Key::Key3 => Some((3, 2)),
        Key::Key4 => Some((3, 3)),
        Key::Key5 => Some((3, 4)),

        // Row 4: 0, 9, 8, 7, 6
        Key::Key0 => Some((4, 0)),
        Key::Key9 => Some((4, 1)),
        Key::Key8 => Some((4, 2)),
        Key::Key7 => Some((4, 3)),
        Key::Key6 => Some((4, 4)),

        // Row 5: P, O, I, U, Y
        Key::P => Some((5, 0)),
        Key::O => Some((5, 1)),
        Key::I => Some((5, 2)),
        Key::U => Some((5, 3)),
        Key::Y => Some((5, 4)),

        // Row 6: ENTER, L, K, J, H
        Key::Enter => Some((6, 0)),
        Key::L => Some((6, 1)),
        Key::K => Some((6, 2)),
        Key::J => Some((6, 3)),
        Key::H => Some((6, 4)),

        // Row 7: SPACE, SYM, M, N, B
        Key::Space => Some((7, 0)),
        Key::LeftCtrl | Key::RightCtrl => Some((7, 1)), // Symbol shift
        Key::M => Some((7, 2)),
        Key::N => Some((7, 3)),
        Key::B => Some((7, 4)),

        _ => None,
    }
}

fn render_screen(screen: &[u8], border: u8, buffer: &mut [u32]) {
    let border_colour = COLOURS[border as usize];

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let pixel = if y < BORDER || y >= BORDER + 192 || x < BORDER || x >= BORDER + 256 {
                border_colour
            } else {
                let screen_y = y - BORDER;
                let screen_x = x - BORDER;
                let x_byte = screen_x / 8;
                let bit = screen_x % 8;

                let bitmap_addr = ((screen_y & 0xC0) << 5)
                    | ((screen_y & 0x07) << 8)
                    | ((screen_y & 0x38) << 2)
                    | x_byte;

                let attr_addr = 0x1800 + (screen_y / 8) * 32 + x_byte;

                let byte = screen[bitmap_addr];
                let attr = screen[attr_addr];

                let bright = if attr & 0x40 != 0 { 8 } else { 0 };
                let ink = (attr & 0x07) as usize + bright;
                let paper = ((attr >> 3) & 0x07) as usize + bright;

                if byte & (0x80 >> bit) != 0 {
                    COLOURS[ink]
                } else {
                    COLOURS[paper]
                }
            };
            buffer[y * WIDTH + x] = pixel;
        }
    }
}

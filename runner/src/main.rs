use machine_spectrum::Spectrum48K;
use minifb::{Key, Window, WindowOptions};

const WIDTH: usize = 256;
const HEIGHT: usize = 192;

fn main() {
    let mut spec = Spectrum48K::new();

    // Load our test program
    spec.load(
        0x0000,
        &[
            0x21, 0x00, 0x40, // LD HL, 0x4000
            0x3E, 0xAA, // LD A, 0xAA (alternating bits: 10101010)
            0x77, // LD (HL), A
            0x23, // INC HL
            0x3C, // INC A
            0xC3, 0x05, 0x00, // JP 0x0005
        ],
    );

    let mut window = Window::new("Spectrum", WIDTH, HEIGHT, WindowOptions::default())
        .expect("Failed to create window");

    // Limit to roughly 50fps
    window.set_target_fps(50);

    let mut buffer = vec![0u32; WIDTH * HEIGHT];

    // Run a few frames to fill the screen
    for _ in 0..3 {
        spec.run_frame();
    }

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Don't run the CPU any more, just display
        render_screen(spec.screen(), &mut buffer);
        window.update_with_buffer(&buffer, WIDTH, HEIGHT).unwrap();
    }
}

fn render_screen(screen: &[u8], buffer: &mut [u32]) {
    for y in 0..192 {
        for x_byte in 0..32 {
            // Spectrum screen address calculation
            let addr = ((y & 0xC0) << 5) | ((y & 0x07) << 8) | ((y & 0x38) << 2) | x_byte;

            let byte = screen[addr];

            for bit in 0..8 {
                let x = x_byte * 8 + bit;
                let pixel = if byte & (0x80 >> bit) != 0 {
                    0xFFFFFFFFu32
                } else {
                    0xFF000000u32
                };
                buffer[y * 256 + x] = pixel;
            }
        }
    }
}

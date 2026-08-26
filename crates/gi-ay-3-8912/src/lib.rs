//! General Instrument AY-3-8912 Programmable Sound Generator.
//!
//! The AY-3-8912 is the AY-3-8910 with **one** I/O port (port A, register
//! 14) instead of two — port B (register 15) is bonded out on the 8910
//! but absent on the 8912. The sound core (3 tone channels, noise,
//! envelope) is identical, so this crate is a thin facade over
//! [`emu198x_gi_ay_3_8910::Ay3_8910`] that exposes only the port-A surface. New
//! machines that need port B should use `gi-ay-3-8910` directly.
//!
//! Used in the ZX Spectrum 128K, Amstrad CPC, Atari ST (as the Yamaha
//! YM2149 clone), and many arcade machines.
//!
//! On the Spectrum 128K:
//! - AY clock = CPU clock / 2 = 1.7734 MHz
//! - Register select: OUT to port $FFFD
//! - Data write: OUT to port $BFFD
//! - Data read: IN from port $FFFD

use emu198x_gi_ay_3_8910::Ay3_8910;

// The register-write tracer lives in the base 8910 crate so every AY-bearing
// core — whether it embeds the 8910 or this 8912 facade — reaches one struct.
pub use emu198x_gi_ay_3_8910::{AyWriteRecord, AyWriteWatch, DEFAULT_AY_WATCH_CAP};

/// AY-3-8912: the single-port member of the AY-3-891x family. Wraps the
/// shared [`Ay3_8910`] core and surfaces only the port-A I/O; port B is
/// not bonded out on this part.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Ay3_8912 {
    inner: Ay3_8910,
}

impl Ay3_8912 {
    /// Create a new AY chip.
    ///
    /// - `ay_clock_hz`: AY clock frequency (e.g., 1_773_400 for 128K Spectrum)
    /// - `sample_rate`: audio output sample rate (e.g., 44100)
    /// - `samples_per_frame`: pre-allocated buffer size
    pub fn new(ay_clock_hz: u32, sample_rate: u32, samples_per_frame: usize) -> Self {
        Self {
            inner: Ay3_8910::new(ay_clock_hz, sample_rate, samples_per_frame),
        }
    }

    /// Configure the host-side wiring of AY I/O port A (register 14).
    /// Bits that are pulled low on the motherboard read back as 0 even
    /// when the chip drives them high. On the Sinclair 128K family this
    /// mask is `0xBF` (bit 6 = serial CTS, tied low). Defaults to `0xFF`.
    pub fn set_port_a_input_mask(&mut self, mask: u8) {
        self.inner.set_port_a_input_mask(mask);
    }

    /// Select which register (0-15) subsequent reads/writes address.
    pub fn select_register(&mut self, reg: u8) {
        self.inner.select_register(reg);
    }

    /// Write a value to the currently selected register.
    pub fn write_data(&mut self, val: u8) {
        self.inner.write_data(val);
    }

    /// Read the currently selected register's value.
    #[must_use]
    pub fn read_data(&self) -> u8 {
        self.inner.read_data()
    }

    /// The currently selected register index (0-15).
    #[must_use]
    pub fn selected_register(&self) -> u8 {
        self.inner.selected_register()
    }

    /// Borrow the full 16-register file in index order.
    #[must_use]
    pub fn registers(&self) -> &[u8; 16] {
        self.inner.registers()
    }

    /// Advance one AY clock cycle. Call at `ay_clock_hz` rate.
    pub fn tick(&mut self) {
        self.inner.tick();
    }

    /// Finish the frame and write audio samples (0.0–1.0) to `out`.
    pub fn end_frame(&mut self, out: &mut [f32]) {
        self.inner.end_frame(out);
    }

    /// Number of samples generated this frame so far.
    #[must_use]
    pub fn samples_per_frame(&self) -> usize {
        self.inner.samples_per_frame()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_read_write() {
        let mut ay = Ay3_8912::new(1_773_400, 44100, 882);
        ay.select_register(0);
        ay.write_data(0xAB);
        assert_eq!(ay.read_data(), 0xAB);

        ay.select_register(1);
        ay.write_data(0xFF);
        assert_eq!(ay.read_data(), 0x0F);
    }

    #[test]
    fn tone_produces_output() {
        let mut ay = Ay3_8912::new(1_773_400, 44100, 882);
        ay.select_register(0);
        ay.write_data(100);
        ay.select_register(1);
        ay.write_data(0);
        ay.select_register(7);
        ay.write_data(0x3E);
        ay.select_register(8);
        ay.write_data(15);
        for _ in 0..35_000 {
            ay.tick();
        }
        let mut out = vec![0.0f32; 882];
        ay.end_frame(&mut out);
        let max = out.iter().cloned().fold(0.0f32, f32::max);
        assert!(max > 0.1, "expected audible output, got max={max}");
    }

    #[test]
    fn sinclair_128k_port_a_pull_returns_bf_for_register_14() {
        let mut ay = Ay3_8912::new(1_773_400, 44100, 882);
        ay.set_port_a_input_mask(0xBF);
        ay.select_register(7);
        ay.write_data(0xFF);
        ay.select_register(14);
        ay.write_data(0xFF);
        assert_eq!(ay.read_data(), 0xBF);

        ay.select_register(7);
        ay.write_data(0x3F);
        ay.select_register(14);
        assert_eq!(ay.read_data(), 0xBF);
    }
}

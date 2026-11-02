//! Physical KR1/KR2 scan circuitry. Behaviour follows the four-state detector
//! and /114 phase in Altirra's ATAudio/source/pokey.cpp. Circuit and scanner
//! descriptions: Atari CX5200 Field Service Manual, Rev. 4 (1983), Figs. 2-4
//! and 6-9; Avery Lee, Altirra Hardware Reference Manual (2026), §§5.8, 13.3.

use serde::{Deserialize, Serialize};

use super::{IRQ_BREAK, IRQ_KEY, Pokey};

#[derive(Serialize, Deserialize)]
pub(super) struct KeyboardScan {
    address: u8,
    compare: u8,
    state: u8,
    control: bool,
    shift: bool,
    break_down: bool,
    kr1: bool,
    kr2: bool,
    remaining: u8,
}

impl KeyboardScan {
    fn new() -> Self {
        Self {
            address: 0,
            compare: 0,
            state: 0,
            control: false,
            shift: false,
            break_down: false,
            kr1: false,
            kr2: false,
            remaining: 114,
        }
    }

    pub(super) fn write_control(&mut self, old: u8, new: u8, status: &mut u8) {
        if old & 3 == 0 && new & 3 != 0 {
            self.remaining = 81;
        }
        if old & 2 != 0 && new & 2 == 0 {
            self.state = 0;
            self.address = 0;
            *status |= 4;
        }
    }

    pub(super) fn encode(&self) -> [u8; 10] {
        [
            self.address,
            self.compare,
            self.state,
            u8::from(self.control),
            u8::from(self.shift),
            u8::from(self.break_down),
            u8::from(self.kr1),
            u8::from(self.kr2),
            self.remaining,
            0,
        ]
    }

    pub(super) fn decode(data: &[u8]) -> Result<Self, String> {
        if data.len() != 10
            || data[0] > 63
            || data[1] > 63
            || data[2] > 3
            || data[3..8].iter().any(|&v| v > 1)
            || !(1..=114).contains(&data[8])
            || data[9] != 0
        {
            return Err("invalid POKEY keyboard scan state".into());
        }
        Ok(Self {
            address: data[0],
            compare: data[1],
            state: data[2],
            control: data[3] != 0,
            shift: data[4] != 0,
            break_down: data[5] != 0,
            kr1: data[6] != 0,
            kr2: data[7] != 0,
            remaining: data[8],
        })
    }
}

impl Pokey {
    /// Attach physical keyboard-return wiring before clocking the chip.
    /// Machines using injected characters can retain `press_key` instead.
    pub fn enable_keyboard_scan(&mut self) {
        self.keyboard = Some(KeyboardScan::new());
    }

    /// Current six-bit keyboard address, sampled on the next /114 edge.
    #[must_use]
    pub fn keyboard_scan_address(&self) -> u8 {
        self.keyboard.as_ref().map_or(0, |scan| scan.address)
    }

    /// Drive KR1/KR2 as asserted switch closures (true means pulled low).
    pub fn set_keyboard_returns(&mut self, kr1: bool, kr2: bool) {
        if let Some(scan) = &mut self.keyboard {
            scan.kr1 = kr1;
            scan.kr2 = kr2;
        }
    }

    pub(super) fn tick_keyboard(&mut self) {
        let Some(scan) = &mut self.keyboard else {
            return;
        };
        if self.skctl & 3 == 0 {
            return;
        }
        scan.remaining -= 1;
        if scan.remaining != 0 {
            return;
        }
        scan.remaining = 114;
        if self.skctl & 2 == 0 {
            return;
        }
        let address = scan.address;
        scan.address = (address + 1) & 63;
        match address {
            0x00 => scan.control = scan.kr2,
            0x10 => {
                scan.shift = scan.kr2;
                if scan.shift {
                    self.skstat &= !8;
                } else {
                    self.skstat |= 8;
                }
            }
            0x30 => {
                if scan.kr2 && !scan.break_down && self.irqen & IRQ_BREAK != 0 {
                    self.irqst &= !IRQ_BREAK;
                }
                scan.break_down = scan.kr2;
            }
            _ => {}
        }
        let same = address == scan.compare || self.skctl & 1 == 0;
        match scan.state {
            0 if scan.kr1 => {
                scan.compare = address;
                scan.state = 1;
            }
            1 if scan.kr1 && same => {
                scan.state = 2;
                self.kbcode = address
                    | if scan.shift { 0x40 } else { 0 }
                    | if scan.control { 0x80 } else { 0 };
                self.skstat &= !4;
                if self.irqen & IRQ_KEY != 0 {
                    if self.irqst & IRQ_KEY == 0 {
                        self.skstat &= !0x40;
                    } else {
                        self.irqst &= !IRQ_KEY;
                    }
                }
            }
            1 if scan.kr1 || same => scan.state = 0,
            2 if same && !scan.kr1 => scan.state = 3,
            3 if same => {
                if scan.kr1 {
                    scan.state = 2;
                } else {
                    scan.state = 0;
                    self.skstat |= 4;
                }
            }
            _ => {}
        }
    }
}

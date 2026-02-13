//! Shift and rotate instruction implementation for the 68000.

use super::Cpu68000;
use crate::common::alu::Size;
use crate::common::flags::{Status, C, X};

impl Cpu68000 {
    /// Execute a register shift/rotate instruction.
    ///
    /// - `kind`: 0=AS, 1=LS, 2=ROX, 3=RO
    /// - `direction`: false=right, true=left
    /// - `count_or_reg`: shift count (immediate) or register number
    /// - `reg`: destination data register
    /// - `size`: operation size
    /// - `immediate`: true=count in opcode, false=count in register
    #[allow(clippy::too_many_lines)]
    pub(super) fn exec_shift_reg(
        &mut self,
        kind: u8,
        direction: bool,
        count_or_reg: u8,
        reg: u8,
        size: Option<Size>,
        immediate: bool,
    ) {
        let Some(size) = size else {
            self.illegal_instruction();
            return;
        };

        let count = if immediate {
            if count_or_reg == 0 { 8 } else { count_or_reg as u32 }
        } else {
            self.regs.d[count_or_reg as usize] % 64
        };

        let value = self.read_data_reg(reg, size);
        let (mask, msb_bit) = match size {
            Size::Byte => (0xFF_u32, 0x80_u32),
            Size::Word => (0xFFFF, 0x8000),
            Size::Long => (0xFFFF_FFFF, 0x8000_0000),
        };

        let (result, carry) = match (kind, direction) {
            // ASL
            (0, true) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    if count >= bits {
                        let c = if count == bits { value & 1 != 0 } else { false };
                        (0, c)
                    } else {
                        let shifted = (value << count) & mask;
                        let c = (value >> (bits - count)) & 1 != 0;
                        (shifted, c)
                    }
                }
            }
            // ASR
            (0, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let sign_bit = value & msb_bit != 0;
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    if count >= bits {
                        let result = if sign_bit { mask } else { 0 };
                        (result, sign_bit)
                    } else {
                        let mut result = value;
                        for _ in 0..count {
                            result = (result >> 1) | if sign_bit { msb_bit } else { 0 };
                        }
                        let c = (value >> (count - 1)) & 1 != 0;
                        (result & mask, c)
                    }
                }
            }
            // LSL
            (1, true) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    if count >= bits {
                        let c = if count == bits { value & 1 != 0 } else { false };
                        (0, c)
                    } else {
                        let shifted = (value << count) & mask;
                        let c = (value >> (bits - count)) & 1 != 0;
                        (shifted, c)
                    }
                }
            }
            // LSR
            (1, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    if count >= bits {
                        let c = if count == bits {
                            (value >> (bits - 1)) & 1 != 0
                        } else {
                            false
                        };
                        (0, c)
                    } else {
                        let shifted = (value >> count) & mask;
                        let c = (value >> (count - 1)) & 1 != 0;
                        (shifted, c)
                    }
                }
            }
            // ROXL
            (2, true) => {
                if count == 0 {
                    let x = self.regs.sr & X != 0;
                    (value, x)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    let total_bits = bits + 1;
                    let count = count % total_bits;
                    if count == 0 {
                        let x = self.regs.sr & X != 0;
                        (value, x)
                    } else {
                        let x_bit = if self.regs.sr & X != 0 { 1u64 } else { 0 };
                        let extended = (x_bit << bits) | u64::from(value & mask);
                        let rotated = ((extended << count) | (extended >> (total_bits - count)))
                            & ((1u64 << total_bits) - 1);
                        let result = (rotated & u64::from(mask)) as u32;
                        let new_x = (rotated >> bits) & 1 != 0;
                        (result, new_x)
                    }
                }
            }
            // ROXR
            (2, false) => {
                if count == 0 {
                    let x = self.regs.sr & X != 0;
                    (value, x)
                } else {
                    let bits = match size {
                        Size::Byte => 8u32,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    let total_bits = bits + 1;
                    let count = count % total_bits;
                    if count == 0 {
                        let x = self.regs.sr & X != 0;
                        (value, x)
                    } else {
                        let x_bit = if self.regs.sr & X != 0 { 1u64 } else { 0 };
                        let extended = (x_bit << bits) | u64::from(value & mask);
                        let rotated = ((extended >> count) | (extended << (total_bits - count)))
                            & ((1u64 << total_bits) - 1);
                        let result = (rotated & u64::from(mask)) as u32;
                        let new_x = (rotated >> bits) & 1 != 0;
                        (result, new_x)
                    }
                }
            }
            // ROL
            (3, true) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    let eff_count = count % bits;
                    if eff_count == 0 {
                        (value, value & 1 != 0)
                    } else {
                        let rotated =
                            ((value << eff_count) | (value >> (bits - eff_count))) & mask;
                        let c = rotated & 1 != 0;
                        (rotated, c)
                    }
                }
            }
            // ROR
            (3, false) => {
                if count == 0 {
                    (value, false)
                } else {
                    let bits = match size {
                        Size::Byte => 8,
                        Size::Word => 16,
                        Size::Long => 32,
                    };
                    let eff_count = count % bits;
                    if eff_count == 0 {
                        (value, value & msb_bit != 0)
                    } else {
                        let rotated =
                            ((value >> eff_count) | (value << (bits - eff_count))) & mask;
                        let c = (value >> (eff_count - 1)) & 1 != 0;
                        (rotated, c)
                    }
                }
            }
            _ => (value, false),
        };

        self.write_data_reg(reg, result, size);

        // Set flags: N and Z based on result
        self.set_flags_move(result, size);

        // C flag
        if count > 0 {
            self.regs.sr = Status::set_if(self.regs.sr, C, carry);
            if kind < 3 {
                self.regs.sr = Status::set_if(self.regs.sr, X, carry);
            }
        } else if kind == 2 {
            let x = self.regs.sr & X != 0;
            self.regs.sr = Status::set_if(self.regs.sr, C, x);
        } else {
            self.regs.sr &= !C;
        }

        // V flag: ASL sets V if MSB changed at ANY point during shift
        if kind == 0 && direction {
            if count == 0 {
                self.regs.sr &= !crate::flags::V;
            } else {
                let bits = match size {
                    Size::Byte => 8u32,
                    Size::Word => 16,
                    Size::Long => 32,
                };
                let v = if count >= bits {
                    (value & mask) != 0
                } else {
                    let check_bits = count + 1;
                    let check_mask = if check_bits >= bits {
                        mask
                    } else {
                        ((1u32 << check_bits) - 1) << (bits - check_bits)
                    };
                    let top_bits = value & check_mask;
                    top_bits != 0 && top_bits != check_mask
                };
                self.regs.sr = Status::set_if(self.regs.sr, crate::flags::V, v);
            }
        } else {
            self.regs.sr &= !crate::flags::V;
        }

        // Timing: 6 + 2*count for byte/word, 8 + 2*count for long
        let base_cycles = if size == Size::Long { 8 } else { 6 };
        self.queue_internal(base_cycles + 2 * count as u8);
    }
}

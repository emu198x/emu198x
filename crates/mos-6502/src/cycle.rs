use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AddrMode {
    Implied,
    Accumulator,
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    IndirectX,
    IndirectY,
    Relative,
    JmpAbs,
    JmpInd,
    Jsr,
    Rts,
    Rti,
    Brk,
    Push,
    Pull,
    Jam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum OpCategory {
    Read,
    Write,
    ReadModWrite,
    Control,
    Implied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Operation {
    Lda,
    Ldx,
    Ldy,
    Ora,
    And,
    Eor,
    Adc,
    Sbc,
    Cmp,
    Cpx,
    Cpy,
    Bit,
    Sta,
    Stx,
    Sty,
    Asl,
    Lsr,
    Rol,
    Ror,
    Inc,
    Dec,
    AslA,
    LsrA,
    RolA,
    RorA,
    Tax,
    Tay,
    Txa,
    Tya,
    Tsx,
    Txs,
    Inx,
    Iny,
    Dex,
    Dey,
    Clc,
    Sec,
    Cli,
    Sei,
    Clv,
    Cld,
    Sed,
    Nop,
    Pha,
    Php,
    Pla,
    Plp,
    Bcc,
    Bcs,
    Beq,
    Bne,
    Bpl,
    Bmi,
    Bvc,
    Bvs,
    Jmp,
    Jsr,
    Rts,
    Rti,
    Brk,
    Lax,
    Sax,
    Dcp,
    Isc,
    Slo,
    Rla,
    Sre,
    Rra,
    Anc,
    Alr,
    Arr,
    Axs,
    NopRead,
    Jam,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct OpcodeInfo {
    pub addr_mode: AddrMode,
    pub operation: Operation,
}

impl Operation {
    #[must_use]
    pub fn category(self) -> OpCategory {
        match self {
            Self::Lda
            | Self::Ldx
            | Self::Ldy
            | Self::Ora
            | Self::And
            | Self::Eor
            | Self::Adc
            | Self::Sbc
            | Self::Cmp
            | Self::Cpx
            | Self::Cpy
            | Self::Bit
            | Self::Lax
            | Self::NopRead
            | Self::Anc
            | Self::Alr
            | Self::Arr
            | Self::Axs => OpCategory::Read,
            Self::Sta | Self::Stx | Self::Sty | Self::Sax => OpCategory::Write,
            Self::Asl
            | Self::Lsr
            | Self::Rol
            | Self::Ror
            | Self::Inc
            | Self::Dec
            | Self::Dcp
            | Self::Isc
            | Self::Slo
            | Self::Rla
            | Self::Sre
            | Self::Rra => OpCategory::ReadModWrite,
            Self::Bcc
            | Self::Bcs
            | Self::Beq
            | Self::Bne
            | Self::Bpl
            | Self::Bmi
            | Self::Bvc
            | Self::Bvs
            | Self::Jmp
            | Self::Jsr
            | Self::Rts
            | Self::Rti
            | Self::Brk
            | Self::Pha
            | Self::Php
            | Self::Pla
            | Self::Plp
            | Self::Jam => OpCategory::Control,
            _ => OpCategory::Implied,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct CycleState {
    pub cycle: u8,
    pub opcode: u8,
    pub info: Option<OpcodeInfo>,
    pub addr: u16,
    pub data: u8,
    pub offset: u8,
    pub page_crossed: bool,
}

impl CycleState {
    pub fn reset(&mut self) {
        self.cycle = 0;
        self.opcode = 0;
        self.info = None;
        self.addr = 0;
        self.data = 0;
        self.offset = 0;
        self.page_crossed = false;
    }
}

pub(crate) fn decode(opcode: u8) -> OpcodeInfo {
    use AddrMode as A;
    use Operation as O;

    match opcode {
        0x00 => OpcodeInfo {
            addr_mode: A::Brk,
            operation: O::Brk,
        },
        0x20 => OpcodeInfo {
            addr_mode: A::Jsr,
            operation: O::Jsr,
        },
        0x40 => OpcodeInfo {
            addr_mode: A::Rti,
            operation: O::Rti,
        },
        0x60 => OpcodeInfo {
            addr_mode: A::Rts,
            operation: O::Rts,
        },
        0x4C => OpcodeInfo {
            addr_mode: A::JmpAbs,
            operation: O::Jmp,
        },
        0x6C => OpcodeInfo {
            addr_mode: A::JmpInd,
            operation: O::Jmp,
        },

        0x10 => OpcodeInfo {
            addr_mode: A::Relative,
            operation: O::Bpl,
        },
        0x30 => OpcodeInfo {
            addr_mode: A::Relative,
            operation: O::Bmi,
        },
        0x50 => OpcodeInfo {
            addr_mode: A::Relative,
            operation: O::Bvc,
        },
        0x70 => OpcodeInfo {
            addr_mode: A::Relative,
            operation: O::Bvs,
        },
        0x90 => OpcodeInfo {
            addr_mode: A::Relative,
            operation: O::Bcc,
        },
        0xB0 => OpcodeInfo {
            addr_mode: A::Relative,
            operation: O::Bcs,
        },
        0xD0 => OpcodeInfo {
            addr_mode: A::Relative,
            operation: O::Bne,
        },
        0xF0 => OpcodeInfo {
            addr_mode: A::Relative,
            operation: O::Beq,
        },

        0x48 => OpcodeInfo {
            addr_mode: A::Push,
            operation: O::Pha,
        },
        0x08 => OpcodeInfo {
            addr_mode: A::Push,
            operation: O::Php,
        },
        0x68 => OpcodeInfo {
            addr_mode: A::Pull,
            operation: O::Pla,
        },
        0x28 => OpcodeInfo {
            addr_mode: A::Pull,
            operation: O::Plp,
        },

        0xAA => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Tax,
        },
        0xA8 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Tay,
        },
        0x8A => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Txa,
        },
        0x98 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Tya,
        },
        0xBA => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Tsx,
        },
        0x9A => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Txs,
        },
        0xE8 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Inx,
        },
        0xC8 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Iny,
        },
        0xCA => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Dex,
        },
        0x88 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Dey,
        },
        0x18 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Clc,
        },
        0x38 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Sec,
        },
        0x58 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Cli,
        },
        0x78 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Sei,
        },
        0xB8 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Clv,
        },
        0xD8 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Cld,
        },
        0xF8 => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Sed,
        },
        0xEA => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Nop,
        },

        0x0A => OpcodeInfo {
            addr_mode: A::Accumulator,
            operation: O::AslA,
        },
        0x2A => OpcodeInfo {
            addr_mode: A::Accumulator,
            operation: O::RolA,
        },
        0x4A => OpcodeInfo {
            addr_mode: A::Accumulator,
            operation: O::LsrA,
        },
        0x6A => OpcodeInfo {
            addr_mode: A::Accumulator,
            operation: O::RorA,
        },

        0x09 => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Ora,
        },
        0x05 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Ora,
        },
        0x15 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Ora,
        },
        0x0D => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Ora,
        },
        0x1D => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Ora,
        },
        0x19 => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Ora,
        },
        0x01 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Ora,
        },
        0x11 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Ora,
        },

        0x29 => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::And,
        },
        0x25 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::And,
        },
        0x35 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::And,
        },
        0x2D => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::And,
        },
        0x3D => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::And,
        },
        0x39 => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::And,
        },
        0x21 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::And,
        },
        0x31 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::And,
        },

        0x49 => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Eor,
        },
        0x45 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Eor,
        },
        0x55 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Eor,
        },
        0x4D => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Eor,
        },
        0x5D => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Eor,
        },
        0x59 => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Eor,
        },
        0x41 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Eor,
        },
        0x51 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Eor,
        },

        0x69 => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Adc,
        },
        0x65 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Adc,
        },
        0x75 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Adc,
        },
        0x6D => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Adc,
        },
        0x7D => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Adc,
        },
        0x79 => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Adc,
        },
        0x61 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Adc,
        },
        0x71 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Adc,
        },

        0xE9 | 0xEB => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Sbc,
        },
        0xE5 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Sbc,
        },
        0xF5 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Sbc,
        },
        0xED => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Sbc,
        },
        0xFD => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Sbc,
        },
        0xF9 => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Sbc,
        },
        0xE1 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Sbc,
        },
        0xF1 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Sbc,
        },

        0xC9 => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Cmp,
        },
        0xC5 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Cmp,
        },
        0xD5 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Cmp,
        },
        0xCD => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Cmp,
        },
        0xDD => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Cmp,
        },
        0xD9 => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Cmp,
        },
        0xC1 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Cmp,
        },
        0xD1 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Cmp,
        },

        0xE0 => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Cpx,
        },
        0xE4 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Cpx,
        },
        0xEC => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Cpx,
        },
        0xC0 => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Cpy,
        },
        0xC4 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Cpy,
        },
        0xCC => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Cpy,
        },

        0xA9 => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Lda,
        },
        0xA5 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Lda,
        },
        0xB5 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Lda,
        },
        0xAD => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Lda,
        },
        0xBD => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Lda,
        },
        0xB9 => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Lda,
        },
        0xA1 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Lda,
        },
        0xB1 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Lda,
        },

        0xA2 => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Ldx,
        },
        0xA6 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Ldx,
        },
        0xB6 => OpcodeInfo {
            addr_mode: A::ZeroPageY,
            operation: O::Ldx,
        },
        0xAE => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Ldx,
        },
        0xBE => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Ldx,
        },

        0xA0 => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Ldy,
        },
        0xA4 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Ldy,
        },
        0xB4 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Ldy,
        },
        0xAC => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Ldy,
        },
        0xBC => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Ldy,
        },

        0x85 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Sta,
        },
        0x95 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Sta,
        },
        0x8D => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Sta,
        },
        0x9D => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Sta,
        },
        0x99 => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Sta,
        },
        0x81 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Sta,
        },
        0x91 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Sta,
        },

        0x86 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Stx,
        },
        0x96 => OpcodeInfo {
            addr_mode: A::ZeroPageY,
            operation: O::Stx,
        },
        0x8E => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Stx,
        },
        0x84 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Sty,
        },
        0x94 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Sty,
        },
        0x8C => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Sty,
        },

        0x24 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Bit,
        },
        0x2C => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Bit,
        },

        0x06 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Asl,
        },
        0x16 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Asl,
        },
        0x0E => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Asl,
        },
        0x1E => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Asl,
        },
        0x46 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Lsr,
        },
        0x56 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Lsr,
        },
        0x4E => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Lsr,
        },
        0x5E => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Lsr,
        },
        0x26 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Rol,
        },
        0x36 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Rol,
        },
        0x2E => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Rol,
        },
        0x3E => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Rol,
        },
        0x66 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Ror,
        },
        0x76 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Ror,
        },
        0x6E => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Ror,
        },
        0x7E => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Ror,
        },
        0xE6 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Inc,
        },
        0xF6 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Inc,
        },
        0xEE => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Inc,
        },
        0xFE => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Inc,
        },
        0xC6 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Dec,
        },
        0xD6 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Dec,
        },
        0xCE => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Dec,
        },
        0xDE => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Dec,
        },

        0xA7 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Lax,
        },
        0xB7 => OpcodeInfo {
            addr_mode: A::ZeroPageY,
            operation: O::Lax,
        },
        0xAF => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Lax,
        },
        0xBF => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Lax,
        },
        0xA3 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Lax,
        },
        0xB3 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Lax,
        },

        0x87 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Sax,
        },
        0x97 => OpcodeInfo {
            addr_mode: A::ZeroPageY,
            operation: O::Sax,
        },
        0x8F => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Sax,
        },
        0x83 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Sax,
        },

        0xC7 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Dcp,
        },
        0xD7 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Dcp,
        },
        0xCF => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Dcp,
        },
        0xDF => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Dcp,
        },
        0xDB => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Dcp,
        },
        0xC3 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Dcp,
        },
        0xD3 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Dcp,
        },

        0xE7 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Isc,
        },
        0xF7 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Isc,
        },
        0xEF => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Isc,
        },
        0xFF => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Isc,
        },
        0xFB => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Isc,
        },
        0xE3 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Isc,
        },
        0xF3 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Isc,
        },

        0x07 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Slo,
        },
        0x17 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Slo,
        },
        0x0F => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Slo,
        },
        0x1F => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Slo,
        },
        0x1B => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Slo,
        },
        0x03 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Slo,
        },
        0x13 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Slo,
        },

        0x27 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Rla,
        },
        0x37 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Rla,
        },
        0x2F => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Rla,
        },
        0x3F => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Rla,
        },
        0x3B => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Rla,
        },
        0x23 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Rla,
        },
        0x33 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Rla,
        },

        0x47 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Sre,
        },
        0x57 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Sre,
        },
        0x4F => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Sre,
        },
        0x5F => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Sre,
        },
        0x5B => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Sre,
        },
        0x43 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Sre,
        },
        0x53 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Sre,
        },

        0x67 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::Rra,
        },
        0x77 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::Rra,
        },
        0x6F => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::Rra,
        },
        0x7F => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::Rra,
        },
        0x7B => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::Rra,
        },
        0x63 => OpcodeInfo {
            addr_mode: A::IndirectX,
            operation: O::Rra,
        },
        0x73 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::Rra,
        },

        0x0B | 0x2B => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Anc,
        },
        0x4B => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Alr,
        },
        0x6B => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Arr,
        },
        0xCB => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Axs,
        },

        0x04 | 0x44 | 0x64 => OpcodeInfo {
            addr_mode: A::ZeroPage,
            operation: O::NopRead,
        },
        0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => OpcodeInfo {
            addr_mode: A::ZeroPageX,
            operation: O::NopRead,
        },
        0x0C => OpcodeInfo {
            addr_mode: A::Absolute,
            operation: O::NopRead,
        },
        0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::NopRead,
        },
        0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::NopRead,
        },
        0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => OpcodeInfo {
            addr_mode: A::Implied,
            operation: O::Nop,
        },

        0x9C => OpcodeInfo {
            addr_mode: A::AbsoluteX,
            operation: O::NopRead,
        },
        0x9E => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::NopRead,
        },
        0x9F => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::NopRead,
        },
        0x93 => OpcodeInfo {
            addr_mode: A::IndirectY,
            operation: O::NopRead,
        },
        0x9B => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::NopRead,
        },
        0xBB => OpcodeInfo {
            addr_mode: A::AbsoluteY,
            operation: O::NopRead,
        },
        0xAB => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::Lax,
        },
        0x8B => OpcodeInfo {
            addr_mode: A::Immediate,
            operation: O::NopRead,
        },

        0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 | 0x92 | 0xB2 | 0xD2 | 0xF2 => {
            OpcodeInfo {
                addr_mode: A::Jam,
                operation: O::Jam,
            }
        }
    }
}

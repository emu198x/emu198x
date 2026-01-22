use crate::Z80;

impl Z80 {
    pub(crate) fn read_register(&self, index: u8) -> u8 {
        match index {
            0 => self.b,
            1 => self.c,
            2 => self.d,
            3 => self.e,
            4 => self.h,
            5 => self.l,
            6 => panic!("(HL) not a simple register"),
            7 => self.a,
            _ => unreachable!(),
        }
    }

    pub(crate) fn set_register(&mut self, index: u8, value: u8) {
        match index {
            0 => self.b = value,
            1 => self.c = value,
            2 => self.d = value,
            3 => self.e = value,
            4 => self.h = value,
            5 => self.l = value,
            6 => panic!("(HL) not a simple register"),
            7 => self.a = value,
            _ => unreachable!(),
        }
    }

    pub(crate) fn hl(&self) -> u16 {
        (self.h as u16) << 8 | self.l as u16
    }
}

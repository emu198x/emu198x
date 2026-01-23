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

    pub(crate) fn set_hl(&mut self, value: u16) {
        self.h = (value >> 8) as u8;
        self.l = value as u8;
    }

    pub(crate) fn set_de(&mut self, value: u16) {
        self.d = (value >> 8) as u8;
        self.e = value as u8;
    }

    pub(crate) fn bc(&self) -> u16 {
        (self.b as u16) << 8 | self.c as u16
    }

    pub(crate) fn set_bc(&mut self, value: u16) {
        self.b = (value >> 8) as u8;
        self.c = value as u8;
    }

    /// Get WZ (MEMPTR) - undocumented internal register
    pub fn wz(&self) -> u16 {
        self.wz
    }

    /// Set WZ (MEMPTR) - undocumented internal register
    pub fn set_wz(&mut self, value: u16) {
        self.wz = value;
    }
}

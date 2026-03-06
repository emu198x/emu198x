//! Thin Motorola 68010 wrapper crate.
//!
//! This is a small composition layer over the shared `motorola-68000` core.
//! It pins the configured CPU model to `M68010` while reusing the same core
//! implementation until model-specific behavior is implemented.

use std::ops::{Deref, DerefMut};

pub use motorola_68000::{Cpu68000 as InnerCpu68000, CpuCapabilities, CpuModel};

/// Thin wrapper that constructs the shared 68k core as a 68010 model.
pub struct Cpu68010 {
    inner: InnerCpu68000,
}

impl Cpu68010 {
    /// Create a new 68010 CPU wrapper.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: InnerCpu68000::new_with_model(CpuModel::M68010),
        }
    }

    /// Borrow the wrapped shared CPU core.
    #[must_use]
    pub const fn as_inner(&self) -> &InnerCpu68000 {
        &self.inner
    }

    /// Mutably borrow the wrapped shared CPU core.
    #[must_use]
    pub fn as_inner_mut(&mut self) -> &mut InnerCpu68000 {
        &mut self.inner
    }

    /// Consume the wrapper and return the shared CPU core.
    #[must_use]
    pub fn into_inner(self) -> InnerCpu68000 {
        self.inner
    }
}

impl Default for Cpu68010 {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for Cpu68010 {
    type Target = InnerCpu68000;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Cpu68010 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<Cpu68010> for InnerCpu68000 {
    fn from(cpu: Cpu68010) -> Self {
        cpu.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::{Cpu68010, CpuModel};
    use motorola_68000::bus::{BusStatus, FunctionCode, M68kBus};

    struct SimpleBus {
        mem: Vec<u8>,
    }

    impl SimpleBus {
        fn new(program: &[(u32, u16)]) -> Self {
            let mut mem = vec![0u8; 0x10000];
            for &(addr, word) in program {
                let a = addr as usize;
                mem[a] = (word >> 8) as u8;
                mem[a + 1] = word as u8;
            }
            Self { mem }
        }
    }

    impl M68kBus for SimpleBus {
        fn poll_cycle(
            &mut self,
            addr: u32,
            _fc: FunctionCode,
            is_read: bool,
            is_word: bool,
            data: Option<u16>,
        ) -> BusStatus {
            if is_read {
                if is_word {
                    let a = (addr as usize) & !1;
                    let w = if a + 1 < self.mem.len() {
                        (u16::from(self.mem[a]) << 8) | u16::from(self.mem[a + 1])
                    } else {
                        0
                    };
                    BusStatus::Ready(w)
                } else {
                    let a = addr as usize;
                    let b = if a < self.mem.len() { self.mem[a] } else { 0 };
                    BusStatus::Ready(u16::from(b))
                }
            } else {
                let val = data.unwrap_or(0);
                if is_word {
                    let a = (addr as usize) & !1;
                    if a + 1 < self.mem.len() {
                        self.mem[a] = (val >> 8) as u8;
                        self.mem[a + 1] = val as u8;
                    }
                } else {
                    let a = addr as usize;
                    if a < self.mem.len() {
                        self.mem[a] = val as u8;
                    }
                }
                BusStatus::Ready(0)
            }
        }

        fn poll_ipl(&mut self) -> u8 {
            0
        }

        fn poll_interrupt_ack(&mut self, level: u8) -> BusStatus {
            BusStatus::Ready(24 + u16::from(level))
        }

        fn reset(&mut self) {}
    }

    fn run_until_idle(cpu: &mut Cpu68010, bus: &mut SimpleBus, max_ticks: u32) {
        let mut clock = 0u64;
        for _ in 0..max_ticks {
            clock += 4;
            cpu.tick(bus, clock);
            if cpu.ir == 0x60FE {
                return;
            }
        }
    }

    #[test]
    fn wrapper_sets_68010_model() {
        let cpu = Cpu68010::new();
        assert_eq!(cpu.model(), CpuModel::M68010);
        assert!(cpu.capabilities().movec);
        assert!(cpu.capabilities().vbr);
        assert!(!cpu.capabilities().cacr);
    }

    #[test]
    fn wrapper_executes_movec_vbr_roundtrip() {
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000), (0x0002, 0x1000),
            (0x0004, 0x0000), (0x0006, 0x0100),
            (0x0100, 0x203C), (0x0102, 0x1234), (0x0104, 0x5678),
            (0x0106, 0x4E7B), (0x0108, 0x0801),
            (0x010A, 0x4E7A), (0x010C, 0x1801),
            (0x010E, 0x60FE),
        ]);
        let mut cpu = Cpu68010::new();

        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 5_000);

        assert_eq!(cpu.regs.vbr, 0x1234_5678);
        assert_eq!(cpu.regs.d[1], 0x1234_5678);
    }

    #[test]
    fn wrapper_rejects_cacr_movec_on_68010() {
        let mut bus = SimpleBus::new(&[
            (0x0000, 0x0000), (0x0002, 0x1000),
            (0x0004, 0x0000), (0x0006, 0x0100),
            (0x0010, 0x0000), (0x0012, 0x0200),
            (0x0100, 0x700B),
            (0x0102, 0x4E7B), (0x0104, 0x0002),
            (0x0106, 0x60FE),
            (0x0200, 0x7EFF),
            (0x0202, 0x60FE),
        ]);
        let mut cpu = Cpu68010::new();

        cpu.reset_to(0x0000_1000, 0x0000_0100);
        run_until_idle(&mut cpu, &mut bus, 10_000);

        assert_eq!(cpu.regs.d[7] as u8, 0xFF);
    }
}

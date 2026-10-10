//! Native DMA ownership and descriptor pipeline. Read/address/ownership
//! intervals are qualified separately from physical NMI timing; see the docs
//! repository's `plans/2026-10-10-maria-native-dma-stages.md`.

use serde::{Deserialize, Serialize};

use super::{Maria, MariaRegion, fetch};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub(super) enum Phase {
    #[default]
    Idle,
    AwaitCpu,
    Startup,
    DescriptorFlag,
    DescriptorHigh,
    DescriptorLow,
    DisplayList,
    Shutdown,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Dma {
    pub phase: Phase,
    pub delay: u8,
    pub request_delay: u8,
    pub requested: bool,
    pub sampled_halt: bool,
    pub slow_inhibit: bool,
    pub initial: bool,
    pub next_zone: bool,
}

impl Maria {
    /// Whether memory must drive `dma_data_in` before the next native DMA tick.
    /// The final descriptor read may occur after `dma_drive` falls: the machine
    /// must retain the undriven address until Sally retakes the bus.
    #[must_use]
    pub fn dma_read_pending(&self) -> bool {
        match self.dma.phase {
            Phase::DescriptorFlag | Phase::DescriptorHigh | Phase::DescriptorLow => {
                self.dma.delay == 1
            }
            Phase::DisplayList => self.fetch_read_address().is_some(),
            _ => false,
        }
    }

    /// Advance MARIA's clock, raster requests and DMA by one native oscillator
    /// period, including line RAM transfer and framebuffer playback. The
    /// machine caller is connected by the following integration stage.
    /// Do not additionally call `tick_clock` for this native period.
    pub fn tick_dma(&mut self) {
        self.tick_clock();
        let old_halt = self.halt;
        let mut starting = false;
        if self.phi1 {
            self.dma.slow_inhibit = old_halt;
            if old_halt && !self.dma.sampled_halt && self.dma.phase == Phase::AwaitCpu {
                self.dma.phase = Phase::Startup;
                self.dma.delay = 10;
                self.dma_cycles = 0;
                starting = true;
            }
            self.dma.sampled_halt = old_halt;
        }

        self.native_cycle = (self.native_cycle + 1) % 908;
        let column = self.native_cycle.div_ceil(2) % 454;
        if self.native_cycle == 907 {
            self.scan_line += 1;
            if self.scan_line == self.region.lines_per_frame() {
                self.scan_line = 0;
                self.frame_complete = true;
            }
        }
        let blank_start = match self.region {
            MariaRegion::Ntsc => 258,
            MariaRegion::Pal => 308,
        };
        self.vblank = self.scan_line < 16 || self.scan_line >= blank_start;
        self.tick_video(column);

        if self.dma.request_delay != 0 {
            self.dma.request_delay -= 1;
            if self.dma.request_delay == 0 {
                self.dma.requested = true;
                self.dma.phase = Phase::AwaitCpu;
            }
        }
        if self.native_cycle % 2 == 1
            && self.ctrl & 0x60 == 0x40
            && ((self.scan_line == 16 && column == 1) || (!self.vblank && column == 440))
        {
            self.dma.initial = column == 1;
            self.dma.request_delay = 2;
        }

        if !starting {
            self.advance_dma_stage();
        }
        // HALT's pad is transparent during CPU phase 1, held during phase 2.
        // Its transition and Sally's subsequent sampling are separate edges.
        if !self.clock.phase2 {
            self.halt = self.dma.requested;
        }
    }

    fn advance_dma_stage(&mut self) {
        match self.dma.phase {
            Phase::Idle | Phase::AwaitCpu => return,
            Phase::DisplayList => {
                self.fetch.data_in = self.dma_data_in;
                self.tick_fetch();
                if self.fetch.phase == fetch::Phase::Idle {
                    self.dma.next_zone = self.zone_scanline + 1 >= self.zone_height;
                    self.dma.phase = Phase::Shutdown;
                    self.dma.delay = if self.dma.next_zone { 6 } else { 5 };
                } else {
                    let setup = match self.fetch.phase {
                        fetch::Phase::Direct
                        | fetch::Phase::Indirect
                        | fetch::Phase::IndirectSecond => 5,
                        _ => 3,
                    };
                    if self.fetch.delay == setup {
                        self.dma_address = self.fetch.address;
                    }
                }
                return;
            }
            _ => {}
        }
        self.dma.delay -= 1;
        match self.dma.phase {
            Phase::Startup if self.dma.delay == 3 => self.dma_drive = true,
            Phase::DescriptorFlag | Phase::DescriptorHigh | Phase::DescriptorLow
                if self.dma.delay == 3 =>
            {
                self.dma_address = self.dll_addr;
            }
            Phase::DescriptorLow if self.dma.delay == 1 => {
                self.dma_drive = false;
                self.dma.requested = false;
            }
            Phase::Shutdown if self.dma.delay == if self.dma.next_zone { 5 } else { 4 } => {
                // The next display-list address is presented even though the
                // terminator suppresses its data consumption.
                self.dma_address = self.fetch.dl_addr;
            }
            _ => {}
        }
        if self.dma.delay != 0 {
            return;
        }
        match self.dma.phase {
            Phase::Startup => {
                if self.dma.initial {
                    self.dll_addr = u16::from_be_bytes([self.dpph, self.dppl]);
                    self.dma.phase = Phase::DescriptorFlag;
                    self.dma.delay = 4;
                } else {
                    self.begin_fetch();
                    self.dma.phase = Phase::DisplayList;
                }
            }
            Phase::DescriptorFlag => {
                let flags = self.dma_data_in;
                self.zone_offset = flags & 15;
                self.zone_height = self.zone_offset + 1;
                self.zone_scanline = 0;
                self.zone_holey = (flags >> 5) & 3;
                self.zone_dli = flags & 0x80 != 0;
                self.dll_addr = self.dll_addr.wrapping_add(1);
                self.dma.phase = Phase::DescriptorHigh;
                self.dma.delay = 4;
            }
            Phase::DescriptorHigh => {
                self.zone_dl_addr = (u16::from(self.dma_data_in) << 8) | (self.zone_dl_addr & 0xff);
                self.dll_addr = self.dll_addr.wrapping_add(1);
                self.dma.phase = Phase::DescriptorLow;
                self.dma.delay = 4;
            }
            Phase::DescriptorLow => {
                self.zone_dl_addr = (self.zone_dl_addr & 0xff00) | u16::from(self.dma_data_in);
                self.dll_addr = self.dll_addr.wrapping_add(1);
                self.dll_active = true;
                self.dli_pending |= self.zone_dli;
                self.dma.phase = Phase::Idle;
            }
            Phase::Shutdown => {
                if self.dma.next_zone {
                    self.dma.phase = Phase::DescriptorFlag;
                    self.dma.delay = 4;
                } else {
                    self.zone_scanline += 1;
                    self.dma_drive = false;
                    self.dma.requested = false;
                    self.dma.phase = Phase::Idle;
                }
            }
            _ => unreachable!("only timed DMA stages reach completion"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Bus {
        cpu_address: u16,
        address: u16,
        sampled_halt: bool,
        released: bool,
    }

    impl Bus {
        fn tick(&mut self, chip: &mut Maria, memory: &[u8]) {
            let halt = chip.halt;
            chip.address_in = self.address;
            chip.dma_data_in = memory[usize::from(self.address)];
            chip.tick_dma();
            self.released = self.sampled_halt;
            if chip.phi1 {
                self.sampled_halt = halt;
            }
            assert!(!chip.dma_drive || self.released);
            if chip.dma_drive {
                self.address = chip.dma_address;
            } else if !self.released {
                self.address = self.cpu_address;
            }
        }
    }

    #[test]
    fn direct_snapshots_resume_dma_stages_and_bus_handoffs() {
        let mut phases = BTreeSet::new();
        let mut checked = 0;
        for case in [[1, 2, 1, 1, 1, 0], [4, 2, 2, 2, 0, 1]] {
            let (mut chip, memory) = fixture(case);
            let mut bus = Bus {
                cpu_address: chip.address_in,
                address: chip.address_in,
                sampled_halt: false,
                released: false,
            };
            let mut seen = BTreeSet::new();
            for tick in 257..17500 {
                bus.tick(&mut chip, &memory);
                if tick < 14560 {
                    continue;
                }
                phases.insert(chip.dma.phase as u8);
                let key = (
                    chip.dma.phase as u8,
                    chip.dma.delay,
                    chip.dma.request_delay,
                    chip.fetch.phase as u8,
                    chip.fetch.delay,
                    chip.clock.remaining,
                    chip.clock.phase2,
                    chip.halt,
                    chip.dma_drive,
                    chip.dma.slow_inhibit,
                    chip.dma.initial,
                    chip.dma.next_zone,
                );
                if !seen.insert(key) {
                    continue;
                }
                let saved = chip.save_state();
                let mut restored = Maria::new(chip.region);
                assert_eq!(restored.load_state(&saved).expect("restore"), saved.len());
                // Assert against the live pre-save state as well as subsequent
                // continuation: two equally incomplete loads must not pass.
                assert_eq!(restored.dma, chip.dma);
                assert_eq!(restored.clock, chip.clock);
                assert_eq!(restored.fetch, chip.fetch);
                assert_eq!(restored.native_cycle, chip.native_cycle);
                assert_eq!(
                    (
                        restored.dma_address,
                        restored.dma_data_in,
                        restored.dma_drive,
                        restored.halt
                    ),
                    (
                        chip.dma_address,
                        chip.dma_data_in,
                        chip.dma_drive,
                        chip.halt
                    )
                );
                let mut original_bus = bus;
                let mut restored_bus = bus;
                for _ in 0..80 {
                    original_bus.tick(&mut chip, &memory);
                    restored_bus.tick(&mut restored, &memory);
                    assert_eq!(restored_bus, original_bus, "case {case:?}, saved at {tick}");
                    assert_eq!(restored.dma, chip.dma);
                    assert_eq!(restored.fetch, chip.fetch);
                    assert_eq!(restored.clock, chip.clock);
                    assert_eq!(restored.dma_read_pending(), chip.dma_read_pending());
                }
                assert_eq!(restored.save_state(), chip.save_state());
                // Return the fixture to this edge so the outer loop can test
                // the next pending tick against its live continuation too.
                chip.load_state(&saved).expect("rewind fixture");
                checked += 1;
            }
        }
        assert_eq!(phases, (0..=7).collect());
        assert!(
            checked > 200,
            "insufficient stage/phase coverage: {checked}"
        );
    }

    fn fixture([mode, width, height, flag, slow, pal]: [u8; 6]) -> (Maria, Vec<u8>) {
        let mut chip = Maria::new(if pal == 0 {
            MariaRegion::Ntsc
        } else {
            MariaRegion::Pal
        });
        // Warm fixture at capture tick 256, after register setup. The preceding
        // CPU phase 1 was at 254; raster reset was released at tick 32.
        chip.clock.remaining = 2;
        chip.native_cycle = 224;
        chip.dma_address = 2; // Observed retained address before the first DMA.
        chip.address_in = if slow == 0 { 0x8000 } else { 0x0280 };
        chip.ctrl = if mode == 4 { 0x50 } else { 0x40 };
        chip.dpph = 0x18;
        chip.chbase = 0x80;
        let mut memory = vec![0; 65536];
        for zone in 0..256 {
            memory[0x1800 + zone * 3] = if flag == 1 || (flag == 2 && zone == 1) {
                0x80
            } else {
                0
            } | (height - 1);
            memory[0x1801 + zone * 3] = 0x1c;
        }
        memory[0x1c02] = 0x90;
        if mode == 1 {
            memory[0x1c01] = 32 - width;
        } else {
            memory[0x1c01] = if mode == 2 { 0x40 } else { 0x60 };
            memory[0x1c03] = 32 - width;
        }
        memory[0x8000..0xa000].fill(0x55);
        (chip, memory)
    }

    fn trace(case: [u8; 6], full_reads: bool) -> Vec<String> {
        let (mut chip, memory) = fixture(case);
        let cpu_address = chip.address_in;
        let mut sampled_halt = false;
        let mut released = false;
        let mut address = cpu_address;
        let mut events = Vec::new();
        for tick in 257..if full_reads { 908 * 40 } else { 18000 } {
            let in_window = tick > 14000 && tick < 18000;
            let old_halt = chip.halt;
            let old_drive = chip.dma_drive;
            let old_inhibit = chip.dma.slow_inhibit;
            let old_released = released;
            let old_address = address;
            let read = chip.dma_read_pending();
            chip.address_in = address;
            chip.dma_data_in = memory[usize::from(address)];
            chip.tick_dma();
            if in_window && (chip.phi1 || chip.phi2) {
                events.push(format!(
                    "{tick} P {} {} {} {} {} {old_address:04x}",
                    if chip.phi1 { 1 } else { 2 },
                    u8::from(!old_halt),
                    u8::from(old_released),
                    u8::from(old_drive),
                    u8::from(old_inhibit)
                ));
            }
            if read && (in_window || full_reads) {
                events.push(format!(
                    "{tick} R {old_address:04x} {:02x}",
                    chip.dma_data_in
                ));
            }
            released = sampled_halt;
            if chip.phi1 {
                sampled_halt = old_halt;
            }
            assert!(!chip.dma_drive || released, "bus collision at {tick}");
            if chip.dma_drive {
                address = chip.dma_address;
            } else if !released {
                address = cpu_address;
            }
            if in_window {
                if chip.dma_drive != old_drive
                    || released != old_released
                    || chip.dma.slow_inhibit != old_inhibit
                {
                    events.push(format!(
                        "{tick} B {} {} {} {} {address:04x}",
                        u8::from(!chip.halt),
                        u8::from(released),
                        u8::from(chip.dma_drive),
                        u8::from(chip.dma.slow_inhibit)
                    ));
                }
                if address != old_address {
                    events.push(format!("{tick} A {address:04x}"));
                }
            }
            if chip.halt != old_halt && (in_window || full_reads) {
                events.push(format!("{tick} H {}", u8::from(!chip.halt)));
            }
        }
        events.sort_by_key(|event| {
            let mut fields = event.split_whitespace();
            (
                fields.next().expect("tick").parse::<u32>().expect("tick"),
                fields.next().expect("kind").as_bytes()[0],
            )
        });
        events
    }

    #[test]
    fn descriptors_sample_live_bytes_including_after_address_drive_release() {
        for pal in 0..2 {
            let (mut chip, mut memory) = fixture([1, 2, 1, 0, 1, pal]);
            let mut bus = Bus {
                cpu_address: chip.address_in,
                address: chip.address_in,
                sampled_halt: false,
                released: false,
            };
            let mut reads = 0;
            for _ in 257..15000 {
                if chip.dma_read_pending() {
                    match chip.dma.phase {
                        Phase::DescriptorFlag => memory[0x1800] = 0x81,
                        Phase::DescriptorHigh => {
                            // The first byte is already latched; later memory
                            // changes must not overwrite its DLI/offset bits.
                            memory[0x1800] = 0;
                            memory[0x1801] = 0x28;
                        }
                        Phase::DescriptorLow => {
                            assert!(!chip.dma_drive);
                            assert!(bus.released);
                            assert_eq!(bus.address, 0x1802);
                            memory[0x1802] = 0x42;
                        }
                        _ => panic!("unexpected display-list read before descriptor"),
                    }
                    reads += 1;
                }
                bus.tick(&mut chip, &memory);
                if chip.dll_active {
                    break;
                }
            }
            assert_eq!(reads, 3);
            assert_eq!(chip.zone_dl_addr, 0x2842);
            assert_eq!(chip.zone_height, 2);
            assert_eq!(chip.zone_offset, 1);
            assert!(chip.take_dli());
            assert!(!chip.take_dli());
        }
    }

    #[test]
    fn the_upcoming_descriptor_owns_each_zone_event() {
        for pal in 0..2 {
            for height in [1, 2, 3, 16] {
                for flag in 0..3 {
                    let (mut chip, memory) = fixture([1, 2, height, flag, 0, pal]);
                    let mut bus = Bus {
                        cpu_address: chip.address_in,
                        address: chip.address_in,
                        sampled_halt: false,
                        released: false,
                    };
                    let mut descriptors = 0;
                    let mut events = 0;
                    for _ in 257..908 * 40 {
                        let low = chip.dma.phase == Phase::DescriptorLow && chip.dma_read_pending();
                        bus.tick(&mut chip, &memory);
                        let event = chip.take_dli();
                        if low {
                            let expected = flag == 1 || (flag == 2 && descriptors == 1);
                            assert_eq!(
                                event, expected,
                                "zone {descriptors}, height {height}, flag {flag}, region {pal}"
                            );
                            descriptors += 1;
                        } else {
                            assert!(!event, "event without an upcoming descriptor");
                        }
                        events += usize::from(event);
                    }
                    assert_eq!(descriptors, 1 + 23 / usize::from(height));
                    assert_eq!(
                        events,
                        match flag {
                            0 => 0,
                            1 => descriptors,
                            _ => 1,
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn native_dma_matches_all_768_qualified_schedules() {
        let mut cases = BTreeSet::new();
        for row in include_str!("../tests/data/dma-matrix.txt")
            .lines()
            .filter(|row| !row.starts_with('#'))
        {
            let fields: Vec<_> = row.split_whitespace().collect();
            assert_eq!(fields.len(), 11);
            let case: [u8; 6] = std::array::from_fn(|i| fields[i].parse().expect("case field"));
            assert!(cases.insert(case), "duplicate case");
            let actual = trace(case, true);
            for (index, kind) in ['A', 'B', 'H', 'P', 'R'].into_iter().enumerate() {
                let mut count = 0;
                let mut hash = 0xcbf2_9ce4_8422_2325_u64;
                for event in &actual {
                    if event.split_whitespace().nth(1).expect("kind").as_bytes()[0] != kind as u8 {
                        continue;
                    }
                    count += 1;
                    for byte in event.bytes().chain(*b"\n") {
                        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
                    }
                }
                assert!(count > 0, "empty {kind} stream for {case:?}");
                assert_eq!(
                    format!("{kind}:{count}:{hash:016x}"),
                    fields[6 + index],
                    "{case:?}"
                );
            }
        }
        assert_eq!(cases.len(), 768);
        for mode in 1..=4 {
            for width in [1, 2, 8, if mode == 1 { 31 } else { 32 }] {
                for height in [1, 2, 3, 16] {
                    for flag in 0..3 {
                        for slow in 0..2 {
                            for pal in 0..2 {
                                assert!(cases.contains(&[mode, width, height, flag, slow, pal]));
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn native_dma_matches_qualified_bus_and_read_traces() {
        let mut cases = BTreeSet::new();
        let fixture = include_str!("../tests/data/dma-vectors.txt");
        for block in fixture.split("CASE ").skip(1) {
            let (name, rows) = block.split_once('\n').expect("case header");
            let fields: Vec<u8> = name
                .split_whitespace()
                .map(|field| field.parse().expect("case field"))
                .collect();
            assert_eq!(fields.len(), 3);
            assert!(
                cases.insert((fields[0], fields[1], fields[2])),
                "duplicate case"
            );
            let expected: Vec<_> = rows.lines().collect();
            let actual = trace([1, 2, fields[1], 1, fields[0], fields[2]], false);
            for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(actual, expected, "case {name}, event {index}");
            }
            assert_eq!(
                actual.len(),
                expected.len(),
                "case {name}: missing/extra events"
            );
        }
        assert_eq!(cases.len(), 8);
        for slow in 0..2 {
            for height in 1..=2 {
                for pal in 0..2 {
                    assert!(cases.contains(&(slow, height, pal)));
                }
            }
        }
    }
}

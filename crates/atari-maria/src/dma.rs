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
    pub cutoff_delay: u8,
    pub mode_disabled: bool,
    pub cancel_pending: bool,
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
            Phase::DisplayList => {
                // HPOS uses the direct output strobe; byte latches have an
                // additional MARIA-clock stage and may still consume at +4.
                !(self.dma.cutoff_delay == 2 && self.fetch.phase == fetch::Phase::HeaderPosition)
                    && self.fetch_read_address().is_some()
            }
            _ => false,
        }
    }

    /// Advance MARIA's clock, raster requests and DMA by one native oscillator
    /// period, including line RAM transfer and framebuffer playback. The
    /// machine caller is connected by the following integration stage.
    /// Do not additionally call `tick_clock` for this native period.
    pub fn tick_dma(&mut self) {
        let old_phase2 = self.clock.phase2;
        let old_drive = self.dma_drive;
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
            && ((self.scan_line == 16 && column == 1) || (!self.vblank && column == 440))
        {
            self.dma.initial = column == 1;
            self.dma.request_delay = 2;
        }

        if self.native_cycle.is_multiple_of(2) {
            self.dma.mode_disabled = self.ctrl & 0x60 != 0x40;
        } else {
            // DMA mode is sampled before the idle-state reset and HALT-pad
            // stages. A cancelled request can therefore make a short HALT
            // pulse without starting any memory reads.
            let waiting = matches!(self.dma.phase, Phase::Idle | Phase::AwaitCpu)
                || (self.dma.phase == Phase::Startup && self.dma.delay >= 4);
            let cancel = self.dma.cancel_pending;
            self.dma.cancel_pending = waiting && self.dma.mode_disabled;
            if waiting && cancel {
                self.dma.requested = false;
                self.dma.phase = Phase::Idle;
                self.dma.delay = 0;
            } else if self.dma.phase == Phase::Startup
                && self.dma.delay == 4
                && self.dma.mode_disabled
            {
                self.dma.phase = Phase::Idle;
                self.dma.delay = 0;
            }
        }

        if !starting {
            self.advance_dma_stage();
        }
        if self.dma.cutoff_delay != 0 {
            self.dma.cutoff_delay -= 1;
            if self.dma.cutoff_delay == 0 && self.dma.phase == Phase::DisplayList {
                // At LRC+5 the outstanding address latch has settled. Cancel
                // later consumption, then use the ordinary zone shutdown.
                // Unlike a terminator, cutoff does not select another DL byte.
                self.stop_fetch();
                self.dma.next_zone = self.zone_scanline + 1 >= self.zone_height;
                self.dma.phase = Phase::Shutdown;
                self.dma.delay = if self.dma.next_zone { 5 } else { 4 };
            }
        }
        if self.native_cycle == 824 && self.dma.phase == Phase::DisplayList {
            self.dma.cutoff_delay = 5;
        }
        // HALT's pad is transparent during CPU phase 1, held during phase 2.
        // Its transition and Sally's subsequent sampling are separate edges.
        if !self.clock.phase2 {
            self.halt = self.dma.requested;
        }
        self.tick_registers(old_phase2, old_drive);
    }

    fn advance_dma_stage(&mut self) {
        match self.dma.phase {
            Phase::Idle | Phase::AwaitCpu => return,
            Phase::DisplayList => {
                self.fetch.data_in = self.dma_data_in;
                let hpos = self.fetch.hpos;
                let suppress_position =
                    self.dma.cutoff_delay == 2 && self.fetch.phase == fetch::Phase::HeaderPosition;
                self.tick_fetch();
                if suppress_position {
                    self.fetch.hpos = hpos;
                }
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

    fn cutoff_trace([mode, width, height, slow, pal, objects]: [u8; 6]) -> Vec<String> {
        let (chip, mut memory) = fixture([mode, width, height, 0, slow, pal]);
        let stride = if mode == 1 { 4 } else { 5 };
        for object in 1..usize::from(objects) {
            memory.copy_within(0x1c00..0x1c00 + stride, 0x1c00 + object * stride);
        }
        raster_state_trace(chip, &memory)
    }

    fn raster_state_trace(mut chip: Maria, memory: &[u8]) -> Vec<String> {
        let mut bus = Bus {
            cpu_address: chip.address_in,
            address: chip.address_in,
            sampled_halt: false,
            released: false,
        };
        let mut events = Vec::new();
        for tick in 257..908 * 24 {
            let old_bus = bus;
            let old_halt = chip.halt;
            let old_drive = chip.dma_drive;
            let read = chip.dma_read_pending();
            bus.tick(&mut chip, memory);
            if tick <= 14000 {
                continue;
            }
            if read {
                events.push(format!(
                    "{tick} R {:04x} {:02x}",
                    old_bus.address, chip.dma_data_in
                ));
            }
            if bus.address != old_bus.address {
                events.push(format!("{tick} A {:04x}", bus.address));
            }
            if chip.dma_drive != old_drive || bus.released != old_bus.released {
                events.push(format!(
                    "{tick} B {} {}",
                    u8::from(chip.dma_drive),
                    u8::from(bus.released)
                ));
            }
            if chip.halt != old_halt {
                events.push(format!("{tick} H {}", u8::from(!chip.halt)));
            }
            if tick > 15000 && chip.native_cycle == 860 {
                let input: String = chip
                    .line_buffer
                    .iter()
                    .map(|cell| format!("{cell:02x}"))
                    .collect();
                let output: String = chip
                    .playback_buffer
                    .iter()
                    .map(|cell| format!("{cell:02x}"))
                    .collect();
                events.push(format!("{tick} L {input} {output}"));
                events.push(format!(
                    "{tick} Z {:04x} {:04x} {:01x}",
                    chip.dll_addr,
                    chip.zone_dl_addr,
                    chip.zone_offset.wrapping_sub(chip.zone_scanline) & 15
                ));
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

    fn holey_fixture(
        [mode, width, height, mask, gfx, kangaroo, slow, pal]: [u16; 8],
    ) -> (Maria, Vec<u8>) {
        let (mut chip, _) = fixture([
            mode as u8,
            width as u8,
            height as u8,
            0,
            slow as u8,
            pal as u8,
        ]);
        chip.ctrl |= (kangaroo as u8) << 2;
        chip.chbase = (gfx >> 8) as u8;
        let mut memory = vec![0; 65536];
        for zone in 0..256 {
            memory[0x1800 + zone * 3] = ((height - 1) | (mask << 5)) as u8;
            memory[0x1801 + zone * 3] = 0x1c;
        }
        memory[0x1c00..0x1c04].copy_from_slice(&[0, 0x1e, 0x50, 0]);
        let stride = if mode == 1 { 4 } else { 5 };
        memory[0x1c04] = if mode < 3 { gfx as u8 } else { 0 };
        memory[0x1c05] = if mode == 1 {
            32 - width as u8
        } else if mode == 2 {
            0x40
        } else {
            0x60
        };
        memory[0x1c06] = if mode < 3 { (gfx >> 8) as u8 } else { 0x40 };
        if mode != 1 {
            memory[0x1c07] = 32 - width as u8;
        }
        memory[0x1c04 + stride..0x1c08 + stride].copy_from_slice(&[0, 0x5f, 0x60, 8]);
        memory[0x4000..0x4020].fill(gfx as u8);
        for offset in 0..16 {
            memory[0x5000 + offset * 256] = 0xaa;
            memory[0x5001 + offset * 256] = 0xaa;
            memory[0x6000 + offset * 256] = 0xff;
            for byte in 0..32 {
                memory[(usize::from(gfx) + offset * 256 + byte) & 0xffff] = 0x55;
            }
        }
        (chip, memory)
    }

    #[test]
    fn holey_dma_matches_qualified_bus_and_line_ram_traces() {
        let mut cases = BTreeSet::new();
        for block in include_str!("../tests/data/holey-vectors.txt")
            .split("CASE ")
            .skip(1)
        {
            let (name, rows) = block.split_once('\n').expect("case header");
            let fields: Vec<u16> = name
                .split_whitespace()
                .map(|field| field.parse().expect("case field"))
                .collect();
            let case: [u16; 8] = fields.try_into().expect("eight fields");
            assert!(cases.insert(case));
            let (chip, memory) = holey_fixture(case);
            let actual = raster_state_trace(chip, &memory);
            let expected: Vec<_> = rows.lines().collect();
            for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(actual, expected, "case {name}, event {index}");
            }
            assert_eq!(actual.len(), expected.len(), "case {name}");
        }
        assert_eq!(cases.len(), 4);
    }

    #[test]
    fn holey_snapshots_keep_suppression_through_indirect_address_wrap() {
        let mut stages = BTreeSet::new();
        let mut wrapped = 0;
        for mode in 1..=4 {
            let (mut chip, memory) = holey_fixture([mode, 8, 1, 3, 0xffff, 1, 0, 0]);
            let mut bus = Bus {
                cpu_address: chip.address_in,
                address: chip.address_in,
                sampled_halt: false,
                released: false,
            };
            let mut seen = BTreeSet::new();
            for _ in 257..15620 {
                bus.tick(&mut chip, &memory);
                if !chip.fetch.holey || !seen.insert((chip.fetch.phase as u8, chip.fetch.delay)) {
                    continue;
                }
                stages.insert(chip.fetch.phase as u8);
                wrapped += usize::from(chip.fetch.address == 0);
                let saved = chip.save_state();
                let mut restored = Maria::new(chip.region);
                assert_eq!(restored.load_state(&saved).expect("restore"), saved.len());
                assert_eq!(restored.save_state(), saved);
                let mut original_bus = bus;
                let mut resumed_bus = bus;
                for _ in 0..256 {
                    original_bus.tick(&mut chip, &memory);
                    resumed_bus.tick(&mut restored, &memory);
                    assert_eq!(original_bus, resumed_bus);
                    assert_eq!(chip.dma_read_pending(), restored.dma_read_pending());
                }
                assert_eq!(restored.save_state(), chip.save_state());
                chip.load_state(&saved).expect("rewind fixture");
            }
            assert!(
                seen.len() >= 6,
                "missing holey delay stages for mode {mode}"
            );
        }
        assert_eq!(
            stages,
            [
                fetch::Phase::Direct as u8,
                fetch::Phase::Indirect as u8,
                fetch::Phase::IndirectSecond as u8
            ]
            .into_iter()
            .collect()
        );
        assert!(
            wrapped >= 6,
            "missing suppressed wrapped second-byte stages"
        );
    }

    #[test]
    fn holey_dma_matches_all_696_qualified_schedules() {
        let mut cases = BTreeSet::new();
        for row in include_str!("../tests/data/holey-matrix.txt")
            .lines()
            .filter(|row| !row.starts_with('#'))
        {
            let fields: Vec<_> = row.split_whitespace().collect();
            assert_eq!(fields.len(), 14);
            let case: [u16; 8] = std::array::from_fn(|i| fields[i].parse().expect("case field"));
            assert!(cases.insert(case));
            let (chip, memory) = holey_fixture(case);
            let actual = raster_state_trace(chip, &memory);
            for (index, kind) in ['A', 'B', 'H', 'L', 'R', 'Z'].into_iter().enumerate() {
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
                assert!(count > 0);
                assert_eq!(
                    format!("{kind}:{count}:{hash:016x}"),
                    fields[8 + index],
                    "{case:?}"
                );
            }
        }
        let mut expected = BTreeSet::new();
        for mode in 1..=4 {
            for mask in 0..4 {
                for gfx in [0x7fff, 0x8000, 0x87ff, 0x8800, 0x8fff, 0x9000, 0xffff] {
                    for width in [2, 8] {
                        for kangaroo in 0..2 {
                            expected.insert([mode, width, 1, mask, gfx, kangaroo, 0, 0]);
                        }
                    }
                }
            }
            for width in [1, 2, 8, if mode == 1 { 31 } else { 32 }] {
                for height in [1, 2, 8, 16] {
                    for slow in 0..2 {
                        for pal in 0..2 {
                            expected.insert([mode, width, height, 2, 0x9000, 0, slow, pal]);
                        }
                    }
                }
            }
        }
        assert_eq!(cases.len(), 696);
        assert_eq!(cases, expected);
    }

    #[test]
    fn cutoff_matches_qualified_bus_and_read_traces() {
        let mut cases = BTreeSet::new();
        for block in include_str!("../tests/data/cutoff-vectors.txt")
            .split("CASE ")
            .skip(1)
        {
            let (name, rows) = block.split_once('\n').expect("case header");
            let fields: Vec<u8> = name
                .split_whitespace()
                .map(|field| field.parse().expect("case field"))
                .collect();
            let case: [u8; 6] = fields.try_into().expect("six case fields");
            assert!(cases.insert(case));
            let actual = cutoff_trace(case);
            let expected: Vec<_> = rows.lines().collect();
            for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(actual, expected, "case {name}, event {index}");
            }
            assert_eq!(actual.len(), expected.len(), "case {name}");
        }
        assert_eq!(cases.len(), 6);
    }

    #[test]
    fn cutoff_snapshots_preserve_late_reads_and_both_line_banks() {
        let mut countdowns = BTreeSet::new();
        let mut checked = 0;
        for case in [[1, 2, 1, 0, 0, 0], [4, 8, 2, 0, 1, 1]] {
            let (mut chip, mut memory) = fixture(case);
            let stride = if case[0] == 1 { 4 } else { 5 };
            for object in 1..128 {
                memory.copy_within(0x1c00..0x1c00 + stride, 0x1c00 + object * stride);
            }
            let mut bus = Bus {
                cpu_address: chip.address_in,
                address: chip.address_in,
                sampled_halt: false,
                released: false,
            };
            for tick in 257..16320 {
                bus.tick(&mut chip, &memory);
                if tick < 16290 {
                    continue;
                }
                countdowns.insert(chip.dma.cutoff_delay);
                let saved = chip.save_state();
                let mut restored = Maria::new(chip.region);
                assert_eq!(restored.load_state(&saved).expect("restore"), saved.len());
                assert_eq!(restored.save_state(), saved, "saved at {tick}");
                let mut resumed_bus = bus;
                let mut original_bus = bus;
                for _ in 0..1024 {
                    original_bus.tick(&mut chip, &memory);
                    resumed_bus.tick(&mut restored, &memory);
                    assert_eq!(original_bus, resumed_bus);
                    assert_eq!(chip.dma_read_pending(), restored.dma_read_pending());
                }
                assert_eq!(restored.save_state(), chip.save_state(), "saved at {tick}");
                chip.load_state(&saved).expect("rewind fixture");
                checked += 1;
            }
        }
        assert_eq!(countdowns, (0..=5).collect());
        assert_eq!(checked, 60);
    }

    #[test]
    fn cutoff_matches_all_896_qualified_schedules() {
        let mut cases = BTreeSet::new();
        for row in include_str!("../tests/data/cutoff-matrix.txt")
            .lines()
            .filter(|row| !row.starts_with('#'))
        {
            let fields: Vec<_> = row.split_whitespace().collect();
            assert_eq!(fields.len(), 12);
            let case: [u8; 6] = std::array::from_fn(|i| fields[i].parse().expect("case field"));
            assert!(cases.insert(case));
            let actual = cutoff_trace(case);
            for (index, kind) in ['A', 'B', 'H', 'L', 'R', 'Z'].into_iter().enumerate() {
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
                assert!(count > 0);
                assert_eq!(
                    format!("{kind}:{count}:{hash:016x}"),
                    fields[6 + index],
                    "{case:?}"
                );
            }
        }
        assert_eq!(cases.len(), 896);
        for mode in 1..=4 {
            for width in [1, 2, 8, if mode == 1 { 31 } else { 32 }] {
                for height in [1, 2, 3, 16] {
                    for slow in 0..2 {
                        for pal in 0..2 {
                            assert!(cases.contains(&[mode, width, height, slow, pal, 128]));
                            if height <= 2 {
                                let cost = u16::from(if mode == 1 { 16_u8 } else { 20 })
                                    + u16::from(width)
                                        * if mode < 3 {
                                            6
                                        } else if mode == 3 {
                                            12
                                        } else {
                                            18
                                        };
                                let edge = (836 / cost).max(3) as u8;
                                for objects in edge - 2..=edge + 2 {
                                    assert!(
                                        cases.contains(&[mode, width, height, slow, pal, objects])
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn cpu_control_write_reaches_dma_after_the_write_cycle_ends() {
        let (mut chip, memory) = fixture([1, 2, 1, 0, 0, 0]);
        let mut bus = Bus {
            cpu_address: chip.address_in,
            address: chip.address_in,
            sampled_halt: false,
            released: false,
        };
        for tick in 257..15433 {
            bus.tick(&mut chip, &memory);
            if tick == 15422 {
                assert!(chip.phi1);
                chip.write_in = true;
                chip.write_data_in = 0x60;
                bus.cpu_address = 0x003c;
                bus.address = 0x003c;
            }
            if tick == 15430 {
                assert!(chip.phi1);
                chip.write_in = false;
                bus.cpu_address = 0x8000;
                bus.address = 0x8000;
            }
            assert_eq!(
                chip.ctrl,
                if tick < 15432 { 0x40 } else { 0x60 },
                "tick {tick}"
            );
        }
    }

    fn restart_trace(case: [u16; 6]) -> Vec<String> {
        restart_trace_with_snapshots(case, false)
    }

    fn restart_trace_with_snapshots(
        [target, scenario, height, slow, pal, objects]: [u16; 6],
        verify_saves: bool,
    ) -> Vec<String> {
        let (mut chip, mut memory) = fixture([1, 2, height as u8, 0, slow as u8, pal as u8]);
        chip.ctrl = if scenario == 1 { 0x60 } else { 0x40 };
        // This is an observed warm fixture, not a claim about power-on state.
        // The reference's reset/setup sequence has already advanced ZONE_PTR.
        chip.dll_addr = 3;
        for object in 1..usize::from(objects) {
            memory.copy_within(0x1c00..0x1c04, 0x1c00 + object * 4);
        }
        let idle_address = chip.address_in;
        let mut bus = Bus {
            cpu_address: idle_address,
            address: idle_address,
            sampled_halt: false,
            released: false,
        };
        let mut next_write = 0;
        let mut shadow = chip.ctrl;
        let mut events = Vec::new();
        let mut saved_edges = BTreeSet::new();
        let mut pending_saves = 0;
        for tick in 257..908 * 24 {
            let old_bus = bus;
            let old_halt = chip.halt;
            let old_drive = chip.dma_drive;
            let old_ctrl = chip.ctrl;
            let read = chip.dma_read_pending();
            bus.tick(&mut chip, &memory);
            if tick > 14000 && read {
                events.push(format!(
                    "{tick} R {:04x} {:02x}",
                    old_bus.address, chip.dma_data_in
                ));
            }
            if chip.ctrl != old_ctrl {
                events.push(format!("{tick} C {:02x}", chip.ctrl));
            }
            if let Some(value) = chip.control.pending_ctrl
                && value != shadow
            {
                shadow = value;
                events.push(format!("{tick} S {shadow:02x}"));
            }
            if chip.phi1 && !old_halt && !old_drive {
                chip.write_in = false;
                bus.cpu_address = idle_address;
                if (next_write == 0 && tick >= target)
                    || (scenario == 0 && next_write == 1 && tick >= target + 96)
                {
                    bus.cpu_address = 0x003c;
                    chip.write_in = true;
                    chip.write_data_in = if scenario == 1 || next_write == 1 {
                        0x40
                    } else {
                        0x60
                    };
                    next_write += 1;
                    events.push(format!("{tick} W 003c {:02x}", chip.write_data_in));
                }
                if !bus.released {
                    bus.address = bus.cpu_address;
                }
            }
            if verify_saves && tick >= target - 8 && tick < target + 1000 {
                let key = (
                    chip.control.write_strobe,
                    chip.control.ctrl_selected,
                    chip.control.pending_ctrl,
                    chip.dma.mode_disabled,
                    chip.dma.cancel_pending,
                    chip.dma.phase as u8,
                    chip.clock.remaining,
                    chip.clock.phase2,
                    chip.halt,
                    chip.dma_drive,
                );
                if saved_edges.insert(key) {
                    pending_saves += usize::from(chip.control.pending_ctrl.is_some());
                    let saved = chip.save_state();
                    let mut restored = Maria::new(chip.region);
                    assert_eq!(restored.load_state(&saved).expect("restore"), saved.len());
                    assert_eq!(restored.save_state(), saved, "saved at {tick}");
                    let mut original_bus = bus;
                    let mut resumed_bus = bus;
                    for _ in 0..256 {
                        for (chip, bus) in [
                            (&mut chip, &mut original_bus),
                            (&mut restored, &mut resumed_bus),
                        ] {
                            let eligible = !chip.halt && !chip.dma_drive;
                            bus.tick(chip, &memory);
                            if chip.phi1 && eligible {
                                chip.write_in = false;
                                bus.cpu_address = idle_address;
                                if !bus.released {
                                    bus.address = idle_address;
                                }
                            }
                        }
                        assert_eq!(original_bus, resumed_bus, "saved at {tick}");
                        assert_eq!(chip.dma_read_pending(), restored.dma_read_pending());
                    }
                    assert_eq!(restored.save_state(), chip.save_state(), "saved at {tick}");
                    chip.load_state(&saved).expect("rewind fixture");
                }
            }
            if tick <= 14000 {
                continue;
            }
            if bus.address != old_bus.address {
                events.push(format!("{tick} A {:04x}", bus.address));
            }
            if chip.dma_drive != old_drive || bus.released != old_bus.released {
                events.push(format!(
                    "{tick} B {} {}",
                    u8::from(chip.dma_drive),
                    u8::from(bus.released)
                ));
            }
            if chip.halt != old_halt {
                events.push(format!("{tick} H {}", u8::from(!chip.halt)));
            }
            if tick > 15000 && chip.native_cycle == 860 {
                let input: String = chip
                    .line_buffer
                    .iter()
                    .map(|cell| format!("{cell:02x}"))
                    .collect();
                let output: String = chip
                    .playback_buffer
                    .iter()
                    .map(|cell| format!("{cell:02x}"))
                    .collect();
                events.push(format!("{tick} L {input} {output}"));
                events.push(format!(
                    "{tick} Z {:04x} {:04x} {:01x}",
                    chip.dll_addr,
                    chip.zone_dl_addr,
                    chip.zone_offset.wrapping_sub(chip.zone_scanline) & 15
                ));
            }
        }
        assert_eq!(next_write, if scenario == 0 { 2 } else { 1 });
        if verify_saves {
            assert!(saved_edges.len() > 20, "insufficient saved stages");
            assert!(pending_saves > 2, "missing pending CTRL coverage");
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
    fn restart_matches_qualified_bus_and_register_traces() {
        let mut cases = BTreeSet::new();
        for block in include_str!("../tests/data/restart-vectors.txt")
            .split("CASE ")
            .skip(1)
        {
            let (name, rows) = block.split_once('\n').expect("case header");
            let fields: Vec<u16> = name
                .split_whitespace()
                .map(|field| field.parse().expect("case field"))
                .collect();
            let case: [u16; 6] = fields.try_into().expect("six case fields");
            assert!(cases.insert(case));
            let actual = restart_trace_with_snapshots(case, true);
            let expected: Vec<_> = rows.lines().collect();
            for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(actual, expected, "case {name}, event {index}");
            }
            assert_eq!(actual.len(), expected.len(), "case {name}");
        }
        assert_eq!(cases.len(), 4);
    }

    #[test]
    fn restart_matches_all_576_qualified_schedules() {
        let mut cases = BTreeSet::new();
        for row in include_str!("../tests/data/restart-matrix.txt")
            .lines()
            .filter(|row| !row.starts_with('#'))
        {
            let fields: Vec<_> = row.split_whitespace().collect();
            assert_eq!(fields.len(), 15);
            let case: [u16; 6] = std::array::from_fn(|i| fields[i].parse().expect("case field"));
            assert!(cases.insert(case));
            let actual = restart_trace(case);
            for (index, kind) in ['A', 'B', 'C', 'H', 'L', 'R', 'S', 'W', 'Z']
                .into_iter()
                .enumerate()
            {
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
                // Consecutive CTRL writes can replace the holding value before
                // either reaches the output; zero commit edges are meaningful.
                assert!(count > 0 || kind == 'C');
                assert_eq!(
                    format!("{kind}:{count}:{hash:016x}"),
                    fields[6 + index],
                    "{case:?}"
                );
            }
        }
        assert_eq!(cases.len(), 576);
        for edge in [14561_i32, 15439] {
            for offset in [-24, -16, -8, -4, 0, 4, 8, 16, 24] {
                for scenario in 0..2 {
                    for height in 1..=2 {
                        for slow in 0..2 {
                            for pal in 0..2 {
                                for objects in [1, 128] {
                                    assert!(cases.contains(&[
                                        (edge + offset) as u16,
                                        scenario,
                                        height,
                                        slow,
                                        pal,
                                        objects
                                    ]));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn cpu_register_commits_match_all_mirrors_and_clock_phases() {
        fn value(chip: &Maria, register: u8) -> u8 {
            match register {
                0 => chip.backgrnd,
                0x0c => chip.dpph,
                0x10 => chip.dppl,
                0x14 => chip.chbase,
                0x1c => chip.ctrl,
                _ => Maria::palette_index(register).map_or(0, |(palette, colour)| {
                    chip.palettes[usize::from(palette)][usize::from(colour)]
                }),
            }
        }
        let mut cases = BTreeSet::new();
        for block in include_str!("../tests/data/register-vectors.txt")
            .split("CASE ")
            .skip(1)
        {
            let (name, rows) = block.split_once('\n').expect("case header");
            let fields: Vec<u16> = name
                .split_whitespace()
                .map(|field| field.parse().expect("case field"))
                .collect();
            let [target, mirror, slow, pal]: [u16; 4] = fields.try_into().expect("four fields");
            assert!(cases.insert((target, mirror, slow, pal)));
            let (mut chip, memory) = fixture([1, 2, 1, 0, slow as u8, pal as u8]);
            chip.ctrl = 0x60;
            let idle_address = chip.address_in;
            let mut bus = Bus {
                cpu_address: idle_address,
                address: idle_address,
                sampled_halt: false,
                released: false,
            };
            let mut observed: [u8; 32] =
                std::array::from_fn(|register| value(&chip, register as u8));
            let mut index = 0_u8;
            let mut idle_cycle = false;
            let mut actual = Vec::new();
            for tick in 257..13000 {
                bus.tick(&mut chip, &memory);
                assert!(!chip.halt && !chip.dma_drive);
                for register in 0..32_u8 {
                    let current = value(&chip, register);
                    let previous = &mut observed[usize::from(register)];
                    if current != *previous {
                        actual.push(format!("{tick} G {register:02x} {current:02x}"));
                        *previous = current;
                    }
                }
                if chip.phi1 {
                    chip.write_in = false;
                    bus.cpu_address = idle_address;
                    if tick >= target && index < 32 {
                        if idle_cycle {
                            idle_cycle = false;
                        } else {
                            bus.cpu_address = 0x20 + u16::from(index) + mirror;
                            chip.write_in = true;
                            chip.write_data_in = if index == 28 { 0x40 } else { 0x40 + 3 * index };
                            actual.push(format!(
                                "{tick} W {:04x} {:02x}",
                                bus.cpu_address, chip.write_data_in
                            ));
                            index += 1;
                            if matches!(index, 4 | 8 | 24) {
                                index += 1;
                            }
                            idle_cycle = true;
                        }
                    }
                    bus.address = bus.cpu_address;
                }
            }
            let expected: Vec<_> = rows.lines().collect();
            assert_eq!(actual.len(), 58, "case {name}");
            for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(actual, expected, "case {name}, event {index}");
            }
            assert_eq!(actual.len(), expected.len());
        }
        assert_eq!(cases.len(), 64);
        for target in (11000..11008).step_by(2) {
            for mirror in (0..1024).step_by(256) {
                for slow in 0..2 {
                    for pal in 0..2 {
                        assert!(cases.contains(&(target, mirror, slow, pal)));
                    }
                }
            }
        }
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

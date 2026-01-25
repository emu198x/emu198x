//! Save state (snapshot) support for C64 emulator.
//!
//! Provides complete machine state capture for:
//! - Save/load functionality
//! - Debugging and state inspection
//! - Regression testing
//! - Reproducible bug reports

use crate::memory::{Cia, Memory};
use crate::sid::Sid;
use crate::vic::Vic;
use cpu_6502::Mos6502;

/// Magic bytes for snapshot file identification.
const SNAPSHOT_MAGIC: &[u8; 4] = b"C64S";

/// Current snapshot format version.
const SNAPSHOT_VERSION: u8 = 1;

/// Complete machine state snapshot.
#[derive(Clone)]
pub struct Snapshot {
    /// CPU state
    pub cpu: CpuState,
    /// Memory state (RAM, registers, I/O)
    pub memory: MemoryState,
    /// VIC-II state
    pub vic: VicState,
    /// SID state
    pub sid: SidState,
    /// CIA1 state
    pub cia1: CiaState,
    /// CIA2 state
    pub cia2: CiaState,
    /// Frame cycle counter
    pub frame_cycles: u32,
}

/// CPU register state.
#[derive(Clone, Debug)]
pub struct CpuState {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub pc: u16,
    pub status: u8,
}

/// Memory state.
#[derive(Clone)]
pub struct MemoryState {
    /// 64KB RAM
    pub ram: Box<[u8; 65536]>,
    /// Processor port DDR ($00)
    pub port_ddr: u8,
    /// Processor port data ($01)
    pub port_data: u8,
    /// VIC-II registers
    pub vic_registers: [u8; 64],
    /// SID registers
    pub sid_registers: [u8; 32],
    /// Color RAM
    pub color_ram: [u8; 1024],
    /// Keyboard matrix state
    pub keyboard_matrix: [u8; 8],
    /// Current raster line
    pub current_raster_line: u16,
}

/// VIC-II state.
#[derive(Clone, Debug)]
pub struct VicState {
    pub raster_line: u16,
    pub frame_cycle: u32,
    pub ba_low: bool,
    pub sprite_dma_active: u8,
    pub sprite_display_count: [u8; 8],
}

/// SID state (simplified - captures oscillator phases).
#[derive(Clone, Debug)]
pub struct SidState {
    /// Voice states (frequency, phase, ADSR, etc.)
    pub voices: [VoiceState; 3],
    /// Filter state
    pub filter_cutoff: u16,
    pub filter_resonance: u8,
    pub filter_mode: u8,
    pub volume: u8,
}

/// SID voice state.
#[derive(Clone, Debug, Default)]
pub struct VoiceState {
    pub frequency: u16,
    pub pulse_width: u16,
    pub control: u8,
    pub attack_decay: u8,
    pub sustain_release: u8,
    pub phase: u32,
    pub envelope: u8,
    pub envelope_state: u8,
    pub envelope_counter: u32,
}

/// CIA chip state.
#[derive(Clone, Debug)]
pub struct CiaState {
    pub pra: u8,
    pub prb: u8,
    pub ddra: u8,
    pub ddrb: u8,
    pub ta_lo: u8,
    pub ta_hi: u8,
    pub ta_latch_lo: u8,
    pub ta_latch_hi: u8,
    pub tb_lo: u8,
    pub tb_hi: u8,
    pub tb_latch_lo: u8,
    pub tb_latch_hi: u8,
    pub cra: u8,
    pub crb: u8,
    pub icr: u8,
    pub icr_mask: u8,
    pub tod_10ths: u8,
    pub tod_sec: u8,
    pub tod_min: u8,
    pub tod_hr: u8,
    pub alarm_10ths: u8,
    pub alarm_sec: u8,
    pub alarm_min: u8,
    pub alarm_hr: u8,
    pub tod_running: bool,
    pub tod_latched: bool,
}

impl Snapshot {
    /// Create a snapshot from current machine state.
    pub fn capture(
        cpu: &Mos6502,
        memory: &Memory,
        vic: &Vic,
        sid: &Sid,
        frame_cycles: u32,
    ) -> Self {
        Self {
            cpu: CpuState::from_cpu(cpu),
            memory: MemoryState::from_memory(memory),
            vic: VicState::from_vic(vic),
            sid: SidState::from_sid(sid),
            cia1: CiaState::from_cia(&memory.cia1),
            cia2: CiaState::from_cia(&memory.cia2),
            frame_cycles,
        }
    }

    /// Serialize snapshot to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(70000);

        // Header
        data.extend_from_slice(SNAPSHOT_MAGIC);
        data.push(SNAPSHOT_VERSION);

        // CPU state (7 bytes)
        data.push(self.cpu.a);
        data.push(self.cpu.x);
        data.push(self.cpu.y);
        data.push(self.cpu.sp);
        data.extend_from_slice(&self.cpu.pc.to_le_bytes());
        data.push(self.cpu.status);

        // Memory (64KB + registers)
        data.extend_from_slice(self.memory.ram.as_ref());
        data.push(self.memory.port_ddr);
        data.push(self.memory.port_data);
        data.extend_from_slice(&self.memory.vic_registers);
        data.extend_from_slice(&self.memory.sid_registers);
        data.extend_from_slice(&self.memory.color_ram);
        data.extend_from_slice(&self.memory.keyboard_matrix);
        data.extend_from_slice(&self.memory.current_raster_line.to_le_bytes());

        // VIC state
        data.extend_from_slice(&self.vic.raster_line.to_le_bytes());
        data.extend_from_slice(&self.vic.frame_cycle.to_le_bytes());
        data.push(self.vic.ba_low as u8);
        data.push(self.vic.sprite_dma_active);
        data.extend_from_slice(&self.vic.sprite_display_count);

        // SID state
        for voice in &self.sid.voices {
            data.extend_from_slice(&voice.frequency.to_le_bytes());
            data.extend_from_slice(&voice.pulse_width.to_le_bytes());
            data.push(voice.control);
            data.push(voice.attack_decay);
            data.push(voice.sustain_release);
            data.extend_from_slice(&voice.phase.to_le_bytes());
            data.push(voice.envelope);
            data.push(voice.envelope_state);
            data.extend_from_slice(&voice.envelope_counter.to_le_bytes());
        }
        data.extend_from_slice(&self.sid.filter_cutoff.to_le_bytes());
        data.push(self.sid.filter_resonance);
        data.push(self.sid.filter_mode);
        data.push(self.sid.volume);

        // CIA1 state
        self.cia1.write_to(&mut data);

        // CIA2 state
        self.cia2.write_to(&mut data);

        // Frame cycles
        data.extend_from_slice(&self.frame_cycles.to_le_bytes());

        data
    }

    /// Deserialize snapshot from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 5 {
            return Err("Snapshot too small");
        }

        if &data[0..4] != SNAPSHOT_MAGIC {
            return Err("Invalid snapshot magic");
        }

        if data[4] != SNAPSHOT_VERSION {
            return Err("Unsupported snapshot version");
        }

        let mut pos = 5;

        // CPU state
        let cpu = CpuState {
            a: data[pos],
            x: data[pos + 1],
            y: data[pos + 2],
            sp: data[pos + 3],
            pc: u16::from_le_bytes([data[pos + 4], data[pos + 5]]),
            status: data[pos + 6],
        };
        pos += 7;

        // Memory
        if data.len() < pos + 65536 + 2 + 64 + 32 + 1024 + 8 + 2 {
            return Err("Snapshot truncated (memory)");
        }

        let mut ram = Box::new([0u8; 65536]);
        ram.copy_from_slice(&data[pos..pos + 65536]);
        pos += 65536;

        let port_ddr = data[pos];
        let port_data = data[pos + 1];
        pos += 2;

        let mut vic_registers = [0u8; 64];
        vic_registers.copy_from_slice(&data[pos..pos + 64]);
        pos += 64;

        let mut sid_registers = [0u8; 32];
        sid_registers.copy_from_slice(&data[pos..pos + 32]);
        pos += 32;

        let mut color_ram = [0u8; 1024];
        color_ram.copy_from_slice(&data[pos..pos + 1024]);
        pos += 1024;

        let mut keyboard_matrix = [0u8; 8];
        keyboard_matrix.copy_from_slice(&data[pos..pos + 8]);
        pos += 8;

        let current_raster_line = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;

        let memory = MemoryState {
            ram,
            port_ddr,
            port_data,
            vic_registers,
            sid_registers,
            color_ram,
            keyboard_matrix,
            current_raster_line,
        };

        // VIC state
        let vic = VicState {
            raster_line: u16::from_le_bytes([data[pos], data[pos + 1]]),
            frame_cycle: u32::from_le_bytes([data[pos + 2], data[pos + 3], data[pos + 4], data[pos + 5]]),
            ba_low: data[pos + 6] != 0,
            sprite_dma_active: data[pos + 7],
            sprite_display_count: data[pos + 8..pos + 16].try_into().unwrap(),
        };
        pos += 16;

        // SID state
        let mut voices = [VoiceState::default(), VoiceState::default(), VoiceState::default()];
        for voice in &mut voices {
            voice.frequency = u16::from_le_bytes([data[pos], data[pos + 1]]);
            voice.pulse_width = u16::from_le_bytes([data[pos + 2], data[pos + 3]]);
            voice.control = data[pos + 4];
            voice.attack_decay = data[pos + 5];
            voice.sustain_release = data[pos + 6];
            voice.phase = u32::from_le_bytes([data[pos + 7], data[pos + 8], data[pos + 9], data[pos + 10]]);
            voice.envelope = data[pos + 11];
            voice.envelope_state = data[pos + 12];
            voice.envelope_counter = u32::from_le_bytes([data[pos + 13], data[pos + 14], data[pos + 15], data[pos + 16]]);
            pos += 17;
        }

        let sid = SidState {
            voices,
            filter_cutoff: u16::from_le_bytes([data[pos], data[pos + 1]]),
            filter_resonance: data[pos + 2],
            filter_mode: data[pos + 3],
            volume: data[pos + 4],
        };
        pos += 5;

        // CIA states
        let cia1 = CiaState::read_from(&data[pos..])?;
        pos += CiaState::SIZE;

        let cia2 = CiaState::read_from(&data[pos..])?;
        pos += CiaState::SIZE;

        // Frame cycles
        let frame_cycles = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);

        Ok(Self {
            cpu,
            memory,
            vic,
            sid,
            cia1,
            cia2,
            frame_cycles,
        })
    }

    /// Dump CPU state for debugging.
    pub fn dump_cpu(&self) -> String {
        format!(
            "A={:02X} X={:02X} Y={:02X} SP={:02X} PC={:04X} P={:02X} [{}{}{}{}{}{}]",
            self.cpu.a,
            self.cpu.x,
            self.cpu.y,
            self.cpu.sp,
            self.cpu.pc,
            self.cpu.status,
            if self.cpu.status & 0x80 != 0 { 'N' } else { '-' },
            if self.cpu.status & 0x40 != 0 { 'V' } else { '-' },
            if self.cpu.status & 0x08 != 0 { 'D' } else { '-' },
            if self.cpu.status & 0x04 != 0 { 'I' } else { '-' },
            if self.cpu.status & 0x02 != 0 { 'Z' } else { '-' },
            if self.cpu.status & 0x01 != 0 { 'C' } else { '-' },
        )
    }

    /// Dump VIC state for debugging.
    pub fn dump_vic(&self) -> String {
        format!(
            "Raster={:03} Cycle={:05} BA={} Sprites={:02X}",
            self.vic.raster_line,
            self.vic.frame_cycle,
            if self.vic.ba_low { "LOW" } else { "HI " },
            self.vic.sprite_dma_active,
        )
    }

    /// Read memory from snapshot.
    pub fn peek(&self, addr: u16) -> u8 {
        self.memory.ram[addr as usize]
    }

    /// Read a range of memory.
    pub fn peek_range(&self, start: u16, len: u16) -> &[u8] {
        let start = start as usize;
        let end = (start + len as usize).min(65536);
        &self.memory.ram[start..end]
    }
}

impl CpuState {
    fn from_cpu(cpu: &Mos6502) -> Self {
        Self {
            a: cpu.a(),
            x: cpu.x(),
            y: cpu.y(),
            sp: cpu.sp(),
            pc: cpu.pc(),
            status: cpu.status(),
        }
    }
}

impl MemoryState {
    fn from_memory(memory: &Memory) -> Self {
        let mut ram = Box::new([0u8; 65536]);
        ram.copy_from_slice(&memory.ram);

        Self {
            ram,
            port_ddr: memory.port_ddr,
            port_data: memory.port_data,
            vic_registers: memory.vic_registers,
            sid_registers: memory.sid_registers,
            color_ram: memory.color_ram,
            keyboard_matrix: memory.keyboard_matrix,
            current_raster_line: memory.current_raster_line,
        }
    }
}

impl VicState {
    fn from_vic(vic: &Vic) -> Self {
        Self {
            raster_line: vic.raster_line,
            frame_cycle: vic.frame_cycle,
            ba_low: vic.ba_low,
            sprite_dma_active: vic.sprite_dma_active,
            sprite_display_count: vic.sprite_display_count,
        }
    }
}

impl SidState {
    fn from_sid(_sid: &Sid) -> Self {
        // SID state capture is simplified - full state would require
        // exposing internal oscillator phases, envelope counters, etc.
        // For now, we rely on SID registers being in memory.sid_registers
        Self {
            voices: [
                VoiceState::default(),
                VoiceState::default(),
                VoiceState::default(),
            ],
            filter_cutoff: 0,
            filter_resonance: 0,
            filter_mode: 0,
            volume: 0,
        }
    }
}

impl CiaState {
    const SIZE: usize = 28;

    fn from_cia(cia: &Cia) -> Self {
        Self {
            pra: cia.pra,
            prb: cia.prb,
            ddra: cia.ddra,
            ddrb: cia.ddrb,
            ta_lo: cia.ta_lo,
            ta_hi: cia.ta_hi,
            ta_latch_lo: cia.ta_latch_lo,
            ta_latch_hi: cia.ta_latch_hi,
            tb_lo: cia.tb_lo,
            tb_hi: cia.tb_hi,
            tb_latch_lo: cia.tb_latch_lo,
            tb_latch_hi: cia.tb_latch_hi,
            cra: cia.cra,
            crb: cia.crb,
            icr: cia.icr,
            icr_mask: cia.icr_mask,
            tod_10ths: cia.tod_10ths,
            tod_sec: cia.tod_sec,
            tod_min: cia.tod_min,
            tod_hr: cia.tod_hr,
            alarm_10ths: cia.alarm_10ths,
            alarm_sec: cia.alarm_sec,
            alarm_min: cia.alarm_min,
            alarm_hr: cia.alarm_hr,
            tod_running: cia.tod_running,
            tod_latched: cia.tod_latched,
        }
    }

    fn write_to(&self, data: &mut Vec<u8>) {
        data.push(self.pra);
        data.push(self.prb);
        data.push(self.ddra);
        data.push(self.ddrb);
        data.push(self.ta_lo);
        data.push(self.ta_hi);
        data.push(self.ta_latch_lo);
        data.push(self.ta_latch_hi);
        data.push(self.tb_lo);
        data.push(self.tb_hi);
        data.push(self.tb_latch_lo);
        data.push(self.tb_latch_hi);
        data.push(self.cra);
        data.push(self.crb);
        data.push(self.icr);
        data.push(self.icr_mask);
        data.push(self.tod_10ths);
        data.push(self.tod_sec);
        data.push(self.tod_min);
        data.push(self.tod_hr);
        data.push(self.alarm_10ths);
        data.push(self.alarm_sec);
        data.push(self.alarm_min);
        data.push(self.alarm_hr);
        data.push(self.tod_running as u8);
        data.push(self.tod_latched as u8);
        data.push(0); // padding
        data.push(0); // padding
    }

    fn read_from(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < Self::SIZE {
            return Err("Snapshot truncated (CIA)");
        }

        Ok(Self {
            pra: data[0],
            prb: data[1],
            ddra: data[2],
            ddrb: data[3],
            ta_lo: data[4],
            ta_hi: data[5],
            ta_latch_lo: data[6],
            ta_latch_hi: data[7],
            tb_lo: data[8],
            tb_hi: data[9],
            tb_latch_lo: data[10],
            tb_latch_hi: data[11],
            cra: data[12],
            crb: data[13],
            icr: data[14],
            icr_mask: data[15],
            tod_10ths: data[16],
            tod_sec: data[17],
            tod_min: data[18],
            tod_hr: data[19],
            alarm_10ths: data[20],
            alarm_sec: data[21],
            alarm_min: data[22],
            alarm_hr: data[23],
            tod_running: data[24] != 0,
            tod_latched: data[25] != 0,
        })
    }
}

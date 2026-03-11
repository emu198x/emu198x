//! NE2000/DP8390 Ethernet controller state machine.
//!
//! Implements the register set and internal memory of a DP8390-based NE2000
//! card as used in PCMCIA Ethernet adapters (CNET CN40BC). The NE2000 has
//! 48 KB of internal memory: a 16 KB PROM area and a 32 KB ring buffer for
//! packet reception. Transmit and receive use queue-based APIs so the runner
//! can wire network I/O without OS-specific socket code in the emulator.

use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Total internal memory: 48 KB (PROM + packet buffer).
const MEM_SIZE: usize = 0xC000;

/// Start of packet memory (ring buffer region).
const PMEM_START: usize = 0x4000;

/// End of packet memory.
const PMEM_END: usize = 0xC000;

/// Default ring buffer start page (0x40 = byte 0x4000).
const DEFAULT_PSTART: u8 = 0x40;

/// Default ring buffer stop page (0xC0 = byte 0xC000).
const DEFAULT_PSTOP: u8 = 0xC0;

// -- Command register bits --------------------------------------------------

const CMD_STP: u8 = 0x01; // Stop
const CMD_STA: u8 = 0x02; // Start
const CMD_TXP: u8 = 0x04; // Transmit packet
const _CMD_RD_MASK: u8 = 0x38; // Remote DMA command
#[cfg(test)]
const CMD_RD_READ: u8 = 0x08; // Remote read
#[cfg(test)]
const CMD_RD_WRITE: u8 = 0x10; // Remote write
const _CMD_RD_SEND: u8 = 0x18; // Send packet (not used)
const CMD_RD_ABORT: u8 = 0x20; // Abort/complete remote DMA
const CMD_PAGE_MASK: u8 = 0xC0; // Page select

// -- ISR bits ---------------------------------------------------------------

const ISR_RX: u8 = 0x01; // Packet received
const ISR_TX: u8 = 0x02; // Packet transmitted
const _ISR_RXE: u8 = 0x04; // Receive error
const ISR_TXE: u8 = 0x08; // Transmit error
const ISR_OVW: u8 = 0x10; // Overwrite warning (ring overflow)
const _ISR_CNT: u8 = 0x20; // Counter overflow
const ISR_RDC: u8 = 0x40; // Remote DMA complete
const ISR_RST: u8 = 0x80; // Reset status

// -- RSR bits ---------------------------------------------------------------

const RSR_RXOK: u8 = 0x01; // Received OK

// -- TSR bits ---------------------------------------------------------------

const TSR_TXOK: u8 = 0x01; // Transmitted OK

// -- RXCR bits --------------------------------------------------------------

const RXCR_AB: u8 = 0x04; // Accept broadcast
const RXCR_AM: u8 = 0x08; // Accept multicast
const RXCR_PRO: u8 = 0x10; // Promiscuous mode

// -- DCR bits ---------------------------------------------------------------

const DCR_WTS: u8 = 0x01; // Word transfer select (1 = 16-bit)

// ---------------------------------------------------------------------------
// NE2000 state
// ---------------------------------------------------------------------------

/// DP8390/NE2000 Ethernet controller state.
#[derive(Debug, Clone)]
pub struct Ne2000State {
    // -- Command register (page select + control) --
    cmd: u8,

    // -- Page 0 write registers --
    pstart: u8,
    pstop: u8,
    boundary: u8,
    tpsr: u8,
    tcnt: u16,
    rsar: u16,
    rcnt: u16,
    rxcr: u8,
    txcr: u8,
    dcfg: u8,
    imr: u8,

    // -- Status registers --
    tsr: u8,
    isr: u8,
    rsr: u8,

    // -- Page 1 registers --
    phys: [u8; 6],
    curpag: u8,
    mult: [u8; 8],

    // -- Internal memory --
    mem: Vec<u8>,

    // -- Packet queues --
    tx_queue: VecDeque<Vec<u8>>,
    rx_queue: VecDeque<Vec<u8>>,
}

impl Ne2000State {
    /// Create a new NE2000 with the given MAC address.
    ///
    /// The PROM area (first 16 KB) is initialised with the MAC address in
    /// the standard NE2000 format: each byte doubled, followed by signature
    /// bytes 0x57 0x57.
    #[must_use]
    pub fn new(mac: [u8; 6]) -> Self {
        let mut mem = vec![0u8; MEM_SIZE];

        // Write MAC into PROM area (doubled bytes, like real NE2000).
        for (i, &b) in mac.iter().enumerate() {
            mem[i * 2] = b;
            mem[i * 2 + 1] = b;
        }
        // NE2000 signature bytes at offset 0x0E/0x0F.
        mem[0x0E] = 0x57;
        mem[0x0F] = 0x57;

        let mut s = Self {
            cmd: CMD_STP | CMD_RD_ABORT, // Stopped, DMA abort
            pstart: DEFAULT_PSTART,
            pstop: DEFAULT_PSTOP,
            boundary: DEFAULT_PSTART,
            tpsr: 0,
            tcnt: 0,
            rsar: 0,
            rcnt: 0,
            rxcr: 0,
            txcr: 0,
            dcfg: DCR_WTS, // 16-bit mode
            imr: 0,
            tsr: TSR_TXOK,
            isr: 0,
            rsr: 0,
            phys: mac,
            curpag: DEFAULT_PSTART + 1,
            mult: [0; 8],
            mem,
            tx_queue: VecDeque::new(),
            rx_queue: VecDeque::new(),
        };
        s.isr = ISR_RST; // Reset status set on power-up
        s
    }

    /// Soft reset — restores registers to power-on defaults, preserves PROM.
    pub fn reset(&mut self) {
        let mac = self.phys;
        self.cmd = CMD_STP | CMD_RD_ABORT;
        self.pstart = DEFAULT_PSTART;
        self.pstop = DEFAULT_PSTOP;
        self.boundary = DEFAULT_PSTART;
        self.tpsr = 0;
        self.tcnt = 0;
        self.rsar = 0;
        self.rcnt = 0;
        self.rxcr = 0;
        self.txcr = 0;
        self.dcfg = DCR_WTS;
        self.imr = 0;
        self.tsr = TSR_TXOK;
        self.isr = ISR_RST;
        self.rsr = 0;
        self.phys = mac;
        self.curpag = DEFAULT_PSTART + 1;
        self.mult = [0; 8];
        self.tx_queue.clear();
        self.rx_queue.clear();
    }

    /// True when an interrupt is pending (ISR & IMR != 0).
    #[must_use]
    pub fn irq_pending(&self) -> bool {
        self.isr & self.imr != 0
    }

    // -----------------------------------------------------------------------
    // Register I/O
    // -----------------------------------------------------------------------

    /// Read a register. `addr` is the 5-bit register offset (0x00-0x1F).
    #[must_use]
    pub fn read_reg(&mut self, addr: u8) -> u8 {
        let offset = addr & 0x1F;

        // Data port at offset 0x10.
        if offset == 0x10 {
            return self.read_data_port() as u8;
        }
        // Reset port at offset 0x1F.
        if offset == 0x1F {
            self.reset();
            return 0;
        }

        let page = (self.cmd & CMD_PAGE_MASK) >> 6;
        let reg = offset & 0x0F;

        match page {
            0 => self.read_page0(reg),
            1 => self.read_page1(reg),
            2 => self.read_page2(reg),
            _ => 0,
        }
    }

    /// Write a register. `addr` is the 5-bit register offset (0x00-0x1F).
    pub fn write_reg(&mut self, addr: u8, val: u8) {
        let offset = addr & 0x1F;

        // Data port at offset 0x10.
        if offset == 0x10 {
            self.write_data_port(u16::from(val));
            return;
        }
        // Reset port at offset 0x1F.
        if offset == 0x1F {
            self.reset();
            return;
        }

        let page = (self.cmd & CMD_PAGE_MASK) >> 6;
        let reg = offset & 0x0F;

        match page {
            0 => self.write_page0(reg, val),
            1 => self.write_page1(reg, val),
            2 => { /* Page 2 writes are mostly ignored */ }
            _ => {}
        }
    }

    /// Read a 16-bit word from the remote DMA data port.
    #[must_use]
    pub fn read_data_port(&mut self) -> u16 {
        if self.rcnt == 0 {
            return 0;
        }

        let addr = self.rsar as usize;
        let lo = if addr < MEM_SIZE { self.mem[addr] } else { 0 };

        let addr_hi = if addr + 1 >= PMEM_END {
            PMEM_START
        } else {
            addr + 1
        };
        let hi = if addr_hi < MEM_SIZE {
            self.mem[addr_hi]
        } else {
            0
        };

        // Word mode: advance by 2, decrement by 2.
        if self.dcfg & DCR_WTS != 0 {
            self.rsar = self.rsar.wrapping_add(2);
            self.rcnt = self.rcnt.saturating_sub(2);
        } else {
            self.rsar = self.rsar.wrapping_add(1);
            self.rcnt = self.rcnt.saturating_sub(1);
        }

        // Wrap RSAR within packet memory if needed.
        if self.rsar as usize >= PMEM_END {
            self.rsar = PMEM_START as u16;
        }

        // Signal DMA complete when count reaches zero.
        if self.rcnt == 0 {
            self.isr |= ISR_RDC;
        }

        u16::from(hi) << 8 | u16::from(lo)
    }

    /// Write a 16-bit word to the remote DMA data port.
    pub fn write_data_port(&mut self, val: u16) {
        if self.rcnt == 0 {
            return;
        }

        let addr = self.rsar as usize;
        if addr < MEM_SIZE {
            self.mem[addr] = val as u8;
        }

        if self.dcfg & DCR_WTS != 0 {
            let addr_hi = if addr + 1 >= PMEM_END {
                PMEM_START
            } else {
                addr + 1
            };
            if addr_hi < MEM_SIZE {
                self.mem[addr_hi] = (val >> 8) as u8;
            }
            self.rsar = self.rsar.wrapping_add(2);
            self.rcnt = self.rcnt.saturating_sub(2);
        } else {
            self.rsar = self.rsar.wrapping_add(1);
            self.rcnt = self.rcnt.saturating_sub(1);
        }

        if self.rsar as usize >= PMEM_END {
            self.rsar = PMEM_START as u16;
        }

        if self.rcnt == 0 {
            self.isr |= ISR_RDC;
        }
    }

    // -----------------------------------------------------------------------
    // Packet I/O (for external wiring by machine-amiga / runner)
    // -----------------------------------------------------------------------

    /// Inject a received Ethernet frame into the ring buffer.
    ///
    /// The frame is wrapped with a 4-byte NE2000 header (status, next_page,
    /// length_lo, length_hi) and written at `curpag`. If the ring buffer
    /// overflows, ISR_OVW is set instead.
    pub fn push_rx_packet(&mut self, data: &[u8]) {
        if self.cmd & CMD_STP != 0 {
            return; // Stopped — don't receive.
        }

        // Check MAC filtering.
        if !self.should_accept(data) {
            return;
        }

        // Total bytes = 4-byte header + frame data, rounded up to page boundary.
        let total_len = 4 + data.len();
        let pages_needed = (total_len + 255) / 256;

        // Check for ring buffer space.
        let avail = if self.curpag >= self.boundary {
            (self.pstop as usize - self.curpag as usize)
                + (self.boundary as usize - self.pstart as usize)
        } else {
            self.boundary as usize - self.curpag as usize
        };

        if pages_needed > avail {
            self.isr |= ISR_OVW;
            return;
        }

        // Calculate next page after this packet.
        let mut next_page = self.curpag as usize + pages_needed;
        if next_page >= self.pstop as usize {
            next_page -= self.pstop as usize - self.pstart as usize;
        }

        // Write 4-byte header at curpag.
        let mut write_addr = self.curpag as usize * 256;
        self.mem_write_ring(write_addr, RSR_RXOK);
        write_addr += 1;
        self.mem_write_ring(write_addr, next_page as u8);
        write_addr += 1;
        self.mem_write_ring(write_addr, total_len as u8);
        write_addr += 1;
        self.mem_write_ring(write_addr, (total_len >> 8) as u8);
        write_addr += 1;

        // Write frame data.
        for &b in data {
            self.mem_write_ring(write_addr, b);
            write_addr += 1;
        }

        self.curpag = next_page as u8;
        self.rsr = RSR_RXOK;
        self.isr |= ISR_RX;
    }

    /// Pop a transmitted packet from the queue, if any.
    #[must_use]
    pub fn pop_tx_packet(&mut self) -> Option<Vec<u8>> {
        self.tx_queue.pop_front()
    }

    // -----------------------------------------------------------------------
    // Page 0 register read/write
    // -----------------------------------------------------------------------

    fn read_page0(&self, reg: u8) -> u8 {
        match reg {
            0x00 => self.cmd,
            0x01 => 0, // CLDA0 (not implemented)
            0x02 => 0, // CLDA1
            0x03 => self.boundary,
            0x04 => self.tsr,
            0x05 => 0, // NCR
            0x06 => 0, // FIFO
            0x07 => self.isr,
            0x08 => 0, // CRDA0
            0x09 => 0, // CRDA1
            0x0A => 0, // reserved
            0x0B => 0, // reserved
            0x0C => self.rsr,
            0x0D => 0, // CNTR0
            0x0E => 0, // CNTR1
            0x0F => 0, // CNTR2
            _ => 0,
        }
    }

    fn write_page0(&mut self, reg: u8, val: u8) {
        match reg {
            0x00 => self.write_cmd(val),
            0x01 => self.pstart = val,
            0x02 => self.pstop = val,
            0x03 => self.boundary = val,
            0x04 => self.tpsr = val,
            0x05 => self.tcnt = (self.tcnt & 0xFF00) | u16::from(val),
            0x06 => self.tcnt = (self.tcnt & 0x00FF) | (u16::from(val) << 8),
            0x07 => {
                // ISR: write 1 to clear bits.
                self.isr &= !val;
            }
            0x08 => self.rsar = (self.rsar & 0xFF00) | u16::from(val),
            0x09 => self.rsar = (self.rsar & 0x00FF) | (u16::from(val) << 8),
            0x0A => self.rcnt = (self.rcnt & 0xFF00) | u16::from(val),
            0x0B => self.rcnt = (self.rcnt & 0x00FF) | (u16::from(val) << 8),
            0x0C => self.rxcr = val,
            0x0D => self.txcr = val,
            0x0E => self.dcfg = val,
            0x0F => self.imr = val,
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Page 1 register read/write
    // -----------------------------------------------------------------------

    fn read_page1(&self, reg: u8) -> u8 {
        match reg {
            0x00 => self.cmd,
            0x01..=0x06 => self.phys[(reg - 1) as usize],
            0x07 => self.curpag,
            0x08..=0x0F => self.mult[(reg - 8) as usize],
            _ => 0,
        }
    }

    fn write_page1(&mut self, reg: u8, val: u8) {
        match reg {
            0x00 => self.write_cmd(val),
            0x01..=0x06 => self.phys[(reg - 1) as usize] = val,
            0x07 => self.curpag = val,
            0x08..=0x0F => self.mult[(reg - 8) as usize] = val,
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Page 2 register read (read-back of page 0 config)
    // -----------------------------------------------------------------------

    fn read_page2(&self, reg: u8) -> u8 {
        match reg {
            0x00 => self.cmd,
            0x01 => self.pstart,
            0x02 => self.pstop,
            0x05 => 0, // reserved
            0x07 => self.isr,
            0x0C => self.rxcr,
            0x0D => self.txcr,
            0x0E => self.dcfg,
            0x0F => self.imr,
            _ => 0,
        }
    }

    // -----------------------------------------------------------------------
    // Command register handling
    // -----------------------------------------------------------------------

    fn write_cmd(&mut self, val: u8) {
        // Preserve page select bits, handle start/stop and transmit.
        self.cmd = val;

        // Clear reset status when started.
        if val & CMD_STA != 0 {
            self.isr &= !ISR_RST;
        }

        // Transmit packet.
        if val & CMD_TXP != 0 {
            self.do_transmit();
        }
    }

    // -----------------------------------------------------------------------
    // Transmit
    // -----------------------------------------------------------------------

    fn do_transmit(&mut self) {
        let start = self.tpsr as usize * 256;
        let len = self.tcnt as usize;

        if len == 0 || start + len > MEM_SIZE {
            self.tsr = 0;
            self.isr |= ISR_TXE;
            self.cmd &= !CMD_TXP;
            return;
        }

        let packet = self.mem[start..start + len].to_vec();
        self.tx_queue.push_back(packet);

        self.tsr = TSR_TXOK;
        self.isr |= ISR_TX;
        self.cmd &= !CMD_TXP;
    }

    // -----------------------------------------------------------------------
    // MAC filtering
    // -----------------------------------------------------------------------

    fn should_accept(&self, data: &[u8]) -> bool {
        if data.len() < 6 {
            return false;
        }

        // Promiscuous mode accepts everything.
        if self.rxcr & RXCR_PRO != 0 {
            return true;
        }

        let dest = &data[0..6];

        // Broadcast: FF:FF:FF:FF:FF:FF.
        if dest == [0xFF; 6] {
            return self.rxcr & RXCR_AB != 0;
        }

        // Multicast: bit 0 of first byte set.
        if dest[0] & 0x01 != 0 {
            if self.rxcr & RXCR_AM != 0 {
                // Check multicast hash filter.
                let crc = crc32(dest);
                let hash_idx = (crc >> 26) & 0x3F;
                let byte_idx = (hash_idx >> 3) as usize;
                let bit_idx = hash_idx & 0x07;
                return self.mult[byte_idx] & (1 << bit_idx) != 0;
            }
            return false;
        }

        // Unicast: match physical address.
        dest == self.phys
    }

    // -----------------------------------------------------------------------
    // Ring buffer helper
    // -----------------------------------------------------------------------

    fn mem_write_ring(&mut self, addr: usize, val: u8) {
        // Wrap address within the ring buffer region.
        let ring_size = (self.pstop as usize - self.pstart as usize) * 256;
        if ring_size == 0 {
            return;
        }
        let ring_base = self.pstart as usize * 256;
        let offset = if addr >= ring_base {
            (addr - ring_base) % ring_size
        } else {
            return;
        };
        let final_addr = ring_base + offset;
        if final_addr < MEM_SIZE {
            self.mem[final_addr] = val;
        }
    }
}

// ---------------------------------------------------------------------------
// CRC-32 (Ethernet FCS polynomial) for multicast hash
// ---------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFF_FFFF
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ne2000() -> Ne2000State {
        Ne2000State::new([0x00, 0x80, 0x10, 0x12, 0x34, 0x56])
    }

    #[test]
    fn prom_contains_mac() {
        let nic = make_ne2000();
        // MAC bytes are doubled in PROM.
        assert_eq!(nic.mem[0], 0x00);
        assert_eq!(nic.mem[1], 0x00);
        assert_eq!(nic.mem[2], 0x80);
        assert_eq!(nic.mem[3], 0x80);
        assert_eq!(nic.mem[4], 0x10);
        assert_eq!(nic.mem[5], 0x10);
        // Signature.
        assert_eq!(nic.mem[0x0E], 0x57);
        assert_eq!(nic.mem[0x0F], 0x57);
    }

    #[test]
    fn reset_sets_isr_rst() {
        let mut nic = make_ne2000();
        nic.isr = 0;
        nic.reset();
        assert_eq!(nic.isr & ISR_RST, ISR_RST);
    }

    #[test]
    fn page_select() {
        let mut nic = make_ne2000();
        // Default: page 0.
        assert_eq!(nic.cmd & CMD_PAGE_MASK, 0);

        // Switch to page 1.
        nic.write_reg(0x00, CMD_STA | 0x40);
        assert_eq!((nic.cmd >> 6) & 0x03, 1);

        // Read MAC from page 1.
        assert_eq!(nic.read_reg(0x01), 0x00);
        assert_eq!(nic.read_reg(0x02), 0x80);
        assert_eq!(nic.read_reg(0x03), 0x10);
    }

    #[test]
    fn remote_dma_read() {
        let mut nic = make_ne2000();

        // Write known data at PROM address 0x0000.
        // (Already there from init — MAC bytes.)
        // Set up remote DMA read: RSAR=$0000, RCNT=4.
        nic.write_reg(0x08, 0x00); // RSAR lo
        nic.write_reg(0x09, 0x00); // RSAR hi
        nic.write_reg(0x0A, 0x04); // RCNT lo
        nic.write_reg(0x0B, 0x00); // RCNT hi

        // Start + remote read.
        nic.write_reg(0x00, CMD_STA | CMD_RD_READ);

        // Read 2 words (4 bytes in word mode).
        let w0 = nic.read_data_port();
        let w1 = nic.read_data_port();

        // PROM: 00 00 80 80 — lo byte first in NE2000.
        assert_eq!(w0, 0x0000); // bytes 0,1 (both 0x00)
        assert_eq!(w1, 0x8080); // bytes 2,3 (both 0x80)

        // RDC should be set after reading all bytes.
        assert_eq!(nic.isr & ISR_RDC, ISR_RDC);
    }

    #[test]
    fn remote_dma_write() {
        let mut nic = make_ne2000();

        // Write to packet memory at page 0x40 (byte address 0x4000).
        nic.write_reg(0x08, 0x00); // RSAR lo
        nic.write_reg(0x09, 0x40); // RSAR hi
        nic.write_reg(0x0A, 0x04); // RCNT lo
        nic.write_reg(0x0B, 0x00); // RCNT hi
        nic.write_reg(0x00, CMD_STA | CMD_RD_WRITE);

        nic.write_data_port(0xDEAD);
        nic.write_data_port(0xBEEF);

        assert_eq!(nic.isr & ISR_RDC, ISR_RDC);
        // Verify in memory (lo byte first).
        assert_eq!(nic.mem[0x4000], 0xAD);
        assert_eq!(nic.mem[0x4001], 0xDE);
        assert_eq!(nic.mem[0x4002], 0xEF);
        assert_eq!(nic.mem[0x4003], 0xBE);
    }

    #[test]
    fn transmit_packet_queued() {
        let mut nic = make_ne2000();

        // Write a small packet at page 0x40.
        let pkt = [0x01, 0x02, 0x03, 0x04];
        for (i, &b) in pkt.iter().enumerate() {
            nic.mem[0x4000 + i] = b;
        }

        // Set TPSR and TCNT.
        nic.write_reg(0x04, 0x40); // TPSR
        nic.write_reg(0x05, 0x04); // TCNT lo
        nic.write_reg(0x06, 0x00); // TCNT hi

        // Start + transmit.
        nic.write_reg(0x00, CMD_STA | CMD_TXP);

        // Packet should be in the queue.
        let tx = nic.pop_tx_packet().expect("expected transmitted packet");
        assert_eq!(tx, vec![0x01, 0x02, 0x03, 0x04]);

        // ISR TX bit should be set.
        assert_eq!(nic.isr & ISR_TX, ISR_TX);

        // TXP bit should be cleared.
        assert_eq!(nic.cmd & CMD_TXP, 0);
    }

    #[test]
    fn receive_packet_into_ring() {
        let mut nic = make_ne2000();
        // Start the NIC.
        nic.write_reg(0x00, CMD_STA);
        // Enable broadcast reception.
        nic.rxcr = RXCR_AB;
        // Set ring: pstart=0x40, pstop=0xC0, curpag=0x41.
        nic.pstart = 0x40;
        nic.pstop = 0xC0;
        nic.boundary = 0x40;
        nic.curpag = 0x41;

        // Build a broadcast frame (14-byte Ethernet header + 4 bytes payload).
        let mut frame = vec![0xFF; 6]; // dest = broadcast
        frame.extend_from_slice(&[0x00, 0x80, 0x10, 0x12, 0x34, 0x56]); // src
        frame.extend_from_slice(&[0x08, 0x00]); // ethertype
        frame.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // payload

        nic.push_rx_packet(&frame);

        // ISR RX should be set.
        assert_eq!(nic.isr & ISR_RX, ISR_RX);

        // Read the header from ring buffer at page 0x41 (byte 0x4100).
        let hdr_base = 0x41 * 256;
        assert_eq!(nic.mem[hdr_base], RSR_RXOK); // status
        // Next page.
        let total = 4 + frame.len();
        let pages = (total + 255) / 256;
        let expected_next = 0x41 + pages;
        assert_eq!(nic.mem[hdr_base + 1], expected_next as u8);
        // Length.
        assert_eq!(nic.mem[hdr_base + 2], total as u8);
        assert_eq!(nic.mem[hdr_base + 3], (total >> 8) as u8);
        // First data byte.
        assert_eq!(nic.mem[hdr_base + 4], 0xFF);
    }

    #[test]
    fn irq_pending_respects_imr() {
        let mut nic = make_ne2000();
        nic.isr = ISR_RX;
        nic.imr = 0;
        assert!(!nic.irq_pending());

        nic.imr = ISR_RX;
        assert!(nic.irq_pending());
    }

    #[test]
    fn isr_write_to_clear() {
        let mut nic = make_ne2000();
        nic.isr = ISR_RX | ISR_TX | ISR_RDC;
        // Writing ISR_TX should clear only that bit.
        nic.write_reg(0x07, ISR_TX);
        assert_eq!(nic.isr, ISR_RX | ISR_RDC);
    }

    #[test]
    fn unicast_filtering() {
        let mut nic = make_ne2000();
        nic.write_reg(0x00, CMD_STA);
        nic.rxcr = 0; // No promiscuous, no broadcast, no multicast.
        nic.pstart = 0x40;
        nic.pstop = 0xC0;
        nic.boundary = 0x40;
        nic.curpag = 0x41;

        // Frame addressed to our MAC — should be accepted.
        let mut frame = vec![0x00, 0x80, 0x10, 0x12, 0x34, 0x56]; // dest = our MAC
        frame.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]); // src
        frame.extend_from_slice(&[0x08, 0x00]);
        frame.extend_from_slice(&[0xAA]);

        nic.push_rx_packet(&frame);
        assert_eq!(nic.isr & ISR_RX, ISR_RX);

        // Frame addressed to different MAC — should be rejected.
        nic.isr = 0;
        nic.curpag = 0x42;
        let mut frame2 = vec![0x00, 0x80, 0x10, 0x99, 0x99, 0x99]; // wrong dest
        frame2.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        frame2.extend_from_slice(&[0x08, 0x00]);
        frame2.extend_from_slice(&[0xBB]);

        nic.push_rx_packet(&frame2);
        assert_eq!(nic.isr & ISR_RX, 0); // Not received.
    }

    #[test]
    fn reset_port_triggers_reset() {
        let mut nic = make_ne2000();
        nic.isr = 0;
        nic.imr = 0xFF;
        // Reading offset 0x1F triggers reset.
        let _ = nic.read_reg(0x1F);
        assert_eq!(nic.isr & ISR_RST, ISR_RST);
        assert_eq!(nic.imr, 0); // Reset clears IMR.
    }
}

//! Flat 16 MB memory model for Musashi test generation.
//!
//! Musashi calls C memory functions (`m68k_read_memory_8`, etc.) which
//! we implement here. A global `MEMORY` array backs all reads and writes.
//!
//! For performance, we track which addresses have been touched during
//! test setup (via `poke`/`poke_word`/`poke_long`) and during instruction
//! execution (via Musashi write callbacks). This avoids scanning all 16 MB.

use std::collections::BTreeSet;
use std::ffi::c_uint;
use std::sync::Mutex;

/// 16 MB flat address space (24-bit, matching 68000).
const MEM_SIZE: usize = 16 * 1024 * 1024;

/// Address mask for 24-bit bus.
const ADDR_MASK: usize = MEM_SIZE - 1;

/// Global memory state. Musashi's C callbacks access this.
static MEMORY: Mutex<MemoryState> = Mutex::new(MemoryState::new());

pub struct MemoryState {
    data: [u8; MEM_SIZE],
    /// Addresses written during test setup (via poke).
    setup_addrs: BTreeSet<u32>,
    /// Addresses written during instruction execution (via Musashi callbacks).
    exec_writes: Vec<(u32, u8)>,
}

impl MemoryState {
    const fn new() -> Self {
        Self {
            data: [0; MEM_SIZE],
            setup_addrs: BTreeSet::new(),
            exec_writes: Vec::new(),
        }
    }
}

/// Clear all memory to zero and reset all tracking.
pub fn clear() {
    let mut mem = MEMORY.lock().expect("memory lock poisoned");
    // Collect addresses to zero before mutating data
    let addrs: Vec<usize> = mem
        .setup_addrs
        .iter()
        .map(|&a| a as usize)
        .chain(mem.exec_writes.iter().map(|&(a, _)| a as usize))
        .collect();
    for addr in addrs {
        mem.data[addr & ADDR_MASK] = 0;
    }
    mem.setup_addrs.clear();
    mem.exec_writes.clear();
}

/// Write a byte to memory (for test setup — tracked for snapshot).
#[allow(dead_code)]
pub fn poke(addr: u32, value: u8) {
    let mut mem = MEMORY.lock().expect("memory lock poisoned");
    let masked = addr & ADDR_MASK as u32;
    mem.data[masked as usize] = value;
    mem.setup_addrs.insert(masked);
}

/// Read a byte from memory (for test verification).
#[allow(dead_code)]
pub fn peek(addr: u32) -> u8 {
    let mem = MEMORY.lock().expect("memory lock poisoned");
    mem.data[addr as usize & ADDR_MASK]
}

/// Write a big-endian word to memory (for test setup).
pub fn poke_word(addr: u32, value: u16) {
    let mut mem = MEMORY.lock().expect("memory lock poisoned");
    let a0 = addr & ADDR_MASK as u32;
    let a1 = addr.wrapping_add(1) & ADDR_MASK as u32;
    mem.data[a0 as usize] = (value >> 8) as u8;
    mem.data[a1 as usize] = (value & 0xFF) as u8;
    mem.setup_addrs.insert(a0);
    mem.setup_addrs.insert(a1);
}

/// Write a big-endian longword to memory (for test setup).
pub fn poke_long(addr: u32, value: u32) {
    let mut mem = MEMORY.lock().expect("memory lock poisoned");
    let a0 = addr & ADDR_MASK as u32;
    let a1 = addr.wrapping_add(1) & ADDR_MASK as u32;
    let a2 = addr.wrapping_add(2) & ADDR_MASK as u32;
    let a3 = addr.wrapping_add(3) & ADDR_MASK as u32;
    mem.data[a0 as usize] = (value >> 24) as u8;
    mem.data[a1 as usize] = (value >> 16) as u8;
    mem.data[a2 as usize] = (value >> 8) as u8;
    mem.data[a3 as usize] = value as u8;
    mem.setup_addrs.insert(a0);
    mem.setup_addrs.insert(a1);
    mem.setup_addrs.insert(a2);
    mem.setup_addrs.insert(a3);
}

/// Reset execution write tracking (call before each instruction execution).
pub fn reset_writes() {
    let mut mem = MEMORY.lock().expect("memory lock poisoned");
    mem.exec_writes.clear();
}

/// Take the accumulated execution writes.
pub fn take_writes() -> Vec<(u32, u8)> {
    let mut mem = MEMORY.lock().expect("memory lock poisoned");
    std::mem::take(&mut mem.exec_writes)
}

/// Snapshot all tracked addresses (setup + writes) as (addr, byte) pairs.
pub fn snapshot_tracked() -> Vec<(u32, u8)> {
    let mem = MEMORY.lock().expect("memory lock poisoned");
    let mut result: Vec<(u32, u8)> = mem
        .setup_addrs
        .iter()
        .map(|&a| (a, mem.data[a as usize]))
        .collect();
    // Include any addresses written during execution that aren't in setup
    for &(a, _) in &mem.exec_writes {
        if !mem.setup_addrs.contains(&a) {
            result.push((a, mem.data[a as usize]));
        }
    }
    result.sort_by_key(|&(a, _)| a);
    result.dedup_by_key(|entry| entry.0);
    result
}

/// Snapshot specific memory addresses.
pub fn snapshot_addrs(addrs: &[u32]) -> Vec<(u32, u8)> {
    let mem = MEMORY.lock().expect("memory lock poisoned");
    addrs
        .iter()
        .map(|&a| (a, mem.data[a as usize & ADDR_MASK]))
        .collect()
}

// --- Musashi C callbacks ---

#[unsafe(no_mangle)]
pub extern "C" fn m68k_read_memory_8(address: c_uint) -> c_uint {
    let mem = MEMORY.lock().expect("memory lock poisoned");
    c_uint::from(mem.data[address as usize & ADDR_MASK])
}

#[unsafe(no_mangle)]
pub extern "C" fn m68k_read_memory_16(address: c_uint) -> c_uint {
    let mem = MEMORY.lock().expect("memory lock poisoned");
    let addr = address as usize & ADDR_MASK;
    let hi = mem.data[addr];
    let lo = mem.data[(addr + 1) & ADDR_MASK];
    (c_uint::from(hi) << 8) | c_uint::from(lo)
}

#[unsafe(no_mangle)]
pub extern "C" fn m68k_read_memory_32(address: c_uint) -> c_uint {
    let mem = MEMORY.lock().expect("memory lock poisoned");
    let addr = address as usize & ADDR_MASK;
    let b0 = mem.data[addr];
    let b1 = mem.data[(addr + 1) & ADDR_MASK];
    let b2 = mem.data[(addr + 2) & ADDR_MASK];
    let b3 = mem.data[(addr + 3) & ADDR_MASK];
    (c_uint::from(b0) << 24)
        | (c_uint::from(b1) << 16)
        | (c_uint::from(b2) << 8)
        | c_uint::from(b3)
}

#[unsafe(no_mangle)]
pub extern "C" fn m68k_write_memory_8(address: c_uint, value: c_uint) {
    let mut mem = MEMORY.lock().expect("memory lock poisoned");
    let addr = address as usize & ADDR_MASK;
    let byte = value as u8;
    mem.data[addr] = byte;
    mem.exec_writes.push((addr as u32, byte));
}

#[unsafe(no_mangle)]
pub extern "C" fn m68k_write_memory_16(address: c_uint, value: c_uint) {
    let mut mem = MEMORY.lock().expect("memory lock poisoned");
    let addr = address as usize & ADDR_MASK;
    let hi = (value >> 8) as u8;
    let lo = (value & 0xFF) as u8;
    mem.data[addr] = hi;
    mem.data[(addr + 1) & ADDR_MASK] = lo;
    mem.exec_writes.push((addr as u32, hi));
    mem.exec_writes
        .push((((addr + 1) & ADDR_MASK) as u32, lo));
}

#[unsafe(no_mangle)]
pub extern "C" fn m68k_write_memory_32(address: c_uint, value: c_uint) {
    let mut mem = MEMORY.lock().expect("memory lock poisoned");
    let addr = address as usize & ADDR_MASK;
    let b0 = (value >> 24) as u8;
    let b1 = (value >> 16) as u8;
    let b2 = (value >> 8) as u8;
    let b3 = value as u8;
    mem.data[addr] = b0;
    mem.data[(addr + 1) & ADDR_MASK] = b1;
    mem.data[(addr + 2) & ADDR_MASK] = b2;
    mem.data[(addr + 3) & ADDR_MASK] = b3;
    mem.exec_writes.push((addr as u32, b0));
    mem.exec_writes
        .push((((addr + 1) & ADDR_MASK) as u32, b1));
    mem.exec_writes
        .push((((addr + 2) & ADDR_MASK) as u32, b2));
    mem.exec_writes
        .push((((addr + 3) & ADDR_MASK) as u32, b3));
}

#[unsafe(no_mangle)]
pub extern "C" fn m68k_write_memory_32_pd(address: c_uint, value: c_uint) {
    m68k_write_memory_32(address, value);
}

// Disassembler memory access
#[unsafe(no_mangle)]
pub extern "C" fn m68k_read_disassembler_8(address: c_uint) -> c_uint {
    m68k_read_memory_8(address)
}

#[unsafe(no_mangle)]
pub extern "C" fn m68k_read_disassembler_16(address: c_uint) -> c_uint {
    m68k_read_memory_16(address)
}

#[unsafe(no_mangle)]
pub extern "C" fn m68k_read_disassembler_32(address: c_uint) -> c_uint {
    m68k_read_memory_32(address)
}

//! Direct-mapped on-chip instruction cache for the 68020+.
//!
//! The 68020 instruction cache is 256 bytes: 64 direct-mapped entries
//! of one long word each, tagged by address bits [31:8] plus the FC2
//! function-code bit (which separates supervisor- from user-program
//! space). The index is address bits [7:2]; the long word holds the
//! two instruction words at the even (bits 31:16) and odd (bits 15:0)
//! word offsets. (M68020UM § 6, "On-Chip Cache Memory".)
//!
//! ## Why a valid bit *per word*, not per line
//!
//! Real hardware fills a whole line in one long-word burst and carries
//! a single valid bit per line. Our prefetch microcode fetches one
//! *word* per bus cycle ([`crate::microcode::MicroOp::FetchIRC`]), so a
//! single-valid-bit line would be filled with only half its data on a
//! word miss and would then serve garbage for the sibling word. We
//! instead track a valid bit per word and fill one word per sequential
//! fetch.
//!
//! For forward (sequential) execution the two models are equivalent —
//! the line fills one word per fetch as the program advances — and they
//! differ only in an unobservable corner: a backward branch onto the
//! high word of a line whose original entry point was the low word
//! misses for us but would hit on hardware (which had burst-filled the
//! whole line). That difference is conservative (an extra cold fetch,
//! never a saved one) and touches *timing only*. The cache never
//! changes the decoded instruction word — a hit serves exactly what the
//! bus would have returned — so it carries no architectural-state risk.
//!
//! ## Forward design (#110 / #111)
//!
//! The 68030 adds a second 256-byte line set (data cache); the 68040
//! moves to 4 KB, 4-way set-associative split caches. Those variants
//! reuse this model by widening [`ENTRIES`] and adding an associativity
//! dimension; that extension is deferred until #110/#111 land so we
//! ship the testable 68020 direct-mapped case now rather than untested
//! associativity machinery.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// Number of direct-mapped entries. 68020: 64 long-word lines = 256 B.
pub const INSTRUCTION_CACHE_LINE_COUNT: usize = 64;

/// One cache line: a long word (two instruction words) plus a tag and a
/// validity bit per word.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Line {
    /// `(addr >> 8) << 1 | fc2` — address bits [31:8] of the line, with
    /// the FC2 (supervisor) bit folded into bit 0 so user- and
    /// supervisor-program lines never alias.
    tag: u32,
    /// The two instruction words: `[even-offset word, odd-offset word]`,
    /// indexed by address bit 1.
    words: [u16; 2],
    /// Validity, one bit per word.
    valid: [bool; 2],
}

/// Stable diagnostic projection of one direct-mapped instruction-cache line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ICacheLineDiagnosticSnapshot {
    /// Direct-mapped line index.
    pub index: u8,
    /// Address/function-code tag retained by the line.
    pub tag: u32,
    /// Even- and odd-offset instruction words.
    pub words: [u16; 2],
    /// Per-word validity flags used by the current compatibility model.
    pub valid: [bool; 2],
}

impl Line {
    const fn empty() -> Self {
        Self {
            tag: 0,
            words: [0, 0],
            valid: [false, false],
        }
    }
}

/// A 68020-class direct-mapped instruction cache.
///
/// Lives on [`crate::Cpu68000`] as `variant_icache`, set to `Some(..)`
/// only by the 68020+ wrapper's `install_variant_hooks`. Cache contents
/// are serialized because a warm hit suppresses an external bus cycle;
/// replacing a restored warm cache with a cold one would therefore alter
/// bus contention and execution timing after a save-state boundary.
#[derive(Clone, Serialize, Deserialize)]
pub struct ICache {
    #[serde(with = "BigArray")]
    lines: [Line; INSTRUCTION_CACHE_LINE_COUNT],
}

impl Default for ICache {
    fn default() -> Self {
        Self::new()
    }
}

impl ICache {
    /// Create an empty (all-invalid) cache.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lines: [Line::empty(); INSTRUCTION_CACHE_LINE_COUNT],
        }
    }

    /// Look up the program word at `addr` (FC2 = supervisor). Returns
    /// the cached word on a hit, or `None` on a miss. Pure read — does
    /// not allocate or update the cache.
    #[must_use]
    pub fn lookup(&self, addr: u32, fc2: bool) -> Option<u16> {
        let line = &self.lines[index(addr)];
        let w = word_sel(addr);
        if line.valid[w] && line.tag == key(addr, fc2) {
            Some(line.words[w])
        } else {
            None
        }
    }

    /// Fill the entry for `addr` (FC2 = supervisor) with `word`. If the
    /// line currently holds a different tag, its sibling word is dropped
    /// (a direct-mapped line can hold only one tag at a time).
    pub fn fill(&mut self, addr: u32, fc2: bool, word: u16) {
        let line = &mut self.lines[index(addr)];
        let k = key(addr, fc2);
        if line.tag != k {
            line.tag = k;
            line.valid = [false, false];
        }
        let w = word_sel(addr);
        line.words[w] = word;
        line.valid[w] = true;
    }

    /// Invalidate every entry (CACR.C, "clear cache").
    pub fn clear(&mut self) {
        for line in &mut self.lines {
            line.valid = [false, false];
        }
    }

    /// Invalidate the single entry the address indexes (CACR.CE, "clear
    /// entry", selected by CAAR on the 68020).
    pub fn clear_entry(&mut self, addr: u32) {
        self.lines[index(addr)].valid = [false, false];
    }

    /// Number of cache lines containing at least one valid word.
    #[must_use]
    pub fn valid_line_count(&self) -> usize {
        self.lines
            .iter()
            .filter(|line| line.valid.iter().any(|&valid| valid))
            .count()
    }

    /// Number of individually valid instruction words.
    #[must_use]
    pub fn valid_word_count(&self) -> usize {
        self.lines
            .iter()
            .map(|line| line.valid.iter().filter(|&&valid| valid).count())
            .sum()
    }

    /// Number of direct-mapped long-word lines in this cache model.
    #[must_use]
    pub const fn line_capacity(&self) -> usize {
        INSTRUCTION_CACHE_LINE_COUNT
    }

    /// Number of independently tracked instruction words.
    #[must_use]
    pub const fn word_capacity(&self) -> usize {
        INSTRUCTION_CACHE_LINE_COUNT * 2
    }

    /// All direct-mapped lines in hardware index order.
    ///
    /// Invalid lines retain their tag and word values because those values can
    /// explain divergence around invalidation and refill boundaries even
    /// though they cannot currently produce a hit.
    #[must_use]
    pub fn diagnostic_lines(&self) -> [ICacheLineDiagnosticSnapshot; INSTRUCTION_CACHE_LINE_COUNT] {
        core::array::from_fn(|index| {
            let line = self.lines[index];
            ICacheLineDiagnosticSnapshot {
                index: index as u8,
                tag: line.tag,
                words: line.words,
                valid: line.valid,
            }
        })
    }
}

/// Direct-mapped line index: address bits [7:2].
#[inline]
fn index(addr: u32) -> usize {
    ((addr >> 2) as usize) & (INSTRUCTION_CACHE_LINE_COUNT - 1)
}

/// Word offset within the long-word line: address bit 1.
#[inline]
fn word_sel(addr: u32) -> usize {
    ((addr >> 1) & 1) as usize
}

/// Tag key: address bits [31:8] with the FC2 bit folded into bit 0.
#[inline]
fn key(addr: u32, fc2: bool) -> u32 {
    ((addr >> 8) << 1) | u32::from(fc2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miss_then_hit_after_fill() {
        let mut c = ICache::new();
        assert_eq!(c.lookup(0x1000, false), None);
        c.fill(0x1000, false, 0xBEEF);
        assert_eq!(c.lookup(0x1000, false), Some(0xBEEF));
    }

    #[test]
    fn two_words_share_one_line() {
        // 0x1000 and 0x1002 are the two words of the same long word.
        let mut c = ICache::new();
        c.fill(0x1000, false, 0xAAAA);
        c.fill(0x1002, false, 0x5555);
        assert_eq!(c.lookup(0x1000, false), Some(0xAAAA));
        assert_eq!(c.lookup(0x1002, false), Some(0x5555));
    }

    #[test]
    fn fc2_disambiguates_user_and_supervisor() {
        let mut c = ICache::new();
        c.fill(0x1000, false, 0x1111);
        // Same address, supervisor program space — a user fill must
        // never be served as a supervisor hit (no aliasing).
        assert_eq!(c.lookup(0x1000, true), None);
        // Supervisor and user lines at the same address share the
        // direct-mapped index but differ in tag, so the supervisor
        // fill evicts the user line (a conflict, not coexistence).
        c.fill(0x1000, true, 0x2222);
        assert_eq!(c.lookup(0x1000, true), Some(0x2222));
        assert_eq!(c.lookup(0x1000, false), None);
    }

    #[test]
    fn conflicting_tag_evicts_the_line() {
        let mut c = ICache::new();
        // 0x1000 and 0x1100 share index (bits [7:2]) but differ in tag.
        assert_eq!(index(0x1000), index(0x1100));
        c.fill(0x1000, false, 0xDEAD);
        c.fill(0x1100, false, 0xC0DE);
        assert_eq!(c.lookup(0x1000, false), None);
        assert_eq!(c.lookup(0x1100, false), Some(0xC0DE));
    }

    #[test]
    fn clear_invalidates_everything() {
        let mut c = ICache::new();
        c.fill(0x1000, false, 0x1234);
        c.fill(0x2000, true, 0x5678);
        c.clear();
        assert_eq!(c.lookup(0x1000, false), None);
        assert_eq!(c.lookup(0x2000, true), None);
    }

    #[test]
    fn clear_entry_invalidates_only_its_index() {
        let mut c = ICache::new();
        c.fill(0x1000, false, 0x1234);
        c.fill(0x1004, false, 0x5678); // different index
        assert_ne!(index(0x1000), index(0x1004));
        c.clear_entry(0x1000);
        assert_eq!(c.lookup(0x1000, false), None);
        assert_eq!(c.lookup(0x1004, false), Some(0x5678));
    }

    #[test]
    fn diagnostic_lines_preserve_index_tag_words_and_validity() {
        let mut cache = ICache::new();
        cache.fill(0x1000, true, 0xAAAA);
        cache.fill(0x1002, true, 0x5555);
        cache.fill(0x1004, false, 0x1234);

        let lines = cache.diagnostic_lines();

        assert!(
            lines
                .iter()
                .enumerate()
                .all(|(index, line)| usize::from(line.index) == index),
        );
        assert_eq!(
            lines[0],
            ICacheLineDiagnosticSnapshot {
                index: 0,
                tag: key(0x1000, true),
                words: [0xAAAA, 0x5555],
                valid: [true, true],
            },
        );
        assert_eq!(
            lines[1],
            ICacheLineDiagnosticSnapshot {
                index: 1,
                tag: key(0x1004, false),
                words: [0x1234, 0],
                valid: [true, false],
            },
        );
        assert_eq!(
            lines[INSTRUCTION_CACHE_LINE_COUNT - 1],
            ICacheLineDiagnosticSnapshot {
                index: (INSTRUCTION_CACHE_LINE_COUNT - 1) as u8,
                tag: 0,
                words: [0, 0],
                valid: [false, false],
            },
        );
    }
}

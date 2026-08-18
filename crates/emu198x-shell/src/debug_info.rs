//! Debug198x sidecar import: symbols and source lines for an Asm198x build.
//!
//! A `.debug198x` file is the NDJSON sidecar Asm198x writes beside an image
//! (`--debug`). It carries the section bases, the symbol table, and a
//! line→byte-range map. Loading one turns the debug surface from bare
//! addresses into named locations and source lines: `disasm` annotates each
//! instruction with the label at its address and the line that produced it,
//! and a breakpoint can be set on `file:line` rather than on a hex address.
//!
//! The dependency direction is one-way and must stay that way: Asm198x writes
//! the sidecar, Emu198x reads it, and the shared `debug198x` crate depends on
//! serde alone — no parser, engine, or dialects. Nothing here may reach back
//! into the assembler.
//!
//! # Two kinds of symbolisation
//!
//! Each instruction is **annotated** with the label defined at its address and
//! the source line that produced it, and its operands are **substituted**, so
//! `JSR $C012` reads `JSR init`. [`DebugSymbols::symbolise`] does the latter
//! and is deliberately narrow about it: the input is text from four different
//! disassemblers rather than a structured operand, so only unambiguous
//! four-digit address literals are rewritten, never immediates or index
//! displacements, and only against labels — a constant that happens to equal
//! an address cannot rename it. See that method for the full rule.
//!
//! # Relocatable sections
//!
//! Absolutely-located builds (C64, Spectrum) carry their address in the
//! sidecar's own section `base`, so they resolve with no help. Relocatable
//! ones — Amiga hunks — are placed by the loader at run time, so the consumer
//! supplies the real address with [`DebugSymbols::set_section_base`]. That
//! override is what [`debug198x::BaseMap`] exists for.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use debug198x::{BaseMap, DebugInfo, FORMAT, FORMAT_VERSION, Header, SectionId, Space};
use serde::{Deserialize, Serialize};

/// Why a sidecar could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum DebugInfoError {
    /// The file could not be read.
    #[error("reading debug info {path}: {source}")]
    Io {
        /// Path we tried to read.
        path: PathBuf,
        /// Underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// A record was not valid JSON, or was malformed.
    #[error("parsing debug info {path}: {source}")]
    Parse {
        /// Path being parsed.
        path: PathBuf,
        /// Underlying record error, carrying the offending line number.
        #[source]
        source: debug198x::ReadError,
    },
    /// The header's `format` was not `debug198x`.
    ///
    /// Worth its own variant: NDJSON is a common enough container that another
    /// tool's line-per-record file will parse cleanly and then answer every
    /// lookup with `None`, which reads as "this build has no symbols" rather
    /// than "this is the wrong file".
    #[error("{path} is not a debug198x file (format {format:?})")]
    NotDebug198x {
        /// Path being read.
        path: PathBuf,
        /// The `format` string actually found.
        format: String,
    },
    /// The sidecar's format version is one this build does not understand.
    #[error("{path} is debug198x format {found}, this build reads {expected}")]
    UnsupportedVersion {
        /// Path being read.
        path: PathBuf,
        /// Version found in the header.
        found: String,
        /// Version this build was compiled against.
        expected: &'static str,
    },
}

/// A source location: the file and 1-based line that produced some bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLine {
    /// Source file name, exactly as the assembler recorded it.
    pub file: String,
    /// 1-based line number within that file.
    pub line: u32,
}

/// One hardware slot and what is paged into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PagedSlot {
    /// Hardware slot number, as the sidecar's `space` names it.
    slot: u8,
    /// Memory page currently in that slot.
    page: u16,
    /// CPU address the slot starts at.
    base: u64,
    /// Size of the slot's window, in bytes.
    size: u64,
}

impl PagedSlot {
    fn contains(self, addr: u64) -> bool {
        addr >= self.base && addr - self.base < self.size
    }
}

/// A loaded `.debug198x` sidecar, plus any section-base overrides.
#[derive(Clone, Debug)]
pub struct DebugSymbols {
    info: DebugInfo,
    bases: BaseMap,
    /// Live paging state, when the consumer is a banked machine. Empty for
    /// flat and relocatable images, which use `bases` alone.
    slots: Vec<PagedSlot>,
    path: PathBuf,
}

impl DebugSymbols {
    /// Reads and validates a `.debug198x` sidecar from disk.
    ///
    /// # Errors
    ///
    /// Returns [`DebugInfoError`] if the file cannot be read, does not parse,
    /// is not a debug198x file, or declares an unsupported format version.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, DebugInfoError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| DebugInfoError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_ndjson(&text, path)
    }

    /// Parses and validates a sidecar already in memory. `path` is used for
    /// error messages and for [`DebugSymbols::path`].
    ///
    /// # Errors
    ///
    /// As [`DebugSymbols::load`], minus the I/O case.
    pub fn from_ndjson(ndjson: &str, path: impl Into<PathBuf>) -> Result<Self, DebugInfoError> {
        let path = path.into();
        let info = DebugInfo::read(ndjson).map_err(|source| DebugInfoError::Parse {
            path: path.clone(),
            source,
        })?;

        if info.header.format != FORMAT {
            return Err(DebugInfoError::NotDebug198x {
                path,
                format: info.header.format,
            });
        }
        // The format is pre-1.0, so an exact match is the honest check: there
        // is no compatibility promise yet to lean on, and answering lookups
        // from a file we may be misreading is worse than refusing it.
        if info.header.format_version != FORMAT_VERSION {
            return Err(DebugInfoError::UnsupportedVersion {
                path,
                found: info.header.format_version,
                expected: FORMAT_VERSION,
            });
        }

        Ok(Self {
            info,
            bases: BTreeMap::new(),
            slots: Vec::new(),
            path,
        })
    }

    /// Overrides where a section actually loaded, for relocatable images.
    ///
    /// Absolutely-located builds never need this: the sidecar already knows
    /// its base. Amiga hunks do — the loader chooses the address at run time,
    /// and until it is supplied, every lookup into that section returns
    /// `None` rather than a wrong answer.
    pub fn set_section_base(&mut self, section: SectionId, base: u64) {
        self.bases.insert(section, base);
    }

    /// Replaces the whole base map with the sections currently mapped.
    ///
    /// This is how a banked machine states its paging: the base map *is* the
    /// paging state, so a bank that has paged out must stop being mapped, not
    /// merely be overwritten. [`DebugSymbols::set_section_base`] accumulates,
    /// which cannot express that — set two banks that share a slot and the
    /// lookups answer by record order, describing a machine that cannot
    /// exist. Rebuild the map on every paging change and that state is
    /// unreachable.
    ///
    /// A page the image has no code in simply is not in `mapped`, so lookups
    /// into it decline to answer rather than answering from another bank.
    pub fn set_paging(&mut self, mapped: impl IntoIterator<Item = (SectionId, u64)>) {
        self.bases = mapped.into_iter().collect();
    }

    /// Adopts a machine's live slot→page state as the paging state.
    ///
    /// Each entry is `(slot, page, base, size)`: a hardware slot, the memory
    /// page currently in it, the CPU address the slot starts at, and how wide
    /// the window is. Sections are matched by their `space`, which is why the
    /// format puts one on a section — otherwise a consumer must scrape the
    /// page out of some symbol, and a section holding only line records has
    /// none to scrape.
    ///
    /// A page that is not named is not mapped, so a paged-out bank contributes
    /// no address and cannot answer. Pass the whole state each time; this
    /// replaces rather than accumulates.
    ///
    /// # Aliased banks
    ///
    /// One bank can be live at two CPU addresses at once — a Spectrum 128 keeps
    /// bank 5 at `$4000` permanently *and* can select it for `$C000` — so a
    /// caller describing the whole machine names page 5 twice.
    ///
    /// That is supported, not merely reported. A [`BaseMap`] holds one base per
    /// section and so cannot express it, so address lookups do not use a single
    /// map: each resolves through the slot that actually contains the address.
    /// Every CPU address lies in exactly one slot, so `symbol_at` and `line_at`
    /// stay unambiguous however many windows a bank appears in.
    ///
    /// Going the other way is genuinely multi-valued — an aliased bank's symbol
    /// has two addresses — so [`DebugSymbols::addrs_of`] returns all of them
    /// and [`DebugSymbols::addr_of`] returns the lowest.
    ///
    /// Callers pass plain integers, so nothing here requires naming a
    /// `debug198x` type to describe a machine's own paging.
    pub fn set_paging_from_slots(&mut self, slots: impl IntoIterator<Item = (u8, u16, u64, u64)>) {
        self.slots = slots
            .into_iter()
            .map(|(slot, page, base, size)| PagedSlot {
                slot,
                page,
                base,
                size,
            })
            .collect();
        // Kept coherent for the flat path and for name lookups: the lowest
        // base wins, so the result does not depend on the order a caller
        // happened to list its slots.
        self.bases = BaseMap::new();
        for slot in &self.slots {
            for section in self.sections_in(*slot) {
                self.bases
                    .entry(section)
                    .and_modify(|base| *base = (*base).min(slot.base))
                    .or_insert(slot.base);
            }
        }
    }

    /// Section ids whose `space` names the page this slot currently holds.
    ///
    /// Matched on the **page**, not on the section's own `slot`. That is the
    /// spec's consumer model — "for each slot, take the sections whose `space`
    /// names the page currently in it" — and it is the only join that
    /// describes real hardware: a bank of code belongs to a page, while which
    /// slot it appears in is a fact about right now. A section's `space.slot`
    /// records where the assembler expected it, and matching on it would make
    /// a bank paged somewhere else invisible, and a bank live in two slots
    /// impossible.
    fn sections_in(&self, live: PagedSlot) -> Vec<SectionId> {
        self.info
            .sections
            .iter()
            .filter(|section| {
                matches!(section.space, Some(Space::Paged { page, .. }) if page == live.page)
            })
            .map(|section| section.id)
            .collect()
    }

    /// The base map to resolve `addr` through: the sections of the one slot
    /// that contains it, or the flat map when no paging state is set.
    ///
    /// This is what makes an aliased bank work. Mapping every live section at
    /// once cannot represent a bank in two windows; resolving per address
    /// never has to, because an address is only ever in one of them.
    fn bases_at(&self, addr: u32) -> Option<BaseMap> {
        let addr = u64::from(addr);
        let live = self.slots.iter().find(|slot| slot.contains(addr))?;
        Some(
            self.sections_in(*live)
                .into_iter()
                .map(|section| (section, live.base))
                .collect(),
        )
    }

    /// The sidecar's header — producing tool, CPU, dialect, source files.
    #[must_use]
    pub fn header(&self) -> &Header {
        &self.info.header
    }

    /// Path this sidecar was loaded from.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Section, symbol, and line-span counts, for reporting what was loaded.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize) {
        (
            self.info.sections.len(),
            self.info.symbols.len(),
            self.info.lines.len(),
        )
    }

    /// Sections carrying a `space` shape this build does not understand.
    ///
    /// Since asm198x#78, an unrecognised qualifier is carried as an opaque
    /// value rather than failing the read — which is what stops a newer
    /// sidecar taking down symbolisation entirely. The cost is that the
    /// failure went quiet: such a section matches no live page, so it never
    /// maps, and its symbols simply never resolve. A typo in a producer's
    /// qualifier now looks exactly like a bank that is paged out.
    ///
    /// Nothing in the format obliges a consumer to notice, so this exists to
    /// make it noticeable. A caller seeing no symbols where it expected some
    /// can ask, and get "this section describes itself in a way this build
    /// cannot read" instead of silence.
    #[must_use]
    pub fn unreadable_spaces(&self) -> Vec<SectionId> {
        self.info
            .sections
            .iter()
            .filter(|section| matches!(section.space, Some(Space::Unknown(_))))
            .map(|section| section.id)
            .collect()
    }

    /// The name of the symbol *exactly at* `addr`, if any.
    ///
    /// Exact-match by design: this labels the head of a routine or data item,
    /// which is what a disassembly listing wants. A nearest-preceding search
    /// would attach `start+7` to every instruction in the program.
    #[must_use]
    pub fn symbol_at(&self, addr: u32) -> Option<&str> {
        let paged = self.bases_at(addr);
        let bases = paged.as_ref().unwrap_or(&self.bases);
        self.info
            .symbol_at(u64::from(addr), Some(bases))
            .map(|sym| sym.name.as_str())
    }

    /// The address of a named symbol, or the value of a named constant.
    ///
    /// A bank that is live in two slots at once gives its symbols two
    /// addresses; this returns the lowest. Use [`DebugSymbols::addrs_of`] when
    /// every window matters.
    #[must_use]
    pub fn addr_of(&self, name: &str) -> Option<u32> {
        self.addrs_of(name).first().copied()
    }

    /// Every address a name is currently live at, ascending.
    ///
    /// Usually one. A bank paged into two slots at once — bank 5 on a Spectrum
    /// 128, permanently at `$4000` and selectable for `$C000` — puts its
    /// symbols at both, and a debugger that showed only one would be wrong
    /// about the machine rather than merely terse.
    ///
    /// Empty when the name is unknown, or when its section is not currently
    /// mapped anywhere.
    #[must_use]
    pub fn addrs_of(&self, name: &str) -> Vec<u32> {
        let resolve = |bases: &BaseMap| -> Option<u32> {
            self.info
                .addr_of(name, Some(bases))
                .and_then(|addr| u32::try_from(addr).ok())
        };

        let mut found: Vec<u32> = Vec::new();
        if self.slots.is_empty() {
            found.extend(resolve(&self.bases));
        } else {
            for live in &self.slots {
                let bases: BaseMap = self
                    .sections_in(*live)
                    .into_iter()
                    .map(|section| (section, live.base))
                    .collect();
                // A constant needs no section mapped, so an empty map would
                // report it once for every slot.
                if !bases.is_empty() {
                    found.extend(resolve(&bases));
                }
            }
            if found.is_empty() {
                // Constants are not section-relative, so they survive here.
                found.extend(resolve(&BaseMap::new()));
            }
        }

        found.sort_unstable();
        found.dedup();
        found
    }

    /// The source line that produced the byte at `addr`.
    #[must_use]
    pub fn line_at(&self, addr: u32) -> Option<SourceLine> {
        let paged = self.bases_at(addr);
        let bases = paged.as_ref().unwrap_or(&self.bases);
        self.info
            .line_at(u64::from(addr), Some(bases))
            .map(|span| SourceLine {
                file: span.file.clone(),
                line: span.line,
            })
    }

    /// The lowest address produced by `file` line `line` — the address to put
    /// a source-line breakpoint on.
    ///
    /// The inverse of [`DebugSymbols::line_at`], which the sidecar format does
    /// not provide directly. A line can emit more than one span (a macro used
    /// twice, a repeated block); the lowest address is the first place control
    /// reaches, which is what "break here" means.
    ///
    /// A line that emitted no bytes — blank, comment, or pure directive — has
    /// no address and returns `None`. Deliberately no search forward to the
    /// next line that did: a breakpoint that silently lands somewhere other
    /// than where it was asked for is worse than one that reports it cannot be
    /// set, and the caller can walk forward itself if it wants that policy.
    #[must_use]
    pub fn addr_of_line(&self, file: &str, line: u32) -> Option<u32> {
        self.info
            .lines
            .iter()
            .filter(|span| span.line == line && file_matches(&span.file, file))
            .filter_map(|span| self.absolute(span.section, span.offset))
            .min()
    }

    /// Rewrites 16-bit address literals in a disassembled instruction into the
    /// labels defined at them: `JSR $C012` becomes `JSR init`.
    ///
    /// Deliberately narrow, because the input is text produced by four
    /// different disassemblers rather than a structured operand:
    ///
    /// - Only four-digit `$XXXX` literals are considered. Two-digit ones are
    ///   ambiguous — `!byte $05` is data, not an address — and zero-page
    ///   *labels* (as opposed to constants) are vanishingly rare, so the
    ///   trade is worth making in the safe direction.
    /// - `#$XX` immediates are never touched. An immediate is a value, and
    ///   substituting there is how `LDA #$05` turns into a symbol because
    ///   something unrelated was equated to 5.
    /// - `+$XX` / `-$XX` index displacements are never touched: they are
    ///   offsets from a register, not addresses.
    /// - Only labels and entry points match. [`DebugSymbols::symbol_at`] does
    ///   not resolve constants, so a constant that happens to equal an address
    ///   cannot rename it.
    ///
    /// The effect is that a substitution happens only when a label is defined
    /// at exactly the address the instruction refers to — which is precisely
    /// when it is the right name for it.
    #[must_use]
    pub fn symbolise(&self, text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$'
                && !matches!(
                    i.checked_sub(1).map(|p| bytes[p]),
                    Some(b'#') | Some(b'+') | Some(b'-')
                )
                && let Some(name) = self.label_for_literal(&bytes[i + 1..])
            {
                out.push_str(name);
                i += 1 + 4; // the '$' and its four hex digits
                continue;
            }
            // Not a substitution site: copy this character across whole, so
            // multi-byte characters survive.
            let ch = text[i..].chars().next().expect("index is on a boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    }

    /// The label at the address spelled by exactly four hex digits at the
    /// start of `after_dollar`, if there is one.
    fn label_for_literal(&self, after_dollar: &[u8]) -> Option<&str> {
        let digits = after_dollar.get(..4)?;
        if !digits.iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        // A fifth hex digit means this is not a 16-bit literal, and taking the
        // first four of it would name the wrong address.
        if after_dollar.get(4).is_some_and(u8::is_ascii_hexdigit) {
            return None;
        }
        let text = std::str::from_utf8(digits).ok()?;
        let addr = u32::from_str_radix(text, 16).ok()?;
        self.symbol_at(addr)
    }

    /// Absolute address of a section-relative location, via the override map
    /// and then the section's own base.
    ///
    /// `debug198x` keeps its equivalent private, and the public lookups do not
    /// cover the line→address direction, so the resolution is repeated here.
    fn absolute(&self, section: SectionId, offset: u64) -> Option<u32> {
        let base = self.bases.get(&section).copied().or_else(|| {
            self.info
                .sections
                .iter()
                .find(|s| s.id == section)
                .and_then(|s| s.base)
        })?;
        u32::try_from(base.checked_add(offset)?).ok()
    }
}

/// Whether a sidecar's recorded file name refers to the same file the caller
/// named.
///
/// The assembler records paths as they were given on its command line, so a
/// sidecar can hold `test-data/c64/border-walk.s` while a debugger UI — which
/// has the file open, not the build script — asks about `border-walk.s`. Both
/// name the same file. Matching the full string first keeps two same-named
/// files in different directories distinguishable whenever the caller supplies
/// enough path to tell them apart.
fn file_matches(recorded: &str, wanted: &str) -> bool {
    if recorded == wanted {
        return true;
    }
    let base = |s: &str| {
        s.rsplit(['/', '\\'])
            .next()
            .unwrap_or(s)
            .to_ascii_lowercase()
    };
    base(recorded) == base(wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed C64 fixture, written by a real `asm198x --debug` run.
    /// Hand-authoring a sidecar would test this reader against my idea of the
    /// format rather than against what the writer emits.
    const FIXTURE: &str =
        include_str!("../../../test-data/commodore/c64/debug198x/border-walk.debug198x");

    fn fixture() -> DebugSymbols {
        DebugSymbols::from_ndjson(FIXTURE, "border-walk.debug198x").expect("fixture loads")
    }

    #[test]
    fn reads_the_header_of_a_real_asm198x_sidecar() {
        let sym = fixture();
        assert_eq!(sym.header().cpu, "6502");
        assert_eq!(sym.header().dialect, "acme");
        assert_eq!(sym.header().tool, "asm198x");
        // 1 section, 5 symbols (border/start/loop/done/counter), 9 line spans.
        assert_eq!(sym.counts(), (1, 5, 9));
    }

    #[test]
    fn resolves_labels_through_the_sections_own_base() {
        let sym = fixture();
        // The section is based at $C000, so offset 5 is $C005.
        assert_eq!(sym.addr_of("start"), Some(0xc000));
        assert_eq!(sym.addr_of("loop"), Some(0xc005));
        assert_eq!(sym.addr_of("counter"), Some(0xc013));
        assert_eq!(sym.addr_of("nosuch"), None);
    }

    #[test]
    fn a_constant_resolves_to_its_value_not_an_address() {
        let sym = fixture();
        assert_eq!(sym.addr_of("border"), Some(0xd020));
        // …and a constant is not a location, so nothing is "at" $d020.
        assert_eq!(sym.symbol_at(0xd020), None);
    }

    #[test]
    fn names_the_symbol_exactly_at_an_address() {
        let sym = fixture();
        assert_eq!(sym.symbol_at(0xc005), Some("loop"));
        // One byte into the instruction at `loop` is not `loop`.
        assert_eq!(sym.symbol_at(0xc006), None);
    }

    #[test]
    fn maps_an_address_to_the_line_that_produced_it() {
        let sym = fixture();
        // `sta border` is line 18, three bytes at $c00b.
        let at_start = sym.line_at(0xc00b).expect("line at instruction start");
        assert_eq!(at_start.line, 18);
        assert!(at_start.file.ends_with("border-walk.s"));
        // Mid-instruction addresses belong to the same line: the span covers
        // the whole encoding, which is what makes "which line is executing?"
        // answerable from any address.
        assert_eq!(sym.line_at(0xc00d).map(|l| l.line), Some(18));
        assert_eq!(sym.line_at(0xc00e).map(|l| l.line), Some(19));
    }

    #[test]
    fn maps_a_source_line_back_to_a_breakpoint_address() {
        let sym = fixture();
        assert_eq!(
            sym.addr_of_line("test-data/commodore/c64/debug198x/border-walk.s", 18),
            Some(0xc00b)
        );
        // The round trip holds for every line the assembler recorded.
        for span in [13u32, 14, 16, 17, 18, 19, 20, 22, 24] {
            let addr = sym.addr_of_line("border-walk.s", span).expect("has code");
            assert_eq!(sym.line_at(addr).map(|l| l.line), Some(span));
        }
    }

    #[test]
    fn a_line_that_emitted_no_bytes_has_no_breakpoint_address() {
        let sym = fixture();
        // Line 15 is `loop:` on its own — a label, no encoded bytes.
        assert_eq!(sym.addr_of_line("border-walk.s", 15), None);
        // Line 1 is a comment.
        assert_eq!(sym.addr_of_line("border-walk.s", 1), None);
    }

    #[test]
    fn matches_the_file_by_basename_when_the_caller_has_no_path() {
        let sym = fixture();
        // The sidecar records the assembler's invocation-relative path; a
        // debugger asking about the open file knows only its name.
        assert_eq!(sym.addr_of_line("border-walk.s", 18), Some(0xc00b));
        assert_eq!(sym.addr_of_line("other.s", 18), None);
    }

    #[test]
    fn an_override_relocates_every_lookup_in_that_section() {
        let mut sym = fixture();
        // Stand in for an Amiga hunk placed somewhere else at load time.
        sym.set_section_base(0, 0x2_0000);
        assert_eq!(sym.addr_of("loop"), Some(0x2_0005));
        assert_eq!(sym.symbol_at(0x2_0005), Some("loop"));
        assert_eq!(sym.addr_of_line("border-walk.s", 18), Some(0x2_000b));
        // The old base is no longer meaningful.
        assert_eq!(sym.symbol_at(0xc005), None);
        // A constant is not section-relative, so relocation leaves it alone.
        assert_eq!(sym.addr_of("border"), Some(0xd020));
    }

    /// A header record with every required field, so a test can vary one
    /// field without tripping the parser on a missing one first.
    fn header_line(format: &str, version: &str) -> String {
        format!(
            r#"{{"t":"header","format":"{format}","format_version":"{version}",
               "tool":"other","tool_version":"1.0","cpu":"6502","dialect":"acme",
               "sources":[]}}"#
        )
        .replace('\n', "")
    }

    #[test]
    fn substitutes_a_label_for_an_address_operand() {
        let sym = fixture();
        // The issue's example, on this fixture's addresses: a branch to the
        // loop head reads as the label.
        assert_eq!(sym.symbolise("BNE $C005"), "BNE loop");
        assert_eq!(sym.symbolise("JMP $C000"), "JMP start");
        assert_eq!(sym.symbolise("STA $C013"), "STA counter");
        // Indexed and indirect forms keep their suffixes.
        assert_eq!(sym.symbolise("LDA $C013,X"), "LDA counter,X");
        assert_eq!(sym.symbolise("JMP ($C000)"), "JMP (start)");
    }

    #[test]
    fn leaves_an_address_with_no_label_alone() {
        let sym = fixture();
        // $C001 is mid-instruction — no label is defined there.
        assert_eq!(sym.symbolise("JSR $C001"), "JSR $C001");
    }

    #[test]
    fn never_substitutes_into_an_immediate() {
        let sym = fixture();
        // The failure this rule exists to prevent: an immediate is a value,
        // not an address, and must survive even when its digits match one.
        assert_eq!(sym.symbolise("LDA #$C005"), "LDA #$C005");
        assert_eq!(sym.symbolise("LDA #$05"), "LDA #$05");
    }

    #[test]
    fn never_substitutes_a_constant_for_an_address() {
        let sym = fixture();
        // `border` is a constant equal to $D020. A constant is not a location,
        // so an instruction referring to $D020 is not referring to `border`'s
        // address — there isn't one. Left alone.
        assert_eq!(sym.addr_of("border"), Some(0xd020));
        assert_eq!(sym.symbolise("STA $D020"), "STA $D020");
    }

    #[test]
    fn never_substitutes_into_a_displacement_or_a_data_byte() {
        let sym = fixture();
        // Z80 index displacements are offsets from a register.
        assert_eq!(sym.symbolise("LD A,(IX+$05)"), "LD A,(IX+$05)");
        assert_eq!(sym.symbolise("LD A,(IY-$05)"), "LD A,(IY-$05)");
        // Two-digit literals are not treated as addresses at all.
        assert_eq!(sym.symbolise("!byte $05"), "!byte $05");
    }

    #[test]
    fn does_not_mistake_part_of_a_longer_literal_for_an_address() {
        let sym = fixture();
        // A 24-bit literal's first four digits must not be read as a 16-bit
        // address — that would name a completely unrelated location.
        assert_eq!(sym.symbolise("LDA $C0051"), "LDA $C0051");
    }

    /// The Spectrum 128 banked fixture, copied from the Asm198x test corpus.
    /// Two labels sixteen bytes into two different banks, both mapped through
    /// slot 3 (`$C000`) — distinguished in the file *only* by `space.page`.
    const BANKED: &str = include_str!(
        "../../../test-data/sinclair/zx-spectrum-128/debug198x/spectrum128-banked.debug198x"
    );

    #[test]
    fn a_banked_section_resolves_nothing_until_its_slot_address_is_supplied() {
        let sym = DebugSymbols::from_ndjson(BANKED, "spectrum128-banked.debug198x")
            .expect("fixture loads");
        // Neither section carries a `base`: a bank has no fixed address until
        // it is paged into a slot. Nothing resolves, rather than resolving to
        // a guess.
        assert_eq!(sym.addr_of("draw"), None);
        assert_eq!(sym.addr_of("music"), None);
    }

    /// A banked section resolves correctly for whichever page is *in* the
    /// slot, because the base map is the paging state.
    ///
    /// The whole of a 128K machine's paging is expressed by which sections are
    /// mapped: a bank that is paged out has no base, so it contributes no
    /// address and cannot answer. Map only what is in the slot and `draw` and
    /// `music` — both at slot 3, offset 16 — stay distinct despite sharing an
    /// address, with no page-aware lookup needed.
    #[test]
    fn a_banked_symbol_resolves_by_which_page_is_in_the_slot() {
        let banked = || {
            DebugSymbols::from_ndjson(BANKED, "spectrum128-banked.debug198x")
                .expect("fixture loads")
        };

        // Page 1 in slot 3: bank1 is live, bank3 is not mapped at all.
        let mut page1 = banked();
        page1.set_section_base(0, 0xc000);
        assert_eq!(page1.symbol_at(0xc010), Some("draw"));
        assert_eq!(page1.line_at(0xc010).map(|l| l.line), Some(5));
        // The paged-out bank contributes nothing, rather than a wrong answer.
        assert_eq!(page1.addr_of("music"), None);

        // Page 3 in slot 3: the same address, the other answer.
        let mut page3 = banked();
        page3.set_section_base(1, 0xc000);
        assert_eq!(page3.symbol_at(0xc010), Some("music"));
        assert_eq!(page3.line_at(0xc010).map(|l| l.line), Some(12));
        assert_eq!(page3.addr_of("draw"), None);
    }

    #[test]
    fn set_paging_replaces_the_map_so_a_bank_can_page_out() {
        let mut sym = DebugSymbols::from_ndjson(BANKED, "spectrum128-banked.debug198x")
            .expect("fixture loads");

        sym.set_paging([(0, 0xc000)]);
        assert_eq!(sym.symbol_at(0xc010), Some("draw"));

        // Page bank3 in. Bank1 must *stop* resolving — with an insert-only
        // map it would linger and make the slot ambiguous.
        sym.set_paging([(1, 0xc000)]);
        assert_eq!(sym.symbol_at(0xc010), Some("music"));
        assert_eq!(sym.addr_of("draw"), None);

        // Nothing paged in: no answers, which is correct — without a paging
        // state the question is ambiguous.
        sym.set_paging([]);
        assert_eq!(sym.symbol_at(0xc010), None);
        assert_eq!(sym.line_at(0xc010), None);
    }

    /// Mapping two pages into one slot describes a machine that cannot exist,
    /// and the lookups answer by record order.
    ///
    /// Kept as a guard on [`DebugSymbols::set_paging`]: the reason that method
    /// replaces the map wholesale rather than accumulating is that an
    /// insert-only base map lets a caller build exactly this state and get a
    /// confidently wrong symbol. This is not a shortfall in the reader — it is
    /// what asking an impossible question returns.
    #[test]
    fn two_pages_in_one_slot_is_an_impossible_state() {
        let mut sym = DebugSymbols::from_ndjson(BANKED, "spectrum128-banked.debug198x")
            .expect("fixture loads");
        sym.set_section_base(0, 0xc000);
        sym.set_section_base(1, 0xc000);
        // Record order decides, which is why callers should use `set_paging`.
        assert_eq!(sym.symbol_at(0xc010), Some("draw"));
    }

    /// The Amiga fixture: two hunks, neither carrying a base, which is the
    /// genuinely relocatable case a loader places at run time.
    const AMIGA: &str =
        include_str!("../../../test-data/commodore/amiga/debug198x/68000-amiga.debug198x");

    fn amiga() -> DebugSymbols {
        DebugSymbols::from_ndjson(AMIGA, "68000-amiga.debug198x").expect("fixture loads")
    }

    #[test]
    fn relocatable_hunks_resolve_nowhere_until_the_loader_says_where() {
        let sym = amiga();
        // Neither section has a `base`: where a hunk lands is not knowable at
        // assembly time. Every lookup declines rather than guessing.
        for name in ["start", "loop", "data", "msg", "tail"] {
            assert_eq!(sym.addr_of(name), None, "{name} has nowhere to be yet");
        }
        assert_eq!(sym.symbol_at(0), None);
        assert_eq!(sym.line_at(0), None);
    }

    #[test]
    fn supplying_hunk_addresses_resolves_both_sections_independently() {
        // #741's third acceptance bullet, against the fixture built for it
        // rather than a synthetic rebase of an absolutely-located image.
        //
        // The two hunks are placed independently and out of order — data below
        // code — because AmigaDOS hands back whatever memory it has, and a
        // consumer that quietly assumed hunks were adjacent or ascending would
        // pass a test that placed them tidily.
        let mut sym = amiga();
        const CODE: u64 = 0x0021_0000;
        const DATA: u64 = 0x0007_C000;
        sym.set_section_base(0, CODE);
        sym.set_section_base(1, DATA);

        assert_eq!(sym.addr_of("start"), Some(0x0021_0000));
        assert_eq!(sym.addr_of("loop"), Some(0x0021_0006));
        assert_eq!(sym.addr_of("data"), Some(0x0007_C000));
        assert_eq!(sym.addr_of("msg"), Some(0x0007_C006));
        assert_eq!(sym.addr_of("tail"), Some(0x0007_C00A));

        // Reverse lookups agree, in both hunks.
        assert_eq!(sym.symbol_at(0x0021_0006), Some("loop"));
        assert_eq!(sym.symbol_at(0x0007_C006), Some("msg"));

        // …and so do the line spans, which is what a source-line breakpoint
        // in a relocatable program depends on.
        assert_eq!(sym.line_at(0x0021_0000).map(|l| l.line), Some(2));
        assert_eq!(sym.line_at(0x0021_0006).map(|l| l.line), Some(4));
        assert_eq!(sym.line_at(0x0007_C000).map(|l| l.line), Some(7));
        assert_eq!(sym.line_at(0x0007_C00A).map(|l| l.line), Some(10));
        assert_eq!(
            sym.addr_of_line("68000-amiga.s", 8),
            Some(0x0007_C006),
            "a breakpoint on a line in the data hunk"
        );
    }

    #[test]
    fn a_second_load_relocates_everything_rather_than_accumulating() {
        // The same binary run twice lands somewhere else. `set_paging` replaces
        // the map wholesale, so the previous run's addresses cannot linger and
        // answer for a program that is no longer there.
        let mut sym = amiga();
        sym.set_section_base(0, 0x0021_0000);
        sym.set_section_base(1, 0x0007_C000);
        assert_eq!(sym.addr_of("start"), Some(0x0021_0000));

        sym.set_paging([(0, 0x0040_0000), (1, 0x0041_0000)]);
        assert_eq!(sym.addr_of("start"), Some(0x0040_0000));
        assert_eq!(sym.addr_of("msg"), Some(0x0041_0006));
        assert_eq!(sym.symbol_at(0x0021_0000), None, "the old load is gone");
    }

    /// An unknown `space` shape is carried, not fatal — and is reportable.
    ///
    /// It used to fail the whole file: `Space` was an untagged enum, so a
    /// shape serde did not recognise matched no variant and the read errored,
    /// taking every lookup with it. This repo was on the receiving end of
    /// that, because its `debug198x` pin is a fixed revision and a sidecar
    /// from a newer Asm198x carries shapes it predates. asm198x#78 added the
    /// `Unknown` catch-all, so the test that pinned the old behaviour now
    /// pins the new.
    ///
    /// The trade is that a loud failure became a quiet one, so the section is
    /// asserted to be *reportable* too: degrading silently and degrading
    /// undetectably are different things.
    #[test]
    fn an_unknown_space_shape_is_carried_and_reportable() {
        let unknown = format!(
            "{}\n{}\n{}",
            header_line(FORMAT, FORMAT_VERSION),
            r#"{"t":"section","id":0,"name":"odd","space":{"segment":7,"window":2}}"#,
            r#"{"t":"symbol","name":"x","kind":"label","section":0,"offset":0}"#
        );
        let sym = DebugSymbols::from_ndjson(&unknown, "future-space.debug198x")
            .expect("an unrecognised space shape must not fail the file");
        assert_eq!(sym.counts(), (1, 1, 0), "the records survive");
        // It maps nowhere, because nothing can say which page it is…
        assert_eq!(sym.addr_of("x"), None);
        // …but a caller can find out why, rather than facing silence.
        assert_eq!(sym.unreadable_spaces(), vec![0]);
    }

    #[test]
    fn a_withdrawn_bank_shape_still_loads() {
        // `Space::Bank` was withdrawn from v1 as unexercised. A file carrying
        // one must still load — it becomes `Unknown` — so nothing already
        // written stops working.
        let banked = format!(
            "{}\n{}",
            header_line(FORMAT, FORMAT_VERSION),
            r#"{"t":"section","id":0,"name":"far","space":{"bank":126}}"#
        );
        let sym =
            DebugSymbols::from_ndjson(&banked, "bank.debug198x").expect("a withdrawn shape loads");
        assert_eq!(sym.unreadable_spaces(), vec![0]);
    }

    #[test]
    fn an_unknown_record_type_is_still_skipped() {
        let unknown_record = format!(
            "{}\n{}",
            header_line(FORMAT, FORMAT_VERSION),
            r#"{"t":"quantum","whatever":1}"#
        );
        let ok = DebugSymbols::from_ndjson(&unknown_record, "future-record.debug198x")
            .expect("an unknown record type is skipped");
        assert_eq!(ok.counts(), (0, 0, 0));
    }

    #[test]
    fn refuses_a_file_that_is_not_debug198x() {
        // A well-formed header that simply is not ours — the case that would
        // otherwise look like a build with no symbols in it.
        let err = DebugSymbols::from_ndjson(&header_line("something-else", "0.1"), "wrong.ndjson")
            .expect_err("must be rejected");
        assert!(
            matches!(err, DebugInfoError::NotDebug198x { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn refuses_a_format_version_it_does_not_understand() {
        let err = DebugSymbols::from_ndjson(&header_line(FORMAT, "9.9"), "future.debug198x")
            .expect_err("must be rejected");
        assert!(
            matches!(err, DebugInfoError::UnsupportedVersion { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn reports_the_line_number_of_a_malformed_record() {
        let err = DebugSymbols::from_ndjson("{not json}\n", "broken.debug198x")
            .expect_err("must be rejected");
        assert!(matches!(err, DebugInfoError::Parse { .. }), "got {err:?}");
    }
}

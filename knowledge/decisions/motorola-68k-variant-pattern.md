# Decision: Motorola 68k variant pattern

**Date:** 2026-05-21

## The decision

**Every member of the M68k family — 68000, 68010, 68020, and every variant we ever add — is a wrapper struct that holds a `Cpu68000` core, `Deref`s through it for the shared 68k bus / pipeline / micro-op state, and registers its ISA delta against the inner core through three narrow extension points: a decode hook, a small set of per-variant behaviour flags, and a continuation hook for multi-step instructions.**

The 68000 core itself never gains 68010+ semantics. Variants stack: `Cpu68020 → inner: Cpu68010 → inner: Cpu68000`. Each variant's `new()` installs its own hooks and flips its own flags; the higher variant inherits everything the lower variant already wired.

## Why this shape

Three constraints drove the design:

1. **The 68000 crate's doc-comment is binding**: "implements the 68000 ISA *only* — no 68010+ instructions, no caches, no FPU, no MMU." The 2026-04-29 strip removed `CpuModel`-flagged behaviour gates throughout the inner core, and we explicitly do not undo that.
2. **`#[forbid(unsafe_code)]`** at the workspace level rules out shared-mutable-state tricks and pointer aliasing.
3. **The decision must scale to ~6 variants** (68010 / 68020 / 68EC020 / 68030 / 68EC030 / 68LC030 / 68040 / 68EC040 / 68LC040 / 68060 / 68EC060 / 68LC060 / AC68080), each layering on the previous. Anything per-variant that requires modifying the 68000 core for every variant is wrong.

What we rejected:

- **Cloning the 68000 core** per variant — the 68000 is ~5,800 lines and cloning would force a fork-and-port cost per variant without buying anything until variant behaviour actually diverges.
- **`Box<dyn Fn>` trait objects** — function pointers (`Option<fn(...)>`) are sufficient, trivially `Copy`, and don't add heap allocation.
- **Resurrecting the runtime `CpuModel` capability flag** that the April-2026 strip removed.

## Layering rule: wrap, don't clone

Every variant wrapper is a Deref/DerefMut wrapper over the next layer down:

```rust
// crates/motorola-68010/src/cpu.rs
pub struct Cpu68010 { inner: Cpu68000 }

impl Cpu68010 {
    pub fn new() -> Self {
        let mut inner = Cpu68000::new();
        inner.variant_decode_hook = Some(decode_68010_opcode);
        inner.variant_continue_hook = Some(continue_68010_opcode);
        inner.variant_six_word_frame = true;
        inner.variant_musashi_bcd_v = true;
        inner.variant_musashi_div_overflow = true;
        Self { inner }
    }
}

impl Deref for Cpu68010 { type Target = Cpu68000; /* … */ }
impl DerefMut for Cpu68010 { /* … */ }
```

The 68020 wraps the 68010, not the 68000 directly. Its `new()` calls `Cpu68010::new()` first, then installs the 68020's own hooks and flips its own flags. Inherited state (every flag the 68010 set) carries through automatically.

Result: zero per-method forwarding code. Call sites that touch `cpu.regs`, `cpu.state`, `cpu.tick()`, `cpu.instr_start_pc`, etc. work transparently through the Deref chain.

## The three extension-point shapes on `Cpu68000`

### 1. Decode hook — whole new opcodes

```rust
pub variant_decode_hook: Option<fn(&mut Cpu68000, u16) -> bool>
```

Each of the 68010+/68020+ `ILLEGAL`-trap arms in `decode_and_execute` calls `self.try_variant_decode(opcode)` first. The hook returns `true` if it handled the opcode (advanced PC, set flags, queued any follow-up); the 68000 then skips its `ILLEGAL` trap. Returning `false` (or leaving the hook `None`) preserves pure-68000 behaviour.

**Chaining**: the 68020 hook handles its own opcodes (EXTB.L, MULL, DIVL, BF*) first, then explicitly falls through to `decode_68010_opcode(cpu, opcode)` for opcodes it doesn't override (MOVEC, MOVE-from-CCR, RTD). The chain is by-call, not by inheritance — each variant's hook function is responsible for delegating.

### 2. Behaviour flags — subtle deltas on existing instructions

`pub variant_<feature>_enable: bool` fields, all `#[serde(skip)]`, default `false`. Shared code paths in `motorola-68000` consult them. Each variant wrapper flips the relevant ones on in `new()`.

Current catalogue:

| Flag | Set by | What it does |
|---|---|---|
| `variant_scaled_index` | 68020 | Brief extension-word bits 10-9 encode `*1/*2/*4/*8` instead of being "don't care". `ea.rs` reads it. |
| `variant_six_word_frame` | 68010 + 68020 | `begin_group1_exception` pushes Format/Vector word above PC + SR (8-byte frame instead of 6-byte). |
| `variant_format2_vectors` | 68020 | Vectors 5 / 6 / 7 / 9 use Format `$2` 12-byte frame with extra Instruction-Address long. PRM § 8.6.3. |
| `variant_extended_sr_writes` | 68020 | SR write mask widens from `$A71F` to `$F71F` (allows M-flag bit 12). The four SR-write sites consult `Cpu68000::sr_write_mask()`. |
| `variant_musashi_bcd_v` | 68010 + 68020 | `bcd_add` / `bcd_sub` / `nbcd_op` report Musashi's "undefined V" instead of the SingleStepTests expectation. Each has both implementations as free functions. |
| `variant_musashi_div_overflow` | 68010 + 68020 | 16-bit `DIVU.W` / `DIVS.W` overflow path: Musashi sets only V; SingleStepTests clears C, sets V and preserves N/Z/X. The PRM specifies C clear and V set but leaves N/Z undefined. |

### 3. Continuation hook — multi-step instructions

```rust
pub variant_continue_hook: Option<fn(&mut Cpu68000) -> bool>
pub variant_pending_disp: u32   // generic stash for continuation state
```

Called at the top of `continue_instruction` before the 68000's match arms. Returning `true` bypasses the inner dispatch. Variants reserve `followup_tag` numbers in the 200+ range (the 68000 uses 0..≈80).

`variant_pending_disp` carries state across follow-up tag transitions — for `RTD` it holds the sign-extended `d16` consumed from the extension word, applied to SP after the PC pop completes.

**Inheritance**: the 68020 doesn't override the continue hook today, so it picks up the 68010's via the inner field. Once memory-EA versions of MULL / DIVL / BF / CHK2 / CAS land, the 68020 will install its own and chain to the 68010's.

## Catalogue of variant tags and pending stash fields

| Symbol | Crate | Used by |
|---|---|---|
| `TAG_RTD_PC_HI` (200) | motorola-68010 | RTD post-PopLongHi |
| `TAG_RTD_PC_LO` (201) | motorola-68010 | RTD post-PopLongLo |
| `variant_pending_disp` | motorola-68000 | RTD `d16`; future opcodes can repurpose |

The variant tag space starts at 200 to leave room: the 68000's own tags occupy `0..=80`-ish, and we may add more before 200 if the 68000 itself grows.

## Drift triggers

Stop and re-read this doc if you find yourself:

- **Adding a `Box<dyn ...>`, `Arc<Mutex<...>>`, or trait-object** to a 68k variant. The pattern is plain function pointers and `Deref`. Anything heavier suggests the wrong shape.
- **Modifying `motorola-68000/src/{cpu,decode,execute,ea}.rs`** to add a `match self.model { … }` arm. That's the 2026-04-29 strip coming back. Use a variant flag instead.
- **Defining a generic `Cpu68k<M: M68kVariant>`** to parameterise the core. The wrap-don't-clone shape is *not* generic over variant; each variant is its own struct. Generics here add complexity without buying variance — the variant set is fixed and small.
- **Cloning** the 68000 core inside a variant crate, or duplicating an instruction handler. Either chain through the existing decode hook or add a flag.
- **Tom Harte regresses against any of the three corpora** simultaneously. The corpus divide is SingleStepTests/680x0 versus Musashi-driven m68k-test-gen. When they disagree, preserve the difference explicitly rather than treating one software oracle as hardware truth — see `variant_musashi_bcd_v` for precedent.

## Forward look — 68030 / 68040 / 68060 / AC68080

The same shape carries through, with one anticipated generalisation.

- **68030** wraps 68020. **Wrapper landed 2026-05-22**; `Cpu68030` is a thin Deref wrapper over `Cpu68020` with no additional ISA delta configured yet. The m68k-test-gen 68030 corpus passes at 100% via inheritance alone. The MMU module (`motorola-68030/src/mmu.rs`, 2,421 lines, unused) waits on the decode-side wiring for PMOVE / PFLUSH / PTEST / PLOAD when an MMU-bearing machine arrives.
- **68040** wraps 68030. **Wrapper landed 2026-05-22**; same pattern. 100% on the m68k-test-gen 68040 corpus via inheritance. The FPU module (`motorola-68040/src/fpu.rs`, 705 lines, unused) waits on F-line cpID=1 dispatch. MOVE16 / CINV / CPUSH and the Format `$7` bus-error frame are deferred until exercised.
- **68060** is a new crate. ISA-wise it's a 68040 subset (no CALLM / RTM / CHK2 / CMP2) plus PCR. Superscalar dispatch is a cycle-accuracy concern, not an ISA one, so the variant pattern still applies for correctness; cycle-accurate superscalar is a separate (and large) decision.
- **AC68080** (Apollo Vampire, FPGA-implemented 68060-class) is a much wider departure: AMMX vector unit, 64-bit registers, instruction extensions. This may be the variant where wrap-don't-clone stops paying. Decision deferred until the Vampire roadmap firms up.

**Anticipated generalisation**: when the 68040 Format `$7` bus-error frame lands, `variant_six_word_frame: bool` will become an enum or per-vector dispatch table — local change, not a redesign.

**Lesson from the 68030 wrapper landing**: a latent 68020 bug surfaced — MOVEC to CACR / CAAR / MSP / ISP raised ILLEGAL on our wrapper but should succeed per Musashi. The 68020 corpus's random extension words across 10 fixtures never hit one of those 4 CRs (4/4096 probability per test); the 68030 corpus hit one. Fixed by extending the 68020 decode hook to handle MOVEC's 68020-additional CRs, chaining to the 68010 hook's helpers for the four 68010-basic CRs (`read_control_register` / `write_control_register` are now `pub`). Each variant's MOVEC handler is composed from the previous variant's helpers + its own CR-encoding deltas.

## Related

- [Motorola 68020 implementation plan](motorola-68020-implementation-plan.md) — the phased work that produced this pattern; final state has all three corpora at 100 %.
- [Amiga full-family architecture review](amiga-full-family-architecture-review.md) — Seam 2 frames the broader 68k-family completion work this pattern serves.
- [CPU bus interface](cpu-bus-interface.md) — the orthogonal rule that constrains the bus shape (pin-level fields, no `Bus` trait). Both rules apply simultaneously.

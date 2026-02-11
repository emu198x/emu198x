# Session Context

## User Prompts

### Prompt 1

Implement the following plan:

# New Amiga Emulator — emu-amiga2

## Context

The existing `emu-amiga` crate doesn't boot Kickstart and has accumulated workarounds (ExecBase checksum patches, library vector patches, reset-loop skips, `FORCE_CPU` env var) that mask fundamental architectural problems:

1. **Wrong CPU crate.** Uses `emu-68000` with byte-level `Bus` trait. The 68000 has a 16-bit data bus with function codes. The CPU never reads `ReadResult.wait` — there's no mechanism for bus co...

### Prompt 2

This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation.

Analysis:
Let me chronologically analyze the conversation:

1. The user provided a detailed implementation plan for creating a new `emu-amiga2` crate to replace the existing `emu-amiga` crate. The plan addresses three fundamental architectural problems:
   - Wrong CPU crate (emu-68000 with byte-level Bus trait instead of emu-m68k with word-level...

### Prompt 3

Run that for me. You'll find a Kickstart 1.0 ROM file in roms/kick10.rom.

### Prompt 4

This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation.

Analysis:
Let me chronologically analyze the conversation:

1. **Context from previous session**: The user had a plan to create `emu-amiga2` crate. Phase 1 was completed - all source files created, 55 tests passing, binary builds. The previous session created the full crate structure.

2. **User's first request**: "Run that for me. You'll find a...

### Prompt 5

This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation.

Analysis:
Let me chronologically analyze the conversation, which continues from a previous session that ran out of context.

**Previous Session Context (from summary):**
- User asked to run headless KS 1.0 boot test with `roms/kick10.rom`
- decode.rs was a STUB (every opcode triggered illegal instruction)
- User explicitly said: "Claude. I _do_ ...

### Prompt 6

This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation.

Analysis:
Let me chronologically analyze the conversation, which continues from a previous session that ran out of context.

**Previous Session Context (from summary):**
- User asked to run headless KS 1.0 boot test with `roms/kick10.rom`
- decode.rs was a STUB (every opcode triggered illegal instruction)
- User explicitly said: "Claude. I _do_ ...

### Prompt 7

Ok. You didn't address my concern.

### Prompt 8

Kill emu-68000. Then tell me exactly how you'll rewrite decode.rs; I think we need an on-disk plan to prevent us from any further context loss.

### Prompt 9

I think it might also be a good idea to write up our plan for dealing with all of the different Amiga variants that we discussed, or did you lose that too?

### Prompt 10

Ok. That seems reasonable for now. I believe your plans make sense.

### Prompt 11

This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation.

Analysis:
Let me chronologically analyze the conversation:

1. **Session Start**: This is a continuation from a previous conversation that ran out of context. The summary from the previous session describes extensive work on emu-amiga2 (a new Amiga emulator crate) with emu-m68k as the CPU. Key bugs were fixed: SkipExt double-advance in decode_mo...

### Prompt 12

[Request interrupted by user]

### Prompt 13

Ok, before you move on. We previously established that the single-step tests expect the first opcode to be prefetched

### Prompt 14

I don't think we had a solid approach; our first attempt in emu-68000 seemed to involve hacking a "prefetch_only" flag into every sodding method

### Prompt 15

[Request interrupted by user]

### Prompt 16

I'm not sure I trust `next_ext_word`
I'm honestly not sure I trust _any_ of the CPU code

### Prompt 17

Scrap emu-m68k as well. And I think - in a change to the previous name - we should be explicit and name it cpu-m68k

### Prompt 18

This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation.

Analysis:
Let me chronologically analyze the conversation:

1. **Session Start**: This is a continuation from a previous conversation that ran out of context. The summary describes extensive work on emu-amiga2 (a new Amiga emulator crate) with emu-m68k as the CPU. Key issues: KS 1.3 stuck in a fill loop, D1 had wrong value causing ~936K frames i...

### Prompt 19

[Request interrupted by user for tool use]


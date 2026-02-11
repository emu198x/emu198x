# Session Context

## User Prompts

### Prompt 1

Implement the following plan:

# cpu-m68k — 68000 CPU Core from Scratch

## Context

The `emu-m68k` crate has fundamental engine-level bugs that can't be fixed incrementally:

1. **Broken extension word pipeline.** `recipe_commit()` resets `ext_count` and `ext_idx`, destroying pre-loaded extension words from `setup_prefetch`. `FetchExtWords` then re-reads from memory at the wrong position. The extension word lifecycle is unfixable without modelling the 68000 prefetch pipeline (IR + IRC) explic...

### Prompt 2

Commit this. Also write our plan to disk.


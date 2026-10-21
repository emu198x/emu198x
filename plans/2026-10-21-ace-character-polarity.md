# Ace character polarity regression

Engineering-frontier accuracy fix discovered while qualifying reusable character uploads.

1. Compare the Jupiter Cantab User's Manual (1982), chapter 12 normal/inverse and redefining characters, with MAME cantab/jupace.cpp screen_update and monochrome palette. Both show set normal glyph bits white, unset bits black. The current renderer maps them in reverse; it also starts blank/border pixels white. Write a source-independent pixel regression in crates/machine-jupiter-ace/tests/character_polarity.rs, using asymmetric rows, normal/inverse cells and unaffected neighbours/border. Demonstrate failure on unchanged source.
2. Correct only render palette choice and inactive/initial fill in crates/machine-jupiter-ace/src/display.rs; update old tests whose expected colours pinned the reversed behaviour. Run crate tests, formatting and Clippy; build the native Ace with locked dependencies for qualification. Preserve timing and bus behaviour.
3. Push a separate reviewed PR; bind Code198x native evidence to its exact source/build identities. Record that hardware character RAM is write-only; debugger introspection is a diagnostic, not a callable font-read contract. The source read-bus discrepancy is shared with MAME and needs separate investigation; do not invent a floating-bus byte in this colour fix.
4. Preserve test output and remotes; remove owned worktree/target and local tools after the pattern evidence is retained.

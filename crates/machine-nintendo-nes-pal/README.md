# Nintendo NES PAL board

Fixed PAL board over `common-nintendo-nes`, with its own construction,
checked snapshot restore and direct guest tests. Shared chip and clock
algorithms live in the common crate; this owner accepts no region selector.

//! The sound module's boot program.
//!
//! The S-SMP boots from a 64-byte ROM mapped at `$FFC0-$FFFF`. Nintendo's
//! IPL ROM is copyrighted code, so Emu198x never ships it: a console built
//! without it boots [`CLEAN_ROOM`], a program written for this project
//! that speaks the same upload protocol, as the sound reference describes
//! it and as every game drives it:
//!
//! 1. Signal ready: `$AA` on port 0, `$BB` on port 1.
//! 2. Wait for `$CC` on port 0.
//! 3. Start a block. Ports 2 and 3 hold its address and port 1 a command.
//!    Echo port 0. A command of zero means jump to the address, anything
//!    else means bytes follow.
//! 4. For each byte the main CPU writes the data to port 1, then a running
//!    index from zero to port 0. Wait for the index, store the byte at the
//!    address plus the index, and echo the index.
//! 5. Anything else appearing on port 0 starts a new block, at step 3.
//!
//! Differences from the original a program could see: the original also
//! clears direct page `$00-$EF` before signalling ready, and leaves A, X
//! and Y with its own values at the jump. This program sets only the stack
//! pointer. A game that relied on a zeroed direct page would need the real
//! ROM, which the machine takes as firmware when it is supplied (see
//! [`crate::Snes::new`]).
//!
//! # The program
//!
//! ```text
//! FFC0  CD EF     mov  x,#$EF
//! FFC2  BD        mov  sp,x
//! FFC3  8F AA F4  mov  $F4,#$AA      ; ready
//! FFC6  8F BB F5  mov  $F5,#$BB
//! FFC9  78 CC F4  cmp  $F4,#$CC      ; wait for the start
//! FFCC  D0 FB     bne  $FFC9
//! FFCE  BA F6     movw ya,$F6        ; block: address from ports 2-3
//! FFD0  DA 00     movw $00,ya
//! FFD2  F8 F4     mov  x,$F4         ; the value to echo
//! FFD4  E4 F5     mov  a,$F5         ; the command
//! FFD6  D8 F4     mov  $F4,x
//! FFD8  D0 07     bne  $FFE1         ; non-zero: bytes follow
//! FFDA  E4 01     mov  a,$01         ; zero: jump, by RET
//! FFDC  2D        push a
//! FFDD  E4 00     mov  a,$00
//! FFDF  2D        push a
//! FFE0  6F        ret
//! FFE1  8D 00     mov  y,#$00        ; bytes: index from zero
//! FFE3  3E F4     cmp  x,$F4         ; port 0 unchanged since the echo?
//! FFE5  F0 FC     beq  $FFE3
//! FFE7  7E F4     cmp  y,$F4         ; the next index: a byte
//! FFE9  F0 02     beq  $FFED
//! FFEB  2F E1     bra  $FFCE         ; anything else: a new block
//! FFED  E4 F5     mov  a,$F5
//! FFEF  D7 00     mov  [$00]+y,a
//! FFF1  CB F4     mov  $F4,y         ; echo the index
//! FFF3  DD        mov  a,y
//! FFF4  5D        mov  x,a           ; and remember it
//! FFF5  FC        inc  y
//! FFF6  D0 EB     bne  $FFE3
//! FFF8  AB 01     inc  $01           ; index wrapped: next page
//! FFFA  2F E7     bra  $FFE3
//! FFFC  00 00
//! FFFE  C0 FF     reset vector
//! ```

/// The clean-room boot program, mapped at `$FFC0`.
pub const CLEAN_ROOM: [u8; 64] = [
    0xCD, 0xEF, 0xBD, 0x8F, 0xAA, 0xF4, 0x8F, 0xBB, // FFC0
    0xF5, 0x78, 0xCC, 0xF4, 0xD0, 0xFB, 0xBA, 0xF6, // FFC8
    0xDA, 0x00, 0xF8, 0xF4, 0xE4, 0xF5, 0xD8, 0xF4, // FFD0
    0xD0, 0x07, 0xE4, 0x01, 0x2D, 0xE4, 0x00, 0x2D, // FFD8
    0x6F, 0x8D, 0x00, 0x3E, 0xF4, 0xF0, 0xFC, 0x7E, // FFE0
    0xF4, 0xF0, 0x02, 0x2F, 0xE1, 0xE4, 0xF5, 0xD7, // FFE8
    0x00, 0xCB, 0xF4, 0xDD, 0x5D, 0xFC, 0xD0, 0xEB, // FFF0
    0xAB, 0x01, 0x2F, 0xE7, 0x00, 0x00, 0xC0, 0xFF, // FFF8
];

/// The firmware id the original ROM is supplied under.
pub const FIRMWARE_ID: &str = "snes-ipl";

/// The original ROM's length.
pub const LEN: usize = 64;

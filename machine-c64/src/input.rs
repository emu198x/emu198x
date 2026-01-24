//! C64 keyboard input handling.
//!
//! The C64 keyboard is an 8x8 matrix scanned through CIA1:
//! - Port A ($DC00): Column select (directly exposed, directly written)
//! - Port B ($DC01): Row input (directly read)
//!
//! When a column is selected (bit low), the corresponding row bits
//! go low for pressed keys in that column.

use emu_core::KeyCode;

/// Map PC key codes to C64 keyboard matrix positions.
/// Returns (column, row) pairs.
pub fn map_key(key: KeyCode) -> Option<(usize, usize)> {
    // C64 keyboard matrix layout:
    //        Col 0    Col 1    Col 2    Col 3    Col 4    Col 5    Col 6    Col 7
    // Row 0: DEL      RETURN   CRSR-R   F7       F1       F3       F5       CRSR-D
    // Row 1: 3        W        A        4        Z        S        E        SHIFT-L
    // Row 2: 5        R        D        6        C        F        T        X
    // Row 3: 7        Y        G        8        B        H        U        V
    // Row 4: 9        I        J        0        M        K        O        N
    // Row 5: +        P        L        -        .        :        @        ,
    // Row 6: £        *        ;        HOME     SHIFT-R  =        ↑        /
    // Row 7: 1        ←        CTRL     2        SPACE    C=       Q        STOP

    match key {
        // Row 7 (top row)
        KeyCode::Digit1 => Some((7, 0)),
        KeyCode::Digit2 => Some((7, 3)),
        KeyCode::Digit3 => Some((1, 0)),
        KeyCode::Digit4 => Some((1, 3)),
        KeyCode::Digit5 => Some((2, 0)),
        KeyCode::Digit6 => Some((2, 3)),
        KeyCode::Digit7 => Some((3, 0)),
        KeyCode::Digit8 => Some((3, 3)),
        KeyCode::Digit9 => Some((4, 0)),
        KeyCode::Digit0 => Some((4, 3)),

        // Letters (QWERTY layout)
        KeyCode::KeyQ => Some((7, 6)),
        KeyCode::KeyW => Some((1, 1)),
        KeyCode::KeyE => Some((1, 6)),
        KeyCode::KeyR => Some((2, 1)),
        KeyCode::KeyT => Some((2, 6)),
        KeyCode::KeyY => Some((3, 1)),
        KeyCode::KeyU => Some((3, 6)),
        KeyCode::KeyI => Some((4, 1)),
        KeyCode::KeyO => Some((4, 6)),
        KeyCode::KeyP => Some((5, 1)),
        KeyCode::KeyA => Some((1, 2)),
        KeyCode::KeyS => Some((1, 5)),
        KeyCode::KeyD => Some((2, 2)),
        KeyCode::KeyF => Some((2, 5)),
        KeyCode::KeyG => Some((3, 2)),
        KeyCode::KeyH => Some((3, 5)),
        KeyCode::KeyJ => Some((4, 2)),
        KeyCode::KeyK => Some((4, 5)),
        KeyCode::KeyL => Some((5, 2)),
        KeyCode::KeyZ => Some((1, 4)),
        KeyCode::KeyX => Some((2, 7)),
        KeyCode::KeyC => Some((2, 4)),
        KeyCode::KeyV => Some((3, 7)),
        KeyCode::KeyB => Some((3, 4)),
        KeyCode::KeyN => Some((4, 7)),
        KeyCode::KeyM => Some((4, 4)),

        // Special keys
        KeyCode::Enter => Some((0, 1)),
        KeyCode::Space => Some((7, 4)),
        KeyCode::Backspace => Some((0, 0)), // DEL/INST
        KeyCode::ShiftLeft => Some((1, 7)),
        KeyCode::ShiftRight => Some((6, 4)),
        KeyCode::ControlLeft | KeyCode::ControlRight => Some((7, 2)),
        KeyCode::AltLeft | KeyCode::AltRight => Some((7, 5)), // C= key
        KeyCode::Escape => Some((7, 7)), // RUN/STOP

        // Cursor keys
        KeyCode::ArrowRight => Some((0, 2)),
        KeyCode::ArrowDown => Some((0, 7)),
        // Arrow Left = Shift + Cursor Right
        // Arrow Up = Shift + Cursor Down (handled in caller)
        KeyCode::ArrowLeft => Some((0, 2)),  // Same as right, needs shift
        KeyCode::ArrowUp => Some((0, 7)),    // Same as down, needs shift

        // Function keys
        KeyCode::F1 => Some((0, 4)),
        KeyCode::F2 => Some((0, 4)), // F2 = Shift + F1 (handled in caller)
        KeyCode::F3 => Some((0, 5)),
        KeyCode::F4 => Some((0, 5)), // F4 = Shift + F3
        KeyCode::F5 => Some((0, 6)),
        KeyCode::F6 => Some((0, 6)), // F6 = Shift + F5
        KeyCode::F7 => Some((0, 3)),
        KeyCode::F8 => Some((0, 3)), // F8 = Shift + F7

        _ => None,
    }
}

/// Check if a key requires the shift modifier on C64.
pub fn needs_shift(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::ArrowLeft | KeyCode::ArrowUp | KeyCode::F2 | KeyCode::F4 | KeyCode::F6 | KeyCode::F8
    )
}

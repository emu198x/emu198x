import AppKit
import Carbon.HIToolbox

/// Mac virtual key codes to the browser `KeyboardEvent.code` names the
/// runtime's keymap uses. These are physical positions, so the mapping
/// does not change with the keyboard layout, as with the browser player.
enum MacKeyCodes {
    static func browserCode(for keyCode: UInt16) -> String? {
        table[Int(keyCode)]
    }

    /// Which modifier keys are down, by side. `NSEvent.ModifierFlags` keeps
    /// the left and right keys apart only in its device-dependent bits.
    static func modifiers(in flags: NSEvent.ModifierFlags) -> [(String, Bool)] {
        let raw = flags.rawValue
        return [
            ("ShiftLeft", raw & 0x0002 != 0),
            ("ShiftRight", raw & 0x0004 != 0),
            ("ControlLeft", raw & 0x0001 != 0),
            ("ControlRight", raw & 0x2000 != 0),
            ("AltLeft", raw & 0x0020 != 0),
            ("AltRight", raw & 0x0040 != 0),
        ]
    }

    private static let table: [Int: String] = {
        var table: [Int: String] = [
            kVK_ANSI_0: "Digit0", kVK_ANSI_1: "Digit1", kVK_ANSI_2: "Digit2",
            kVK_ANSI_3: "Digit3", kVK_ANSI_4: "Digit4", kVK_ANSI_5: "Digit5",
            kVK_ANSI_6: "Digit6", kVK_ANSI_7: "Digit7", kVK_ANSI_8: "Digit8",
            kVK_ANSI_9: "Digit9",
            kVK_Return: "Enter", kVK_ANSI_KeypadEnter: "Enter",
            kVK_Space: "Space", kVK_Delete: "Backspace", kVK_ForwardDelete: "Delete",
            kVK_Escape: "Escape",
            kVK_LeftArrow: "ArrowLeft", kVK_RightArrow: "ArrowRight",
            kVK_UpArrow: "ArrowUp", kVK_DownArrow: "ArrowDown",
        ]
        let letters: [(Int, Character)] = [
            (kVK_ANSI_A, "A"), (kVK_ANSI_B, "B"), (kVK_ANSI_C, "C"), (kVK_ANSI_D, "D"),
            (kVK_ANSI_E, "E"), (kVK_ANSI_F, "F"), (kVK_ANSI_G, "G"), (kVK_ANSI_H, "H"),
            (kVK_ANSI_I, "I"), (kVK_ANSI_J, "J"), (kVK_ANSI_K, "K"), (kVK_ANSI_L, "L"),
            (kVK_ANSI_M, "M"), (kVK_ANSI_N, "N"), (kVK_ANSI_O, "O"), (kVK_ANSI_P, "P"),
            (kVK_ANSI_Q, "Q"), (kVK_ANSI_R, "R"), (kVK_ANSI_S, "S"), (kVK_ANSI_T, "T"),
            (kVK_ANSI_U, "U"), (kVK_ANSI_V, "V"), (kVK_ANSI_W, "W"), (kVK_ANSI_X, "X"),
            (kVK_ANSI_Y, "Y"), (kVK_ANSI_Z, "Z"),
        ]
        for (code, letter) in letters { table[code] = "Key\(letter)" }
        return table
    }()
}

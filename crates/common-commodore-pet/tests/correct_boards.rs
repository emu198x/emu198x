use common_commodore_pet::{Board, Pet, PetKey};
use std::path::Path;
fn screen(p: &Pet, cols: u16) -> String {
    (0..cols * 25)
        .map(|i| {
            let c = p.peek(0x8000 + i) & 127;
            if (1..=26).contains(&c) {
                (b'@' + c) as char
            } else if (32..=63).contains(&c) {
                c as char
            } else {
                ' '
            }
        })
        .collect()
}
#[test]
#[ignore = "FIXTURE: exact PET BASIC 2/4 firmware required"]
fn three_real_boards_boot_and_type() {
    let root = std::env::var("EMU198X_PET_REFERENCE_ROMS").expect("ROM directory");
    let root = Path::new(&root);
    for (board, version, editor, expected) in [
        (Board::Pet2001n32k, 2, "edit-2-n.901447-24.bin", 16640),
        (Board::Pet4032, 4, "edit-4-40-n-50Hz.901498-01.bin", 20000),
        (Board::Pet8032, 4, "edit-4-80-b-50Hz.901474-04.bin", 19992),
    ] {
        let read = |name: &str| std::fs::read(root.join(name)).expect(name);
        let mut pet = Pet::new(
            read(if version == 2 {
                "kernal-2.901465-03.bin"
            } else {
                "kernal-4.901465-22.bin"
            }),
            read(if version == 2 {
                "basic-2.901465-01-02.bin"
            } else {
                "basic-4.901465-23-20-21.bin"
            }),
            read(editor),
            read("characters-2.901447-10.bin"),
            board,
        );
        for _ in 0..120 {
            pet.run_frame();
        }
        let before = screen(&pet, board.columns() as u16);
        println!("{board:?} PC {:04x} {before}", pet.cpu().regs.pc);
        assert!(before.contains("READY."), "{board:?}: {before}");
        assert_eq!(pet.run_frame(), expected);
        for key in [
            PetKey::P,
            PetKey::R,
            PetKey::I,
            PetKey::N,
            PetKey::T,
            PetKey::Num3,
            PetKey::Plus,
            PetKey::Num4,
            PetKey::Return,
        ] {
            pet.press_key(key);
            for _ in 0..5 {
                pet.run_frame();
            }
            pet.release_key(key);
            for _ in 0..5 {
                pet.run_frame();
            }
        }
        for _ in 0..30 {
            pet.run_frame();
        }
        let after = screen(&pet, board.columns() as u16);
        assert!(after.contains("PRINT3+4"), "{board:?}: {after}");
        assert!(after.contains("7"), "{board:?}: {after}");
        if let Ok(dir) = std::env::var("EMU198X_PET_REAL_CAPTURE") {
            let stem = match board {
                Board::Pet2001n32k => "3032",
                Board::Pet4032 => "4032",
                Board::Pet8032 => "8032",
            };
            let dir = Path::new(&dir);
            std::fs::create_dir_all(dir).expect("capture directory");
            let screen: Vec<u8> = (0..board.columns() * 25)
                .map(|i| pet.peek(0x8000 + i as u16))
                .collect();
            let pixels: Vec<u8> = pet
                .framebuffer()
                .iter()
                .flat_map(|p| p.to_le_bytes())
                .collect();
            std::fs::write(dir.join(format!("{stem}-screen.bin")), screen).expect("screen");
            std::fs::write(dir.join(format!("{stem}.argb")), pixels).expect("pixels");
        }
    }
}

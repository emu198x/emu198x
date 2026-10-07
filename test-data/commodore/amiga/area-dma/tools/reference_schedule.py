#!/usr/bin/env python3
"""Compile unchanged registered vAmiga area programs into a flag recorder.

This executes the active table, not the complete emulator or silicon.
"""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    source = args.reference / "Core/Components/Agnus/Blitter/SlowBlitter.cpp"
    text = source.read_text()
    constants = text[
        text.index("static constexpr u16 NOTHING") : text.index("\nvoid\n")
    ]
    start = text.index("    void (Blitter::*copyBlitInstr[16][2][2][6])")
    end = text.index("\n    };", start) + len("\n    };")
    args.output.mkdir(parents=True, exist_ok=True)
    program = args.output / "reference_table.cpp"
    program.write_text(
        "#include <iostream>\nusing u16 = unsigned short;\n"
        + constants
        + "\nstruct Blitter { unsigned flags;\n"
        "template<u16 instruction> void exec() { flags = instruction; }\n"
        "template<u16 instruction> void fakeExec() { flags = instruction; }\n"
        "};\nint main() {\n"
        + text[start:end]
        + "\nBlitter chip;\nfor (unsigned mode=0; mode<16; ++mode) {\n"
        "for (unsigned fill=0; fill<2; ++fill) {\n"
        "std::cout << mode << ' ' << fill;\n"
        "for (unsigned stage=0; stage<6; ++stage) {\n"
        "(chip.*copyBlitInstr[mode][0][fill][stage])();\n"
        "std::cout << ' ' << chip.flags;\n"
        "if (chip.flags & REPEAT) break;\n"
        "if (stage == 5) return 1;\n"
        "}\nstd::cout << '\\n';\n}\n}\n}\n"
    )
    executable = args.output / "reference_table"
    subprocess.run(
        ["c++", "-std=c++17", str(program), "-o", str(executable)], check=True
    )
    result = subprocess.run(
        [str(executable)], check=True, capture_output=True, text=True
    )
    rows = [[int(value) for value in row.split()] for row in result.stdout.splitlines()]
    if len(rows) != 32 or {(row[0], row[1]) for row in rows} != {
        (mode, fill) for mode in range(16) for fill in range(2)
    }:
        raise ValueError("incomplete active reference programs")
    if any(len(row) < 4 or not row[-1] & 2048 for row in rows):
        raise ValueError("a reference program did not reach REPEAT")
    checksum = hashlib.sha256(source.read_bytes()).hexdigest()
    (args.output / "reference-programs.tsv").write_text(
        "# vAmiga 60fd1e6b69dcd77c9f44d1291bd37ec715362ab0 SlowBlitter.cpp\n"
        f"# source SHA256 {checksum}\n"
        "# mode fill main-program flags (compiled unchanged active table)\n"
        + result.stdout
    )
    (args.output / "reference-schedule.json").write_text(
        json.dumps(
            {
                "boundary": "compiled active source table; not full emulator or silicon",
                "source_sha256": checksum,
                "programs": [
                    {"mode": row[0], "fill": row[1], "flags": row[2:]} for row in rows
                ],
            },
            indent=2,
        )
        + "\n"
    )
    print(
        f"Executed {len(rows)} registered reference programs; source SHA256 {checksum}"
    )


if __name__ == "__main__":
    main()

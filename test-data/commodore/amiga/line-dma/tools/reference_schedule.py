"""Execute the registered vAmiga line microprogram table in a flag recorder.

This checks source-defined stages, not a complete reference-emulator run.
No source in the registered reference checkout is modified.
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
    start = text.index("    void (Blitter::*lineBlitInstr[4][2][8])")
    end = text.index("\n    };", start) + len("\n    };")
    table = text[start:end]
    args.output.mkdir(parents=True, exist_ok=True)
    program = args.output / "reference_table.cpp"
    program.write_text(
        "#include <iostream>\nusing u16 = unsigned short;\n"
        + constants
        + "\nstruct Blitter { unsigned flags;\n"
        "template<u16 instruction> void execLine() { flags = instruction; }\n"
        "template<u16 instruction> void fakeExecLine() { flags = instruction; }\n"
        "};\nint main() {\n"
        + table
        + "\nBlitter chip;\nfor (unsigned mode=0; mode<4; ++mode) {\n"
        "std::cout << mode << ':';\n"
        "for (unsigned stage=0; stage<8; ++stage) {\n"
        "(chip.*lineBlitInstr[mode][0][stage])();\n"
        "std::cout << ' ' << chip.flags;\n"
        "if (chip.flags & REPEAT) break;\n"
        "if (stage == 7) return 1;\n"
        "}\nstd::cout << '\\n';\n}\n}\n"
    )
    executable = args.output / "reference_table"
    subprocess.run(
        ["c++", "-std=c++17", str(program), "-o", str(executable)], check=True
    )
    result = subprocess.run(
        [str(executable)], check=True, capture_output=True, text=True
    )
    schedules = {}
    for row in result.stdout.splitlines():
        mode, flags = row.split(":")
        values = [int(value) for value in flags.split()]
        schedules[mode] = [
            {
                "cck": index + 1,
                "b_read": bool(value & 16),
                "c_read": bool(value & 32),
                "d_stage": bool(value & 4),
                "reserved": bool(value & 2),
                "result": bool(value & 256),
            }
            for index, value in enumerate(values)
        ]
    if len(schedules) != 4 or [len(schedules[str(i)]) for i in range(4)] != [
        4,
        4,
        6,
        6,
    ]:
        raise ValueError("unexpected or incomplete reference line programs")
    report = {
        "boundary": "compiled registered source table; full emulator and silicon not executed",
        "source": str(source.resolve()),
        "sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
        "programs": schedules,
    }
    (args.output / "reference_schedule.json").write_text(
        json.dumps(report, indent=2) + "\n"
    )
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()

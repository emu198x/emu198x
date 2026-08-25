#!/usr/bin/env python3
"""Parse NDK 3.2 `.i` LVO files into Rust `&[(i32, &str)]` literals.

Why this exists: the LVO tables in
`crates/emu198x-amiga/src/mcp/lvo.rs` are large and hand-editing
them is how the first attempt drifted by 12 bytes across the entire
exec.library table. This script is the canonical regeneration path —
point it at the Commodore NDK 3.2 mirror in the reference library
and pipe the output into `lvo.rs`'s table region.

Format of each .i line we care about:
    _LVOFoo equ -123

Skips comments, the leading two-line ` * `-prefixed banner, blank
lines, and anything else.

Usage:
    tools/lvo-from-ndk.py [NDK_LVO_DIR] > lvo-tables.rs

If NDK_LVO_DIR is omitted, defaults to the copy in the private reference
library, located relative to this script:
    <198x>/reference/by-system/commodore-amiga/ndk/ndk-3.2/Include_I/lvo
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

LINE_PATTERN = re.compile(r"^_LVO([A-Za-z0-9_]+)\s+equ\s+(-\d+)\s*$")

# The umbrella root, resolved from this file's own location rather than from
# $HOME, so the default survives the tree being checked out anywhere. It did
# not before: it hardcoded ~/Projects/198x and silently missed otherwise.
_ROOT = Path(__file__).resolve().parents[3]
DEFAULT_NDK = (
    _ROOT / "reference/by-system/commodore-amiga/ndk/ndk-3.2/Include_I/lvo"
)


def parse(path: Path) -> list[tuple[int, str]]:
    """Return all `(offset, name)` pairs from one NDK .i file."""
    entries: list[tuple[int, str]] = []
    for raw in path.read_text().splitlines():
        m = LINE_PATTERN.match(raw.strip())
        if not m:
            continue
        name, off = m.group(1), int(m.group(2))
        entries.append((off, name))
    return entries


def emit(varname: str, source_file: str, entries: list[tuple[int, str]]) -> None:
    """Emit one `const NAME: &[(i32, &str)] = &[ ... ];` block."""
    print(
        f"// Generated from NDK 3.2 Include_I/lvo/{source_file} — "
        f"{len(entries)} entries + 4 inherited Library slots."
    )
    print(f"const {varname}: &[(i32, &str)] = &[")
    print('    // Inherited from struct Library')
    print('    (-6, "Open"),')
    print('    (-12, "Close"),')
    print('    (-18, "Expunge"),')
    print('    (-24, "Reserved"),')
    print('    // library-specific')
    for off, fn in entries:
        print(f'    ({off}, "{fn}"),')
    print("];\n")


def main(argv: list[str]) -> int:
    ndk_dir = Path(argv[1]) if len(argv) > 1 else DEFAULT_NDK
    if not ndk_dir.is_dir():
        print(f"NDK LVO directory not found: {ndk_dir}", file=sys.stderr)
        return 1

    libs = [
        ("exec_lib.i", "EXEC_LIBRARY"),
        ("dos_lib.i", "DOS_LIBRARY"),
        ("intuition_lib.i", "INTUITION_LIBRARY"),
        ("graphics_lib.i", "GRAPHICS_LIBRARY"),
    ]

    for libfile, varname in libs:
        path = ndk_dir / libfile
        if not path.exists():
            print(f"// MISSING: {path}", file=sys.stderr)
            continue
        entries = parse(path)
        print(f"//   {libfile}: {len(entries)} LVOs", file=sys.stderr)
        emit(varname, libfile, entries)

    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

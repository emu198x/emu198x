#!/usr/bin/env python3
"""Record priority outputs from the pinned local Altirra function.

Requires a C++ compiler. Compiles in a temporary directory without modifying
third-party sources. The output is a numeric fixture, not a copied implementation.
"""
import hashlib
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / '../../emulators/atari/altirra/src/Altirra/source/gtiatables.cpp'
SOURCE_SHA256 = '7a890f1e55c9a83d5836140670a6d3d9af2f99b6b5290646463f3f4db1d07306'
OUTPUT = ROOT / 'crates/atari-gtia/tests/data/priority-signals.txt'


def main():
    raw = SOURCE.read_bytes()
    if hashlib.sha256(raw).hexdigest() != SOURCE_SHA256:
        raise SystemExit('Altirra source hash differs; review the oracle before updating it')
    source = raw.decode()
    start = source.index('void ATInitGTIAPriorityTables(')
    end = source.index('\nvoid ATComputeLumaRamp', start)
    function = source[start:end]
    # Keep the entire input decoding and priority calculation, but capture
    # its signal mask before the switch maps it to a private colour-table index.
    function = function[:function.index('\n\t\t\tuint8 c;')]
    function += '\n priorityTables[prior][i] = out;\n }\n }\n}\n'
    function = function.replace(
        'uint8 priorityTables[32][256]', 'unsigned short priorityTables[32][256]')
    program = '#include <cstdio>\n#include <cstring>\nusing uint8=unsigned char;\n'
    program += function + '''
int main() {
    unsigned short table[32][256];
    ATInitGTIAPriorityTables(table);
    for (auto &row : table) {
        for (auto value : row) std::printf("%03x ", value);
        std::puts("");
    }
}
'''
    with tempfile.TemporaryDirectory() as directory:
        temporary = Path(directory)
        cpp = temporary / 'oracle.cpp'
        binary = temporary / 'oracle'
        cpp.write_text(program)
        subprocess.run(['c++', '-std=c++11', str(cpp), '-o', str(binary)], check=True)
        result = subprocess.check_output([str(binary)]).decode()
    OUTPUT.write_text('\n'.join(line.rstrip() for line in result.splitlines()) + '\n')
    print(f'Wrote {OUTPUT.relative_to(ROOT)} from source {SOURCE_SHA256}')


if __name__ == '__main__':
    main()

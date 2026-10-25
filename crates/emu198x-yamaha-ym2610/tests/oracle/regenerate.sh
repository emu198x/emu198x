#!/bin/sh
# Rebuild cases/ and golden/ from ymfm's ADPCM engines at a pinned commit.
#
# Needs git, python3 and a C++ compiler. ymfm (BSD-3-Clause) is fetched into
# a temporary directory and never committed: only its output is kept, in
# golden/.
set -eu

YMFM_URL=https://github.com/aaronsgiles/ymfm.git
YMFM_COMMIT=81aec25ccbb98f4873a255f7551ac4dadac59b4a

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

git clone --quiet "$YMFM_URL" "$work/ymfm"
git -C "$work/ymfm" checkout --quiet "$YMFM_COMMIT"
${CXX:-c++} -O2 -std=c++14 -I"$work/ymfm/src" -o "$work/harness" \
    "$here/ymfm_harness.cpp" "$work/ymfm/src/ymfm_adpcm.cpp"

python3 "$here/make_cases.py"
mkdir -p "$here/golden"
for script in "$here"/cases/*.adpcm; do
    name=$(basename "$script" .adpcm)
    "$work/harness" "$script" > "$here/golden/$name.txt"
done
echo "regenerated $(ls "$here"/golden | wc -l) golden files"

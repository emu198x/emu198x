#!/bin/sh
# Rebuild cases/ and golden/ from Nuked-OPN2 at the commit the crate follows.
#
# Needs git, python3 and a C compiler. Nuked-OPN2 is fetched into a temporary
# directory and never committed: only its output is kept, in golden/.
set -eu

NUKED_URL=https://github.com/nukeykt/Nuked-OPN2.git
NUKED_COMMIT=335747d78cb0abbc3b55b004e62dad9763140115

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

git clone --quiet "$NUKED_URL" "$work/nuked"
git -C "$work/nuked" checkout --quiet "$NUKED_COMMIT"
${CC:-cc} -O2 -std=c99 -I"$work/nuked" -o "$work/harness" \
    "$here/nuked_harness.c" "$work/nuked/ym3438.c"

python3 "$here/make_cases.py"
mkdir -p "$here/golden"
for script in "$here"/cases/*.ym; do
    name=$(basename "$script" .ym)
    "$work/harness" "$script" > "$here/golden/$name.txt"
done
echo "regenerated $(ls "$here"/golden | wc -l) golden files"

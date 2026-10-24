#!/bin/sh
# Rebuild cases/ and golden/ from Nuked-OPLL at the commit the crate follows.
#
# Needs git, python3 and a C compiler. Nuked-OPLL is fetched into a temporary
# directory and never committed: only its output is kept, in golden/.
set -eu

NUKED_URL=https://github.com/nukeykt/Nuked-OPLL.git
NUKED_COMMIT=1269cf5a783b65583b50fa2464d08be75830aaa0

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

git clone --quiet "$NUKED_URL" "$work/nuked"
git -C "$work/nuked" checkout --quiet "$NUKED_COMMIT"
${CC:-cc} -O2 -std=c99 -I"$work/nuked" -o "$work/harness" \
    "$here/nuked_harness.c" "$work/nuked/opll.c"

python3 "$here/make_cases.py"
mkdir -p "$here/golden"
rm -f "$here"/golden/*.txt
for script in "$here"/cases/*.opll; do
    name=$(basename "$script" .opll)
    "$work/harness" "$script" > "$here/golden/$name.txt"
done
echo "regenerated $(ls "$here"/golden | wc -l) golden files"

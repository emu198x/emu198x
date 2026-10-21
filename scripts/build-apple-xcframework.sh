#!/usr/bin/env bash

# Builds the emu198x-apple static library for macOS, Apple TV and the Apple
# TV simulator, and packages the three as apple/build/Emu198xCore.xcframework
# for the Swift package in apple/Emu198xKit, plus apple/build/baseline.json,
# the checkpoints that package's parity test compares against.
#
# Each library is linked against apple/link-check.c with Apple's clang, using
# the system libraries rustc reports for it, and the macOS build is run. The
# objects embed bitcode from Rust's LLVM, which Xcode's nm cannot read, so a
# successful link is the check that the exported functions resolve.
#
# Usage: scripts/build-apple-xcframework.sh
#   EMU198X_APPLE_FEATURES  Cargo features (default: the families the apps
#                           run: the Spectrum, C64, NES, Master System and
#                           Game Gear)
#
# The deployment targets match the app: macOS 14 and tvOS 17, the first
# releases with the Observation framework the Swift host uses.

set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/apple/build"
include="$root/crates/emu198x-apple/include"
features=${EMU198X_APPLE_FEATURES:-sinclair-zx-spectrum,commodore-c64,nintendo-nes,sega-master-system,sega-game-gear}
target_dir=${CARGO_TARGET_DIR:-$root/target}
targets=(aarch64-apple-darwin aarch64-apple-tvos aarch64-apple-tvos-sim)

export MACOSX_DEPLOYMENT_TARGET=14.0
export TVOS_DEPLOYMENT_TARGET=17.0

sdk_for() {
    case $1 in
        aarch64-apple-darwin) echo macosx ;;
        aarch64-apple-tvos) echo appletvos ;;
        aarch64-apple-tvos-sim) echo appletvsimulator ;;
    esac
}

clang_target_for() {
    case $1 in
        aarch64-apple-darwin) echo "arm64-apple-macos$MACOSX_DEPLOYMENT_TARGET" ;;
        aarch64-apple-tvos) echo "arm64-apple-tvos$TVOS_DEPLOYMENT_TARGET" ;;
        aarch64-apple-tvos-sim) echo "arm64-apple-tvos$TVOS_DEPLOYMENT_TARGET-simulator" ;;
    esac
}

cd "$root"
installed=$(rustup target list --installed)
for target in "${targets[@]}"; do
    if ! grep -qx "$target" <<<"$installed"; then
        echo "Rust target $target is not installed: rustup target add $target" >&2
        exit 1
    fi
done

rm -rf "$out"
mkdir -p "$out"
xcframework_args=()
link_flags_json=""

for target in "${targets[@]}"; do
    echo "== $target"
    release="$target_dir/$target/release"
    # The flags file sits beside the library it describes, so a cached build
    # that skips rustc still has the flags from the run that produced it.
    flags_file="$release/emu198x-apple.native-static-libs"
    cargo rustc --quiet --release -p emu198x-apple --lib --target "$target" \
        --no-default-features --features "$features" \
        -- --print "native-static-libs=$flags_file"
    library="$release/libemu198x_apple.a"
    if [[ ! -s "$flags_file" ]]; then
        echo "no native-static-libs recorded for $target; remove $release and rerun" >&2
        exit 1
    fi
    flags=$(<"$flags_file")

    sdk=$(sdk_for "$target")
    # $flags is deliberately unquoted: it is a list of linker arguments.
    # shellcheck disable=SC2086
    xcrun --sdk "$sdk" clang -target "$(clang_target_for "$target")" \
        -std=c11 -Wall -Werror -I "$include" \
        "$root/apple/link-check.c" "$library" $flags \
        -o "$out/link-check-$target"
    if [[ $target == aarch64-apple-darwin ]]; then
        "$out/link-check-$target"
    else
        echo "link-check: linked (not runnable on this Mac)"
    fi

    xcframework_args+=(-library "$library" -headers "$include")
    link_flags_json+="${link_flags_json:+,}\"$target\":\"$flags\""
done

xcodebuild -create-xcframework "${xcframework_args[@]}" \
    -output "$out/Emu198xCore.xcframework" >/dev/null
rm -f "$out"/link-check-*

# Direct-Fleet checkpoints the Swift package's parity test compares against.
cargo run --quiet --release -p emu198x-apple --example baseline >"$out/baseline.json"
python3 -m json.tool "$out/baseline.json" >/dev/null

dirty=false
git diff --quiet HEAD || dirty=true
cat >"$out/build.json" <<EOF
{
  "revision": "$(git rev-parse HEAD)",
  "modified": $dirty,
  "features": "$features",
  "rustc": "$(rustc --version)",
  "xcode": "$(xcodebuild -version | head -1)",
  "sdks": {
    "macosx": "$(xcrun --sdk macosx --show-sdk-version)",
    "appletvos": "$(xcrun --sdk appletvos --show-sdk-version)",
    "appletvsimulator": "$(xcrun --sdk appletvsimulator --show-sdk-version)"
  },
  "deploymentTargets": {"macos": "$MACOSX_DEPLOYMENT_TARGET", "tvos": "$TVOS_DEPLOYMENT_TARGET"},
  "linkFlags": {$link_flags_json}
}
EOF
python3 -m json.tool "$out/build.json" >/dev/null
echo "Built $out/Emu198xCore.xcframework"

#!/usr/bin/env bash
# Build the GTK window for a release target, ready for dist to pack.
#
# usage: scripts/build-gtk-window.sh TARGET
#
# Every release archive includes target/gtk-window/libexec (the workspace's
# dist `include`), because dist's includes are the same for every target. On
# Linux this builds emu198x-gtk-window into it; on other targets the
# directory stays empty. A machine binary finds the window in libexec/ beside
# it, or in share/<binary>/libexec/ once Homebrew has installed it
# (knowledge/decisions/gtk-window-process.md).
set -euo pipefail

target=${1:?usage: build-gtk-window.sh TARGET}
out=target/gtk-window/libexec
rm -rf "$out"
mkdir -p "$out"

case "$target" in
  *-unknown-linux-gnu)
    cargo build --locked --profile dist --target "$target" \
      -p emu198x-gtk-window --features gtk
    cp "target/$target/dist/emu198x-gtk-window" "$out/"
    ;;
  *)
    echo "no GTK window for $target; $out stays empty"
    ;;
esac

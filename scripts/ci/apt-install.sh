#!/usr/bin/env bash
# Install apt packages on a GitHub-hosted Ubuntu runner, bounded and retried,
# moving to another mirror on each retry.
#
# usage: scripts/ci/apt-install.sh PACKAGE...
#
# Every attempt runs under a hard `timeout` with explicit acquire timeouts, so
# an unreachable or crawling mirror costs minutes rather than the twenty to
# thirty apt's defaults take (scripts/check-workflow-apt.py). Bounding alone
# is not enough: the runners list their mirrors in /etc/apt/apt-mirrors.txt,
# Azure's first, and apt moves down that list only when a request fails, not
# when it is slow. A slow Azure mirror then fails all three attempts. So the
# first attempt uses the runner's own list, and each retry puts a different
# mirror first. Arm runners use ports.ubuntu.com without that list and keep
# their own mirror on every attempt.
set -uo pipefail

if [ $# -eq 0 ]; then
  echo "usage: $0 PACKAGE..." >&2
  exit 2
fi

list=/etc/apt/apt-mirrors.txt
# The mirrors each retry puts first, after the runner's own list.
fallbacks=(http://archive.ubuntu.com/ubuntu/ http://mirrors.edge.kernel.org/ubuntu/)

rotates() {
  [ -f "$list" ] && [ "$(dpkg --print-architecture)" = amd64 ] \
    && grep -rqs "mirror+file:$list" /etc/apt/sources.list /etc/apt/sources.list.d
}

# Put $1 first, keeping the runner's own mirrors after it to fall back on.
prefer() {
  {
    printf '%s\tpriority:1\n' "$1"
    grep -v -F "$1" "$list.runner" | awk -F'\t' '{ print $1 "\tpriority:" NR + 1 }'
  } | sudo tee "$list" > /dev/null
  echo "apt: mirror $1 first"
}

if rotates; then
  sudo cp "$list" "$list.runner"
  echo "apt: mirrors from $list; a retry tries ${fallbacks[*]} first"
else
  echo "apt: no mirror list here; retries use the runner's own mirror"
fi

apt_opts=(-o Acquire::Retries=2 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20)
for attempt in 1 2 3; do
  if [ "$attempt" -gt 1 ] && rotates; then
    prefer "${fallbacks[attempt - 2]}"
  fi
  sudo timeout 120 apt-get "${apt_opts[@]}" update \
    || echo "::warning::apt-get update attempt ${attempt} incomplete (mirror unreachable)"
  if sudo timeout 180 apt-get "${apt_opts[@]}" install -y --no-install-recommends "$@"; then
    exit 0
  fi
  echo "::warning::apt-get install attempt ${attempt} failed; retrying"
  sleep 10
done
echo "::error::apt failed after three bounded attempts"
exit 1

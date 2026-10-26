#!/usr/bin/env python3
"""Print the apt packages dist's Linux release builds need, one line.

They are the keys of `[workspace.metadata.dist.dependencies.apt]` in the
workspace `Cargo.toml`. The release job installs them with
`scripts/ci/apt-install.sh` rather than dist's own unbounded
`packages_install` command (scripts/check-workflow-apt.py).
"""

import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
packages = manifest["workspace"]["metadata"]["dist"]["dependencies"]["apt"]
print(" ".join(sorted(packages)))

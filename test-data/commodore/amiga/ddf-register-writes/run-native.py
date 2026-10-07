"""Build standalone native probes using existing workspace crates, without editing them."""

import argparse
import json
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]


def run(output: Path, fixtures: Path, probe: str) -> int:
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    manifest = '[package]\nname="ddf-register-probe"\nversion="0.0.0"\nedition="2024"\n[dependencies]\n'
    for name in (
        "commodore-agnus-ocs",
        "common-commodore-amiga",
        "machine-commodore-amiga-ecs",
        "motorola-68000",
    ):
        manifest += f"{name}={{path={json.dumps(str(REPO / 'crates' / name))}}}\n"
    for name, source in (
        ("register", "probe-native.rs"),
        ("copper", "probe-copper.rs"),
    ):
        manifest += f'[[bin]]\nname="{name}"\npath={json.dumps(str(HERE / source))}\n'
    path = output / "Cargo.toml"
    path.write_text(manifest)
    command = [
        "cargo",
        "run",
        "--release",
        "--offline",
        "--manifest-path",
        str(path),
        "--bin",
        probe,
    ]
    if probe == "register":
        command += ["--", str(fixtures.resolve())]
    return subprocess.run(command, check=False).returncode


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="scratch build directory")
    parser.add_argument("--fixtures", type=Path, default=HERE)
    parser.add_argument("--probe", choices=("register", "copper"), default="register")
    args = parser.parse_args()
    raise SystemExit(run(args.output, args.fixtures, args.probe))

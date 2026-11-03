#!/usr/bin/env python3
"""Fail if a published crate's code changed without a new version.

`publish-crates.yml` publishes a crate's version only when crates.io doesn't
have it yet. A change to a crate that keeps its version is therefore never
released: crates.io goes on serving the older code under the number this
workspace builds. On 2026-11-03 seven crates were in that state (POKEY, TIA,
GTIA, the ST GLUE/MMU, OCS Agnus, S-PPU and tape playback). It came to light
only when the Atari 7800 moved to its own repository, took POKEY from
crates.io, and its frozen saves no longer matched.

This compares each publishable crate's packaged files, as `cargo package
--list` names them, with the published `.crate` of the same version. A
version crates.io doesn't have yet is a release waiting for the workflow and
passes. The manifest and the other files cargo writes when packaging are
left out, since they never match a workspace's own.

## The fix a failure is asking for

Raise the crate's version (a patch for a compatible change; `cargo
semver-checks --baseline-version <published>` says which), add a changelog
entry, and make the crates that need the change require the new version.

`--self-test` runs the comparison on known-good and known-bad samples first.
"""

from __future__ import annotations

import argparse
import io
import json
import pathlib
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
USER_AGENT = "emu198x-published-crates-check (https://github.com/emu198x/emu198x)"
# Written by cargo when it packages, so never the same as the workspace's.
GENERATED = {"Cargo.toml", "Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json"}


def publishable() -> list[dict[str, str]]:
    out = subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "publishable-crates.py")],
        cwd=ROOT, check=True, capture_output=True, text=True,
    )
    return json.loads(out.stdout)


def packaged_files(name: str) -> list[str]:
    out = subprocess.run(
        ["cargo", "package", "--list", "--allow-dirty", "--locked", "-p", name],
        cwd=ROOT, check=True, capture_output=True, text=True,
    )
    return sorted(line for line in out.stdout.splitlines() if line not in GENERATED)


def published(name: str, version: str) -> dict[str, bytes] | None:
    """The published crate's files by path, or None if that version is absent.

    Existence is asked of the API, which answers a missing version with 404.
    The download link ends at a static store that answers 403 for a missing
    file, which is indistinguishable from a refused request.
    """
    api = f"https://crates.io/api/v1/crates/{name}/{version}"
    try:
        with urllib.request.urlopen(
            urllib.request.Request(api, headers={"User-Agent": USER_AGENT}), timeout=60
        ):
            pass
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise
    request = urllib.request.Request(f"{api}/download", headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read()
    files = {}
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        for member in archive.getmembers():
            if member.isfile():
                path = member.name.split("/", 1)[1]
                if path not in GENERATED:
                    files[path] = archive.extractfile(member).read()
    return files


def differences(local_dir: pathlib.Path, files: list[str], remote: dict[str, bytes]) -> list[str]:
    found = []
    for path in sorted(set(files) | set(remote)):
        if path not in remote:
            found.append(f"{path} is new")
        elif path not in files:
            found.append(f"{path} is gone")
        elif (local_dir / path).read_bytes() != remote[path]:
            found.append(f"{path} differs")
    return found


def self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        (root / "src").mkdir()
        (root / "src" / "lib.rs").write_bytes(b"pub fn a() {}\n")
        same = {"src/lib.rs": b"pub fn a() {}\n"}
        cases = [
            ("unchanged", ["src/lib.rs"], same, 0),
            ("edited", ["src/lib.rs"], {"src/lib.rs": b"pub fn b() {}\n"}, 1),
            ("file added", ["src/lib.rs", "src/new.rs"], same, 1),
            ("file removed", ["src/lib.rs"], {**same, "src/old.rs": b""}, 1),
        ]
        (root / "src" / "new.rs").write_bytes(b"")
        for name, files, remote, expected in cases:
            got = len(differences(root, files, remote))
            if got != expected:
                sys.exit(f"self-test FAILED: {name} should give {expected} difference(s), got {got}")
    print("self-test passed: flags edited, added and removed files, accepts an unchanged crate")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()

    crates = publishable()
    drifted, pending = [], []
    for crate in crates:
        name, version = crate["name"], crate["version"]
        remote = published(name, version)
        if remote is None:
            pending.append(f"{name} {version}")
            continue
        local_dir = ROOT / pathlib.Path(crate["manifest"]).parent
        found = differences(local_dir, packaged_files(name), remote)
        if found:
            drifted.append((name, version, found))

    for line in pending:
        print(f"not yet on crates.io, so it will publish: {line}")
    if not drifted:
        print(f"every one of {len(crates) - len(pending)} published crates matches its version on crates.io")
        return 0
    for name, version, found in drifted:
        print(f"{name} {version} differs from its published version: {', '.join(found[:5])}")
    print(
        f"\n{len(drifted)} crate(s) changed without a new version, so the change would never "
        "be released. Raise each version, add a changelog entry, and update the crates "
        "that need it (see this script's docstring)."
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())

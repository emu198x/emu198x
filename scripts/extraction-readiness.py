#!/usr/bin/env python3
"""Report extraction observations without changing repositories or the registry.

The manifest is the movable queue, not an inventory of every future crate.
Verification, publication from the extracted repository, and consumer-switch
results are not inferred from package or repository existence.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import json
import pathlib
import subprocess
import sys
import tomllib
import urllib.error
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
AGENT = "emu198x-extraction-readiness (https://github.com/emu198x/emu198x)"


def registry_version(name: str, version: str) -> dict:
    url = f"https://crates.io/api/v1/crates/{urllib.parse.quote(name)}/{urllib.parse.quote(version)}"
    request = urllib.request.Request(url, headers={"User-Agent": AGENT, "Accept": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            data = json.load(response)
        return {"state": "published", "yanked": data["version"]["yanked"]}
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return {"state": "absent"}
        return {"state": "unknown", "reason": f"HTTP {exc.code}"}
    except (urllib.error.URLError, TimeoutError, OSError, ValueError, KeyError) as exc:
        return {"state": "unknown", "reason": str(exc)}


def github_repos() -> tuple[set[str] | None, str | None]:
    try:
        result = subprocess.run(
            ["gh", "api", "--paginate", "orgs/emu198x/repos?per_page=100", "--jq", ".[].name"],
            check=True, text=True, capture_output=True, timeout=60,
        )
        return set(result.stdout.splitlines()), None
    except (OSError, subprocess.SubprocessError) as exc:
        return None, str(exc)


def build_rows(entries: list[dict], metadata: dict) -> list[dict]:
    packages = {p["name"]: p for p in metadata["packages"]}
    owners = {name: entry["name"] for entry in entries for name in entry["crates"]}
    rows = []
    for entry in entries:
        names = entry["crates"]
        crates = []
        for name in names:
            package = packages.get(name)
            if package is None:
                crates.append({"name": name, "local": "not in workspace", "version": None, "dependencies": []})
                continue
            dependencies = []
            for dep in package["dependencies"]:
                dep_name = dep["name"]
                if dep_name in owners and owners[dep_name] != entry["name"]:
                    item = {"name": dep_name, "requirement": dep["req"], "repository": owners[dep_name]}
                    if item not in dependencies:
                        dependencies.append(item)
            crates.append({"name": name, "local": "workspace", "version": package["version"],
                           "dependencies": sorted(dependencies, key=lambda d: d["name"])})
        rows.append({"repository": entry["name"], "crates": crates,
                     "repository_observation": "not checked", "verification": "not checked",
                     "consumer_switch": "not checked"})
    return rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--live", action="store_true", help="query crates.io and accessible GitHub repositories")
    parser.add_argument("--json", action="store_true", help="output JSON instead of a table")
    args = parser.parse_args()
    entries = tomllib.loads((ROOT / "scripts/extract-repos.toml").read_text())["repo"]
    try:
        result = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1"],
                                cwd=ROOT, check=True, text=True, capture_output=True, timeout=120)
        rows = build_rows(entries, json.loads(result.stdout))
    except (OSError, subprocess.SubprocessError, ValueError, KeyError) as exc:
        print(f"Cannot read extraction manifest or Cargo metadata: {exc}", file=sys.stderr)
        return 1
    all_crates = {c["name"]: c for row in rows for c in row["crates"]}
    if args.live:
        repos, error = github_repos()
        for row in rows:
            row["repository_observation"] = ("accessible" if row["repository"] in repos else "not in accessible listing") if repos is not None else f"unknown: {error}"
        checks = [(c["name"], c["version"]) for c in all_crates.values() if c["version"]]
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            states = dict(zip(checks, pool.map(lambda pair: registry_version(*pair), checks)))
        for c in all_crates.values():
            c["registry"] = states.get((c["name"], c["version"]), {"state": "unknown", "reason": "local version unavailable"})
    else:
        for c in all_crates.values():
            c["registry"] = {"state": "not checked"}
    for row in rows:
        for c in row["crates"]:
            for dep in c["dependencies"]:
                source = all_crates[dep["name"]]
                dep["local_version"] = source["version"]
                dep["registry"] = source["registry"]
    if args.json:
        print(json.dumps(rows, indent=2))
    else:
        print("Manifest entries and dependencies between them only; registry checks use exact workspace versions, not Cargo requirement resolution.")
        print("Repository listing is visibility-limited; verification and consumer switch are not checked.")
        print("repository | crate @ workspace version | registry | external manifest dependencies | GitHub")
        for row in rows:
            for c in row["crates"]:
                deps = ", ".join(f'{d["name"]} {d["requirement"]} (workspace {d["local_version"]}: {d["registry"]["state"]})' for d in c["dependencies"]) or "—"
                state = c["registry"]["state"]
                if c["registry"].get("yanked"):
                    state += " (yanked)"
                print(f'{row["repository"]} | {c["name"]} @ {c["version"] or "unknown"} | {state} | {deps} | {row["repository_observation"]}')
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Extract crates from this workspace into their own repository, with history.

Layers 1 and 2 of `knowledge/decisions/workspace-shape.md` § Amendment move
each CPU core and shared chip to an `emu198x/<repo>` repository of its own
(#1997, #1998). Every move is the same steps, so they live here rather than in
someone's shell history. `scripts/extract-repos.toml` names each repository and
the crates it takes.

    scripts/extract-repo.py mos-6502              # build it in target/extract/
    scripts/extract-repo.py mos-6502 --verify     # ...and run its checks
    scripts/extract-repo.py mos-6502 --push       # ...and push it

The result is a repository whose `main` is the crates' own history, filtered
out of this one with `git filter-repo` so commits from before a directory
rename are kept, and a `extract/skeleton` branch on top adding the family
skeleton: workspace manifest, shared CI and release workflows, foundation
documents and agent context. `--push` sends `main` and that branch to an
existing empty repository, ready for a pull request. Creating the repository
and publishing the crates stay with a maintainer.

Nothing here changes this workspace. Switching it to the published versions
and deleting the crates is the follow-up pull request, once they are on
crates.io.

Requires `git-filter-repo` (`pip install git-filter-repo`).
"""

from __future__ import annotations

import argparse
import datetime
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tomllib
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
CONFIG = ROOT / "scripts" / "extract-repos.toml"
ORG = "emu198x"
USER_AGENT = "emu198x-extract-repo (https://github.com/emu198x/emu198x)"

# Published test helpers a split-out crate takes from crates.io instead of a
# path. The version is the requirement written into its manifest.
PUBLISHED_HELPERS = {"emu198x-test-skip": "0.1"}

# Files every extracted repository carries unchanged from this one.
COPIED = ["LICENSE", "MANIFESTO.md", "PRINCIPLES.md", "rust-toolchain.toml"]


def run(*args: str, cwd: pathlib.Path = ROOT, capture: bool = False) -> str:
    completed = subprocess.run(args, cwd=cwd, check=True, text=True, capture_output=capture)
    return completed.stdout if capture else ""


def load_config(repo: str) -> dict:
    repos = tomllib.loads(CONFIG.read_text())["repo"]
    for entry in repos:
        if entry["name"] == repo:
            return entry
    names = ", ".join(entry["name"] for entry in repos)
    sys.exit(f"no repository named {repo!r} in {CONFIG.name}; known: {names}")


def historical_dirs(crate: str) -> list[str]:
    """Every directory the crate has lived in, newest first.

    Follows the crate's `src/lib.rs` through renames. A plain `git subtree
    split` sees only the current directory, which loses the history of every
    crate renamed for publication on 2026-08-26.
    """
    log = run(
        "git", "log", "--follow", "--name-status", "--format=", "--",
        f"crates/{crate}/src/lib.rs",
        capture=True,
    )
    dirs = [f"crates/{crate}"]
    for line in log.splitlines():
        fields = line.split("\t")
        if fields[0].startswith("R") and len(fields) == 3:
            old = fields[1].removesuffix("/src/lib.rs")
            if old not in dirs and old.startswith("crates/"):
                dirs.append(old)
    return dirs


def workspace_dependencies() -> dict[str, str]:
    """This workspace's `[workspace.dependencies]`, one inline spec per name."""
    text = (ROOT / "Cargo.toml").read_text()
    table = re.search(r"^\[workspace\.dependencies\]\n(.*?)(?=^\[)", text, re.M | re.S)
    specs = {}
    for line in table.group(1).splitlines() if table else []:
        match = re.match(r"^([A-Za-z0-9_-]+)\s*=\s*(.+)$", line)
        if match:
            specs[match.group(1)] = match.group(2).strip()
    return specs


def published(name: str, version: str) -> bool:
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{name}/{version}",
        headers={"User-Agent": USER_AGENT},
    )
    try:
        with urllib.request.urlopen(request, timeout=30):
            return True
    except urllib.error.HTTPError:
        return False


def rewrite_manifest(path: pathlib.Path, moving: set[str], used: set[str]) -> list[str]:
    """Point a moved crate's manifest at its new home. Returns warnings.

    Path dependencies on crates moving with it stay paths. Anything else must
    already be published: its `path` is dropped and its `version` kept. A
    published test helper takes its registry requirement.
    """
    text = path.read_text()
    warnings = []
    if re.search(r"^version\.workspace\s*=\s*true", text, re.M):
        sys.exit(
            f"{path.parent.name} still takes the suite version; give it its own "
            "version, and the emu198x- prefix, before it moves"
        )
    if re.search(r"^publish\s*=\s*false", text, re.M):
        sys.exit(f"{path.parent.name} is publish = false; make it publishable before it moves")

    def dependency(match: re.Match[str]) -> str:
        key, body = match.group(1), match.group(2)
        target = re.search(r'path = "\.\./([^"]+)"', body)
        if not target or target.group(1) in moving:
            return match.group(0)
        name = target.group(1)
        if name in PUBLISHED_HELPERS:
            return f'{key} = "{PUBLISHED_HELPERS[name]}"'
        version = re.search(r'version = "([^"]+)"', body)
        if not version:
            sys.exit(f"{path.parent.name}: dependency {name} has no version to publish against")
        if not published(name, version.group(1)):
            warnings.append(f"{path.parent.name}: {name} {version.group(1)} is not on crates.io yet")
        rest = re.sub(r'path = "\.\./[^"]+",?\s*', "", body).strip().rstrip(",").strip()
        if rest == f'version = "{version.group(1)}"':
            return f'{key} = "{version.group(1)}"'
        return f"{key} = {{ {rest} }}"

    text = re.sub(r"^([A-Za-z0-9_-]+) = \{([^}\n]*)\}", dependency, text, flags=re.M)
    # The comment above a helper's path dependency explains why it was a path.
    for helper in PUBLISHED_HELPERS:
        text = re.sub(rf"(?:^#.*\n)+(?={re.escape(helper)} = \")", "", text, flags=re.M)
    used.update(re.findall(r"^([A-Za-z0-9_-]+) = \{ workspace = true", text, re.M))
    path.write_text(text)
    return warnings


def nightly_workflow(entry: dict) -> str | None:
    """A nightly corpus workflow for the repository, from its config entry.

    The corpora come from the family's private store, the same as this
    repository's `nightly-accuracy.yml`, whose jobs for these crates move here.
    """
    corpora = entry.get("corpus", [])
    if not corpora:
        return None
    jobs = []
    for corpus in corpora:
        exports = [f'echo "{corpus["env_var"]}=$(pwd)/{corpus["artifact"]}" >> "$GITHUB_ENV"']
        for var, relative in corpus.get("extra_env", {}).items():
            exports.append(f'echo "{var}=$(pwd)/{corpus["artifact"]}/{relative}" >> "$GITHUB_ENV"')
        exports = "\n".join(f"          {line}" for line in exports)
        filter_arg = f' {corpus["filter"]}' if corpus.get("filter") else ""
        extra = f' {corpus["extra_args"]}' if corpus.get("extra_args") else ""
        jobs.append(f"""\
  {corpus["artifact"]}:
    name: {corpus["artifact"]}
    runs-on: ubuntu-latest
    needs: preflight
    if: needs.preflight.outputs.configured == 'true'
    timeout-minutes: {corpus.get("timeout", 90)}
    steps:
      - uses: actions/checkout@v7

      - name: Read pinned toolchain
        id: toolchain
        run: echo "channel=$(awk -F'"' '/^channel/{{print $2}}' rust-toolchain.toml)" >> "$GITHUB_OUTPUT"

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{{{ steps.toolchain.outputs.channel }}}}

      - name: Provision corpus from store
        env:
          GH_TOKEN: ${{{{ secrets.ACCURACY_CORPORA_TOKEN }}}}
        run: |
          set -euo pipefail
          mkdir -p corpora
          cd corpora
          gh release download "${{CORPORA_TAG}}" -R "${{CORPORA_REPO}}" \\
            -p '{corpus["artifact"]}.tar.zst' -p 'SHA256SUMS'
          grep ' {corpus["artifact"]}.tar.zst$' SHA256SUMS | sha256sum -c -
          tar --zstd -xf '{corpus["artifact"]}.tar.zst'
{exports}

      - name: Run {corpus["artifact"]}
        run: cargo test --release -p {corpus["crate"]} --test {corpus["test"]}{filter_arg} -- --include-ignored{extra} --nocapture
""")
    return f"""\
name: Nightly accuracy

# External CPU and chip corpora too large, or too dependent on assets, to gate
# every push. They moved here from emu198x/emu198x's nightly-accuracy.yml with
# the crates they test. The tests are ignored by default because the corpora
# are not in this repository; a private release store delivers them, and its
# checksums are verified. The store is a cache, not their provenance or a grant
# of redistribution rights: see emu198x/emu198x test-data/accuracy-corpora.md.
on:
  schedule:
    - cron: '{entry.get("nightly_cron", "40 2 * * *")}'
  workflow_dispatch:

concurrency:
  group: nightly-accuracy-${{{{ github.ref }}}}
  cancel-in-progress: true

permissions:
  contents: read

env:
  # ACCURACY_CORPORA_REPO is an Actions variable, ACCURACY_CORPORA_TOKEN a
  # read-only secret; ACCURACY_CORPORA_TAG defaults to v1.
  CORPORA_REPO: ${{{{ vars.ACCURACY_CORPORA_REPO }}}}
  CORPORA_TAG: ${{{{ vars.ACCURACY_CORPORA_TAG || 'v1' }}}}
  # Every job provisions its fixtures, so a skip here means provisioning
  # failed. emu198x-test-skip makes that a failure instead of a pass.
  EMU198X_STRICT_FIXTURES: "1"

jobs:
  # Secrets can't be read in a job-level `if`. On a fork without the store
  # everything skips; on the canonical repository a missing store fails.
  preflight:
    name: Preflight
    runs-on: ubuntu-latest
    timeout-minutes: 5
    outputs:
      configured: ${{{{ steps.check.outputs.configured }}}}
    steps:
      - name: Check corpora store is configured
        id: check
        env:
          TOKEN: ${{{{ secrets.ACCURACY_CORPORA_TOKEN }}}}
        run: |
          if [ -n "$TOKEN" ] && [ -n "${{CORPORA_REPO}}" ]; then
            echo "configured=true" >> "$GITHUB_OUTPUT"
          else
            echo "configured=false" >> "$GITHUB_OUTPUT"
            if [ "${{{{ github.repository }}}}" = "{ORG}/{entry["name"]}" ]; then
              echo "::error::Corpora store not configured (ACCURACY_CORPORA_TOKEN secret and/or ACCURACY_CORPORA_REPO variable unset)."
              exit 1
            fi
            echo "::warning::Corpora store not configured on this fork. Corpus jobs skipped."
          fi

""" + "\n".join(jobs)


def skeleton(entry: dict, repo_dir: pathlib.Path, crates: list[str], deps: set[str]) -> None:
    name = entry["name"]
    url = f"https://github.com/{ORG}/{name}"
    specs = workspace_dependencies()
    missing = sorted(dep for dep in deps if dep not in specs)
    if missing:
        sys.exit(f"workspace dependencies with no single-line spec here: {missing}")
    dependency_lines = "\n".join(f"{dep} = {specs[dep]}" for dep in sorted(deps))
    crate_rows = "\n".join(
        f"| [`{crate}`](crates/{crate}) | [crates.io](https://crates.io/crates/{crate}) |"
        for crate in crates
    )
    files = {
        "Cargo.toml": f"""\
# {name}: {entry["summary"]}
#
# Moved out of emu198x/emu198x with its history on {datetime.date.today().isoformat()},
# under knowledge/decisions/workspace-shape.md § Amendment there: one repository
# per CPU and per shared chip, published, so each system family and sibling
# takes it from crates.io.
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
edition = "2024"
license = "GPL-2.0-or-later"
homepage = "https://emu198x.github.io/"
repository = "{url}"

[workspace.dependencies]
{dependency_lines}

[workspace.lints.rust]
unsafe_code = "forbid"
unused_must_use = "deny"

[workspace.lints.clippy]
dbg_macro = "deny"
todo = "deny"
unwrap_used = "deny"

[profile.dev]
debug = "line-tables-only"
split-debuginfo = "unpacked"

[profile.release]
lto = "thin"
codegen-units = 1
""" + (f"\n{entry['cargo_extra'].strip()}\n" if entry.get("cargo_extra") else ""),
        ".gitignore": "/target\n.DS_Store\n",
        "README.md": f"""\
# {name}

{entry["summary"]}

| Crate | Registry |
|---|---|
{crate_rows}

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of
cycle-accurate retro-computing emulators. {entry.get("consumers", "")}

Each crate is versioned on its own and bumps only when it changes. Releases go
through release-plz: merging its release pull request tags the crate and
publishes it.

Licensed GPL-2.0-or-later.
""",
        "AGENTS.md": f"""\
# {name}

> Read [`PRINCIPLES.md`](PRINCIPLES.md) first. [`MANIFESTO.md`](MANIFESTO.md) is why the project exists.

{entry["summary"]} It moved out of
[emu198x/emu198x]({url.rsplit("/", 1)[0]}/emu198x) with its history under that
repository's [workspace-shape decision](https://github.com/emu198x/emu198x/blob/main/knowledge/decisions/workspace-shape.md),
and is published so machines and siblings take it from crates.io.

## Binding rules

Emu198x's [RULES.md](https://github.com/emu198x/emu198x/blob/main/RULES.md)
rules 5 and 6 bind here, as they did in-tree:

- **Rule 5:** a CPU ticks at its evidence-backed native granularity, always
  cycle-accurate, never instruction-accurate.
- **Rule 6:** no Bus trait, no bus callback, no method-call memory access.
  Bus state is public pin fields, and the machine inspects them between ticks.
  See [cpu-bus-interface.md](https://github.com/emu198x/emu198x/blob/main/knowledge/decisions/cpu-bus-interface.md).

Those rules are what let this crate live apart from the machines that use it.
A change that would need a machine's help to stay correct belongs back in the
argument, not in this repository.

## Working here

- Rust pinned in `rust-toolchain.toml`. Before pushing: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- Hardware facts cite the umbrella's `reference/` and `syntheses/`, never another
  emulator's code.
- Land work through pull requests, rebase-merged. Every commit subject and PR
  title is a conventional commit; `feat`, `fix`, `perf` and `revert` earn a release.
- A change to the public API is a change for every machine that uses it. Say what
  an existing caller has to do.
""",
        "release-plz.toml": RELEASE_PLZ,
        ".github/dependabot.yml": DEPENDABOT,
        ".github/workflows/ci.yml": CI.format(floor=entry.get("coverage_floor", 50)),
        ".github/workflows/codeql.yml": CODEQL,
        ".github/workflows/pr-title.yml": PR_TITLE,
        ".github/workflows/release-plz.yml": RELEASE_PLZ_WORKFLOW,
        ".github/workflows/release-pr.yml": RELEASE_PR,
        ".github/workflows/rust-toolchain.yml": RUST_TOOLCHAIN,
    }
    nightly = nightly_workflow(entry)
    if nightly:
        files[".github/workflows/nightly-accuracy.yml"] = nightly
    for relative, content in files.items():
        target = repo_dir / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)
    for relative in COPIED:
        shutil.copy2(ROOT / relative, repo_dir / relative)
    claude = repo_dir / "CLAUDE.md"
    if not claude.is_symlink():
        claude.symlink_to("AGENTS.md")


def extract(repo: str, ref: str = "main") -> tuple[pathlib.Path, dict, list[str]]:
    entry = load_config(repo)
    crates = entry["crates"]
    repo_dir = ROOT / "target" / "extract" / repo
    if repo_dir.exists():
        shutil.rmtree(repo_dir)
    repo_dir.parent.mkdir(parents=True, exist_ok=True)

    run("git", "clone", "--quiet", "--no-local", "--single-branch", "--branch", ref,
        str(ROOT), str(repo_dir))
    if ref != "main":
        run("git", "branch", "--quiet", "--force", "--move", ref, "main", cwd=repo_dir)
    if run("git", "rev-parse", "--is-shallow-repository", cwd=repo_dir, capture=True).strip() == "true":
        sys.exit("this checkout is shallow; run `git fetch --unshallow origin main` first")

    filter_args = []
    for crate in crates:
        dirs = historical_dirs(crate)
        for directory in dirs:
            filter_args += ["--path", f"{directory}/"]
        for old in dirs[1:]:
            filter_args += ["--path-rename", f"{old}/:crates/{crate}/"]
    # Data the crates' tests read from this workspace's `test-data/` keeps its
    # path, and its history, in the new repository. It is copied, not moved:
    # other crates here may read it too.
    for data in entry.get("data", []):
        filter_args += ["--path", data]
    run("git", "filter-repo", "--force", "--quiet", *filter_args, cwd=repo_dir)
    # filter-repo drops `origin` itself, so a push can't reach this repository.
    run("git", "checkout", "--quiet", "-b", "extract/skeleton", cwd=repo_dir)

    moving = set(crates)
    used: set[str] = set()
    warnings = []
    for crate in crates:
        warnings += rewrite_manifest(repo_dir / "crates" / crate / "Cargo.toml", moving, used)
    skeleton(entry, repo_dir, crates, used)
    for warning in warnings:
        print(f"warning: {warning}", file=sys.stderr)

    run("git", "add", "-A", cwd=repo_dir)
    message = (
        f"build: Stand {repo} up as its own workspace\n\n"
        f"Moves {', '.join(crates)} out of emu198x/emu198x with "
        f"{'its' if len(crates) == 1 else 'their'} history, "
        "under its workspace-shape decision (layers 1 and 2), and adds the family "
        "skeleton: workspace manifest, the shared CI, CodeQL, PR-title, release-PR "
        "and toolchain workflows, release-plz, and the foundation documents.\n\n"
        "Dependencies that stayed behind are taken from crates.io.\n"
    )
    run("git", "commit", "--quiet", "-m", message, cwd=repo_dir)
    return repo_dir, entry, crates


def verify(repo_dir: pathlib.Path) -> None:
    """Format, Clippy and tests, as the shared CI runs them.

    A helper this workspace has made publishable but not yet published is
    patched in from here, so the checks can run before it is released. The
    patch is a command-line `--config`, never written to the repository.
    """
    patches = []
    for name in PUBLISHED_HELPERS:
        manifest = ROOT / "crates" / name / "Cargo.toml"
        version = re.search(r'^version = "([^"]+)"', manifest.read_text(), re.M)
        if not version:
            sys.exit(f"{name} still takes the suite version here, so it can't stand in for its release")
        if not published(name, version.group(1)):
            patches += ["--config", f'patch.crates-io.{name}.path="{manifest.parent}"']
    run("cargo", "fmt", "--all", "--check", cwd=repo_dir)
    run("cargo", "clippy", *patches, "--workspace", "--all-targets", "--", "-D", "warnings", cwd=repo_dir)
    run("cargo", "test", *patches, "--workspace", cwd=repo_dir)
    # A dry-run publish resolves against the registry, so it waits for the
    # helpers to be released.
    if patches:
        print("skipping cargo publish --dry-run until the test helpers are published")
        return
    metadata = run("cargo", "metadata", "--no-deps", "--format-version", "1", cwd=repo_dir, capture=True)
    for package in json.loads(metadata)["packages"]:
        run("cargo", "publish", "--dry-run", "-p", package["name"], cwd=repo_dir)


def push(repo_dir: pathlib.Path, repo: str) -> None:
    remote = f"https://github.com/{ORG}/{repo}.git"
    run("git", "remote", "add", "origin", remote, cwd=repo_dir)
    run("git", "push", "origin", "main", cwd=repo_dir)
    run("git", "push", "-u", "origin", "extract/skeleton", cwd=repo_dir)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("repo", help="repository name from extract-repos.toml")
    parser.add_argument("--verify", action="store_true", help="run fmt, Clippy and the tests")
    parser.add_argument("--push", action="store_true", help="push to an existing empty repository")
    parser.add_argument("--ref", default="main", help="local branch to extract from (default: main)")
    args = parser.parse_args()

    if args.push and args.ref != "main":
        sys.exit("--push takes main only: a repository's history must be what this one merged")
    repo_dir, _, crates = extract(args.repo, args.ref)
    commits = run("git", "rev-list", "--count", "main", cwd=repo_dir, capture=True).strip()
    print(f"{repo_dir}: {', '.join(crates)} with {commits} commits of history")
    if args.verify:
        verify(repo_dir)
        print("verified: fmt, clippy and tests pass")
    if args.push:
        push(repo_dir, args.repo)
    return 0


RELEASE_PLZ = """\
# On each push to main, release-plz opens or updates a "chore: release" PR
# (version bump and CHANGELOG from conventional-commit prefixes). Merging it
# tags each changed crate and publishes it to crates.io over OIDC (Trusted
# Publishing) in the same run, with no stored registry token.
[workspace]
publish = true
git_release_enable = true
git_tag_enable = true
changelog_update = true
# Only reader-facing changes earn a release. Release-process work uses `ci:` or
# `chore(release):`; changelog filtering alone does not prevent a version bump.
release_commits = "^(feat|fix|perf|revert)[(:]"
# Package-prefixed tags, so a second crate here stays unambiguous.
git_tag_name = "{{ package }}-v{{ version }}"

[changelog]
protect_breaking_commits = true
commit_parsers = [
  { message = '^chore(\\([^)]*\\))?: release\\b', skip = true },
  { message = '^(feat|fix|perf|revert)\\((ci|release)\\)', skip = true },
  { message = "^feat", group = "Added" },
  { message = "^fix", group = "Fixed" },
  { message = "^perf", group = "Performance" },
  { message = "^revert", group = "Reverted" },
  { message = '^(chore|build)\\(deps\\)', group = "Dependencies" },
  { message = "^(docs|test|chore|ci|build|refactor|style)", skip = true },
  { message = "^Merge", skip = true },
  { message = ".*", group = "Other" },
]
"""

DEPENDABOT = """\
# Dependabot version updates, the same as the other 198x Rust repos. Minor and
# patch updates are grouped per ecosystem; majors open their own PRs.
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 10
    groups:
      cargo-minor-patch:
        update-types:
          - "minor"
          - "patch"

  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 5
    groups:
      github-actions:
        update-types:
          - "minor"
          - "patch"
"""

CI = """\
name: CI

on:
  push:
    branches:
      - main
  pull_request:

concurrency:
  group: ci-${{{{ github.event.pull_request.number || github.ref }}}}
  cancel-in-progress: true

permissions:
  contents: read

jobs:
  # Format, Clippy, coverage (the Linux test run) and a build and test on
  # macOS and Windows, shared by every 198x Rust repo
  # (stevehill1981/198x-workflows).
  rust:
    name: Rust
    uses: stevehill1981/198x-workflows/.github/workflows/rust-ci.yml@v1
    with:
      # Ratchet up as tests grow; never down. Corpus tests are ignored here
      # and run nightly, so this measures the unit and fixture-free tests.
      coverage-floor: {floor}
"""

CODEQL = """\
name: CodeQL

# Code scanning on pushes to main, weekly and by hand, not on every PR push,
# using the family's shared workflow.
on:
  push:
    branches:
      - main
  schedule:
    - cron: '41 4 * * 0'
  workflow_dispatch:

concurrency:
  group: codeql-${{ github.ref }}
  cancel-in-progress: true

permissions:
  contents: read

jobs:
  codeql:
    name: CodeQL
    uses: stevehill1981/198x-workflows/.github/workflows/codeql.yml@v1
    permissions:
      contents: read
      security-events: write
      actions: read
    with:
      languages: '["actions", "rust"]'
"""

PR_TITLE = """\
name: PR title

# Shared by every 198x repo (stevehill1981/198x-workflows): the title and
# every commit subject must be conventional commits. `edited` re-runs them
# when only the title changes.
on:
  pull_request:
    types: [opened, edited, reopened, synchronize]

concurrency:
  group: pr-title-${{ github.event.pull_request.number }}
  cancel-in-progress: true

permissions:
  contents: read

jobs:
  pr-title:
    name: PR title
    uses: stevehill1981/198x-workflows/.github/workflows/pr-title.yml@v1
"""

RELEASE_PLZ_WORKFLOW = """\
name: Release-plz

# Tags merged releases, then opens or updates the next release PR, using the
# workflow every 198x Rust repo shares (stevehill1981/198x-workflows).
on:
  push:
    branches:
      - main

permissions:
  contents: write
  pull-requests: write
  # crates.io Trusted Publishing matches this file's name, so keep it.
  id-token: write

jobs:
  release-plz:
    name: Release-plz
    uses: stevehill1981/198x-workflows/.github/workflows/release-plz.yml@v1
    # Named, not `inherit`: GitHub passes inherited secrets only within one
    # organisation or enterprise, and the shared workflow is in a personal
    # account. A secret this org lacks is passed empty.
    secrets:
      RELEASE_PLZ_TOKEN: ${{ secrets.RELEASE_PLZ_TOKEN }}
      DISCORD_RELEASE_WEBHOOK: ${{ secrets.DISCORD_RELEASE_WEBHOOK }}
      DISCORD_RELEASE_WEBHOOK_ALL: ${{ secrets.DISCORD_RELEASE_WEBHOOK_ALL }}
    with:
      announce: true
"""

RELEASE_PR = """\
name: Release PR

# Shared by every 198x repo (stevehill1981/198x-workflows): a release PR must
# add a changelog heading for every version it sets.
on:
  pull_request:
    types: [opened, edited, reopened, synchronize]

concurrency:
  group: release-pr-${{ github.event.pull_request.number }}
  cancel-in-progress: true

permissions:
  contents: read

jobs:
  release-pr:
    name: Release PR
    uses: stevehill1981/198x-workflows/.github/workflows/release-pr.yml@v1
"""

RUST_TOOLCHAIN = """\
name: Rust toolchain

# Keeps rust-toolchain.toml on the latest stable Rust: when stable moves on,
# the shared workflow opens a `chore: build with Rust <version>` PR so CI
# checks the new compiler first.
on:
  schedule:
    - cron: '17 6 * * 1'
  workflow_dispatch:

concurrency:
  group: rust-toolchain
  cancel-in-progress: false

permissions:
  contents: write
  pull-requests: write

jobs:
  toolchain:
    name: Rust toolchain
    uses: stevehill1981/198x-workflows/.github/workflows/rust-toolchain.yml@v1
    # Named, not `inherit`: inherited secrets don't cross owners.
    secrets:
      RELEASE_PLZ_TOKEN: ${{ secrets.RELEASE_PLZ_TOKEN }}
"""


if __name__ == "__main__":
    raise SystemExit(main())

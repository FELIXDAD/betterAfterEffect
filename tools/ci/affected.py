#!/usr/bin/env python3
"""Which workspace crates a pull request affects, for the fast CI tier.

Prints (and with --github writes to $GITHUB_OUTPUT) one of three modes:

  none    no crate is affected (docs and other non-Rust files only): skip building;
  crates  test only the changed crates and everything that depends on them;
  full    test the whole workspace (root manifests, lockfile, toolchain, CI, xtask,
          vendored crates, or a path the script cannot attribute).

A changed file outside every crate still affects a crate whose sources name it in a
string literal (`include_str!("../../../docs/ui/ae-commands.md")`, a test reading
`docs/reference/ae/menus.json`), so docs that code reads are not "docs only".

Reverse dependencies come from `cargo metadata --no-deps` (normal, dev and build
dependencies between workspace members). See docs/spec/ci.md.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from collections import deque
from pathlib import Path

# Changes under these paths rebuild or reconfigure everything.
FULL_PREFIXES = (".github/", ".cargo/", "vendor/", "xtask/")
# Top-level files that are not workspace inputs (everything else at the root, such as
# Cargo.toml, Cargo.lock, rust-toolchain.toml or rustfmt.toml, means full).
ROOT_DOC_SUFFIXES = (".md",)

STRING_LITERAL = re.compile(r'"((?:[^"\\\n]|\\.)*)"')


def run(cmd: list[str], cwd: Path) -> str:
    return subprocess.run(cmd, cwd=cwd, check=True, capture_output=True, text=True).stdout


def member_globs(root_manifest: str) -> tuple[list[str], list[str]]:
    """The `[workspace] members` patterns: directory roots of `dir/*` globs and exact paths."""
    m = re.search(r"^\s*members\s*=\s*\[(.*?)\]", root_manifest, re.S | re.M)
    patterns = re.findall(r'"([^"]+)"', m.group(1)) if m else []
    roots = [p[:-2] for p in patterns if p.endswith("/*")]
    exact = [p for p in patterns if "*" not in p]
    return roots, exact


def literal_references(text: str, names: list[str]) -> set[str]:
    """Repository paths under the top-level `names` that string literals in `text` name.

    A bare word (`"tools"`, a JSON key) is not a path: the name must be preceded or
    followed by a `/` inside the literal.
    """
    if not names:
        return set()
    alternation = "|".join(re.escape(n) for n in sorted(names, key=len, reverse=True))
    path = re.compile(r"(?:(?<=/)|^)((?:%s)(?:/[^\s\"]*)?)" % alternation)
    refs = set()
    for lit in STRING_LITERAL.finditer(text):
        s = lit.group(1)
        for m in path.finditer(s):
            ref = m.group(1)
            preceded = m.start(1) > 0 and s[m.start(1) - 1] == "/"
            if not preceded and "/" not in ref:
                continue
            ref = ref.rstrip("/")
            if ref:
                refs.add(ref)
    return refs


def classify(
    changed: list[str],
    members: dict[str, str],
    glob_roots: list[str],
    exact_members: list[str],
    refs: dict[str, set[str]],
) -> tuple[str, set[str], list[str]]:
    """Returns (mode, directly affected members, reasons).

    `members` maps a member's directory (relative, posix) to its package name;
    `refs` maps a package name to the outside paths its sources name.
    """
    reasons: list[str] = []
    direct: set[str] = set()
    dirs = sorted(members, key=len, reverse=True)
    for path in changed:
        if path.startswith(FULL_PREFIXES):
            reasons.append(f"full: {path}")
            continue
        if "/" not in path and not path.endswith(ROOT_DOC_SUFFIXES):
            reasons.append(f"full: {path} (workspace root)")
            continue
        owner = next((members[d] for d in dirs if path == d or path.startswith(d + "/")), None)
        if owner is not None:
            direct.add(owner)
            continue
        if any(path.startswith(r + "/") for r in glob_roots) or any(
            path == e or path.startswith(e + "/") for e in exact_members
        ):
            # A crate that no longer (or does not yet) exist as a member: added,
            # removed or renamed crates change the workspace itself.
            reasons.append(f"full: {path} (not in a current member)")
            continue
        readers = {pkg for pkg, paths in refs.items() if any(path == r or path.startswith(r + "/") for r in paths)}
        if readers:
            direct |= readers
            reasons.append(f"{path}: read by {', '.join(sorted(readers))}")
    if any(r.startswith("full:") for r in reasons):
        return "full", direct, reasons
    if not direct:
        return "none", direct, reasons
    return "crates", direct, reasons


def downstream(direct: set[str], deps: dict[str, set[str]]) -> set[str]:
    """`direct` plus every member that depends on one of them, transitively."""
    rev: dict[str, set[str]] = {}
    for pkg, ds in deps.items():
        for d in ds:
            rev.setdefault(d, set()).add(pkg)
    out, queue = set(direct), deque(direct)
    while queue:
        for user in rev.get(queue.popleft(), ()):
            if user not in out:
                out.add(user)
                queue.append(user)
    return out


def workspace(root: Path) -> tuple[dict[str, str], dict[str, set[str]]]:
    """Member directories → names, and each member's workspace dependencies."""
    meta = json.loads(run(["cargo", "metadata", "--no-deps", "--format-version", "1"], root))
    ws_root = Path(meta["workspace_root"])
    by_dir: dict[Path, str] = {}
    for pkg in meta["packages"]:
        by_dir[Path(pkg["manifest_path"]).parent] = pkg["name"]
    members = {d.relative_to(ws_root).as_posix(): name for d, name in by_dir.items()}
    deps: dict[str, set[str]] = {}
    for pkg in meta["packages"]:
        deps[pkg["name"]] = {
            by_dir[Path(d["path"])] for d in pkg["dependencies"] if d.get("path") and Path(d["path"]) in by_dir
        }
    return members, deps


def source_references(root: Path, members: dict[str, str], outside: list[str]) -> dict[str, set[str]]:
    refs: dict[str, set[str]] = {}
    for rel, name in members.items():
        found: set[str] = set()
        for rs in (root / rel).rglob("*.rs"):
            if "target" in rs.relative_to(root / rel).parts:
                continue
            found |= literal_references(rs.read_text(encoding="utf-8", errors="replace"), outside)
        if found:
            refs[name] = found
    return refs


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--base", default="HEAD^1", help="compare --head with this commit")
    parser.add_argument("--head", default="HEAD", help="the pull request's (merge) commit")
    parser.add_argument("--full", action="store_true", help="force full mode")
    parser.add_argument("--github", action="store_true", help="write $GITHUB_OUTPUT / $GITHUB_STEP_SUMMARY")
    args = parser.parse_args()
    root = Path(run(["git", "rev-parse", "--show-toplevel"], Path.cwd()).strip())

    members, deps = workspace(root)
    if args.full:
        mode, direct, reasons, changed = "full", set(), ["full: requested"], []
    else:
        try:
            changed = run(["git", "diff", "--name-only", "--no-renames", args.base, args.head], root).split()
        except subprocess.CalledProcessError as err:
            changed = []
            mode, direct, reasons = "full", set(), [f"full: cannot diff against {args.base}: {err.stderr.strip()}"]
        else:
            glob_roots, exact = member_globs((root / "Cargo.toml").read_text(encoding="utf-8"))
            member_tops = {d.split("/")[0] for d in members}
            outside = sorted(
                p.name
                for p in root.iterdir()
                if not p.name.startswith(".") and p.name not in member_tops and p.name != "target"
            )
            # tools/ holds a member (tools/dep-probe) and non-crate files; keep it scannable.
            if "tools" not in outside and (root / "tools").is_dir():
                outside.append("tools")
            refs = source_references(root, members, outside)
            mode, direct, reasons = classify(changed, members, glob_roots, exact, refs)
    packages = sorted(downstream(direct, deps)) if mode == "crates" else []

    print(f"mode: {mode}")
    print(f"changed files: {len(changed)}")
    for r in reasons:
        print(f"  {r}")
    if mode == "crates":
        print(f"directly changed: {' '.join(sorted(direct))}")
        print(f"to test ({len(packages)}): {' '.join(packages)}")

    if args.github:
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
            out.write(f"mode={mode}\npackages={' '.join(packages)}\n")
        summary = os.environ.get("GITHUB_STEP_SUMMARY")
        if summary:
            with open(summary, "a", encoding="utf-8") as out:
                out.write(f"### Fast tier: `{mode}`\n\n")
                if mode == "crates":
                    out.write(f"Tests: {', '.join(f'`{p}`' for p in packages)}\n\n")
                for r in reasons[:50]:
                    out.write(f"- {r}\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())

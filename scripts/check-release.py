#!/usr/bin/env python3
"""Validate finalized release source or its exact signed tag."""

from __future__ import annotations

import re
import subprocess
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PACKAGES = ("zhold-core", "zhold-store", "zhold")


def fail(message: str) -> None:
    raise SystemExit(f"release check failed: {message}")


def git(*arguments: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *arguments],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if check and result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        fail(f"git {' '.join(arguments)}: {detail}")
    return result.stdout.strip()


def load(path: Path) -> dict[str, object]:
    with path.open("rb") as source:
        return tomllib.load(source)


def table(value: object, name: str) -> dict[str, object]:
    if not isinstance(value, dict):
        fail(f"{name} must be a table")
    return value


def text(value: object, name: str) -> str:
    if not isinstance(value, str):
        fail(f"{name} must be text")
    return value


def check_manifests(version: str) -> None:
    workspace = table(load(ROOT / "Cargo.toml").get("workspace"), "workspace")
    package = table(workspace.get("package"), "workspace.package")
    if text(package.get("version"), "workspace.package.version") != version:
        fail(f"workspace version is not {version}")
    if text(package.get("rust-version"), "workspace.package.rust-version") != "1.91.1":
        fail("workspace MSRV is not 1.91.1")

    dependencies = table(workspace.get("dependencies"), "workspace.dependencies")
    for name in ("zhold-core", "zhold-store"):
        dependency = table(dependencies.get(name), f"workspace.dependencies.{name}")
        if text(dependency.get("version"), f"{name}.version") != version:
            fail(f"{name} dependency version is not {version}")

    for name in PACKAGES:
        directory = "zhold-cli" if name == "zhold" else name
        manifest = load(ROOT / "crates" / directory / "Cargo.toml")
        crate = table(manifest.get("package"), f"{name}.package")
        if crate.get("version") != {"workspace": True}:
            fail(f"{name} does not inherit the workspace version")
        if crate.get("publish") != ["crates-io"]:
            fail(f"{name} is not restricted to crates.io")


def check_clean() -> None:
    if git("status", "--porcelain", "--untracked-files=all"):
        fail("worktree is not clean")


def check_tag(version: str) -> None:
    tag = f"v{version}"
    tag_type = git("cat-file", "-t", f"refs/tags/{tag}")
    if tag_type != "tag":
        fail(f"{tag} is not an annotated tag")
    head = git("rev-parse", "HEAD")
    target = git("rev-parse", f"refs/tags/{tag}^{{commit}}")
    if target != head:
        fail(f"{tag} does not identify HEAD")
    git("verify-commit", "HEAD")
    git("tag", "-v", tag)


def main() -> None:
    if len(sys.argv) != 3 or sys.argv[1] not in {"source", "tag"}:
        fail("usage: scripts/check-release.py source|tag VERSION")
    mode, version = sys.argv[1:]
    if re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) is None:
        fail(f"invalid version {version!r}")
    check_manifests(version)
    check_clean()
    if mode == "tag":
        check_tag(version)
    print(f"release {mode} passed: {version}")


if __name__ == "__main__":
    main()

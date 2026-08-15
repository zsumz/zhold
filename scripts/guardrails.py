#!/usr/bin/env python3
"""Repository architecture and hygiene checks that do not require Rust."""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CRATES = ROOT / "crates"
MAX_RUST_LINES = 300


class Checks:
    def __init__(self) -> None:
        self.failures: list[str] = []

    def require(self, condition: bool, message: str) -> None:
        if not condition:
            self.failures.append(message)

    def finish(self) -> int:
        if self.failures:
            print("guardrails failed:", file=sys.stderr)
            for failure in self.failures:
                print(f"  - {failure}", file=sys.stderr)
            return 1
        print("guardrails passed")
        return 0


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def rust_files() -> list[Path]:
    return sorted(CRATES.rglob("*.rs"))


def check_required_files(checks: Checks) -> None:
    required = [
        "Cargo.toml",
        "README.md",
        "LICENSE",
        "scripts/check",
    ]
    for name in required:
        checks.require((ROOT / name).is_file(), f"missing required file {name}")


def check_workspace(checks: Checks) -> None:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    workspace = manifest.get("workspace", {})
    package = workspace.get("package", {})
    checks.require(workspace.get("resolver") == "3", "workspace resolver must be 3")
    checks.require(package.get("edition") == "2024", "workspace edition must be 2024")
    checks.require(package.get("rust-version") == "1.91", "workspace MSRV must be 1.91")

    expected = {
        "crates/zhold-cli",
        "crates/zhold-core",
        "crates/zhold-store",
    }
    checks.require(set(workspace.get("members", [])) == expected, "unexpected workspace members")

    manifests: dict[str, dict] = {}
    for path in sorted(CRATES.glob("*/Cargo.toml")):
        data = tomllib.loads(path.read_text(encoding="utf-8"))
        manifests[data["package"]["name"]] = data

    checks.require(set(manifests) == {"zhold", "zhold-core", "zhold-store"}, "unexpected crate packages")
    cli = manifests["zhold"]
    binaries = cli.get("bin", [])
    checks.require(
        len(binaries) == 1 and binaries[0].get("name") == "zhold",
        "the public package must install one zhold binary",
    )

    core_deps = manifests["zhold-core"].get("dependencies", {})
    store_deps = manifests["zhold-store"].get("dependencies", {})
    checks.require("zhold-store" not in core_deps, "zhold-core must not depend on zhold-store")
    checks.require("zhold" not in core_deps, "zhold-core must not depend on zhold")
    checks.require("zhold" not in store_deps, "zhold-store must not depend on zhold")


def check_name_contract(checks: Checks) -> None:
    legacy = "z" + "stash"
    suffixes = {".json", ".md", ".py", ".rs", ".toml", ".yml", ".yaml"}
    for path in sorted(ROOT.rglob("*")):
        if not path.is_file() or ".git" in path.parts:
            continue
        if path.suffix not in suffixes and path.parent.name != "scripts":
            continue
        text = path.read_text(encoding="utf-8")
        checks.require(legacy not in text.casefold(), f"{relative(path)} contains the legacy name")

    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    checks.require("zhold cargo test" in readme, "README must lead with the simple Cargo path")
    checks.require("zhold gc 100GiB" in readme, "README must show positional GC budget syntax")


def check_text_hygiene(checks: Checks, path: Path, text: str) -> None:
    name = relative(path)
    checks.require(text.endswith("\n"), f"{name} must end with a newline")
    for number, line in enumerate(text.splitlines(), start=1):
        checks.require("\t" not in line, f"{name}:{number} contains a tab")
        checks.require(line == line.rstrip(), f"{name}:{number} has trailing whitespace")


def check_rust(checks: Checks) -> None:
    forbidden = {
        r"\bunsafe\b": "unsafe code",
        r"\.unwrap\s*\(": "unwrap",
        r"\.expect\s*\(": "expect",
        r"\bpanic!\s*\(": "panic!",
        r"\btodo!\s*\(": "todo!",
        r"\bunimplemented!\s*\(": "unimplemented!",
        r"\bdbg!\s*\(": "dbg!",
        r"^\s*pub\s+mod\s+": "public module",
        r"^\s*pub\s+use\s+.*::\*": "wildcard public re-export",
    }
    facade_item = re.compile(
        r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:fn|struct|enum|trait|impl)\b",
        re.MULTILINE,
    )

    for path in rust_files():
        text = path.read_text(encoding="utf-8")
        name = relative(path)
        check_text_hygiene(checks, path, text)
        line_count = len(text.splitlines())
        checks.require(
            line_count <= MAX_RUST_LINES,
            f"{name} has {line_count} lines; maximum is {MAX_RUST_LINES}",
        )
        for pattern, label in forbidden.items():
            if re.search(pattern, text, flags=re.MULTILINE):
                checks.failures.append(f"{name} contains forbidden {label}")
        if "#[test]" in text:
            checks.require(path.name.endswith("_test.rs"), f"tests must be separate: {name}")
        if path.name in {"lib.rs", "mod.rs"}:
            checks.require(not facade_item.search(text), f"facade contains implementation item: {name}")


def check_test_modules(checks: Checks) -> None:
    for test_path in sorted(CRATES.rglob("*_test.rs")):
        parent = test_path.parent
        if parent.name == "tests":
            continue
        facade = parent / "mod.rs"
        if not facade.exists():
            facade = parent / "lib.rs"
        module = test_path.stem
        text = facade.read_text(encoding="utf-8") if facade.exists() else ""
        checks.require(
            re.search(rf"\bmod\s+{re.escape(module)}\s*;", text) is not None,
            f"{relative(test_path)} is not declared by its facade",
        )


def check_delimiters(checks: Checks) -> None:
    pairs = {"(": ")", "[": "]", "{": "}"}
    closers = set(pairs.values())
    for path in rust_files():
        text = strip_rust_literals_and_comments(path.read_text(encoding="utf-8"))
        stack: list[tuple[str, int]] = []
        for index, character in enumerate(text):
            if character in pairs:
                stack.append((character, index))
            elif character in closers:
                if not stack or pairs[stack[-1][0]] != character:
                    checks.failures.append(f"{relative(path)} has an unmatched {character}")
                    break
                stack.pop()
        if stack:
            checks.failures.append(f"{relative(path)} has an unmatched {stack[-1][0]}")


def strip_rust_literals_and_comments(text: str) -> str:
    output = list(text)
    index = 0
    block_depth = 0
    while index < len(text):
        if block_depth:
            if text.startswith("/*", index):
                block_depth += 1
                output[index : index + 2] = "  "
                index += 2
            elif text.startswith("*/", index):
                block_depth -= 1
                output[index : index + 2] = "  "
                index += 2
            else:
                if text[index] != "\n":
                    output[index] = " "
                index += 1
            continue
        if text.startswith("//", index):
            end = text.find("\n", index)
            end = len(text) if end == -1 else end
            output[index:end] = " " * (end - index)
            index = end
        elif text.startswith("/*", index):
            block_depth = 1
            output[index : index + 2] = "  "
            index += 2
        elif text[index] == '"':
            index = blank_quoted(text, output, index, '"')
        elif text[index] == "'":
            char_end = char_literal_end(text, index)
            if char_end is None:
                index += 1
            else:
                output[index:char_end] = " " * (char_end - index)
                index = char_end
        elif text[index] == "r":
            raw_end = raw_string_end(text, index)
            if raw_end is None:
                index += 1
            else:
                output[index:raw_end] = " " * (raw_end - index)
                index = raw_end
        else:
            index += 1
    return "".join(output)


def char_literal_end(text: str, start: int) -> int | None:
    index = start + 1
    if index >= len(text) or text[index] == "\n":
        return None
    if text[index] == "\\":
        index += 2
        while index < len(text) and text[index] not in {"'", "\n"}:
            index += 1
    else:
        index += 1
    if index < len(text) and text[index] == "'":
        return index + 1
    return None


def blank_quoted(text: str, output: list[str], start: int, quote: str) -> int:
    index = start + 1
    output[start] = " "
    while index < len(text):
        character = text[index]
        if character != "\n":
            output[index] = " "
        if character == "\\":
            index += 2
            continue
        index += 1
        if character == quote:
            break
    return index


def raw_string_end(text: str, start: int) -> int | None:
    index = start + 1
    while index < len(text) and text[index] == "#":
        index += 1
    if index >= len(text) or text[index] != '"':
        return None
    hashes = index - start - 1
    terminator = '"' + "#" * hashes
    end = text.find(terminator, index + 1)
    return len(text) if end == -1 else end + len(terminator)


def main() -> int:
    checks = Checks()
    check_required_files(checks)
    check_workspace(checks)
    check_name_contract(checks)
    check_rust(checks)
    check_test_modules(checks)
    check_delimiters(checks)
    return checks.finish()


if __name__ == "__main__":
    raise SystemExit(main())

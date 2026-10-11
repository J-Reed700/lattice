#!/usr/bin/env python3
"""Check that every Tauri command is registered in all three places it needs to be.

A Tauri v2 command only works if it appears in *all* of:

  1. the plugin's ``tauri::generate_handler![...]``  (src-tauri/src/**/plugin*.rs)
  2. ``build.rs``'s ``InlinedPlugin::new().commands(&[...])``
  3. ``capabilities/main.json`` as ``<plugin>:allow-<command-with-dashes>``

Miss (1) and the build fails, which is fine. Miss (2) or (3) and everything
compiles, the TypeScript binding is generated, and the command is rejected by
the ACL *at runtime* with an error that does not name the missing permission.
That failure mode is why this script exists: `compact_conversation` shipped in
the handler and in the bindings while missing from both (2) and (3), and nothing
caught it until someone tried to run it.

`export_bindings --check` and `scripts/check-ipc-contracts.mjs` cover the
bindings side only. This covers the ACL side.

It also ratchets orphans: production functions carrying `#[tauri::command]`
that no generate_handler! registers. Today's orphans are recorded in
``scripts/tauri-command-orphans.baseline``; a new orphan fails, and so does a
baseline entry whose orphan is gone, until ``--write-baseline`` (or
``bash scripts/check-rust-layer-boundaries.sh --write-baseline``, which
regenerates every architecture baseline) shrinks the file.

Exit code 0 when the three agree and the orphans match the baseline, 1 otherwise.
"""

from __future__ import annotations

import json
import os
import re
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri"
ORPHAN_BASELINE = ROOT / "scripts" / "tauri-command-orphans.baseline"
ORPHAN_RULE = "orphan-command"
REGENERATE = "bash scripts/check-rust-layer-boundaries.sh --write-baseline"
ORPHAN_HEADER = """\
# Tauri command-orphan baseline, read by scripts/check-tauri-command-inventory.py.
#
# Each line is `orphan-command <file under src-tauri/src> <fn> <count>`: a
# production function carrying #[tauri::command] that no generate_handler!
# registers. CI fails when a new orphan appears and when a listed one is gone,
# so this file only shrinks. Delete the dead command (or register it), then
# regenerate every architecture baseline with
#
#     bash scripts/check-rust-layer-boundaries.sh --write-baseline
#
# and commit the smaller file.
"""

# Commands that are deliberately reachable without a capability entry, each with
# the reason. Keep this empty unless there is a real one; an entry here is a
# permanently unchecked command.
ALLOWED_WITHOUT_CAPABILITY: dict[tuple[str, str], str] = {}


def strip_comments(text: str) -> str:
    """Remove // and /* */ so a commented-out command is not counted as present."""
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"//[^\n]*", "", text)


def balanced(text: str, start: int, open_ch: str, close_ch: str) -> str:
    """Return the text between the delimiters beginning at `start`."""
    depth = 0
    for index in range(start, len(text)):
        if text[index] == open_ch:
            depth += 1
        elif text[index] == close_ch:
            depth -= 1
            if depth == 0:
                return text[start + 1 : index]
    return ""


def handler_commands() -> dict[str, set[str]]:
    """Plugin name -> commands in its generate_handler!."""
    found: dict[str, set[str]] = {}
    # Both layouts exist: `plugin.rs` beside its feature, and `plugin/mod.rs`
    # for features whose registration outgrew one file.
    paths = sorted(
        path
        for path in SRC.glob("src/**/*.rs")
        if path.stem.startswith("plugin") or "plugin" in path.parent.name
    )
    for path in paths:
        text = strip_comments(path.read_text(encoding="utf-8"))
        for match in re.finditer(r'Builder::new\(\s*"([^"]+)"\s*\)', text):
            plugin = match.group(1)
            handler = re.search(r"generate_handler!\s*\[", text[match.end() :])
            if not handler:
                continue
            body_start = match.end() + handler.end() - 1
            body = balanced(text, body_start, "[", "]")
            commands = {
                # `module::command` and plain `command` both register as the
                # final segment, which is the name the frontend invokes.
                entry.strip().split("::")[-1]
                for entry in body.split(",")
                if entry.strip() and not entry.strip().startswith("#")
            }
            found.setdefault(plugin, set()).update(c for c in commands if c.isidentifier())
    return found


def build_rs_commands() -> dict[str, set[str]]:
    """Plugin name -> commands declared in build.rs."""
    text = strip_comments((SRC / "build.rs").read_text(encoding="utf-8"))
    found: dict[str, set[str]] = {}
    for match in re.finditer(r'\.plugin\(\s*"([^"]+)"\s*,', text):
        plugin = match.group(1)
        commands_at = text.find("commands(&[", match.end())
        if commands_at == -1:
            continue
        body = balanced(text, text.index("[", commands_at), "[", "]")
        found.setdefault(plugin, set()).update(re.findall(r'"([^"]+)"', body))
    return found


def capability_permissions() -> set[str]:
    data = json.loads((SRC / "capabilities" / "main.json").read_text(encoding="utf-8"))
    return {p for p in data.get("permissions", []) if isinstance(p, str)}


COMMAND_FN = re.compile(
    r"#\[\s*tauri\s*::\s*command\b[^\]]*\]"
    r"(?:\s*#\[[^\]]*\])*"
    r"\s*(?:pub(?:\s*\([^)]*\))?\s+)?"
    r"(?:(?:async|const|unsafe)\s+)*"
    r"fn\s+(?:r#)?([A-Za-z_][A-Za-z0-9_]*)"
)
# `#[cfg(test)]`, `#[cfg(any(test, doc))]` or `#[cfg(all(test, ...))]`.
CFG_TEST = (
    r"#\[\s*cfg\s*\(\s*(?:test|any\s*\(\s*(?:test\s*,\s*doc|doc\s*,\s*test)\s*\)"
    r"|all\s*\([^)]*\btest\b[^)]*\))\s*\)\s*\]"
)
ATTRIBUTES = r"(?:\s*#\[[^\]]*\])*"
VISIBILITY = r"\s*(?:pub(?:\s*\([^)]*\))?\s+)?"
TEST_MODULE = re.compile(CFG_TEST + ATTRIBUTES + VISIBILITY + r"mod\s+\w+\s*\{")
TEST_MODULE_FILE = re.compile(
    CFG_TEST + r"(" + ATTRIBUTES + r")" + VISIBILITY + r"mod\s+(\w+)\s*;"
)


def is_test_file(relative: Path) -> bool:
    """Test sources by convention: the crate's tests/ tree and *tests.rs files."""
    name = relative.name
    return (
        relative.parts[0] == "tests"
        or name in {"tests.rs", "test.rs"}
        or name.endswith(("_tests.rs", "_test.rs"))
        or name.startswith(("tests_", "test_"))
    )


# Comments, then string, raw-string and char literals, in one left-to-right pass
# so a `//` inside a string or a brace inside a literal cannot confuse the scan.
LEXEMES = re.compile(
    r"//[^\n]*"
    r"|/\*.*?\*/"
    r'|b?r(#*)".*?"\1'
    r'|b?"(?:\\.|[^"\\])*"'
    r"|b?'(?:\\(?:u\{[0-9a-fA-F]*\}|x[0-9a-fA-F]{2}|.)|[^\\'\n])'",
    re.S,
)


def code_only(text: str, keep_literals: bool = False) -> str:
    """Source with comments blanked and, unless kept, every literal emptied."""

    def blank(match: re.Match[str]) -> str:
        lexeme = match.group(0)
        if lexeme.startswith("/"):
            return " "
        if keep_literals:
            return lexeme
        return "''" if lexeme.lstrip("b").startswith("'") else '""'

    return LEXEMES.sub(blank, text)


def production_text(text: str) -> str:
    """Code outside test files' `#![cfg(test)]` and `#[cfg(test)] mod name { ... }`."""
    text = code_only(text)
    if re.search(CFG_TEST.replace("#", "#!", 1), text):
        return ""
    while match := TEST_MODULE.search(text):
        depth = 0
        for index in range(match.end() - 1, len(text)):
            depth += {"{": 1, "}": -1}.get(text[index], 0)
            if depth == 0:
                break
        text = text[: match.start()] + text[index + 1 :]
    return text


def test_module_files(relative: Path, text: str) -> list[Path]:
    """Files (or module directories) that `text` declares behind a test-only cfg."""
    module_dir = (
        relative.parent
        if relative.name in {"mod.rs", "lib.rs", "main.rs"}
        else relative.parent / relative.stem
    )
    found = []
    for match in TEST_MODULE_FILE.finditer(text):
        explicit = re.search(r'#\[\s*path\s*=\s*"([^"]*)"\s*\]', match.group(1))
        if explicit and explicit.group(1):
            found.append(Path(os.path.normpath(relative.parent / explicit.group(1))))
        else:
            found += [module_dir / f"{match.group(2)}.rs", module_dir / match.group(2)]
    return found


def production_sources(src_dir: Path) -> dict[str, str]:
    """`/`-separated path under src_dir -> production text; test files skipped."""
    raw = {
        path.relative_to(src_dir): path.read_text(encoding="utf-8")
        for path in sorted(src_dir.rglob("*.rs"))
    }
    excluded = {
        test
        for relative, text in raw.items()
        for test in test_module_files(relative, code_only(text, keep_literals=True))
    }
    return {
        relative.as_posix(): production_text(text)
        for relative, text in raw.items()
        if not is_test_file(relative)
        and not any(relative == test or test in relative.parents for test in excluded)
    }


def registered_names(sources: dict[str, str]) -> set[str]:
    """Every command name any generate_handler! registers, by its final segment."""
    names: set[str] = set()
    for text in sources.values():
        for handler in re.finditer(r"generate_handler!\s*\[", text):
            body = balanced(text, handler.end() - 1, "[", "]")
            names.update(
                entry.strip().split("::")[-1]
                for entry in body.split(",")
                if entry.strip() and not entry.strip().startswith("#")
            )
    return names


def orphan_commands(sources: dict[str, str]) -> Counter[tuple[str, str]]:
    """(file, fn) -> count of #[tauri::command] functions nothing registers."""
    registered = registered_names(sources)
    orphans: Counter[tuple[str, str]] = Counter()
    for relative, text in sources.items():
        for match in COMMAND_FN.finditer(text):
            if match.group(1) not in registered:
                orphans[(relative, match.group(1))] += 1
    return orphans


def parse_orphan_baseline(text: str) -> Counter[tuple[str, str]]:
    baseline: Counter[tuple[str, str]] = Counter()
    for number, line in enumerate(text.splitlines(), start=1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        fields = line.split()
        if len(fields) != 4 or fields[0] != ORPHAN_RULE or not fields[3].isdigit() or fields[3] == "0":
            raise ValueError(
                f"{ORPHAN_BASELINE.name} line {number}: expected "
                f"`{ORPHAN_RULE} <file> <fn> <positive count>`"
            )
        if (fields[1], fields[2]) in baseline:
            raise ValueError(f"{ORPHAN_BASELINE.name} line {number}: duplicate entry")
        baseline[(fields[1], fields[2])] = int(fields[3])
    return baseline


def render_orphan_baseline(orphans: Counter[tuple[str, str]]) -> str:
    lines = [f"{ORPHAN_RULE} {file} {name} {count}\n" for (file, name), count in sorted(orphans.items())]
    return ORPHAN_HEADER + ("\n" + "".join(lines) if lines else "")


def orphan_problems(
    baseline: Counter[tuple[str, str]], current: Counter[tuple[str, str]]
) -> list[str]:
    problems = []
    for key in sorted(set(baseline) | set(current)):
        file, name = key
        was, now = baseline.get(key, 0), current.get(key, 0)
        if now > was:
            problems.append(
                f"{file}: `{name}` carries #[tauri::command] but no generate_handler! registers it "
                f"({now} found, baseline allows {was}); register it or delete it"
            )
        elif now < was:
            problems.append(
                f"STALE: {file}: `{name}` baseline records {was} orphan(s), the code now has {now}; "
                f"run `{REGENERATE}` and commit the smaller baseline"
            )
    return problems


def main(argv: list[str]) -> int:
    write_baseline = "--write-baseline" in argv
    unknown = [argument for argument in argv if argument != "--write-baseline"]
    if unknown:
        print(f"unknown argument(s): {' '.join(unknown)}", file=sys.stderr)
        return 2
    handlers = handler_commands()
    declared = build_rs_commands()
    permissions = capability_permissions()

    problems: list[str] = []
    checked = 0

    for plugin, commands in sorted(handlers.items()):
        for command in sorted(commands):
            checked += 1
            if command not in declared.get(plugin, set()):
                problems.append(
                    f"{plugin}:{command} is in generate_handler! but not in "
                    f"build.rs InlinedPlugin::commands — the ACL will reject it at runtime"
                )
            permission = f"{plugin}:allow-{command.replace('_', '-')}"
            if permission in permissions:
                continue
            if (plugin, command) in ALLOWED_WITHOUT_CAPABILITY:
                continue
            problems.append(
                f"{plugin}:{command} has no `{permission}` in capabilities/main.json — "
                f"the ACL will reject it at runtime"
            )

    # Stale entries are worth reporting too: a permission for a command that no
    # longer exists grants nothing, but it hides the fact that it grants nothing.
    for permission in sorted(permissions):
        plugin, separator, operation = permission.partition(":allow-")
        if separator and plugin in declared:
            command = operation.replace("-", "_")
            if command not in declared[plugin]:
                problems.append(
                    f"{permission} is in capabilities/main.json but has no command "
                    f"declaration in build.rs — tauri-build will reject this permission"
                )

    for plugin, commands in sorted(declared.items()):
        for command in sorted(commands - handlers.get(plugin, set())):
            problems.append(
                f"{plugin}:{command} is declared in build.rs but no generate_handler! "
                f"registers it"
            )

    orphans = orphan_commands(production_sources(SRC / "src"))
    if write_baseline:
        ORPHAN_BASELINE.write_text(render_orphan_baseline(orphans), encoding="utf-8")
        print(f"Wrote {ORPHAN_BASELINE.relative_to(ROOT)} ({sum(orphans.values())} orphan command(s)).")
    elif ORPHAN_BASELINE.exists():
        try:
            baseline = parse_orphan_baseline(ORPHAN_BASELINE.read_text(encoding="utf-8"))
        except ValueError as error:
            problems.append(str(error))
        else:
            problems.extend(orphan_problems(baseline, orphans))
    else:
        problems.append(f"{ORPHAN_BASELINE.relative_to(ROOT)} is missing; run `{REGENERATE}`")

    if problems:
        print("Tauri command inventory: MISMATCH")
        for problem in problems:
            print(f"  - {problem}")
        print(f"\n{len(problems)} problem(s) across {checked} registered command(s).")
        return 1

    print(
        f"Tauri command inventory: clean "
        f"({checked} commands across {len(handlers)} plugins agree in "
        f"generate_handler!, build.rs and capabilities/main.json; "
        f"{sum(orphans.values())} baselined orphan command(s))."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

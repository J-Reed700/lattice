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

Exit code 0 when the three agree, 1 otherwise.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri"

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


def main() -> int:
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
    for plugin, commands in sorted(declared.items()):
        for command in sorted(commands - handlers.get(plugin, set())):
            problems.append(
                f"{plugin}:{command} is declared in build.rs but no generate_handler! "
                f"registers it"
            )

    if problems:
        print("Tauri command inventory: MISMATCH")
        for problem in problems:
            print(f"  - {problem}")
        print(f"\n{len(problems)} problem(s) across {checked} registered command(s).")
        return 1

    print(
        f"Tauri command inventory: clean "
        f"({checked} commands across {len(handlers)} plugins agree in "
        f"generate_handler!, build.rs and capabilities/main.json)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

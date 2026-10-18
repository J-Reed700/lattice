#!/usr/bin/env python3
"""Regenerate architecture metadata from a checked-out pinned llama.cpp source."""
import argparse
import re
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    args = parser.parse_args()
    scripts = Path(__file__).resolve().parent
    lock = (scripts / "llama-server.lock").read_text()
    tag = next(line.split()[1] for line in lock.splitlines() if line.startswith("llama_cpp_tag "))
    head = subprocess.check_output(["git", "-C", str(args.source), "rev-parse", "HEAD"], text=True).strip()
    pinned = subprocess.check_output(["git", "-C", str(args.source), "rev-parse", f"{tag}^{{commit}}"], text=True).strip()
    if head != pinned:
        raise SystemExit(f"Source HEAD {head} does not match {tag}: {pinned}")
    source = subprocess.check_output(
        ["git", "-C", str(args.source), "show", f"{pinned}:src/llama-arch.cpp"], text=True
    )
    block = source.split("LLM_ARCH_NAMES", 1)[1].split("};", 1)[0]
    architectures = sorted(set(re.findall(r'\{\s*LLM_ARCH_\w+,\s*"([a-z0-9_-]+)"\s*\}', block)) - {"clip", "unknown"})
    if len(architectures) < 50:
        raise SystemExit("Architecture table was not recognized; inspect the upstream format")
    (scripts / "llama-architectures.txt").write_text(
        f"# Generated from ggml-org/llama.cpp {tag} src/llama-arch.cpp LLM_ARCH_NAMES\n"
        + tag + "\n" + "\n".join(architectures) + "\n"
    )
    print(f"Recorded {len(architectures)} architectures from {tag} ({head})")


if __name__ == "__main__":
    main()

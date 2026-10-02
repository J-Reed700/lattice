#!/usr/bin/env python3
"""Create the deterministic pure-Python stdlib archive and runtime manifest."""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path
from zipfile import ZIP_STORED, ZipFile, ZipInfo


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def package(source: Path, resources: Path) -> None:
    wasm = resources / "python.wasm"
    archive_path = resources / "lib" / "python314.zip"
    archive_path.parent.mkdir(parents=True, exist_ok=True)
    # WASI CPython intentionally ships without zlib. Store entries so zipimport
    # works at startup and does not require that optional native extension.
    with ZipFile(archive_path, "w", compression=ZIP_STORED) as archive:
        for path in sorted(source.rglob("*")):
            if not path.is_file() or "__pycache__" in path.parts or path.suffix in {".pyc", ".pyo"}:
                continue
            relative = path.relative_to(source)
            if relative.parts and relative.parts[0] in {
                "test", "tkinter", "turtledemo", "idlelib", "ensurepip", "venv", "site-packages"
            }:
                continue
            info = ZipInfo(relative.as_posix(), date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_STORED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, path.read_bytes())

    manifest = {
        "pythonVersion": "3.14.8",
        "wasmSha256": sha256(wasm),
        "stdlibSha256": sha256(archive_path),
    }
    (resources / "runtime-manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: package-learning-python.py CPYTHON_LIB_DIR RESOURCE_DIR")
    package(Path(sys.argv[1]), Path(sys.argv[2]))

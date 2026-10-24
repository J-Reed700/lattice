#!/usr/bin/env python3
"""Package only the public Lattice website, installer download, and container config."""

import argparse
import hashlib
import json
import shutil
from pathlib import Path


ALLOWED_PUBLIC_ROOT = {
    "index.html",
    "styles.css",
    "script.js",
    "robots.txt",
    "sitemap.xml",
}
ALLOWED_ASSET_SUFFIXES = {".png", ".svg", ".woff2", ".txt"}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="New, empty release directory")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[2]
    site = repo / "website"
    public_dir = site / "public"
    output = args.output.resolve()
    if output.exists():
        parser.error(f"Refusing to replace an existing release: {output}")

    files: list[tuple[Path, Path]] = []

    for name in sorted(ALLOWED_PUBLIC_ROOT):
        src = public_dir / name
        if not src.is_file():
            parser.error(f"Missing public root file: {src}")
        files.append((src, Path("public") / name))

    assets_dir = public_dir / "assets"
    for p in sorted(assets_dir.rglob("*")):
        if p.is_file() and not p.name.startswith("."):
            if p.suffix not in ALLOWED_ASSET_SUFFIXES:
                parser.error(f"Unexpected asset type in public/assets: {p}")
            rel = p.relative_to(public_dir)
            files.append((p, Path("public") / rel))

    downloads_manifest = site / "downloads.json"
    if downloads_manifest.is_file():
        downloads = json.loads(downloads_manifest.read_text())
        index_html = (public_dir / "index.html").read_text()
        for artifact in downloads.get("artifacts", []):
            name = artifact["filename"]
            if Path(name).name != name or not name.endswith(".dmg"):
                parser.error(f"Downloads must be named DMG files: {name}")
            public_path = f"./downloads/{name}"
            if f'href="{public_path}"' not in index_html:
                parser.error(f"index.html does not link to manifest download: {public_path}")
            if f'href="{public_path}.sha256"' not in index_html:
                parser.error(f"index.html does not link to manifest checksum: {public_path}.sha256")
            source = (repo / artifact["sourcePath"]).resolve()
            if not source.is_file():
                parser.error(f"Missing download binary: {source}")
            digest = sha256_file(source)
            size = source.stat().st_size
            if digest != artifact["sha256"] or size != artifact["bytes"]:
                parser.error(
                    f"Download differs from reviewed manifest: {source} "
                    f"(expected {artifact['bytes']}B / {artifact['sha256']}, got {size}B / {digest})"
                )
            sidecar = source.parent / f"{name}.sha256"
            if not sidecar.is_file() or sidecar.read_text().split() != [digest, name]:
                parser.error(f"Missing or incorrect download checksum sidecar: {sidecar}")
            files.append((source, Path("public/downloads") / name))
            files.append((sidecar, Path("public/downloads") / sidecar.name))

    for name in ("Dockerfile", "nginx.conf", "compose.yaml"):
        deploy_file = site / "deploy" / name
        if not deploy_file.is_file():
            parser.error(f"Missing deploy config file: {deploy_file}")
        files.append((deploy_file, Path(name)))

    output.mkdir(parents=True)
    checksums: list[str] = []
    for source, destination in files:
        target = output / destination
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        target.chmod(0o644)
        checksums.append(f"{sha256_file(target)}  {destination}\n")

    manifest = "".join(sorted(checksums))
    (output / "SHA256SUMS").write_text(manifest)
    tag = "site-" + hashlib.sha256(manifest.encode()).hexdigest()[:12]
    (output / ".env").write_text(f"LATTICE_SITE_TAG={tag}\n")
    print(f"Packaged {len(files)} files in {output}\nRelease: {tag}")


if __name__ == "__main__":
    main()

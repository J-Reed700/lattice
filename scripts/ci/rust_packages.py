"""Print the cargo package selection for the Rust files a pull request changed."""
import json
import re
import subprocess
import sys

WORKSPACE = "--workspace"
CRATE_DIRS = ("src", "tests", "benches", "examples")
BINDINGS_FILES = {"src/lib/bindings.ts", "src/shared/ipc/routes.generated.ts"}
EXPORTER_BIN = "export_bindings"
SAFE_NAME = re.compile(r"^[A-Za-z0-9_-]+$")


def member_dirs(metadata):
    root = metadata["workspace_root"].replace("\\", "/").rstrip("/")
    dirs = {}
    for package in metadata["packages"]:
        manifest = package["manifest_path"].replace("\\", "/")
        directory = manifest.rsplit("/", 1)[0]
        if directory == root:
            dirs[""] = package["name"]
        elif directory.startswith(root + "/"):
            dirs[directory[len(root) + 1 :]] = package["name"]
        else:
            dirs[directory] = package["name"]
    return dirs


def dependents_of(names, metadata):
    """The names plus every member that depends on one, through any dependency kind."""
    root = metadata["workspace_root"].replace("\\", "/").rstrip("/")
    by_path = {}
    for directory, name in member_dirs(metadata).items():
        by_path[f"{root}/{directory}".rstrip("/")] = name
    edges = {}
    for package in metadata["packages"]:
        for dep in package["dependencies"]:
            path = dep.get("path")
            target = by_path.get(path.replace("\\", "/").rstrip("/")) if path else None
            if target:
                edges.setdefault(target, set()).add(package["name"])
    found = set(names)
    pending = list(found)
    while pending:
        for dependent in edges.get(pending.pop(), ()):
            if dependent not in found:
                found.add(dependent)
                pending.append(dependent)
    return found


def exporter(metadata):
    for package in metadata["packages"]:
        for target in package["targets"]:
            if target["name"] == EXPORTER_BIN and "bin" in target["kind"]:
                return package["name"]
    return None


def select(files, metadata):
    if not files:
        return WORKSPACE
    dirs = member_dirs(metadata)
    export_member = exporter(metadata)
    if export_member is None:
        return WORKSPACE
    chosen = {export_member}
    for file in files:
        if file in BINDINGS_FILES:
            continue
        for directory, name in dirs.items():
            prefix = f"{directory}/" if directory else ""
            if directory and any(file.startswith(f"{prefix}{d}/") for d in CRATE_DIRS):
                chosen.add(name)
                break
        else:
            return WORKSPACE
    chosen = dependents_of(chosen, metadata)
    if chosen >= {p["name"] for p in metadata["packages"]}:
        return WORKSPACE
    if not all(SAFE_NAME.match(name) for name in chosen):
        return WORKSPACE
    return " ".join(f"-p {name}" for name in sorted(chosen))


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    try:
        with open(argv[0], encoding="utf-8") as handle:
            files = [line.strip() for line in handle if line.strip()]
        metadata = json.loads(
            subprocess.run(
                ["cargo", "metadata", "--no-deps", "--format-version", "1"],
                capture_output=True,
                text=True,
                check=True,
                timeout=120,
            ).stdout
        )
        selection = select(files, metadata)
    except Exception as error:
        sys.stderr.write(f"rust_packages: {type(error).__name__}: {error}\n")
        selection = WORKSPACE
    print(f"packages={selection}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

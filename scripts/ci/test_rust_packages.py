import importlib.util
import json
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

_SPEC = importlib.util.spec_from_file_location(
    "rust_packages",
    Path(__file__).resolve().parent / "rust_packages.py",
)
rust_packages = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(rust_packages)

ROOT = "/ws"


def member(name, deps=(), bins=()):
    # deps are (crate, kind) pairs; kind is None, "dev" or "build".
    targets = [{"name": name, "kind": ["lib"]}]
    targets += [{"name": b, "kind": ["bin"]} for b in bins]
    return {
        "name": name,
        "id": f"path+file://{ROOT}/crates/{name}#0.1.0",
        "manifest_path": f"{ROOT}/crates/{name}/Cargo.toml",
        "dependencies": [
            {"name": dep, "kind": kind, "path": f"{ROOT}/crates/{dep}"}
            for dep, kind in deps
        ],
        "targets": targets,
    }


def metadata(*members):
    return {"workspace_root": ROOT, "packages": list(members)}


def lattice():
    return metadata(
        member("lattice-core"),
        member("lattice-index", [("lattice-core", None)]),
        member("lattice-store", [("lattice-core", None)]),
        member(
            "lattice-learning",
            [("lattice-core", None), ("lattice-index", None), ("lattice-store", None)],
        ),
        member(
            "lattice-desktop",
            [
                ("lattice-core", None),
                ("lattice-index", None),
                ("lattice-store", None),
                ("lattice-learning", None),
            ],
            bins=["export_bindings"],
        ),
    )


class SelectTests(unittest.TestCase):
    def check(self, files, expected, meta=None):
        self.assertEqual(rust_packages.select(files, meta or lattice()), expected, files)

    def test_desktop_changes_select_only_desktop(self):
        self.check(["crates/lattice-desktop/src/a.rs"], "-p lattice-desktop")
        self.check(["crates/lattice-desktop/tests/x.rs"], "-p lattice-desktop")

    def test_dependents_are_added(self):
        self.check(
            ["crates/lattice-learning/src/a.rs"],
            "-p lattice-desktop -p lattice-learning",
        )
        self.check(
            ["crates/lattice-index/src/a.rs"],
            "-p lattice-desktop -p lattice-index -p lattice-learning",
        )
        self.check(
            ["crates/lattice-store/src/a.rs"],
            "-p lattice-desktop -p lattice-learning -p lattice-store",
        )

    def test_core_selects_everything(self):
        self.check(["crates/lattice-core/src/a.rs"], "--workspace")

    def test_several_files_union(self):
        self.check(
            ["crates/lattice-desktop/src/a.rs", "crates/lattice-learning/src/a.rs"],
            "-p lattice-desktop -p lattice-learning",
        )

    def test_generated_bindings_select_the_exporting_member(self):
        self.check(["src/lib/bindings.ts"], "-p lattice-desktop")
        self.check(["src/shared/ipc/routes.generated.ts"], "-p lattice-desktop")

    def test_bindings_without_exporter_select_workspace(self):
        self.check(["src/lib/bindings.ts"], "--workspace", metadata(member("a"), member("b")))

    def test_exporter_is_always_selected(self):
        meta = metadata(member("a"), member("c"), member("exp", bins=["export_bindings"]))
        self.check(["crates/a/src/x.rs"], "-p a -p exp", meta)

    def test_everything_else_selects_workspace(self):
        for path in [
            "crates/lattice-desktop/Cargo.toml",
            "crates/lattice-desktop/build.rs",
            "crates/lattice-desktop/scripts/llama-architectures.txt",
            "crates/lattice-desktop/resources/learning-node/Dockerfile",
            "Cargo.lock",
            "Cargo.toml",
            "evals/provenance/x.json",
            "scripts/x.py",
            "crates/lattice-sync/src/lib.rs",
            "crates/vendor/readability-js/src/lib.rs",
            ".cargo/config.toml",
        ]:
            with self.subTest(path=path):
                self.check([path], "--workspace")

    def test_one_unmapped_file_widens_the_selection(self):
        self.check(["crates/lattice-desktop/src/a.rs", "Cargo.lock"], "--workspace")

    def test_empty_list_selects_workspace(self):
        self.check([], "--workspace")

    def test_dev_dependency_edge_counts(self):
        meta = metadata(
            member("a", [("b", "dev")]),
            member("b"),
            member("c"),
            member("exp", bins=["export_bindings"]),
        )
        self.check(["crates/b/src/x.rs"], "-p a -p b -p exp", meta)

    def test_build_dependency_edge_counts(self):
        meta = metadata(
            member("a", [("b", "build")]),
            member("b"),
            member("c"),
            member("exp", bins=["export_bindings"]),
        )
        self.check(["crates/b/src/x.rs"], "-p a -p b -p exp", meta)

    def test_unsafe_member_name_selects_workspace(self):
        meta = metadata(member("a b"), member("c"), member("exp", bins=["export_bindings"]))
        self.check(["crates/a b/src/x.rs"], "--workspace", meta)


class MainTests(unittest.TestCase):
    def run_main(self, path, **run):
        with mock.patch("builtins.print") as out, mock.patch.object(
            subprocess, "run", **run
        ), mock.patch("sys.stderr"):
            code = rust_packages.main([path])
        return code, [c.args for c in out.call_args_list]

    def files(self, text):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        path = Path(directory.name) / "rust-files.txt"
        path.write_text(text, encoding="utf-8")
        return str(path)

    def test_cargo_metadata_failure_falls_back(self):
        path = self.files("crates/lattice-desktop/src/a.rs\n")
        code, printed = self.run_main(
            path, side_effect=subprocess.CalledProcessError(101, "cargo")
        )
        self.assertEqual(code, 0)
        self.assertEqual(printed, [("packages=--workspace",)])

    def test_cargo_metadata_timeout_falls_back(self):
        path = self.files("crates/lattice-desktop/src/a.rs\n")
        code, printed = self.run_main(
            path, side_effect=subprocess.TimeoutExpired("cargo", 120)
        )
        self.assertEqual(code, 0)
        self.assertEqual(printed, [("packages=--workspace",)])

    def test_unreadable_files_list_falls_back(self):
        code, printed = self.run_main("/no/such/rust-files.txt")
        self.assertEqual(code, 0)
        self.assertEqual(printed, [("packages=--workspace",)])

    def test_prints_the_selection(self):
        path = self.files("\ncrates/lattice-desktop/src/a.rs\n\n")
        done = subprocess.CompletedProcess([], 0, stdout=json.dumps(lattice()))
        code, printed = self.run_main(path, return_value=done)
        self.assertEqual(code, 0)
        self.assertEqual(printed, [("packages=-p lattice-desktop",)])


class RepoGuardTests(unittest.TestCase):
    def test_cross_member_reads_are_covered(self):
        if shutil.which("cargo") is None:
            raise unittest.SkipTest("cargo is not on PATH")
        repo = Path(__file__).resolve().parents[2]
        meta = json.loads(
            subprocess.run(
                ["cargo", "metadata", "--no-deps", "--format-version", "1"],
                cwd=repo,
                capture_output=True,
                text=True,
                check=True,
            ).stdout
        )
        workspace = Path(meta["workspace_root"])
        dirs = {
            Path(p["manifest_path"]).parent.relative_to(workspace).as_posix(): p["name"]
            for p in meta["packages"]
        }
        # A change to X selects every crate in rust_packages.select's closure
        # of X, so M is covered when X's closure contains M.
        literal = re.compile(r'"[^"\n]*"')
        problems = []
        for directory, name in dirs.items():
            for path in (workspace / directory).rglob("*.rs"):
                if "target" in path.relative_to(workspace / directory).parts:
                    continue
                lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
                for number, line in enumerate(lines, 1):
                    for text in literal.findall(line):
                        for other_dir, other in dirs.items():
                            if other == name:
                                continue
                            base = other_dir.rsplit("/", 1)[-1]
                            if not any(
                                f"{base}/{part}/" in text
                                for part in ("src", "tests", "benches", "examples")
                            ):
                                continue
                            if name not in rust_packages.dependents_of({other}, meta):
                                problems.append(
                                    f"{path.relative_to(workspace).as_posix()}:{number}:"
                                    f" {name} reads {other} but does not depend on it"
                                )
        self.assertEqual(problems, [], "\n".join(problems))


if __name__ == "__main__":
    unittest.main()

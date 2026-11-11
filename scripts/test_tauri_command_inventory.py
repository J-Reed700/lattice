import importlib.util
import tempfile
import unittest
from collections import Counter
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "check_tauri_command_inventory",
    Path(__file__).resolve().parent / "check-tauri-command-inventory.py",
)
inventory = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(inventory)


class OrphanCommandTests(unittest.TestCase):
    def sources(self, files: dict[str, str]) -> dict[str, str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative, text in files.items():
                (root / relative).parent.mkdir(parents=True, exist_ok=True)
                (root / relative).write_text(text, encoding="utf-8")
            return inventory.production_sources(root)

    def test_unregistered_production_commands_are_orphans(self):
        sources = self.sources({
            "features/notes/plugin.rs": """
                pub fn init() { Builder::new("notes").invoke_handler(tauri::generate_handler![
                    commands::list_notes,
                    // commands::commented_out,
                ]); }
            """,
            "features/notes/commands.rs": """
                #[tauri::command]
                #[specta::specta]
                pub async fn list_notes() {}
                #[tauri::command(rename_all = "snake_case")]
                pub(crate) fn commented_out() {}
                /// Docs between attribute and fn are comments, not code.
                #[tauri::command]
                fn never_registered() { let brace = "}"; let url = "https://x.test/{"; }
                #[cfg(test)]
                mod tests { #[tauri::command] fn test_only() { let s = "}"; } }
            """,
            "features/notes/mod.rs": """
                mod commands; mod plugin;
                #[cfg(any(test, doc))] pub mod examples;
                #[cfg(test)] #[path = "fixtures/live.rs"] mod live;
            """,
            "features/notes/examples.rs": "#[tauri::command] fn doc_example() {}",
            "features/notes/fixtures/live.rs": "#[tauri::command] fn live_fixture() {}",
            "features/notes/commands_tests.rs": "#[tauri::command] fn test_file() {}",
            "tests/plugins/mod.rs": "#[tauri::command] fn integration() {}",
        })
        self.assertEqual(
            inventory.orphan_commands(sources),
            Counter({
                ("features/notes/commands.rs", "commented_out"): 1,
                ("features/notes/commands.rs", "never_registered"): 1,
            }),
        )

    def test_new_and_rising_orphans_fail_and_vanished_ones_are_stale(self):
        baseline = Counter({("a.rs", "kept"): 1, ("a.rs", "deleted"): 1, ("b.rs", "twice"): 1})
        current = Counter({("a.rs", "kept"): 1, ("b.rs", "twice"): 2, ("c.rs", "new"): 1})
        problems = inventory.orphan_problems(baseline, current)
        self.assertEqual(len(problems), 3)
        self.assertTrue(problems[0].startswith("STALE: a.rs: `deleted`"))
        self.assertIn(inventory.REGENERATE, problems[0])
        self.assertIn("b.rs: `twice`", problems[1])
        self.assertIn("baseline allows 1", problems[1])
        self.assertIn("c.rs: `new`", problems[2])
        self.assertEqual(inventory.orphan_problems(current, current), [])

    def test_baseline_round_trips_and_rejects_malformed_lines(self):
        orphans = Counter({("features/a.rs", "old_command"): 1, ("features/b.rs", "other"): 2})
        text = inventory.render_orphan_baseline(orphans)
        self.assertTrue(text.startswith("# Tauri command-orphan baseline"))
        self.assertEqual(inventory.parse_orphan_baseline(text), orphans)
        self.assertEqual(inventory.parse_orphan_baseline(inventory.render_orphan_baseline(Counter())), Counter())
        for malformed in [
            "orphan-command features/a.rs old_command",
            "orphan-command features/a.rs old_command 0",
            "raw-spawn features/a.rs old_command 1",
            "orphan-command a.rs x 1\norphan-command a.rs x 1",
        ]:
            with self.assertRaises(ValueError):
                inventory.parse_orphan_baseline(malformed)


if __name__ == "__main__":
    unittest.main()

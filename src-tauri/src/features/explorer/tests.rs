use super::dto::ExplorerEntryKind;
use super::fs::{self, SearchRequest, WalkBounds};
use super::scope::Scope;
use crate::shared::AppError;
use std::path::Path;
use std::time::Duration;

fn write(root: &Path, relative: &str, contents: &[u8]) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// A small project: a source tree, a `.gitignore` that hides `target/` and
/// `*.log`, a binary file, and a `.git` directory that must never show.
fn fixture() -> (tempfile::TempDir, Scope) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, ".gitignore", b"target/\n*.log\n");
    write(root, "README.md", b"# Demo\nfn main lives in src\n");
    write(
        root,
        "src/main.rs",
        b"fn main() {\n    println!(\"hello\");\n}\n",
    );
    write(
        root,
        "src/lib/util.rs",
        b"pub fn helper() {}\n// fn main is elsewhere\n",
    );
    write(root, "target/debug/out.rs", b"fn main() {}\n");
    write(root, "build.log", b"fn main failed\n");
    write(root, "logo.png", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR fn main");
    write(root, ".git/HEAD", b"ref: refs/heads/main\n");
    let scope = Scope::open(root.to_str().unwrap()).unwrap();
    (dir, scope)
}

fn search_paths(scope: &Scope, query: &str) -> Vec<String> {
    let result = fs::search(
        scope,
        &SearchRequest {
            query,
            regex: false,
            path_prefix: None,
            max_results: 100,
        },
    )
    .unwrap();
    let mut paths: Vec<String> = result.matches.into_iter().map(|m| m.path).collect();
    paths.sort();
    paths
}

#[test]
fn a_parent_dir_component_is_refused() {
    let (_dir, scope) = fixture();
    let error = scope.resolve("src/../../etc/passwd").unwrap_err();
    assert!(matches!(error, AppError::PermissionDenied(_)), "{error:?}");
    assert!(scope.resolve("..").is_err());
}

#[test]
fn an_absolute_path_is_refused_even_inside_the_root() {
    let (_dir, scope) = fixture();
    let inside = scope.root().join("src/main.rs");
    let error = scope.resolve(inside.to_str().unwrap()).unwrap_err();
    assert!(matches!(error, AppError::PermissionDenied(_)), "{error:?}");
}

#[cfg(unix)]
#[test]
fn a_symlink_pointing_out_of_the_root_is_refused_and_not_listed() {
    let (_dir, scope) = fixture();
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "secret.txt", b"do not read");
    std::os::unix::fs::symlink(outside.path(), scope.root().join("escape")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        scope.root().join("secret-link.txt"),
    )
    .unwrap();

    assert!(matches!(
        scope.resolve("escape/secret.txt").unwrap_err(),
        AppError::PermissionDenied(_)
    ));
    assert!(fs::read_file(&scope, "secret-link.txt").is_err());
    let listing = fs::list_dir(&scope, "").unwrap();
    assert!(listing
        .entries
        .iter()
        .all(|entry| entry.name != "escape" && entry.name != "secret-link.txt"));
    assert!(search_paths(&scope, "do not read").is_empty());
}

#[cfg(unix)]
#[test]
fn a_symlink_staying_inside_the_root_resolves() {
    let (_dir, scope) = fixture();
    std::os::unix::fs::symlink(scope.root().join("src"), scope.root().join("alias")).unwrap();
    let resolved = scope.resolve("alias/main.rs").unwrap();
    assert_eq!(resolved.relative, "alias/main.rs");
    assert!(resolved.canonical.starts_with(scope.root()));
}

#[test]
fn the_root_must_be_an_existing_absolute_directory() {
    let (_dir, scope) = fixture();
    assert!(Scope::open("relative/path").is_err());
    assert!(Scope::open(scope.root().join("README.md").to_str().unwrap()).is_err());
    assert!(Scope::open(scope.root().join("missing").to_str().unwrap()).is_err());
}

#[test]
fn listing_flags_ignored_entries_sorts_dirs_first_and_hides_git() {
    let (_dir, scope) = fixture();
    let listing = fs::list_dir(&scope, "").unwrap();
    assert_eq!(listing.path, "");
    let names: Vec<(&str, bool)> = listing
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.ignored))
        .collect();
    assert_eq!(
        names,
        vec![
            ("src", false),
            ("target", true),
            (".gitignore", false),
            ("build.log", true),
            ("logo.png", false),
            ("README.md", false),
        ]
    );
    assert_eq!(listing.entries[0].kind, ExplorerEntryKind::Directory);
    assert_eq!(listing.entries[0].size, None);
    assert!(listing.entries[2].size.is_some());
}

#[test]
fn children_of_an_ignored_directory_are_ignored() {
    let (_dir, scope) = fixture();
    let listing = fs::list_dir(&scope, "target/debug").unwrap();
    assert_eq!(listing.path, "target/debug");
    assert_eq!(listing.entries.len(), 1);
    assert_eq!(listing.entries[0].path, "target/debug/out.rs");
    assert!(listing.entries[0].ignored);

    let clean = fs::list_dir(&scope, "./src/").unwrap();
    assert_eq!(clean.path, "src");
    assert!(clean.entries.iter().all(|entry| !entry.ignored));
}

#[test]
fn search_skips_ignored_and_binary_files() {
    let (_dir, scope) = fixture();
    assert_eq!(
        search_paths(&scope, "fn main"),
        vec!["README.md", "src/lib/util.rs", "src/main.rs"]
    );
}

#[test]
fn search_reports_line_column_and_respects_the_prefix() {
    let (_dir, scope) = fixture();
    let result = fs::search(
        &scope,
        &SearchRequest {
            query: r"println!\(",
            regex: true,
            path_prefix: Some("src"),
            max_results: 10,
        },
    )
    .unwrap();
    assert_eq!(result.matches.len(), 1);
    let found = &result.matches[0];
    assert_eq!(
        (found.path.as_str(), found.line, found.column),
        ("src/main.rs", 2, 5)
    );
    assert_eq!(found.preview, "println!(\"hello\");");
    assert!(!result.truncated);

    let outside = fs::search(
        &scope,
        &SearchRequest {
            query: "x",
            regex: false,
            path_prefix: Some("../"),
            max_results: 10,
        },
    );
    assert!(outside.is_err());
}

#[test]
fn search_bounds_set_truncated() {
    let (_dir, scope) = fixture();
    let capped = fs::search(
        &scope,
        &SearchRequest {
            query: "fn",
            regex: false,
            path_prefix: None,
            max_results: 1,
        },
    )
    .unwrap();
    assert_eq!(capped.matches.len(), 1);
    assert!(capped.truncated);

    let few_files = fs::search_with_bounds(
        &scope,
        &SearchRequest {
            query: "zzz-not-there",
            regex: false,
            path_prefix: None,
            max_results: 10,
        },
        WalkBounds {
            max_files: 1,
            max_bytes_per_file: 1024,
            time_budget: Duration::from_secs(2),
        },
    )
    .unwrap();
    assert!(few_files.truncated);
    assert_eq!(few_files.files_scanned, 1);
}

#[test]
fn search_rejects_an_empty_query_and_a_bad_regex() {
    let (_dir, scope) = fixture();
    let request = |query, regex| SearchRequest {
        query,
        regex,
        path_prefix: None,
        max_results: 10,
    };
    assert!(fs::search(&scope, &request("  ", false)).is_err());
    assert!(fs::search(&scope, &request("(unclosed", true)).is_err());
    // The same text as a literal is fine.
    assert!(fs::search(&scope, &request("(unclosed", false)).is_ok());
}

#[test]
fn binary_and_oversized_files_carry_no_text() {
    let (_dir, scope) = fixture();
    let png = fs::read_file(&scope, "logo.png").unwrap();
    assert!(png.binary);
    assert!(png.text.is_none());

    let big = vec![b'a'; fs::MAX_TEXT_BYTES as usize + 1];
    write(scope.root(), "big.txt", &big);
    let large = fs::read_file(&scope, "big.txt").unwrap();
    assert!(large.too_large);
    assert!(!large.binary);
    assert!(large.text.is_none());
    assert_eq!(large.size_bytes, fs::MAX_TEXT_BYTES + 1);
}

#[test]
fn reading_a_text_file_counts_lines_and_names_the_language() {
    let (_dir, scope) = fixture();
    let file = fs::read_file(&scope, "src/main.rs").unwrap();
    assert_eq!(file.path, "src/main.rs");
    assert_eq!(file.line_count, 3);
    assert_eq!(file.language.as_deref(), Some("rust"));
    assert!(file.text.unwrap().starts_with("fn main()"));
    assert!(fs::read_file(&scope, "src").is_err());
}

#[test]
fn find_files_matches_substrings_and_globs_and_skips_ignored() {
    let (_dir, scope) = fixture();
    let mut by_glob = fs::find_files(&scope, "*.rs", 50).unwrap().paths;
    by_glob.sort();
    assert_eq!(by_glob, vec!["src/lib/util.rs", "src/main.rs"]);
    assert_eq!(
        fs::find_files(&scope, "UTIL", 50).unwrap().paths,
        vec!["src/lib/util.rs"]
    );
    assert_eq!(
        fs::find_files(&scope, "src/**/*.rs", 50)
            .unwrap()
            .paths
            .len(),
        2
    );
    assert!(fs::find_files(&scope, "out.rs", 50)
        .unwrap()
        .paths
        .is_empty());
}

mod model_tools {
    use super::*;
    use crate::features::explorer::dto::{ExplorerFocusDto, ExplorerLineRangeDto};
    use crate::features::explorer::{prompt, tools};
    use serde_json::json;

    fn numbered_file(scope: &Scope, relative: &str, lines: u32) {
        let text: String = (1..=lines).map(|n| format!("line {n}\n")).collect();
        write(scope.root(), relative, text.as_bytes());
    }

    #[test]
    fn read_file_returns_numbered_lines_and_says_how_to_read_on() {
        let (_dir, scope) = fixture();
        numbered_file(&scope, "src/long.txt", 1000);
        let first = tools::run(
            &scope,
            "read_file",
            &json!({"path": "src/long.txt"}),
            1_000_000,
        )
        .unwrap();
        assert!(
            first.starts_with("src/long.txt · lines 1–400 of 1000\n"),
            "{first}"
        );
        assert!(first.contains("   1│ line 1\n"));
        assert!(first.contains(" 400│ line 400\n"));
        assert!(!first.contains("│ line 401"));
        assert!(first.contains("start_line 401"));

        // Numbers as strings, as small local models often write them.
        let tail = tools::run(
            &scope,
            "read_file",
            &json!({"path": "src/long.txt", "start_line": "995", "end_line": 2000}),
            1_000_000,
        )
        .unwrap();
        assert!(tail.contains("lines 995–1000 of 1000"), "{tail}");
        assert!(tail.ends_with("[End of file.]"));
    }

    #[test]
    fn read_file_cuts_at_a_line_when_the_allowance_is_small() {
        let (_dir, scope) = fixture();
        numbered_file(&scope, "notes.txt", 300);
        let cut = tools::run(&scope, "read_file", &json!({"path": "notes.txt"}), 900).unwrap();
        assert!(cut.chars().count() <= 900, "{}", cut.chars().count());
        let shown = cut.lines().filter(|line| line.contains('│')).count() as u32;
        assert!(shown > 0 && shown < 300);
        assert!(cut.contains(&format!("start_line {}", shown + 1)), "{cut}");
    }

    #[test]
    fn folder_tools_refuse_escapes_and_report_binary_files() {
        let (_dir, scope) = fixture();
        assert!(tools::run(
            &scope,
            "read_file",
            &json!({"path": "../etc/passwd"}),
            10_000
        )
        .is_err());
        assert!(tools::run(&scope, "read_file", &json!({"path": "/etc/passwd"}), 10_000).is_err());
        assert!(
            tools::run(&scope, "read_file", &json!({"path": "logo.png"}), 10_000)
                .unwrap_err()
                .to_string()
                .contains("binary")
        );
    }

    #[test]
    fn list_directory_leaves_ignored_entries_out_and_reads_slash_as_the_root() {
        let (_dir, scope) = fixture();
        let listing = tools::run(&scope, "list_directory", &json!({"path": "/"}), 10_000).unwrap();
        assert!(listing.contains("src/\n"), "{listing}");
        assert!(listing.contains("README.md  ("));
        assert!(!listing.contains("target"));
        assert!(!listing.contains("build.log"));
        assert!(listing.contains("2 entries ignored by .gitignore left out"));
    }

    #[test]
    fn search_and_find_name_paths_and_lines() {
        let (_dir, scope) = fixture();
        let found =
            tools::run(&scope, "search_files", &json!({"query": "println"}), 10_000).unwrap();
        assert!(
            found.contains("src/main.rs:2: println!(\"hello\");"),
            "{found}"
        );
        let files = tools::run(&scope, "find_files", &json!({"pattern": "*.rs"}), 10_000).unwrap();
        assert!(
            files.contains("src/main.rs") && files.contains("src/lib/util.rs"),
            "{files}"
        );
    }

    #[test]
    fn step_labels_are_plain() {
        assert_eq!(
            tools::activity_label("read_file", &json!({"path": "src/foo.rs"})),
            "Read src/foo.rs 1–400"
        );
        assert_eq!(
            tools::activity_label(
                "read_file",
                &json!({"path": "src/foo.rs", "start_line": 10, "end_line": 24})
            ),
            "Read src/foo.rs 10–24"
        );
        assert_eq!(
            tools::activity_label("search_files", &json!({"query": "fn main"})),
            "Searched for \"fn main\""
        );
        assert_eq!(
            tools::activity_label("list_directory", &json!({})),
            "Listed the folder"
        );
    }

    #[test]
    fn the_prompt_block_carries_the_selection_with_line_numbers_and_the_rule() {
        let (_dir, scope) = fixture();
        numbered_file(&scope, "src/long.txt", 50);
        let focus = ExplorerFocusDto {
            open_path: Some("src/long.txt".to_string()),
            selection: Some(ExplorerLineRangeDto {
                start_line: 10,
                end_line: 12,
            }),
        };
        let block = prompt::render_context(
            &scope,
            Some(&focus),
            20_000,
            true,
            false,
            &prompt::FolderPassages::default(),
        );
        assert!(
            block.contains("Top level: src/, .gitignore, logo.png, README.md"),
            "{block}"
        );
        assert!(block.contains("selected lines 10–12"), "{block}");
        assert!(
            block.contains("  10│ line 10\n  11│ line 11\n  12│ line 12\n"),
            "{block}"
        );
        assert!(!block.contains("line 13"));
        assert!(block.contains("`src/main.rs:10-24`"));
        assert!(block.contains("read_file"));
    }

    #[test]
    fn a_focus_outside_the_folder_is_dropped_without_a_word() {
        let (_dir, scope) = fixture();
        for open_path in ["../outside.txt", "missing.rs", "/etc/hosts"] {
            let focus = ExplorerFocusDto {
                open_path: Some(open_path.to_string()),
                selection: None,
            };
            let block = prompt::render_context(
                &scope,
                Some(&focus),
                20_000,
                false,
                false,
                &prompt::FolderPassages::default(),
            );
            assert!(!block.contains("Open file"), "{open_path}: {block}");
            assert!(block.contains("You cannot open other files this turn"));
        }
    }

    #[test]
    fn the_open_file_is_cut_to_the_budget_and_says_so() {
        let (_dir, scope) = fixture();
        numbered_file(&scope, "big.txt", 5_000);
        let focus = ExplorerFocusDto {
            open_path: Some("big.txt".to_string()),
            selection: None,
        };
        let block = prompt::render_context(
            &scope,
            Some(&focus),
            3_000,
            true,
            false,
            &prompt::FolderPassages::default(),
        );
        assert!(block.len() <= 3_000, "{}", block.len());
        assert!(
            block.contains("not shown for length. read_file reads on from line"),
            "{block}"
        );
    }
}

/// What a line reference in an answer meant, when it is not a path as written.
#[test]
fn a_cited_path_is_located_without_discarding_its_folders() {
    let (dir, scope) = fixture();
    write(dir.path(), "contracts/effect_api.hpp", b"// api\n");
    write(
        dir.path(),
        "modules/engine/include/util.rs",
        b"// another util\n",
    );

    // As written, when it is there.
    assert_eq!(
        fs::locate_file(&scope, "src/main.rs").unwrap(),
        ["src/main.rs"]
    );
    assert_eq!(
        fs::locate_file(&scope, "./src/main.rs").unwrap(),
        ["src/main.rs"]
    );
    let absolute = format!("{}/src/main.rs", scope.root_string());
    assert_eq!(fs::locate_file(&scope, &absolute).unwrap(), ["src/main.rs"]);

    // The name alone, or a path that lost its first folders.
    assert_eq!(
        fs::locate_file(&scope, "effect_api.hpp").unwrap(),
        ["contracts/effect_api.hpp"]
    );
    assert_eq!(
        fs::locate_file(&scope, "lib/util.rs").unwrap(),
        ["src/lib/util.rs"],
        "a path ending excludes unrelated files with the same name"
    );
    assert_eq!(
        fs::locate_file(&scope, "UTIL.RS").unwrap(),
        ["src/lib/util.rs", "modules/engine/include/util.rs"],
        "shortest first, case ignored"
    );

    // Missing intermediate folders are fine; invented folders are not.
    assert_eq!(
        fs::locate_file(&scope, "engine/util.rs").unwrap(),
        ["modules/engine/include/util.rs"]
    );
    assert!(fs::locate_file(&scope, "engine/effect_api.hpp")
        .unwrap()
        .is_empty());
    assert!(fs::locate_file(&scope, "out.rs").unwrap().is_empty());
    assert!(fs::locate_file(&scope, "missing.cpp").unwrap().is_empty());
    assert!(fs::locate_file(&scope, "  ").unwrap().is_empty());
}

#[test]
fn locate_file_crash_reference_excludes_other_mod_files() {
    let (_dir, scope) = fixture();
    let crash = "src-tauri/src/infrastructure/crash/mod.rs";
    write(scope.root(), crash, b"// crash handler\n");
    for index in 0..30 {
        write(
            scope.root(),
            &format!("modules/other{index}/mod.rs"),
            b"// unrelated\n",
        );
    }
    // Even plausible but weaker abbreviations cannot dilute an exact ending.
    write(scope.root(), "src/crash/helpers/mod.rs", b"// helpers\n");
    write(
        scope.root(),
        "src/notcrash/mod.rs",
        b"// different folder\n",
    );
    for reference in [
        "crash/mod.rs",
        "CRASH/MOD.RS",
        "./crash//./mod.rs",
        r"crash\mod.rs",
    ] {
        assert_eq!(
            fs::locate_file(&scope, reference).unwrap(),
            [crash],
            "{reference}"
        );
    }
}

#[test]
fn locate_file_offers_only_genuinely_ambiguous_paths() {
    let (_dir, scope) = fixture();
    write(scope.root(), "src/crash/mod.rs", b"// app\n");
    write(scope.root(), "tests/crash/mod.rs", b"// tests\n");
    write(
        scope.root(),
        "src/crash/helpers/mod.rs",
        b"// weaker match\n",
    );
    write(scope.root(), "api/http/mod.rs", b"// unrelated\n");
    assert_eq!(
        fs::locate_file(&scope, "crash/mod.rs").unwrap(),
        ["src/crash/mod.rs", "tests/crash/mod.rs"]
    );
    // A complete path still wins immediately.
    assert_eq!(
        fs::locate_file(&scope, "src/crash/mod.rs").unwrap(),
        ["src/crash/mod.rs"]
    );
}

#[test]
fn locate_file_abbreviations_require_every_component_in_order() {
    let (_dir, scope) = fixture();
    write(
        scope.root(),
        "src-tauri/src/infrastructure/crash/mod.rs",
        b"// app\n",
    );
    write(
        scope.root(),
        "src-tauri/src/features/chat/mod.rs",
        b"// unrelated\n",
    );
    for reference in [
        "src/crash/mod.rs",
        "src-tauri/crash/mod.rs",
        "src-tauri/infrastructure/mod.rs",
    ] {
        assert_eq!(
            fs::locate_file(&scope, reference).unwrap(),
            ["src-tauri/src/infrastructure/crash/mod.rs"],
            "{reference}"
        );
    }
    for reference in [
        "crash/src/mod.rs",
        "infra/crash/mod.rs",
        "src/crash/crash/mod.rs",
        "missing/mod.rs",
    ] {
        assert!(
            fs::locate_file(&scope, reference).unwrap().is_empty(),
            "{reference}"
        );
    }
    write(
        scope.root(),
        "src/lib/crash/mod.rs",
        b"// another matching abbreviation\n",
    );
    assert_eq!(
        fs::locate_file(&scope, "src/crash/mod.rs").unwrap(),
        [
            "src/lib/crash/mod.rs",
            "src-tauri/src/infrastructure/crash/mod.rs"
        ]
    );
}

#[test]
fn locate_file_keeps_dotfiles_and_refuses_parent_components() {
    let (_dir, scope) = fixture();
    write(scope.root(), "config/.env", b"EXAMPLE=1\n");
    write(scope.root(), "config/env", b"not the dotfile\n");
    assert_eq!(fs::locate_file(&scope, ".env").unwrap(), ["config/.env"]);
    for reference in ["../main.rs", "src/../main.rs"] {
        assert!(matches!(
            fs::locate_file(&scope, reference),
            Err(AppError::PermissionDenied(_))
        ));
    }
}

#[test]
fn locate_file_does_not_auto_open_a_match_from_an_incomplete_search() {
    let (_dir, scope) = fixture();
    let result = fs::locate_file_with_bounds(
        &scope,
        "main.rs",
        WalkBounds {
            max_files: 0,
            ..WalkBounds::default()
        },
    );
    assert!(result.unwrap_err().to_string().contains("search limit"));
    // Exact paths do not depend on walking the folder.
    assert_eq!(
        fs::locate_file_with_bounds(
            &scope,
            "src/main.rs",
            WalkBounds {
                max_files: 0,
                ..WalkBounds::default()
            }
        )
        .unwrap(),
        ["src/main.rs"]
    );
}

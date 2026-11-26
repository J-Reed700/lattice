use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};

struct Rule {
    table: &'static str,
    columns: &'static [&'static str],
    allowed: &'static [&'static str],
}

const RULES: &[Rule] = &[
    Rule {
        table: "learning_blocks",
        columns: &["title", "body"],
        allowed: &["crates/lattice-learning/src/lessons/claim_reviews/repository/blocks.rs"],
    },
    Rule {
        table: "learning_assessment_candidates",
        columns: &["prompt", "options_json", "content_sha256"],
        allowed: &["crates/lattice-learning/src/lessons/claim_reviews/repository/questions.rs"],
    },
    Rule {
        table: "learning_assessment_answer_keys",
        columns: &["explanation", "key_json", "distractor_rationales_json"],
        allowed: &["crates/lattice-learning/src/lessons/claim_reviews/repository/questions.rs"],
    },
    Rule {
        table: "study_cards",
        columns: &[
            "question",
            "answer",
            "options_json",
            "correct_index",
            "explanation",
        ],
        allowed: &["crates/lattice-learning/src/recall/recall_repository/card_text.rs"],
    },
    Rule {
        table: "learning_recall_card_profiles",
        columns: &["prompt_json", "answer_json"],
        allowed: &["crates/lattice-learning/src/recall/recall_repository/card_text.rs"],
    },
    Rule {
        table: "learning_lessons",
        columns: &["title", "objective"],
        allowed: &[
            "crates/lattice-learning/src/planning/curriculum_repository/corrections.rs",
            "crates/lattice-learning/src/planning/curriculum_repository.rs",
            "crates/lattice-learning/src/repository/outline_draft.rs",
        ],
    },
    Rule {
        table: "learning_modules",
        columns: &["title", "summary"],
        allowed: &[
            "crates/lattice-learning/src/planning/curriculum_repository/corrections.rs",
            "crates/lattice-learning/src/planning/curriculum_repository.rs",
            "crates/lattice-learning/src/repository/outline_draft.rs",
        ],
    },
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn is_test_source(path: &Path) -> bool {
    if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
        return true;
    }
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    if file_name == "tests.rs" || file_name.ends_with("_tests.rs") {
        return true;
    }
    path.components()
        .any(|component| matches!(component, Component::Normal(part) if part == "tests"))
}

fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) != Some("tests") {
                collect_sources(&path, out);
            }
        } else if !is_test_source(&path) {
            out.push(path);
        }
    }
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn extract_string_literals(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut literals = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if bytes.get(i..i + 2) == Some(b"//") {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes.get(i..i + 2) == Some(b"/*") {
            i += 2;
            let mut depth = 1usize;
            while i < bytes.len() && depth > 0 {
                if bytes.get(i..i + 2) == Some(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes.get(i..i + 2) == Some(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        // Skip character literals such as `'"'` and `'\''` without eating lifetimes `'a`.
        if bytes[i] == b'\'' {
            if bytes.get(i + 1) == Some(&b'\\') && bytes.get(i + 3) == Some(&b'\'') {
                i += 4;
                continue;
            }
            if i + 2 < bytes.len()
                && bytes[i + 1] != b'\n'
                && bytes[i + 1] != b'\r'
                && bytes[i + 2] == b'\''
            {
                i += 3;
                continue;
            }
            i += 1;
            continue;
        }
        // Raw string literal: `r` or `br` followed by zero or more `#` and `"`.
        let raw_start = if bytes[i] == b'r'
            && (i == 0 || !is_ident_byte(bytes[i - 1]))
            && matches!(bytes.get(i + 1), Some(b'#' | b'"'))
        {
            Some(i + 1)
        } else if bytes.get(i..i + 2) == Some(b"br")
            && (i == 0 || !is_ident_byte(bytes[i - 1]))
            && matches!(bytes.get(i + 2), Some(b'#' | b'"'))
        {
            Some(i + 2)
        } else {
            None
        };
        if let Some(mut j) = raw_start {
            let mut hashes = 0usize;
            while j < bytes.len() && bytes[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if bytes.get(j) == Some(&b'"') {
                j += 1;
                let content_start = j;
                while j < bytes.len() {
                    if bytes[j] == b'"' && bytes[j + 1..].starts_with(&vec![b'#'; hashes]) {
                        literals.push(source[content_start..j].to_string());
                        j += 1 + hashes;
                        break;
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
        }
        if bytes[i] == b'"' {
            i += 1;
            let mut out = String::new();
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    if bytes.get(i + 1) == Some(&b'\n') {
                        i += 2;
                        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                            i += 1;
                        }
                        continue;
                    }
                    if bytes.get(i..i + 3) == Some(b"\\\r\n") {
                        i += 3;
                        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                            i += 1;
                        }
                        continue;
                    }
                    if i + 1 < bytes.len() {
                        out.push(bytes[i] as char);
                        out.push(bytes[i + 1] as char);
                        i += 2;
                        continue;
                    }
                }
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                let ch = source[i..].chars().next().expect("valid utf8");
                out.push(ch);
                i += ch.len_utf8();
            }
            literals.push(out);
            continue;
        }
        i += 1;
    }

    literals
}

fn normalize_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_ws = false;
    for ch in text.chars() {
        if ch.is_ascii_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out
}

fn find_word_case_insensitive(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let lower_hay = haystack.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let bytes = lower_hay.as_bytes();
    let mut offset = from;
    while let Some(pos) = lower_hay[offset..].find(&lower_needle) {
        let abs = offset + pos;
        let end = abs + lower_needle.len();
        let before_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after_ok = end == bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            return Some(abs);
        }
        offset = abs + 1;
    }
    None
}

fn assigns_column(set_clause: &str, column: &str) -> bool {
    let bytes = set_clause.as_bytes();
    let mut offset = 0;
    while let Some(pos) = set_clause[offset..].find(column) {
        let abs = offset + pos;
        let end = abs + column.len();
        let before_ok = abs == 0 || !is_ident_byte(bytes[abs - 1]);
        let after_ident_ok = end == bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ident_ok {
            let mut rest = end;
            while rest < bytes.len() && bytes[rest].is_ascii_whitespace() {
                rest += 1;
            }
            if bytes.get(rest) == Some(&b'=') && bytes.get(rest + 1) != Some(&b'=') {
                return true;
            }
        }
        offset = abs + 1;
    }
    false
}

fn find_content_updates(normalized: &str, table: &str, columns: &[&str]) -> Vec<String> {
    let pattern = format!("UPDATE {table} SET");
    let mut found = Vec::new();
    let mut offset = 0;
    while let Some(start) = find_word_case_insensitive(normalized, &pattern, offset) {
        let clause_start = start + pattern.len();
        let clause_end = find_word_case_insensitive(normalized, "WHERE", clause_start)
            .unwrap_or(normalized.len());
        let set_clause = &normalized[clause_start..clause_end];
        if columns
            .iter()
            .any(|column| assigns_column(set_clause, column))
        {
            found.push(normalized[start..clause_end].trim().to_string());
        }
        offset = clause_end;
    }
    found
}

#[test]
fn content_columns_have_one_writer() {
    let root = workspace_root();
    let crates_dir = root.join("crates");
    let mut files = Vec::new();
    for entry in fs::read_dir(&crates_dir)
        .expect("crates directory")
        .flatten()
    {
        let src = entry.path().join("src");
        if src.is_dir() {
            collect_sources(&src, &mut files);
        }
    }
    files.sort();

    let mut per_table_files: HashMap<&'static str, BTreeMap<String, Vec<String>>> = HashMap::new();
    for file in &files {
        let Ok(rel) = file.strip_prefix(&root) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let text = fs::read_to_string(file).expect("read source file");
        let literals = extract_string_literals(&text);
        for literal in literals {
            let normalized = normalize_ws(&literal);
            for rule in RULES {
                let matches = find_content_updates(&normalized, rule.table, rule.columns);
                if !matches.is_empty() {
                    per_table_files
                        .entry(rule.table)
                        .or_default()
                        .entry(rel_str.clone())
                        .or_default()
                        .extend(matches);
                }
            }
        }
    }

    let mut problems = Vec::new();
    for rule in RULES {
        let counts = per_table_files.remove(rule.table).unwrap_or_default();
        for (path, statements) in &counts {
            if !rule.allowed.contains(&path.as_str()) {
                for stmt in statements {
                    problems.push(format!(
                        "{} assigned outside allowed files in {path}: {stmt}",
                        rule.table
                    ));
                }
            }
        }
        for &allowed in rule.allowed {
            let count = counts.get(allowed).map_or(0, Vec::len);
            if count != 1 {
                problems.push(format!(
                    "{} expected exactly 1 content UPDATE statement in {allowed}, found {count}",
                    rule.table
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "content-column single-writer violations:\n{}",
        problems.join("\n")
    );
}

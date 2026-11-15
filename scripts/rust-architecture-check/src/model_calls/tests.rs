use super::*;
use crate::ratchet::{counts, parse_baseline, render_baseline, Counts};

const CALLER: &str = "features/chat/ask.rs";
const IMPORT: &str = "use lattice_core::application::services::model_contract::legacy;\n";

fn source(prefix: &str, relative: &str, text: &str) -> SourceFile {
    SourceFile {
        relative: relative.to_string(),
        prefix: prefix.to_string(),
        module: Vec::new(),
        library: true,
        source: text.to_string(),
        ast: syn::parse_file(text).unwrap(),
    }
}

fn rows(relative: &str, text: &str) -> Vec<(String, String, String, usize)> {
    rows_in("", relative, text)
}

fn rows_in(prefix: &str, relative: &str, text: &str) -> Vec<(String, String, String, usize)> {
    counts(&findings(&source(prefix, relative, text)))
        .into_iter()
        .map(|(key, count)| (key.rule, key.subject, key.target, count))
        .collect()
}

fn row(rule: &str, subject: &str, target: &str, count: usize) -> (String, String, String, usize) {
    (rule.into(), subject.into(), target.into(), count)
}

fn old(target: &str, count: usize) -> (String, String, String, usize) {
    row(OLD_MODEL_CALL, CALLER, target, count)
}

fn locations(relative: &str, text: &str) -> Vec<String> {
    findings(&source("", relative, text))
        .into_iter()
        .map(|finding| finding.location)
        .collect()
}

#[test]
fn every_path_through_the_imported_legacy_module_counts_once() {
    let text = format!(
        "{IMPORT}
fn build(json: Value) -> legacy::Completion {{
    let mut request = legacy::completion(legacy::Completion {{ input, ..Default::default() }});
    legacy::says(&mut request);
    let schema = legacy::schema(json);
    let Some(legacy::Completion {{ input, .. }}) = fields else {{ return; }};
    legacy::grounded(a, b)
}}
"
    );
    assert_eq!(rows(CALLER, &text), vec![old("legacy", 7)]);
}

#[test]
fn legacy_paths_inside_macros_count() {
    let text = format!(
        "{IMPORT}
fn build() {{
    let request = vec![legacy::completion(fields)];
    tracing::info!(\"{{:?}}\", legacy::fields(r));
}}
"
    );
    assert_eq!(rows(CALLER, &text), vec![old("legacy", 2)]);
}

#[test]
fn the_full_path_counts_without_an_import_of_the_module() {
    let text = "
use lattice_core::application::services::model_contract;
fn build() {
    let r = model_contract::legacy::completion(fields);
    let s = lattice_core::application::services::model_contract::legacy::schema(json);
}
";
    assert_eq!(rows(CALLER, text), vec![old("legacy", 2)]);
}

#[test]
fn a_legacy_name_the_file_did_not_import_from_model_contract_is_not_counted() {
    let text = "
mod legacy { pub fn completion() {} }
fn build() { legacy::completion(); }
";
    assert_eq!(rows(CALLER, text), Vec::new());
    let other = "
use crate::storage::legacy;
fn build() { legacy::open(); }
";
    assert_eq!(rows(CALLER, other), Vec::new());
}

#[test]
fn the_module_may_be_imported_through_a_group_or_self() {
    for import in [
        "use lattice_core::application::services::{model_contract::{legacy, answer}, other};",
        "use lattice_core::application::services::model_contract::{self, legacy};",
        "use lattice_core::application::services::model_contract::legacy::{self};",
    ] {
        let text = format!("{import}\nfn build() {{ legacy::says(&mut r); }}\n");
        assert_eq!(rows(CALLER, &text), vec![old("legacy", 1)], "{import}");
    }
}

#[test]
fn calls_of_the_answer_function_count_when_it_is_imported_or_qualified() {
    let text = "
use lattice_core::application::services::model_contract::{answer, ask};
async fn run() {
    let a = answer(&transport, request).await;
    let b = answer::<T, A>(&transport, request).await;
    let c = model_contract::answer(&transport, request).await;
    let d = lattice_core::application::services::model_contract::answer(&t, r).await;
    let e = reply.answer(question);
    let f = answer;
}
";
    assert_eq!(rows(CALLER, text), vec![old("answer", 4)]);
}

#[test]
fn an_answer_function_of_another_module_is_not_counted() {
    let text = "
use crate::quiz::answer;
fn run() { answer(1); other::answer(2); }
fn answer_again() { answer(3); }
";
    assert_eq!(rows(CALLER, text), Vec::new());
}

#[test]
fn a_renamed_or_glob_import_of_answer_counts_its_calls() {
    let renamed = "
use lattice_core::application::services::model_contract::answer as old_answer;
async fn run() { old_answer(&t, r).await; answer(1); }
";
    assert_eq!(rows(CALLER, renamed), vec![old("answer", 1)]);
    let glob = "
use lattice_core::application::services::model_contract::*;
async fn run() { answer(&t, r).await; }
";
    assert_eq!(rows(CALLER, glob), vec![old("answer", 1)]);
}

#[test]
fn answer_calls_inside_macros_count_but_methods_and_definitions_do_not() {
    let text = "
use lattice_core::application::services::model_contract::answer;
async fn run() {
    let n = format!(\"{}\", answer(&t, r).await.len());
    let m = format!(\"{}\", reply.answer(q));
    tokio::spawn(async move { model_contract::answer(&t, r).await });
}
";
    assert_eq!(rows(CALLER, text), vec![old("answer", 2)]);
}

#[test]
fn both_kinds_count_under_their_own_target() {
    let text = format!(
        "{IMPORT}use lattice_core::application::services::model_contract::answer;
async fn run() {{
    let r = legacy::completion(fields);
    answer(&t, r).await;
    answer(&t, r).await;
}}
"
    );
    assert_eq!(
        rows(CALLER, &text),
        vec![old("answer", 2), old("legacy", 1)]
    );
}

#[test]
fn test_code_is_not_counted() {
    let text = format!(
        "{IMPORT}use lattice_core::application::services::model_contract::answer;
#[cfg(test)]
mod tests {{ fn t() {{ legacy::says(&mut r); answer(&t, r); }} }}
#[cfg(any(test, feature = \"test-support\"))]
pub mod fixtures {{ pub fn f() {{ legacy::completion(fields); }} }}
pub mod test_support {{ pub fn g() {{ legacy::completion(fields); }} }}
#[cfg(test)]
fn helper() {{ legacy::schema(json); }}
fn real() {{
    #[cfg(test)]
    let x = legacy::schema(json);
    legacy::schema(json);
}}
"
    );
    assert_eq!(rows(CALLER, &text), vec![old("legacy", 1)]);
}

#[test]
fn test_files_are_not_counted() {
    let text = format!("{IMPORT}fn t() {{ legacy::says(&mut r); }}\n");
    for relative in [
        "features/chat/tests.rs",
        "features/chat/ask_tests.rs",
        "features/chat/tests/support.rs",
    ] {
        assert_eq!(rows(relative, &text), Vec::new(), "{relative}");
    }
}

#[test]
fn the_legacy_module_is_not_counted_but_the_same_path_in_another_crate_is() {
    let text = "
use lattice_core::application::services::model_contract::legacy;
fn again() { legacy::completion(fields); }
";
    assert_eq!(rows(LEGACY_FILE, text), Vec::new());
    assert_eq!(
        rows_in("lattice-learning/", LEGACY_FILE, text),
        vec![row(
            OLD_MODEL_CALL,
            &format!("lattice-learning/{LEGACY_FILE}"),
            "legacy",
            1
        )]
    );
}

#[test]
fn a_library_files_subject_carries_its_crate_prefix_and_each_site_its_line() {
    let text = format!("{IMPORT}fn a() {{\n    legacy::says(&mut r);\n}}\n");
    let found = findings(&source("lattice-learning/", "lessons/ask.rs", &text));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].key.subject, "lattice-learning/lessons/ask.rs");
    assert_eq!(found[0].location, "lattice-learning/lessons/ask.rs:3");
}

#[test]
fn importing_an_item_a_glob_or_a_renamed_module_is_flagged_not_counted() {
    for import in [
        "use lattice_core::application::services::model_contract::legacy::completion;",
        "use lattice_core::application::services::model_contract::legacy::{completion, Completion};",
        "use lattice_core::application::services::model_contract::legacy::*;",
        "use lattice_core::application::services::model_contract::legacy as old;",
        "use lattice_core::application::services::model_contract::legacy::{self as old};",
        "use lattice_core::application::services::model_contract::{legacy::says};",
    ] {
        let hidden = rows(CALLER, &format!("{import}\nfn f() {{ completion(x); }}\n"));
        assert_eq!(hidden.len(), 1, "{import}");
        assert_eq!(
            (hidden[0].0.as_str(), hidden[0].2.as_str()),
            (LEGACY_ITEM_IMPORT, "legacy"),
            "{import}"
        );
    }
}

#[test]
fn each_hidden_import_is_its_own_site_with_the_path_it_names() {
    let text = "
use lattice_core::application::services::model_contract::legacy::{completion, says};
use lattice_core::application::services::model_contract::legacy::*;
";
    assert_eq!(
        locations(CALLER, text),
        vec![
            format!("{CALLER}:2 lattice_core::application::services::model_contract::legacy::completion"),
            format!("{CALLER}:2 lattice_core::application::services::model_contract::legacy::says"),
            format!("{CALLER}:3 lattice_core::application::services::model_contract::legacy::*"),
        ]
    );
    assert_eq!(
        rows(CALLER, text),
        vec![row(LEGACY_ITEM_IMPORT, CALLER, "legacy", 3)]
    );
}

#[test]
fn an_import_in_test_code_is_not_flagged() {
    let text = "
#[cfg(test)]
mod tests { use lattice_core::application::services::model_contract::legacy::*; }
";
    assert_eq!(rows(CALLER, text), Vec::new());
}

#[test]
fn old_model_calls_are_ratcheted_and_item_imports_are_held_at_zero() {
    let baseline = parse_baseline(&format!(
        "{OLD_MODEL_CALL} {CALLER} legacy 3\n{OLD_MODEL_CALL} {CALLER} answer 1\n"
    ))
    .unwrap();
    assert_eq!(baseline.len(), 2);
    assert!(render_baseline(&baseline).contains(&format!("{OLD_MODEL_CALL} {CALLER} answer 1\n")));
    assert!(parse_baseline(&format!("{LEGACY_ITEM_IMPORT} {CALLER} legacy 1\n")).is_err());
    let hidden: Counts = Counts::from([(Key::new(LEGACY_ITEM_IMPORT, CALLER, "legacy"), 1)]);
    assert!(!render_baseline(&hidden)
        .lines()
        .any(|line| line.starts_with(LEGACY_ITEM_IMPORT)));
}

#[test]
fn a_tree_with_an_old_call_fails_until_the_baseline_records_it() {
    use crate::ratchet::{check, Files};
    use std::fs;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "lattice-old-model-calls-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("lib.rs"),
        format!("{IMPORT}pub fn run() {{ legacy::says(&mut r); legacy::schema(j); }}"),
    )
    .unwrap();
    let baseline = root.join("baseline.txt");
    let feature_graph = root.join("feature-graph.txt");
    fs::write(&feature_graph, "# nothing allowed\n").unwrap();
    let files = Files {
        baseline: &baseline,
        feature_graph: &feature_graph,
        libraries: &[],
    };
    let mut writing = 0;
    check(&[&root], &files, true, &mut writing).unwrap();
    let written = fs::read_to_string(&baseline).unwrap();
    let mut recorded = 0;
    check(&[&root], &files, false, &mut recorded).unwrap();
    fs::write(&baseline, "").unwrap();
    let mut unrecorded = 0;
    check(&[&root], &files, false, &mut unrecorded).unwrap();
    fs::remove_dir_all(&root).unwrap();

    assert!(written.contains("old-model-call lib.rs legacy 2\n"));
    assert_eq!((writing, recorded, unrecorded), (0, 0, 1));
}

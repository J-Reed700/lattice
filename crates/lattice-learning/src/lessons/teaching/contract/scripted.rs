//! Test doubles for the asks of the check: scripted answers written as a
//! reviewer would give them about the lesson's words (items by id, sentences
//! by quote) are turned into the pointers the asks take, by reading the names
//! the request shows.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::confirm::{CLASH_TASK, INTRO as CONFIRM_INTRO, PLANNED_TASK};
use super::{FIND_TASK, INTRO, REPORTS_TASK};
use lattice_core::shared::text::same_text::fold;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

/// What a scripted check answers when nothing is found, written as one answer to the whole check.
pub(crate) const CONTRACT_NOTHING: &str =
    "{\"planned\":[],\"usedBeforeTaught\":[],\"contradictions\":[],\"unseen\":[]}";

/// Which ask a request is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Ask {
    Planned,
    Find,
    Reports,
    ConfirmPlanned,
    ConfirmUse,
    ConfirmClash,
}

/// The ask a request with these messages is, when it is one of the check's.
pub(crate) fn ask_of(system: &str, user: &str) -> Option<Ask> {
    if system.starts_with(CONFIRM_INTRO) {
        return Some(if user.contains(PLANNED_TASK) {
            Ask::ConfirmPlanned
        } else if user.contains(CLASH_TASK) {
            Ask::ConfirmClash
        } else {
            Ask::ConfirmUse
        });
    }
    if system.starts_with(INTRO) {
        return Some(if user.contains(REPORTS_TASK) {
            Ask::Reports
        } else if user.contains(FIND_TASK) {
            Ask::Find
        } else {
            Ask::Planned
        });
    }
    None
}

/// The task a request carries.
pub(crate) fn task_of(user: &str) -> Value {
    user.rsplit("\n\n")
        .find_map(|chunk| {
            serde_json::from_str::<Value>(chunk)
                .ok()
                .filter(|task| task.get("task").is_some())
        })
        .unwrap_or(Value::Null)
}

/// Every sentence the task shows, as its name and its words.
pub(crate) fn rows(task: &Value) -> Vec<(String, String)> {
    fn walk(value: &Value, found: &mut Vec<(String, String)>) {
        match value {
            Value::String(text) => {
                for line in text.lines() {
                    let Some((name, words)) = line
                        .strip_prefix('[')
                        .and_then(|rest| rest.split_once("] "))
                    else {
                        continue;
                    };
                    let number = name.strip_prefix(['u', 'e']).unwrap_or_default();
                    if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
                        found.push((name.to_owned(), words.to_owned()));
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, found)),
            Value::Object(fields) => fields.values().for_each(|item| walk(item, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(task, &mut found);
    found
}

/// The names of every sentence that holds `quote`; when none does, of every sentence a quote over several sentences holds.
fn holding(rows: &[(String, String)], quote: &str) -> Vec<String> {
    let needle = fold(quote);
    if needle.is_empty() {
        return Vec::new();
    }
    let holds: Vec<String> = rows
        .iter()
        .filter(|(_, words)| fold(words).contains(&needle))
        .map(|(name, _)| name.clone())
        .collect();
    if !holds.is_empty() {
        return holds;
    }
    rows.iter()
        .filter(|(_, words)| {
            let words = fold(words);
            !words.is_empty() && needle.contains(&words)
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// The name of the first sentence that holds `quote`.
fn first(rows: &[(String, String)], quote: &str) -> Vec<String> {
    holding(rows, quote).into_iter().take(1).collect()
}

/// The words of the sentence a confirming read of a use asks about.
pub(crate) fn use_sentence(user: &str) -> Option<String> {
    let task = task_of(user);
    let at = task["use"]["sentence"].as_str()?.to_owned();
    rows(&task)
        .into_iter()
        .find(|(name, _)| *name == at)
        .map(|(_, words)| words)
}

fn quote_of(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

/// What the scripted check answers, written as one answer to all the asks.
enum Old {
    Json(Value),
    Raw(String),
}

/// The scripted answers of the check, one for each time it is made. Its asks of a pass are made in the order planned, find, reports.
pub(crate) struct Script {
    answers: Vec<Old>,
    finds: AtomicUsize,
}

impl Script {
    /// The check answers `answers`, each written as one answer to the whole check.
    pub(crate) fn new(answers: Vec<String>) -> Self {
        let answers = answers
            .into_iter()
            .map(|text| match serde_json::from_str::<Value>(&text) {
                Ok(old) => Old::Json(old),
                Err(_) => Old::Raw(text),
            })
            .collect();
        Self {
            answers,
            finds: AtomicUsize::new(0),
        }
    }

    fn listed(&self, pass: usize, name: &str) -> Vec<Value> {
        match self.answers.get(pass) {
            Some(Old::Json(old)) => old[name].as_array().cloned().unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// The reply to the ask `ask`, a request with the user message `user`.
    pub(crate) fn reply(&self, ask: Ask, user: &str) -> String {
        let task = task_of(user);
        let rows = rows(&task);
        match ask {
            Ask::Planned => {
                let pass = self.finds.load(Ordering::SeqCst);
                planned(&task, &rows, &self.listed(pass, "planned"))
            }
            Ask::Find => {
                let pass = self.finds.fetch_add(1, Ordering::SeqCst);
                match self.answers.get(pass) {
                    Some(Old::Json(old)) => found(&rows, old),
                    Some(Old::Raw(text)) => text.clone(),
                    None => nothing(ask, user),
                }
            }
            Ask::Reports => {
                let pass = self.finds.load(Ordering::SeqCst).saturating_sub(1);
                reports(&task, &self.listed(pass, "reports"))
            }
            Ask::ConfirmPlanned | Ask::ConfirmUse | Ask::ConfirmClash => nothing(ask, user),
        }
    }
}

/// The entries of a planned item each: taught where `old` says so and its quote is a sentence the request shows.
fn planned(task: &Value, rows: &[(String, String)], old: &[Value]) -> String {
    let listed = task["thisLesson"]["planned"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut counts: Vec<(String, usize)> = Vec::new();
    let mut entries = Vec::new();
    for item in &listed {
        let kind = item["kind"].as_str().unwrap_or_default().to_owned();
        let id = if kind == "contrast" {
            kind.clone()
        } else {
            let at = counts.iter().position(|(seen, _)| *seen == kind);
            let number = match at {
                Some(at) => {
                    counts[at].1 += 1;
                    counts[at].1 - 1
                }
                None => {
                    counts.push((kind.clone(), 1));
                    0
                }
            };
            format!("{kind}-{number}")
        };
        let entry = old.iter().find(|entry| entry["item"] == json!(id));
        let units = match entry {
            Some(entry) if entry["taught"] == json!(true) => first(rows, quote_of(&entry["quote"])),
            _ => Vec::new(),
        };
        entries.push(json!({
            "of": item["id"],
            "taught": if units.is_empty() { "not_taught" } else { "taught" },
            "units": units,
        }));
    }
    json!({ "entries": entries }).to_string()
}

/// The findings of `old`, each quote read as the sentences of the request that hold it.
fn found(rows: &[(String, String)], old: &Value) -> String {
    let mut items = Vec::new();
    for used in old["usedBeforeTaught"].as_array().into_iter().flatten() {
        let uses: Vec<String> = used["uses"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|quote| holding(rows, quote_of(quote)))
            .collect();
        if uses.is_empty() {
            continue;
        }
        let later = first(rows, quote_of(&used["taughtLater"]));
        items.push(json!({"finding": {"kind": "usedBeforeTaught", "what": used["what"], "uses": uses, "taughtLater": later}}));
    }
    for clash in old["contradictions"].as_array().into_iter().flatten() {
        let (one, other) = (
            first(rows, quote_of(&clash["quote"])),
            first(rows, quote_of(&clash["otherQuote"])),
        );
        if let (Some(one), Some(other)) = (one.first(), other.first()) {
            items
                .push(json!({"finding": {"kind": "contradiction", "first": one, "second": other}}));
        }
    }
    for unseen in old["unseen"].as_array().into_iter().flatten() {
        let never = if unseen["neverShown"] == json!(true) {
            "yes"
        } else {
            "no"
        };
        for name in holding(rows, quote_of(&unseen["quote"])) {
            items.push(json!({"finding": {"kind": "unseen", "words": name, "what": unseen["what"], "neverShown": never}}));
        }
    }
    json!({ "items": items }).to_string()
}

/// A ruling for each report the request shows: what `old` says of it, by its position.
fn reports(task: &Value, old: &[Value]) -> String {
    let listed = task["reported"].as_array().cloned().unwrap_or_default();
    let entries: Vec<Value> = listed
        .iter()
        .enumerate()
        .map(|(position, report)| {
            let ruled = old
                .iter()
                .find(|entry| entry["report"] == json!(format!("report-{position}")));
            json!({
                "of": report["id"],
                "about": if ruled.is_some_and(|entry| entry["about"] == json!(true)) { "decided" } else { "other" },
            })
        })
        .collect();
    json!({ "entries": entries }).to_string()
}

/// The reply of a confirming read that `old`, written as a reviewer would give it about the words, makes.
pub(crate) fn confirm(ask: Ask, old: &str, user: &str) -> String {
    let Ok(old) = serde_json::from_str::<Value>(old) else {
        return old.to_owned();
    };
    let task = task_of(user);
    let rows = rows(&task);
    let said = quote_of(&old["quote"]);
    match ask {
        Ask::ConfirmPlanned => {
            let units = if old["taught"] == json!(true) {
                first(&rows, said)
            } else {
                Vec::new()
            };
            let belongs = old["belongs"]
                .as_str()
                .and_then(|part| {
                    let (list, number) = part.split_once('-')?;
                    let list = if list == "section" {
                        "sections"
                    } else {
                        "questions"
                    };
                    task[list].get(number.parse::<usize>().ok()?)?["id"]
                        .as_str()
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| "none".to_owned());
            json!({
                "taught": if units.is_empty() { "not_taught" } else { "taught" },
                "units": units,
                "belongs": belongs,
            })
            .to_string()
        }
        Ask::ConfirmUse => {
            let named = if old["taught"] == json!(true) {
                holding(&rows, said)
            } else {
                Vec::new()
            };
            let (given, units): (Vec<String>, Vec<String>) =
                named.into_iter().partition(|name| name.starts_with('e'));
            let mark = |yes: bool| if yes { "yes" } else { "no" };
            json!({
                "asked": mark(old["asked"] == json!(true)),
                "taught": mark(!units.is_empty() || !given.is_empty()),
                "units": units,
                "given": given,
            })
            .to_string()
        }
        _ => old.to_string(),
    }
}

/// The reply that finds nothing: no planned item taught, nothing found, every report of another kind, and a confirming read that confirms nothing.
pub(crate) fn nothing(ask: Ask, user: &str) -> String {
    let task = task_of(user);
    match ask {
        Ask::Planned => {
            let entries: Vec<Value> = task["thisLesson"]["planned"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|item| json!({"of": item["id"], "taught": "not_taught", "units": []}))
                .collect();
            json!({ "entries": entries }).to_string()
        }
        Ask::Find => json!({"items": []}).to_string(),
        Ask::Reports => reports(&task, &[]),
        Ask::ConfirmPlanned => {
            json!({"taught": "not_taught", "units": [], "belongs": "none"}).to_string()
        }
        Ask::ConfirmUse => {
            json!({"asked": "yes", "taught": "no", "units": [], "given": []}).to_string()
        }
        Ask::ConfirmClash => json!({"case": "", "contradict": false}).to_string(),
    }
}

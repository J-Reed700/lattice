//! Apply explicit edits atomically. Unmentioned lesson bytes and items cannot
//! change as a side effect of asking a model to correct one fact. What the
//! model is asked lives in `asking`.
use super::copied_wording::{self, Wording, SAY_IT_YOURSELF};
use super::repeats::{self, Counts};
use super::review_findings::{index_segment, FindingSite};
use lattice_core::domain::learning::claim_review::span_of;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::text::same_text;
use serde::Deserialize;
#[cfg(any(test, feature = "test-support"))]
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

mod asking;
mod citations;
#[cfg(test)]
use asking::schema;
pub(super) use asking::{
    instructions, rewrite_instructions, schema_for, schema_for_findings, INSTRUCTIONS,
    NEVER_SAY_WHERE, OWN_FIGURES, OWN_SENTENCE, QUOTED_WORDING, STEPS_IN_ORDER, UNQUOTED_REMARKS,
};
#[cfg(test)]
pub(crate) use citations::NamedUncite;
use citations::{append, taught_at};
pub(super) use citations::{citable_texts, citation_entry};
pub(crate) use citations::{Cite, Patch, Uncite};

fn paths(value: &Value, path: &str, output: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if key != "kind" {
                    paths(
                        value,
                        &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                        output,
                    );
                }
            }
        }
        Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                paths(value, &format!("{path}/{index}"), output);
            }
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) => output.push(path.into()),
        _ => (),
    }
}

/// The path of every citation `candidate` lists, `/blocks/{i}/citations/{j}`
/// or `/questions/{i}/citations/{j}`.
fn cited_paths(candidate: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for list in ["blocks", "questions"] {
        let parts = candidate.get(list).and_then(Value::as_array);
        for (index, part) in parts.into_iter().flatten().enumerate() {
            let count = part
                .get("citations")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            found.extend((0..count).map(|at| format!("/{list}/{index}/citations/{at}")));
        }
    }
    found
}

/// The part and position a path from [`cited_paths`] names, if `candidate`
/// lists that citation.
fn cited_at(candidate: &Value, path: &str) -> Option<(String, usize)> {
    let (part, at) = path.rsplit_once("/citations/")?;
    let at = index_segment(at)?;
    let part = FindingSite::of_pointer(part)
        .pointer()
        .filter(|canonical| canonical == part)?;
    candidate
        .pointer(&format!("{part}/citations/{at}"))
        .map(|_| (part, at))
}

/// Takes back the citations at `uncite` from `result`, the later ones of a
/// part first so that no index shifts under another.
fn take_back(result: &mut Value, mut uncite: Vec<(String, usize)>) {
    uncite.sort();
    uncite.dedup();
    for (part, at) in uncite.into_iter().rev() {
        if let Some(listed) = result
            .pointer_mut(&format!("{part}/citations"))
            .and_then(Value::as_array_mut)
        {
            if at < listed.len() {
                listed.remove(at);
            }
        }
    }
}

#[derive(Clone, Deserialize)]
#[cfg_attr(
    any(test, feature = "test-support"),
    derive(Serialize, Debug, PartialEq)
)]
#[serde(deny_unknown_fields)]
pub(crate) struct Edit {
    #[serde(default)]
    pub(crate) finding: Option<String>,
    pub(crate) path: String,
    pub(crate) before: Value,
    pub(crate) after: Value,
}

/// The part of the input a path belongs to: its block or question, or else
/// the rest of the input.
fn part_of(path: &str) -> String {
    FindingSite::of_pointer(path).pointer().unwrap_or_default()
}

/// The quotes of the block or question at `part`, and those of the citations
/// the patch appends to it: the source wording its sentences may not copy.
fn quotes_for(candidate: &Value, part: &str, cites: &[Cite]) -> Wording {
    let Some(held) = Some(part)
        .filter(|part| !part.is_empty())
        .and_then(|part| candidate.pointer(part))
    else {
        return Wording::default();
    };
    Wording::of_part(
        held,
        cites
            .iter()
            .filter(|cite| cite.path == part)
            .map(|cite| cite.quote.as_str()),
    )
}

/// Why `before` is not exactly in the field at `path` of `candidate`: when its
/// words stand there, or in one other taught field, once under the text
/// identity ([`same_text`]), the reason gives the lesson's own sentences to
/// copy from.
fn not_found(candidate: &Value, path: &str, before: &str, index: usize) -> String {
    let plain = format!("the before text of edit {index} does not occur in {path}.");
    let exact_in = |field: &str| -> Option<(Vec<(usize, usize)>, String)> {
        let text = candidate.pointer(field)?.as_str()?;
        let hits = same_text::find_all(text, before);
        let span = hits.first().copied()?;
        let exact = same_text::exact_sentences(text, span)?.to_owned();
        Some((hits, exact))
    };
    if let Some((hits, exact)) = exact_in(path) {
        return if hits.len() == 1 {
            format!("the before text of edit {index} is not in {path} exactly; its words stand there as \"{exact}\". Copy the before text exactly from that, with its formatting and punctuation.")
        } else {
            plain
        };
    }
    let mut fields = Vec::new();
    paths(candidate, "", &mut fields);
    let mut elsewhere = fields
        .into_iter()
        .filter(|other| other != path && repeats::teaches_path(other))
        .filter_map(|other| exact_in(&other).map(|(hits, exact)| (other, hits.len(), exact)));
    match (elsewhere.next(), elsewhere.next()) {
        (Some((other, 1, exact)), None) => format!("the before text of edit {index} does not occur in {path}; its words stand in {other} as \"{exact}\". Give that path, and copy the before text exactly."),
        _ => plain,
    }
}

/// Where one edit falls in a text field, and what replaces it.
struct Range {
    start: usize,
    end: usize,
    after: String,
    index: usize,
}

/// One field's change under a group of edits: where its edits fall in a text,
/// or the new value of a number or a boolean.
enum Change {
    Text {
        path: String,
        ranges: Vec<Range>,
    },
    Scalar {
        path: String,
        index: usize,
        value: Value,
    },
}

impl Change {
    fn path(&self) -> &str {
        match self {
            Self::Text { path, .. } | Self::Scalar { path, .. } => path,
        }
    }
}

/// The change one field takes under its edits, or the first problem with
/// them, as `(edit number, what is wrong)`.
fn field_change(
    candidate: &Value,
    path: &str,
    original: &Value,
    edits: Vec<(usize, Edit)>,
    (lesson, part, wording): (&Counts, &Wording, &Wording),
) -> std::result::Result<Change, (usize, String)> {
    let first = edits.first().map_or(0, |(index, _)| *index);
    if let Some(text) = original.as_str() {
        let mut ranges = Vec::new();
        for (index, edit) in &edits {
            let index = *index;
            let (Some(before), Some(after)) = (
                edit.before.as_str().filter(|s| !s.is_empty()),
                edit.after.as_str(),
            ) else {
                return Err((
                    index,
                    format!("edit {index} to {path} must replace nonempty text with text."),
                ));
            };
            if !text.contains(before) {
                return Err((index, not_found(candidate, path, before, index)));
            }
            let Some((start, _)) = span_of(text, before) else {
                return Err((index, format!("the before text of edit {index} occurs more than once in {path}; include enough surrounding text to make it unique.")));
            };
            let mut alone = text.to_owned();
            alone.replace_range(start..start + before.len(), after);
            let added = repeats::added(text, &alone);
            let repeated = repeats::says_at(path)
                .then(|| {
                    added
                        .iter()
                        .find(|sentence| lesson.contains_key(&repeats::key(sentence)))
                })
                .flatten();
            if let Some(sentence) = repeated {
                return Err((
                    index,
                    format!("edit {index} to {path} adds a sentence the lesson already has: \"{sentence}\". Reword it or leave it out."),
                ));
            }
            let copied = taught_at(path)
                .then(|| {
                    added.iter().find_map(|sentence| {
                        copied_wording::copies(sentence, part, wording)
                            .into_iter()
                            .next()
                    })
                })
                .flatten();
            if let Some(copied_wording::Copied { sentence, words }) = copied {
                return Err((
                    index,
                    format!("edit {index} to {path} writes the source's wording as the lesson's own sentence: \"{sentence}\" (the source's words are \"{words}\"). {SAY_IT_YOURSELF}"),
                ));
            }
            ranges.push(Range {
                start,
                end: start + before.len(),
                after: after.to_owned(),
                index,
            });
        }
        ranges.sort_by_key(|range| range.start);
        if ranges
            .windows(2)
            .any(|pair| matches!(pair, [left, right] if same_text::overlaps((left.start, left.end), (right.start, right.end))))
        {
            return Err((first, format!("two edits to {path} overlap.")));
        }
        if blanks_citation_sentence(path, text, &ranges) {
            return Err((
                first,
                format!("edits to {path} would leave the citation's sentence blank; to take the citation back, list {} in uncite.", path.strip_suffix("/sentence").unwrap_or(path)),
            ));
        }
        return Ok(Change::Text {
            path: path.to_owned(),
            ranges,
        });
    }
    let [(index, edit)] = <[_; 1]>::try_from(edits).map_err(|_| {
        (
            first,
            format!("{path} holds one value; give one edit for it."),
        )
    })?;
    if &edit.before != original
        || !(original.is_number() && edit.after.is_number()
            || original.is_boolean() && edit.after.is_boolean())
    {
        return Err((index, format!("edit {index} to {path} must give its current value as before and a value of the same type as after.")));
    }
    Ok(Change::Scalar {
        path: path.to_owned(),
        index,
        value: edit.after,
    })
}

/// Whether `ranges`, replaced in `text` at `path`, leave a citation's sentence blank.
fn blanks_citation_sentence(path: &str, text: &str, ranges: &[Range]) -> bool {
    let sentence_path = matches!(
        path.split('/').collect::<Vec<_>>().as_slice(),
        ["", "blocks" | "questions", _, "citations", _, "sentence"]
    );
    if !sentence_path {
        return false;
    }
    let mut result = text.to_owned();
    for range in ranges.iter().rev() {
        result.replace_range(range.start..range.end, &range.after);
    }
    result.trim().is_empty()
}

type Problems = Vec<(usize, String)>;

/// One group's edits checked together: the change to each of its fields, or
/// every problem found, so that a group with any problem changes nothing.
fn group_changes(
    candidate: &Value,
    allowed: &[String],
    edits: Vec<(usize, Edit)>,
    held: (&Counts, &Wording, &Wording),
) -> std::result::Result<Vec<Change>, Problems> {
    let mut problems = Vec::new();
    let mut fields = std::collections::BTreeMap::<String, Vec<(usize, Edit)>>::new();
    for (index, edit) in edits {
        if allowed.contains(&edit.path) {
            fields
                .entry(edit.path.clone())
                .or_default()
                .push((index, edit));
        } else {
            problems.push((
                index,
                format!(
                    "edit {index} names {}, which is not an editable field.",
                    edit.path
                ),
            ));
        }
    }
    let mut changes = Vec::new();
    for (path, edits) in fields {
        let first = edits.first().map_or(0, |(index, _)| *index);
        match candidate.pointer(&path) {
            None => problems.push((first, format!("{path} is not in the draft."))),
            Some(original) => match field_change(candidate, &path, original, edits, held) {
                Ok(change) => changes.push(change),
                Err(problem) => problems.push(problem),
            },
        }
    }
    if problems.is_empty() {
        Ok(changes)
    } else {
        Err(problems)
    }
}

/// Applies a patch part by part: the edits of a block or question that has
/// any problem are left out and the other parts' edits apply.
#[cfg(test)]
pub(super) fn apply(candidate: &Value, patch: &str) -> Result<Value> {
    apply_in(candidate, patch, &Value::Null, &Wording::default())
}

/// [`apply`] where `lesson`, the lesson `candidate` is a part of, also holds
/// the sentences an edit may not add again, and `wording`, the lesson's saved
/// excerpts, is source wording an edit may not add a sentence that copies.
pub(super) fn apply_in(
    candidate: &Value,
    patch: &str,
    lesson: &Value,
    wording: &Wording,
) -> Result<Value> {
    apply_for(candidate, patch, lesson, wording, &|_, _| Ok(())).map(|applied| applied.value)
}

/// What a patch that names its findings did.
pub(super) struct AppliedFor {
    pub value: Value,
    /// For each finding the patch named: the fields its applied changes wrote,
    /// and why its other changes were left out.
    pub named: BTreeMap<String, Named>,
    /// The sentences applied edits wrote: (finding, path, sentence).
    pub written: Vec<(String, String, String)>,
}

#[derive(Default)]
pub(super) struct Named {
    /// JSON pointers of the fields its applied edits wrote, and of the
    /// `…/citations` lists its applied citations and uncites changed.
    pub applied: BTreeSet<String>,
    pub problems: Vec<String>,
    /// Every group of this finding that was refused was refused only because
    /// it overlapped an edit for another finding.
    pub deferred: bool,
}

impl Named {
    /// A reason a change of this finding was left out, and not an overlap.
    fn refused(&mut self, reason: String) {
        self.deferred = false;
        self.problems.push(reason);
    }
}

/// Whether a change naming `finding` may touch `part` (`/blocks/N`,
/// `/questions/N`, or "" for the rest of the input); otherwise why not, worded
/// to follow "edit 3 " / "citation 1 " / "uncite 0 ".
pub(super) type MayChange<'a> = &'a dyn Fn(Option<&str>, &str) -> std::result::Result<(), String>;

/// One finding's edits to one part of the input.
struct Group {
    finding: Option<String>,
    part: String,
    edits: Vec<(usize, Edit)>,
}

/// The first edit of `changes` that falls on what a group accepted before it
/// changes, as `(edit number, what is wrong)`.
fn clash(
    changes: &[Change],
    texts: &BTreeMap<String, Vec<(usize, usize)>>,
    scalars: &BTreeSet<String>,
) -> Option<(usize, String)> {
    changes
        .iter()
        .filter_map(|change| match change {
            Change::Text { path, ranges } => {
                let taken = texts.get(path)?;
                ranges
                    .iter()
                    .filter(|range| {
                        taken
                            .iter()
                            .any(|taken| same_text::overlaps((range.start, range.end), *taken))
                    })
                    .map(|range| (range.index, path.clone()))
                    .min_by_key(|(index, _)| *index)
            }
            Change::Scalar { path, index, .. } => {
                scalars.contains(path).then(|| (*index, path.clone()))
            }
        })
        .min_by_key(|(index, _)| *index)
        .map(|(index, path)| {
            (
                index,
                format!("edit {index} to {path} overlaps an edit for another finding."),
            )
        })
}

/// The edits of a patch grouped by finding and part, in the order of their first edit.
fn grouped(edits: Vec<Edit>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (index, edit) in edits.into_iter().enumerate() {
        if edit.before == edit.after {
            continue;
        }
        let part = part_of(&edit.path);
        match groups
            .iter_mut()
            .find(|group| group.finding == edit.finding && group.part == part)
        {
            Some(group) => group.edits.push((index, edit)),
            None => groups.push(Group {
                finding: edit.finding.clone(),
                part,
                edits: vec![(index, edit)],
            }),
        }
    }
    groups
}

/// The path with every array index ignored.
fn shape_of(path: &str) -> Vec<&str> {
    path.split('/')
        .map(|segment| {
            if index_segment(segment).is_some() {
                ""
            } else {
                segment
            }
        })
        .collect()
}

/// Gives each text edit whose text stands in no field of its path, but once in
/// exactly one field of the same shape, the path of that field.
fn placed(candidate: &Value, mut edits: Vec<Edit>) -> Vec<Edit> {
    let mut fields = Vec::new();
    paths(candidate, "", &mut fields);
    for edit in &mut edits {
        let Some(before) = edit.before.as_str().filter(|text| !text.is_empty()) else {
            continue;
        };
        let holding = |path: &str| {
            candidate
                .pointer(path)
                .and_then(Value::as_str)
                .filter(|text| text.contains(before))
        };
        if edit.before == edit.after || holding(&edit.path).is_some() {
            continue;
        }
        let shape = shape_of(&edit.path);
        let mut found = fields
            .iter()
            .filter(|field| shape_of(field) == shape)
            .filter_map(|field| holding(field).map(|text| (field, text)));
        let only = match (found.next(), found.next()) {
            (Some((field, text)), None) if span_of(text, before).is_some() => Some(field.clone()),
            _ => None,
        };
        if let Some(field) = only {
            edit.path = field;
        }
    }
    edits
}

/// What accepting and refusing the groups of a patch came to.
struct Accepted {
    named: BTreeMap<String, Named>,
    problems: Problems,
    applied: bool,
    kept: Vec<(Option<String>, Change)>,
}

/// Accepts or refuses each group in turn, against the rule and the groups accepted before it.
fn accept(
    groups: Vec<Group>,
    candidate: &Value,
    (lesson, wording): (&Value, &Wording),
    cites: &[Cite],
    may_change: MayChange<'_>,
) -> Accepted {
    let mut allowed = Vec::new();
    paths(candidate, "", &mut allowed);
    let held = repeats::said(&[candidate, lesson]);
    let mut accepted = Accepted {
        named: BTreeMap::new(),
        problems: Vec::new(),
        applied: false,
        kept: Vec::new(),
    };
    let mut texts = BTreeMap::<String, Vec<(usize, usize)>>::new();
    let mut scalars = BTreeSet::<String>::new();
    for group in groups {
        let finding = group.finding.as_deref();
        let mut overlapped = false;
        let found = match may_change(finding, &group.part) {
            Err(reason) => Err(group
                .edits
                .iter()
                .map(|(index, _)| (*index, format!("edit {index} {reason}")))
                .collect::<Problems>()),
            Ok(()) => {
                let quotes = quotes_for(candidate, &group.part, cites);
                group_changes(candidate, &allowed, group.edits, (&held, &quotes, wording)).and_then(
                    |changes| match clash(&changes, &texts, &scalars) {
                        Some(problem) => {
                            overlapped = true;
                            Err(vec![problem])
                        }
                        None => Ok(changes),
                    },
                )
            }
        };
        match found {
            Ok(changes) => {
                accepted.applied |= !changes.is_empty();
                for change in &changes {
                    match change {
                        Change::Text { path, ranges } => texts
                            .entry(path.clone())
                            .or_default()
                            .extend(ranges.iter().map(|range| (range.start, range.end))),
                        Change::Scalar { path, .. } => {
                            scalars.insert(path.clone());
                        }
                    }
                }
                if let Some(finding) = finding {
                    accepted
                        .named
                        .entry(finding.to_owned())
                        .or_default()
                        .applied
                        .extend(changes.iter().map(|change| change.path().to_owned()));
                }
                accepted.kept.extend(
                    changes
                        .into_iter()
                        .map(|change| (group.finding.clone(), change)),
                );
            }
            Err(found) => {
                if let Some(finding) = finding {
                    let named = accepted.named.entry(finding.to_owned()).or_default();
                    let only_overlaps = overlapped && (named.problems.is_empty() || named.deferred);
                    named
                        .problems
                        .extend(found.iter().map(|(_, reason)| reason.clone()));
                    named.deferred = only_overlaps;
                }
                accepted.problems.extend(found);
            }
        }
    }
    accepted
}

/// Writes the accepted changes into `result`; the sentences written, as (finding, path, sentence).
fn write_changes(
    candidate: &Value,
    kept: Vec<(Option<String>, Change)>,
    result: &mut Value,
) -> Vec<(String, String, String)> {
    let mut fields = BTreeMap::<String, Vec<(Option<String>, Range)>>::new();
    for (finding, change) in kept {
        match change {
            Change::Scalar { path, value, .. } => {
                if let Some(slot) = result.pointer_mut(&path) {
                    *slot = value;
                }
            }
            Change::Text { path, ranges } => fields
                .entry(path)
                .or_default()
                .extend(ranges.into_iter().map(|range| (finding.clone(), range))),
        }
    }
    let mut written = Vec::new();
    for (path, mut ranges) in fields {
        let Some(text) = candidate.pointer(&path).and_then(Value::as_str) else {
            continue;
        };
        ranges.sort_by_key(|(_, range)| range.start);
        let mut updated = text.to_owned();
        for (_, range) in ranges.iter().rev() {
            updated.replace_range(range.start..range.end, &range.after);
        }
        let (mut removed, mut added) = (0usize, 0usize);
        for (finding, range) in &ranges {
            if let Some(finding) = finding {
                let place = (range.start + added).saturating_sub(removed);
                for (from, to) in
                    same_text::sentences_over(&updated, (place, place + range.after.len()))
                {
                    if let Some(sentence) = updated.get(from..to).filter(|s| !s.trim().is_empty()) {
                        written.push((finding.clone(), path.clone(), sentence.to_owned()));
                    }
                }
            }
            removed += range.end - range.start;
            added += range.after.len();
        }
        if let Some(slot) = result.pointer_mut(&path) {
            *slot = json!(updated);
        }
    }
    written
}

/// Takes back the citations the patch withdraws; whether any went, and why others stay.
fn withdraw(
    candidate: &Value,
    uncites: &[Uncite],
    may_change: MayChange<'_>,
    result: &mut Value,
    named: &mut BTreeMap<String, Named>,
) -> (bool, Vec<String>) {
    let mut gone = Vec::new();
    let mut unnamed = Vec::new();
    for (index, uncite) in uncites.iter().enumerate() {
        let path = uncite.path();
        let outcome = match cited_at(candidate, path) {
            Some((part, at)) => may_change(uncite.finding(), &part)
                .map_err(|reason| format!("uncite {index} {reason}"))
                .map(|()| (part, at)),
            None => Err(format!(
                "uncite {index} names {path}, which is not a citation of the draft."
            )),
        };
        match outcome {
            Ok((part, at)) => {
                if let Some(finding) = uncite.finding() {
                    named
                        .entry(finding.to_owned())
                        .or_default()
                        .applied
                        .insert(format!("{part}/citations"));
                }
                gone.push((part, at));
            }
            Err(reason) => {
                if let Some(finding) = uncite.finding() {
                    named
                        .entry(finding.to_owned())
                        .or_default()
                        .refused(reason.clone());
                }
                unnamed.push(reason);
            }
        }
    }
    let removed = !gone.is_empty();
    take_back(result, gone);
    (removed, unnamed)
}

/// Appends the patch's citations to `result`; the reasons for the ones left out.
fn cite_all(
    cites: &[Cite],
    may_change: MayChange<'_>,
    result: &mut Value,
    named: &mut BTreeMap<String, Named>,
) -> Vec<String> {
    let mut refused: Vec<String> = Vec::new();
    for (index, cite) in cites.iter().enumerate() {
        let outcome = may_change(cite.finding.as_deref(), &cite.path)
            .and_then(|()| append(result, cite))
            .map_err(|reason| format!("citation {index} {reason}"));
        match (outcome, cite.finding.as_deref()) {
            (Ok(()), Some(finding)) => {
                named
                    .entry(finding.to_owned())
                    .or_default()
                    .applied
                    .insert(format!("{}/citations", cite.path));
            }
            (Ok(()), None) => {}
            (Err(reason), finding) => {
                if let Some(finding) = finding {
                    named
                        .entry(finding.to_owned())
                        .or_default()
                        .refused(reason.clone());
                }
                refused.push(reason);
            }
        }
    }
    refused
}

/// The patch a repair reply holds, before any of it is checked against the input.
pub(crate) fn read_patch(text: &str) -> Result<Patch> {
    crate::generation::parse_json(text)
}

/// `apply_patch_for` on the patch `patch` holds.
pub(super) fn apply_for(
    candidate: &Value,
    patch: &str,
    lesson: &Value,
    wording: &Wording,
    may_change: MayChange<'_>,
) -> Result<AppliedFor> {
    apply_patch_for(candidate, read_patch(patch)?, lesson, wording, may_change)
}

/// Applies a patch whose changes may name the finding they are for. The edits
/// of one finding to one block or question that has any problem are left out,
/// and everything else applies. `may_change` says which parts a finding's
/// changes may touch.
pub(super) fn apply_patch_for(
    candidate: &Value,
    patch: Patch,
    lesson: &Value,
    wording: &Wording,
    may_change: MayChange<'_>,
) -> Result<AppliedFor> {
    let fail = |reason: String| {
        AppError::InvalidInput(format!(
            "The repair's edits could not be applied: {reason} The draft was preserved."
        ))
    };
    if patch.asks_nothing() {
        return Err(fail("it gave no edits.".into()));
    }
    let mut accepted = accept(
        grouped(placed(candidate, patch.edits)),
        candidate,
        (lesson, wording),
        &patch.citations,
        may_change,
    );
    let mut result = candidate.clone();
    let written = write_changes(candidate, accepted.kept, &mut result);
    accepted.problems.sort_by_key(|(index, _)| *index);
    let problems: Vec<String> = accepted
        .problems
        .into_iter()
        .map(|(_, reason)| reason)
        .collect();
    let mut named = accepted.named;
    let (removed, unnamed) = withdraw(
        candidate,
        &patch.uncite,
        may_change,
        &mut result,
        &mut named,
    );
    let refused = cite_all(&patch.citations, may_change, &mut result, &mut named);
    if !accepted.applied && !removed && refused.len() == patch.citations.len() {
        let mut reasons = problems;
        reasons.extend(unnamed);
        reasons.extend(refused);
        if reasons.is_empty() {
            reasons.push("it changed nothing.".into());
        }
        return Err(fail(reasons.join(" ")));
    }
    if !problems.is_empty() || !refused.is_empty() || !unnamed.is_empty() {
        tracing::warn!(
            left_out = %problems.join(" "),
            refused = %[unnamed, refused].concat().join(" "),
            "Left out the edits and citations of a repair that the draft cannot hold"
        );
    }
    Ok(AppliedFor {
        value: result,
        named,
        written,
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod uncite_tests;

#[cfg(test)]
mod copied_tests;

#[cfg(test)]
mod locate_tests;

#[cfg(test)]
mod citation_entry_tests;

#[cfg(test)]
mod patch_tests;

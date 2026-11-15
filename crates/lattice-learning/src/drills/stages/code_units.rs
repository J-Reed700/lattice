//! Code as lines the model points at: one `Units` per file, and a change of lines made on them.

use super::ask::describe_fault;
use lattice_core::application::services::model_contract::ask::{Refusal, Settled, Why};
use lattice_core::application::services::model_contract::edit::{
    Anchor, Applied, Change, Consistent,
};
use lattice_core::application::services::model_contract::shape::Vocabulary;
use lattice_core::application::services::model_contract::shown::{Picked, Showing, Told};
use lattice_core::application::services::model_contract::units::{Grain, Unit, UnitRef, Units};
use lattice_core::domain::learning::drills::{apply, DrillFile, Edit, EditFault, Severity};
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};

/// A file of code a model reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CodeFile {
    pub path: Told<String>,
    pub text: Told<String>,
}

impl CodeFile {
    pub(crate) fn of(file: &DrillFile) -> Self {
        Self {
            path: Told(file.path.clone()),
            text: Told(file.text.clone()),
        }
    }
}

/// A severity as the model writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Weight(pub Severity);

impl Vocabulary for Weight {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("critical", Weight(Severity::Critical)),
        ("major", Weight(Severity::Major)),
        ("minor", Weight(Severity::Minor)),
        ("nit", Weight(Severity::Nit)),
    ];
}

impl Weight {
    /// The word the model writes for this severity.
    pub(crate) fn word(self) -> &'static str {
        Self::WORDS
            .iter()
            .find(|(_, weight)| *weight == self)
            .map_or("", |(word, _)| *word)
    }
}

/// The files of a request as lines, and the changes made on them.
pub(crate) struct CodeUnits {
    files: Vec<(String, Units)>,
}

impl CodeUnits {
    pub(crate) fn of(files: &[CodeFile]) -> Self {
        Self {
            files: files
                .iter()
                .map(|file| (file.path.0.clone(), Units::of(&file.text.0, Grain::Line)))
                .collect(),
        }
    }

    /// Every file between its own tags, one line to a row, each row under its handle.
    pub(crate) fn rendered(&self, showing: &mut Showing<'_>) -> Result<String> {
        let mut blocks = Vec::with_capacity(self.files.len());
        for (path, units) in &self.files {
            let shown = units.shown(showing)?;
            blocks.push(format!(
                "<file path=\"{path}\">\n{}\n</file>",
                units.rows(&shown)
            ));
        }
        Ok(blocks.join("\n\n"))
    }

    /// The path and the lines of the file a change edits. A refusal when it edits more than one file, or when
    /// it places something at the start with several files shown.
    pub(crate) fn file_of(
        &self,
        change: &Change<Unit, String>,
    ) -> std::result::Result<usize, Refusal> {
        let anchors = change
            .insert
            .iter()
            .map(|(anchor, _)| anchor)
            .chain(change.moved.iter().map(|(_, anchor)| anchor));
        let mut touched: Vec<UnitRef> = change
            .replace
            .iter()
            .map(|(unit, _)| unit.reference)
            .chain(change.remove.iter().map(|unit| unit.reference))
            .chain(change.moved.iter().map(|(unit, _)| unit.reference))
            .collect();
        let mut at_start = false;
        for anchor in anchors {
            match anchor {
                Anchor::Start => at_start = true,
                Anchor::After(unit) => touched.push(unit.reference),
            }
        }
        if at_start && self.files.len() > 1 {
            return Err(Refusal(
                "Place an insert or a move after a line of its file, not at the start.".into(),
            ));
        }
        let mut owners: Vec<usize> = Vec::new();
        for unit in touched {
            let Some(owner) = self
                .files
                .iter()
                .position(|(_, units)| units.text_of(unit).is_some())
            else {
                return Err(Refusal(
                    "A line of the change is not one of the lines shown.".into(),
                ));
            };
            if !owners.contains(&owner) {
                owners.push(owner);
            }
        }
        match owners.as_slice() {
            [] => Ok(0),
            [only] => Ok(*only),
            _ => Err(Refusal(
                "One repair changes lines of one file. Give a repair for each file.".into(),
            )),
        }
    }

    /// The shown text of a line.
    pub(crate) fn line(&self, unit: &Picked<Unit>) -> Option<&str> {
        self.files
            .iter()
            .find_map(|(_, units)| units.text_of(unit.reference))
    }

    /// Whether `change` leaves every line it names as it was.
    pub(crate) fn changes_nothing(&self, change: &Change<Unit, String>) -> bool {
        change.insert.is_empty()
            && change.remove.is_empty()
            && change.moved.is_empty()
            && change
                .replace
                .iter()
                .all(|(unit, with)| self.line(unit) == Some(with.as_str()))
    }

    /// The change made on the file it edits, whole or not at all.
    pub(crate) fn apply(&mut self, change: Consistent<Unit, String>) -> Settled<Applied<Unit>> {
        let file = match self.file_of(change.change()) {
            Ok(file) => file,
            Err(Refusal(told)) => return Settled::Unanswered(Why::Contradicted(told)),
        };
        match self.files.get_mut(file) {
            Some((_, units)) => units.apply(change),
            None => {
                Settled::Unanswered(Why::Contradicted("The change names no file shown.".into()))
            }
        }
    }

    /// Applies `change` and returns the file it edited, as it was and as it became.
    pub(crate) fn rewrite(&mut self, change: Consistent<Unit, String>) -> Option<Rewrite> {
        let at = self.file_of(change.change()).ok()?;
        let (path, units) = self.files.get(at)?;
        let (path, was) = (path.clone(), units.text());
        if !matches!(self.apply(change), Settled::Answered(_)) {
            return None;
        }
        let (_, units) = self.files.get(at)?;
        let now = units.text();
        Some(Rewrite { path, was, now })
    }

    /// The files with every change applied so far.
    pub(crate) fn files(&self) -> Vec<DrillFile> {
        self.files
            .iter()
            .map(|(path, units)| DrillFile {
                path: path.clone(),
                text: units.text(),
            })
            .collect()
    }
}

/// One file as it was and as a change made it.
pub(crate) struct Rewrite {
    pub path: String,
    pub was: String,
    pub now: String,
}

impl Rewrite {
    /// The edit that turns `was` into `now`, taking in `extra` more lines on each side of what changed.
    /// The lines between the first and last that differ come with one neighbouring line when either side would
    /// be empty. `None` when nothing differs or no line is left to keep.
    pub(crate) fn edit(&self, extra: usize) -> Option<Edit> {
        let before: Vec<&str> = self.was.split_inclusive('\n').collect();
        let after: Vec<&str> = self.now.split_inclusive('\n').collect();
        let shortest = before.len().min(after.len());
        let head = before
            .iter()
            .zip(&after)
            .take_while(|(old, new)| old == new)
            .count();
        let tail = before
            .iter()
            .rev()
            .zip(after.iter().rev())
            .take(shortest.saturating_sub(head))
            .take_while(|(old, new)| old == new)
            .count();
        if head == before.len() && head == after.len() {
            return None;
        }
        let mut start = head;
        let mut kept = tail;
        if before.len().saturating_sub(tail) <= start || after.len().saturating_sub(tail) <= start {
            if head > 0 {
                start = head.saturating_sub(1);
            } else if tail > 0 {
                kept = tail.saturating_sub(1);
            } else {
                return None;
            }
        }
        let start = start.saturating_sub(extra);
        let kept = kept.saturating_sub(extra);
        Some(Edit {
            path: self.path.clone(),
            before: before
                .get(start..before.len().saturating_sub(kept))?
                .concat(),
            after: after.get(start..after.len().saturating_sub(kept))?.concat(),
        })
    }

    /// How many lines of the file there were: no edit takes in more than this many extra lines.
    pub(crate) fn lines(&self) -> usize {
        self.was.split_inclusive('\n').count()
    }
}

/// The edit a change makes when it applies beside the other edits and can be undone, and the files it gives
/// alone. The edit takes in as many lines around the change as it needs to occur once in its file and be undone.
pub(crate) fn placed(
    files: &[CodeFile],
    others: &[Edit],
    change: &Consistent<Unit, String>,
) -> std::result::Result<(Edit, Vec<DrillFile>), Refusal> {
    let clean: Vec<DrillFile> = files
        .iter()
        .map(|file| DrillFile {
            path: file.path.0.clone(),
            text: file.text.0.clone(),
        })
        .collect();
    CodeUnits::of(files).file_of(change.change())?;
    let nothing = || Refusal("The change leaves the code as it is. Change the code.".into());
    let rewrite = CodeUnits::of(files)
        .rewrite(change.clone())
        .ok_or_else(nothing)?;
    let paths: Vec<String> = clean.iter().map(|file| file.path.clone()).collect();
    let mut labels: Vec<String> = others.iter().map(|_| "Another change".to_owned()).collect();
    labels.push("The new change".to_owned());
    for extra in 0..=rewrite.lines() {
        let new = rewrite.edit(extra).ok_or_else(nothing)?;
        if new.before == new.after {
            return Err(nothing());
        }
        if new.after.trim().is_empty() {
            continue;
        }
        let mut edits = others.to_vec();
        edits.push(new.clone());
        let changed = match apply(&clean, &edits) {
            Ok((changed, _)) => changed,
            Err(EditFault::Ambiguous { edit, .. }) if edit == others.len() => continue,
            Err(fault) => {
                return Err(Refusal(describe_fault(fault, &edits, &labels, &paths)));
            }
        };
        let fix = Edit {
            path: new.path.clone(),
            before: new.after.clone(),
            after: new.before.clone(),
        };
        if apply(&changed, std::slice::from_ref(&fix)).is_err() {
            continue;
        }
        let (alone, _) = apply(&clean, std::slice::from_ref(&new))
            .map_err(|_| Refusal("The new change does not apply to the code on its own.".into()))?;
        return Ok((new, alone));
    }
    Err(Refusal(
        "The new change cannot be placed so that it can be undone. Change other lines.".into(),
    ))
}

/// An edit as a prompt shows it: the file, the lines it replaces, and what they become.
pub(crate) fn describe_edit(edit: &Edit) -> String {
    format!(
        "In {}, replacing:\n<before>\n{}\n</before>\nwith:\n<after>\n{}\n</after>",
        edit.path, edit.before, edit.after
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use lattice_core::application::services::model_contract::edit::{consistent, Change};
    use lattice_core::application::services::model_contract::shown::{Kind, Scope};

    fn rewrite(was: &str, now: &str) -> Rewrite {
        Rewrite {
            path: "tides/almanac.py".into(),
            was: was.into(),
            now: now.into(),
        }
    }

    #[test]
    fn an_edit_is_the_lines_that_differ() {
        let edit = rewrite("a\nb\nc\n", "a\nB\nc\n").edit(0);
        assert_eq!(
            edit.map(|edit| (edit.before, edit.after)),
            Some(("b\n".to_owned(), "B\n".to_owned()))
        );
    }

    #[test]
    fn an_edit_takes_in_the_lines_around_it_when_asked() {
        let edit = rewrite("a\nb\nc\n", "a\nB\nc\n").edit(1);
        assert_eq!(
            edit.map(|edit| (edit.before, edit.after)),
            Some(("a\nb\nc\n".to_owned(), "a\nB\nc\n".to_owned()))
        );
    }

    #[test]
    fn an_edit_that_adds_or_removes_lines_keeps_a_neighbouring_line_on_both_sides() {
        let added = rewrite("a\nb\n", "a\nx\nb\n").edit(0);
        assert_eq!(
            added.map(|edit| (edit.before, edit.after)),
            Some(("a\n".to_owned(), "a\nx\n".to_owned()))
        );
        let removed = rewrite("a\nb\nc\n", "a\nc\n").edit(0);
        assert_eq!(
            removed.map(|edit| (edit.before, edit.after)),
            Some(("a\nb\n".to_owned(), "a\n".to_owned()))
        );
        let first = rewrite("a\nb\n", "x\na\nb\n").edit(0);
        assert_eq!(
            first.map(|edit| (edit.before, edit.after)),
            Some(("a\n".to_owned(), "x\na\n".to_owned()))
        );
    }

    #[test]
    fn nothing_differs_is_no_edit() {
        assert!(rewrite("a\nb\n", "a\nb\n").edit(0).is_none());
        assert!(rewrite("a\nb\n", "a\nb\n").edit(3).is_none());
    }

    fn files(texts: &[(&str, &str)]) -> Vec<CodeFile> {
        texts
            .iter()
            .map(|(path, text)| CodeFile {
                path: Told((*path).to_owned()),
                text: Told((*text).to_owned()),
            })
            .collect()
    }

    #[test]
    fn units_of_unchanged_files_give_the_files_back_byte_for_byte() {
        let shown = files(&[("a.py", "x = 1\r\n\r\n    y = 2"), ("b.py", "")]);
        let units = CodeUnits::of(&shown);
        let texts: Vec<String> = units.files().into_iter().map(|file| file.text).collect();
        assert_eq!(texts, ["x = 1\r\n\r\n    y = 2".to_owned(), String::new()]);
    }

    const PATH: &str = "tides/almanac.py";

    fn replacing(shown: &[CodeFile], line: usize, with: &str) -> Consistent<Unit, String> {
        let mut scope = Scope::for_question();
        let mut showing = scope.request();
        let units: Vec<Units> = shown
            .iter()
            .map(|file| Units::of(&file.text.0, Grain::Line))
            .collect();
        let mut picked: Vec<Picked<Unit>> = Vec::new();
        for lines in &units {
            let handles = lines.shown(&mut showing).unwrap();
            picked.extend(handles.iter().map(|(handle, unit)| Picked {
                handle: handle.clone(),
                reference: unit.reference(),
            }));
        }
        consistent(Change {
            replace: vec![(picked[line].clone(), with.to_owned())],
            insert: Vec::new(),
            remove: Vec::new(),
            moved: Vec::new(),
        })
        .unwrap()
    }

    fn other(before: &str, after: &str) -> Edit {
        Edit {
            path: PATH.into(),
            before: before.into(),
            after: after.into(),
        }
    }

    #[test]
    fn a_change_beside_other_edits_that_can_be_undone_is_placed_with_the_files_it_gives_alone() {
        let shown = files(&[(PATH, "a1\nb2\nc3\nd4\ne5\n")]);
        let change = replacing(&shown, 1, "B2");
        let (edit, alone) = placed(&shown, &[other("e5\n", "E5\n")], &change).unwrap();
        assert_eq!(
            (edit.before.as_str(), edit.after.as_str()),
            ("b2\n", "B2\n")
        );
        assert_eq!(
            alone,
            [DrillFile {
                path: PATH.into(),
                text: "a1\nB2\nc3\nd4\ne5\n".into()
            }]
        );
    }

    #[test]
    fn a_change_that_leaves_the_code_as_it_is_is_refused() {
        let shown = files(&[(PATH, "a1\nb2\nc3\n")]);
        let change = replacing(&shown, 1, "b2");
        let refusal = placed(&shown, &[], &change).unwrap_err();
        assert!(
            refusal.0.contains("leaves the code as it is"),
            "{}",
            refusal.0
        );
    }

    #[test]
    fn a_change_that_collides_with_another_edit_is_refused() {
        let shown = files(&[(PATH, "a1\nb2\nc3\nd4\ne5\n")]);
        let change = replacing(&shown, 1, "B2");
        let refusal = placed(&shown, &[other("b2\n", "X2\n")], &change).unwrap_err();
        assert!(
            refusal
                .0
                .contains("Another change and The new change overlap"),
            "{}",
            refusal.0
        );
    }

    #[test]
    fn a_change_whose_lines_occur_twice_takes_in_neighbouring_lines_until_it_occurs_once() {
        let shown = files(&[(PATH, "a1\nsame\nb2\nc3\nsame\nd4\n")]);
        let change = replacing(&shown, 1, "SAME");
        let (edit, alone) = placed(&shown, &[], &change).unwrap();
        assert_eq!(
            (edit.before.as_str(), edit.after.as_str()),
            ("a1\nsame\nb2\n", "a1\nSAME\nb2\n")
        );
        assert_eq!(
            alone.first().map(|file| file.text.as_str()),
            Some("a1\nSAME\nb2\nc3\nsame\nd4\n")
        );
    }
}

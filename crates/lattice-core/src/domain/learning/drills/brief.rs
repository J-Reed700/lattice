//! The brief of the lesson a drill is built from: what a model is told of the course, the module and the lesson, and
//! the evidence of what the lesson teaches that the issues of a drill cite by number.

use super::drill::{LessonLink, LessonSentence};
use super::fences::fenced_blocks;
use crate::shared::text::segmentation::flowed_sentences;
use serde::{Deserialize, Serialize};

/// What a drill is told of the lesson it is built from: the course, the module, the lesson, and evidence of what the
/// lesson teaches. A build makes it once and keeps it in its first checkpoint, so every later stage sees the same.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonBrief {
    pub lesson_id: String,
    /// The course's title.
    pub course: String,
    pub goal: String,
    pub prior_knowledge: String,
    /// The module's title.
    pub module: String,
    pub module_summary: String,
    pub outcomes: Vec<String>,
    /// The lesson's title.
    pub lesson: String,
    pub objective: String,
    /// Numbered from 1 when shown: L1, L2, ...
    pub evidence: Vec<LessonSentence>,
}

impl LessonBrief {
    /// The brief as a model is shown it: the course, module and lesson, then the evidence under a heading for the
    /// lesson's own teaching or for each source, every unit numbered L1, L2, .... A value that is blank is left out.
    pub fn render(&self) -> String {
        fn line(shown: &mut String, label: &str, value: &str) {
            if !value.trim().is_empty() {
                shown.push_str(&format!("{label}: {}\n", value.trim()));
            }
        }
        let mut shown = String::from("<brief>\n");
        line(&mut shown, "Course", &self.course);
        line(&mut shown, "Course goal", &self.goal);
        line(
            &mut shown,
            "What the learner already knows",
            &self.prior_knowledge,
        );
        line(&mut shown, "Module", &self.module);
        line(&mut shown, "Module summary", &self.module_summary);
        let outcomes: Vec<&str> = self
            .outcomes
            .iter()
            .map(|outcome| outcome.trim())
            .filter(|outcome| !outcome.is_empty())
            .collect();
        if !outcomes.is_empty() {
            shown.push_str("Module outcomes:\n");
            for outcome in outcomes {
                shown.push_str(&format!("- {outcome}\n"));
            }
        }
        line(&mut shown, "Lesson", &self.lesson);
        line(&mut shown, "Lesson objective", &self.objective);
        if !self.evidence.is_empty() {
            shown.push_str("\nEvidence:\n");
            write_evidence(
                &mut shown,
                self.evidence
                    .iter()
                    .enumerate()
                    .map(|(index, unit)| (index + 1, unit)),
            );
        }
        shown.push_str("</brief>");
        shown
    }

    /// The evidence units `numbers` cite (1-based, in the order given), written as `render` writes its evidence and
    /// joined with `\n`. `None` exactly when `link` is `None`.
    pub fn cited(&self, numbers: &[usize]) -> Option<String> {
        if numbers.is_empty() {
            return None;
        }
        let units = numbers
            .iter()
            .map(|&number| Some((number, self.evidence.get(number.checked_sub(1)?)?)))
            .collect::<Option<Vec<_>>>()?;
        let mut shown = String::new();
        write_evidence(&mut shown, units.into_iter());
        if shown.ends_with('\n') {
            shown.pop();
        }
        Some(shown)
    }

    /// The link of an issue that cites the evidence `numbers` (1-based): the cited units, cloned, in the order of
    /// `numbers`. `None` when `numbers` is empty or any number is 0 or beyond the evidence.
    pub fn link(&self, numbers: &[usize]) -> Option<LessonLink> {
        if numbers.is_empty() {
            return None;
        }
        let sentences = numbers
            .iter()
            .map(|&number| self.evidence.get(number.checked_sub(1)?).cloned())
            .collect::<Option<Vec<_>>>()?;
        Some(LessonLink {
            lesson_id: self.lesson_id.clone(),
            sentences,
        })
    }
}

/// Writes the numbered `units` as the evidence of a brief: a heading wherever the source changes, then each unit
/// under its number.
fn write_evidence<'a>(
    shown: &mut String,
    units: impl Iterator<Item = (usize, &'a LessonSentence)>,
) {
    let mut heading: Option<Option<&str>> = None;
    for (number, unit) in units {
        let source = unit.source.as_deref();
        if heading != Some(source) {
            heading = Some(source);
            match source {
                Some(title) => shown.push_str(&format!("From {title}:\n")),
                None => shown.push_str("From the lesson:\n"),
            }
        }
        let separator = if unit.text.contains('\n') { '\n' } else { ' ' };
        shown.push_str(&format!("L{number}{separator}{}\n", unit.text));
    }
}

/// The units a lesson's text is cut into for evidence: each fenced block of code whole, and the prose around the
/// blocks cut into sentences.
pub fn evidence_units(text: &str) -> Vec<String> {
    let text = text.replace("\r\n", "\n");
    let mut units = Vec::new();
    let mut from = 0;
    for block in fenced_blocks(&text) {
        prose_units(text.get(from..block.start).unwrap_or_default(), &mut units);
        let fenced = text.get(block.start..block.end).unwrap_or_default().trim();
        if !fenced.is_empty() {
            units.push(fenced.to_owned());
        }
        from = block.end;
    }
    prose_units(text.get(from..).unwrap_or_default(), &mut units);
    units
}

/// The sentences of `prose`, added to `units`.
fn prose_units(prose: &str, units: &mut Vec<String>) {
    for (start, end) in flowed_sentences(prose) {
        if let Some(sentence) = prose.get(start..end).map(str::trim) {
            if !sentence.is_empty() {
                units.push(sentence.to_owned());
            }
        }
    }
}

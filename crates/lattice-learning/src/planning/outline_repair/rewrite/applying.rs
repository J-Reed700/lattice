//! The one place a rewrite is applied to the saved module and checked.
use super::kinds::{Asked, Citation, Lesson, Module, NewLesson, Place, Revision, Row};
use crate::generation::{module_rules, GeneratedModule, Titles};
use lattice_core::application::services::model_contract::ask::Refusal;
use lattice_core::application::services::model_contract::edit::{
    consistent, Anchor, Change, Edited, Laid,
};
use lattice_core::application::services::model_contract::refusal;
use lattice_core::application::services::model_contract::shape::{Settled, Why};
use lattice_core::application::services::model_contract::shown::{Picked, Told};

/// The module as the rewrite leaves it, and where each of its lessons came from.
pub(super) struct Applied {
    pub module: GeneratedModule,
    pub origins: Vec<Option<usize>>,
}

/// The three lists of a module whose rows can be added, removed and moved.
#[derive(Debug, Clone, Copy, PartialEq)]
enum List {
    Outcomes,
    Deliverables,
    Criteria,
}

fn list_of(place: &Place) -> Option<List> {
    match place {
        Place::Outcome(_) => Some(List::Outcomes),
        Place::Deliverable(_) => Some(List::Deliverables),
        Place::Criterion(_) => Some(List::Criteria),
        _ => None,
    }
}

fn refused<T>(text: impl Into<String>) -> Result<T, Refusal> {
    Err(Refusal(text.into()))
}

fn blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// The edits of the texts that no row of a list can take.
fn check(change: &Change<Row, String>) -> Result<(), Refusal> {
    for (row, with) in &change.replace {
        if blank(with) {
            return refused(format!(
                "{} would be left blank: give it its new wording, or leave it out of replace.",
                row.handle.written()
            ));
        }
    }
    for (anchor, with) in &change.insert {
        let Anchor::After(after) = anchor else {
            return refused(
                "An inserted row names the row it follows in after; \"start\" names no list here.",
            );
        };
        if list_of(&after.reference).is_none() {
            return refused(format!(
                "Nothing can be inserted after {}: only outcomes, deliverables and success criteria take new rows.",
                after.handle.written()
            ));
        }
        if blank(with) {
            return refused("An inserted row needs its text.");
        }
    }
    for row in &change.remove {
        if list_of(&row.reference).is_none() {
            return refused(format!(
                "{} cannot be removed: only outcomes, deliverables and success criteria can.",
                row.handle.written()
            ));
        }
    }
    for (row, anchor) in &change.moved {
        let list = list_of(&row.reference);
        let fits = list.is_some()
            && match anchor {
                Anchor::Start => true,
                Anchor::After(after) => list_of(&after.reference) == list,
            };
        if !fits {
            return refused(format!(
                "{} can only move after a row of its own list.",
                row.handle.written()
            ));
        }
    }
    Ok(())
}

/// The outcome of a change to a target: refused, with the reason the correction tells.
fn settled<V>(result: Settled<V>) -> Result<V, Refusal> {
    match result {
        Settled::Answered(done) => Ok(done),
        Settled::Unanswered(Why::Contradicted(text)) => refused(text),
        Settled::Unanswered(_) => refused("That change cannot be placed."),
    }
}

/// The rows of one list after the change: each the position among `saved` it
/// came from, `None` for a new row, and its text.
fn lay(
    list: List,
    saved: &[(Place, &str)],
    change: &Change<Row, String>,
) -> Result<Vec<(Option<usize>, String)>, Refusal> {
    let mine = |row: &Picked<Row>| list_of(&row.reference) == Some(list);
    let part = Change {
        replace: change
            .replace
            .iter()
            .filter(|(row, _)| mine(row))
            .cloned()
            .collect(),
        insert: change
            .insert
            .iter()
            .filter(|(anchor, _)| matches!(anchor, Anchor::After(row) if mine(row)))
            .cloned()
            .collect(),
        remove: change
            .remove
            .iter()
            .filter(|row| mine(row))
            .cloned()
            .collect(),
        moved: change
            .moved
            .iter()
            .filter(|(row, _)| mine(row))
            .cloned()
            .collect(),
    };
    let part = consistent(part).map_err(|contradiction| Refusal(contradiction.told()))?;
    let mut edited =
        Edited::<Row, String>::of(saved.iter().map(|(place, _)| place.clone()).collect());
    settled(edited.apply(part))?;
    let at = |place: &Place| saved.iter().position(|(held, _)| held == place);
    Ok(edited
        .places()
        .into_iter()
        .map(|place| match place.laid {
            Laid::Kept(kept) => {
                let from = at(kept);
                let text = from
                    .and_then(|from| saved.get(from))
                    .map_or_else(String::new, |(_, text)| (*text).to_owned());
                (from, text)
            }
            Laid::Wrote { content, replaces } => (replaces.and_then(at), content.clone()),
        })
        .collect())
}

/// The module as `revision` leaves it.
pub(super) fn applied(asked: &Asked, revision: &Revision) -> Result<Applied, Refusal> {
    let change = revision.text.change();
    check(change)?;
    let module = &asked.module;
    let text_of = |row: &Row| -> String {
        change
            .replace
            .iter()
            .find(|(picked, _)| picked.reference == row.place)
            .map_or_else(|| row.text.0.clone(), |(_, with)| with.clone())
    };
    let changed = |row: &Row| text_of(row) != row.text.0;
    let row = |place: Place, text: String| Row {
        place,
        text: Told(text),
    };

    let saved_outcomes: Vec<(Place, &str)> = module
        .outcomes
        .iter()
        .map(|(row, _)| (row.place.clone(), row.text.0.as_str()))
        .collect();
    let mut outcomes = Vec::new();
    for (from, written) in lay(List::Outcomes, &saved_outcomes, change)? {
        let kept = from
            .and_then(|from| module.outcomes.get(from))
            .filter(|(saved, _)| saved.text.0 == written);
        let citation = kept.map_or_else(Citation::none, |(_, citation)| citation.clone());
        let place = Place::Outcome(outcomes.len());
        outcomes.push((row(place, written), citation));
    }
    let listed = |list: List, saved: &[Row], at: fn(usize) -> Place| -> Result<Vec<Row>, Refusal> {
        let saved: Vec<(Place, &str)> = saved
            .iter()
            .map(|row| (row.place.clone(), row.text.0.as_str()))
            .collect();
        Ok(lay(list, &saved, change)?
            .into_iter()
            .enumerate()
            .map(|(number, (_, written))| row(at(number), written))
            .collect())
    };
    let deliverables = listed(List::Deliverables, &module.deliverables, Place::Deliverable)?;
    let criteria = listed(List::Criteria, &module.criteria, Place::Criterion)?;

    let mut estimates: Vec<(usize, u32)> = Vec::new();
    for estimate in &revision.minutes {
        match estimates
            .iter()
            .find(|(lesson, _)| *lesson == estimate.lesson.reference)
        {
            Some((_, minutes)) if *minutes != estimate.minutes => {
                return refused(format!(
                    "{} is given two different estimates.",
                    estimate.lesson.handle.written()
                ));
            }
            Some(_) => {}
            None => estimates.push((estimate.lesson.reference, estimate.minutes)),
        }
    }

    let inserts = revision
        .new_lessons
        .iter()
        .map(|new| {
            let anchor = match &new.after {
                None => Anchor::Start,
                Some(after) => Anchor::After(after.clone()),
            };
            (anchor, new.clone())
        })
        .collect();
    let mut placed = Edited::<Lesson, NewLesson>::of((0..module.lessons.len()).collect());
    let inserts = consistent(Change {
        replace: Vec::new(),
        insert: inserts,
        remove: Vec::new(),
        moved: Vec::new(),
    })
    .map_err(|contradiction| Refusal(contradiction.told()))?;
    settled(placed.apply(inserts))?;
    let mut lessons: Vec<Lesson> = Vec::new();
    let mut origins = Vec::new();
    for place in placed.places() {
        let number = lessons.len();
        match place.laid {
            Laid::Kept(position) => {
                let Some(saved) = module.lessons.get(*position) else {
                    continue;
                };
                let minutes = estimates
                    .iter()
                    .find(|(lesson, _)| lesson == position)
                    .map_or(saved.minutes.0, |(_, minutes)| i64::from(*minutes));
                lessons.push(Lesson {
                    position: number,
                    title: row(Place::LessonTitle(number), text_of(&saved.title)),
                    objective: row(Place::LessonObjective(number), text_of(&saved.objective)),
                    minutes: Told(minutes),
                    citation: if changed(&saved.objective) {
                        Citation::none()
                    } else {
                        saved.citation.clone()
                    },
                });
                origins.push(Some(*position));
            }
            Laid::Wrote { content, .. } => {
                lessons.push(Lesson {
                    position: number,
                    title: row(Place::LessonTitle(number), content.title.clone()),
                    objective: row(Place::LessonObjective(number), content.objective.clone()),
                    minutes: Told(i64::from(content.minutes)),
                    citation: Citation::none(),
                });
                origins.push(None);
            }
        }
    }

    let prerequisites = &revision.prerequisites;
    if let Some(both) = prerequisites.add.iter().find(|added| {
        prerequisites
            .remove
            .iter()
            .any(|removed| removed.reference == added.reference)
    }) {
        return refused(format!(
            "{} is both added and removed.",
            both.handle.written()
        ));
    }
    let mut positions: Vec<usize> = module
        .prerequisites
        .iter()
        .copied()
        .filter(|saved| {
            !prerequisites
                .remove
                .iter()
                .any(|removed| removed.reference == *saved)
        })
        .chain(prerequisites.add.iter().map(|added| added.reference))
        .collect();
    positions.sort_unstable();
    positions.dedup();

    let rewritten = Module {
        title: row(Place::Title, text_of(&module.title)),
        summary: row(Place::Summary, text_of(&module.summary)),
        summary_citation: if changed(&module.summary) {
            Citation::none()
        } else {
            module.summary_citation.clone()
        },
        outcomes,
        project_title: row(Place::ProjectTitle, text_of(&module.project_title)),
        project_brief: row(Place::ProjectBrief, text_of(&module.project_brief)),
        deliverables,
        criteria,
        prerequisites: positions,
        lessons,
    }
    .generated();

    let index = *asked.index.value();
    let mut titles = Titles::default();
    let others = |later: bool| {
        asked
            .sequence
            .iter()
            .filter(move |entry| (entry.position > index) == later && entry.position != index)
    };
    let record = |titles: &mut Titles, later: bool| -> Result<(), Refusal> {
        for entry in others(later) {
            let number = entry.position + 1;
            titles
                .module(number, &entry.title.0)
                .map_err(|error| Refusal(refusal(&error)))?;
            for (at, title) in entry.lessons.iter().enumerate() {
                titles
                    .lesson((number, at + 1), &title.0)
                    .map_err(|error| Refusal(refusal(&error)))?;
            }
        }
        Ok(())
    };
    record(&mut titles, false)?;
    module_rules(index + 1, &rewritten, *asked.depth.value(), &mut titles)
        .map_err(|error| Refusal(refusal(&error)))?;
    record(&mut titles, true)?;
    Ok(Applied {
        module: rewritten,
        origins,
    })
}

//! The sections of a lesson by unit number: finding them, setting them back, and which are left with too little.

use super::*;

pub(super) fn unit_at(lesson: &Value, unit: usize) -> Option<&Value> {
    lesson.pointer(&FindingSite::in_candidate(lesson, unit).pointer()?)
}

pub(super) fn set_unit(lesson: &mut Value, unit: usize, section: Value) -> bool {
    let Some(pointer) = FindingSite::in_candidate(lesson, unit).pointer() else {
        return false;
    };
    match lesson.pointer_mut(&pointer) {
        Some(slot) => {
            *slot = section;
            true
        }
        None => false,
    }
}

/// A section's title, or a question's prompt: what a record of it is checked
/// against.
pub(super) fn unit_label(lesson: &Value, unit: usize) -> Option<String> {
    let (list, _) = FindingSite::in_candidate(lesson, unit).array()?;
    let field = if list == "blocks" { "title" } else { "prompt" };
    unit_at(lesson, unit)?
        .get(field)?
        .as_str()
        .map(str::to_owned)
}

/// The sections `cuts` leaves with too little to teach in `lesson`, whatever
/// else of it could be published.
pub(super) fn left_with_too_little(
    lesson: &Value,
    cuts: &Cuts,
    shape: &dyn ShapePolicy,
) -> Vec<usize> {
    let mut cuts = cuts.clone();
    cuts.keep_current(lesson);
    too_little(lesson, &cuts, shape)
}

/// Puts `units` of `lesson` back as they are in `from`, and what `cuts` took
/// from them back as it is in `earlier`.
pub(super) fn restore_units(
    (lesson, cuts): (&mut Value, &mut Cuts),
    (from, earlier): (&Value, &Cuts),
    units: &[usize],
) {
    for unit in units {
        if let Some(section) = unit_at(from, *unit).cloned() {
            set_unit(lesson, *unit, section);
        }
        match earlier.taken.get(unit) {
            Some(taken) => cuts.taken.insert(*unit, taken.clone()),
            None => cuts.taken.remove(unit),
        };
    }
}

/// The sections that took cuts and are left with too little to teach, by the
/// shape policy's measure.
pub(super) fn too_little(lesson: &Value, cuts: &Cuts, shape: &dyn ShapePolicy) -> Vec<usize> {
    cuts.taken
        .iter()
        .filter(|(unit, taken)| {
            let site = FindingSite::in_candidate(lesson, **unit);
            site.array().is_some_and(|(list, _)| list == "blocks")
                && shape.too_little(
                    site.field_pointer("body")
                        .and_then(|pointer| lesson.pointer(&pointer))
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    taken.answer_lost,
                )
        })
        .map(|(unit, _)| *unit)
        .collect()
}

/// Sends the sections that go out of `lesson` into `cuts`: those left with too
/// little to teach, then the checks that lose the worked example they follow.
/// Whether it sent any.
pub(super) fn send_out(lesson: &Value, cuts: &mut Cuts, shape: &dyn ShapePolicy) -> bool {
    let mut sent = false;
    for unit in too_little(lesson, cuts, shape) {
        if let (false, Some(label)) = (cuts.going.contains_key(&unit), unit_label(lesson, unit)) {
            cuts.go(unit, label);
            sent = true;
        }
    }
    for unit in shape.dependants(lesson, cuts.units().units()) {
        if let (false, Some(label)) = (cuts.going.contains_key(&unit), unit_label(lesson, unit)) {
            cuts.go(unit, label);
            sent = true;
        }
    }
    sent
}

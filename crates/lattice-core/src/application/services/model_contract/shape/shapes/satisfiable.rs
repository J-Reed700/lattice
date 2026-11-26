//! Whether a schema field can be satisfied under what is shown.

use super::{Among, Field, Form, Showing};

pub(super) fn shown_written(prefix: &str, among: Among, showing: &Showing<'_>) -> Vec<String> {
    match among {
        Among::Shown => showing.written(prefix),
        Among::Own => showing.written_under(prefix),
    }
}

/// Whether `field` can be satisfied under `showing`: a required pointer with no
/// `"none"` fallback cannot be satisfied when nothing of its kind is shown.
pub(super) fn field_satisfiable(field: &Field, showing: &Showing<'_>) -> bool {
    match field {
        Field::Point {
            prefix,
            among,
            none: false,
        }
        | Field::Cite {
            prefix,
            among,
            some: true,
        } => !shown_written(prefix, *among, showing).is_empty(),
        Field::Word(list) => !list.is_empty(),
        Field::Group(form) => form_satisfiable(form, showing),
        Field::List {
            form,
            count: Some(count),
        } => showing
            .asked(count)
            .is_none_or(|(least, _)| least == 0 || form_satisfiable(form, showing)),
        Field::OneOf(forms) => forms
            .iter()
            .any(|(_, form)| form_satisfiable(form, showing)),
        _ => true,
    }
}

pub(super) fn form_satisfiable(form: &Form, showing: &Showing<'_>) -> bool {
    form.fields
        .iter()
        .all(|(_, field)| field_satisfiable(field, showing))
}

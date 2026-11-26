//! Schema-driven validation rules across a `FieldSpec` form and its document.

use serde_json::Value;

use super::types::{FieldProblem, FieldSpec, TextFormat};

fn check_leaf(
    spec: &FieldSpec,
    path: &str,
    value: Option<&Value>,
    problems: &mut Vec<FieldProblem>,
) {
    let FieldSpec::Text {
        label,
        format,
        required,
        ..
    } = spec
    else {
        return;
    };
    let text = value.and_then(Value::as_str).unwrap_or_default();
    if *required && text.trim().is_empty() {
        problems.push(FieldProblem {
            path: Some(path.to_owned()),
            message: format!("{label} can't be empty."),
        });
    }
    if matches!(format, TextFormat::Line) && (text.contains('\n') || text.contains('\r')) {
        problems.push(FieldProblem {
            path: Some(path.to_owned()),
            message: format!("{label} must be on one line."),
        });
    }
}

fn check_list(
    path: &str,
    label: &str,
    item_label: &str,
    min_items: u32,
    max_items: Option<u32>,
    item_specs: &[FieldSpec],
    members: &[Value],
    problems: &mut Vec<FieldProblem>,
) {
    if members.len() < usize::try_from(min_items).unwrap_or(usize::MAX) {
        problems.push(FieldProblem {
            path: Some(path.to_owned()),
            message: format!("{label} needs at least {min_items}."),
        });
    }
    if let Some(max) = max_items {
        if members.len() > usize::try_from(max).unwrap_or(usize::MAX) {
            problems.push(FieldProblem {
                path: Some(path.to_owned()),
                message: format!("{label} can have at most {max}."),
            });
        }
    }

    for item_spec in item_specs {
        let FieldSpec::Flag {
            path: flag_path,
            label: flag_label,
            exclusive: true,
        } = item_spec
        else {
            continue;
        };
        let Some(flag_key) = flag_path.strip_prefix('/') else {
            continue;
        };
        let marked = members
            .iter()
            .filter(|member| member.get(flag_key).and_then(Value::as_bool) == Some(true))
            .count();
        if marked > 1 {
            problems.push(FieldProblem {
                path: Some(path.to_owned()),
                message: format!("Mark only one {item_label} as \"{flag_label}\"."),
            });
        }
    }

    for (index, member) in members.iter().enumerate() {
        for item_spec in item_specs {
            let item_path = item_spec.path();
            let Some(item_key) = item_path.strip_prefix('/') else {
                continue;
            };
            let full_path = format!("{path}/{index}{item_path}");
            check_leaf(item_spec, &full_path, member.get(item_key), problems);
        }
    }
}

pub(crate) fn form_problems(form: &[FieldSpec], document: &Value) -> Vec<FieldProblem> {
    let mut problems = Vec::new();
    let Some(obj) = document.as_object() else {
        return problems;
    };
    for spec in form {
        let path = spec.path();
        let Some(key) = path.strip_prefix('/') else {
            continue;
        };
        match spec {
            FieldSpec::List {
                label,
                item_label,
                min_items,
                max_items,
                item,
                ..
            } => {
                if let Some(members) = obj.get(key).and_then(Value::as_array) {
                    check_list(
                        path,
                        label,
                        item_label,
                        *min_items,
                        *max_items,
                        item,
                        members,
                        &mut problems,
                    );
                }
            }
            leaf => check_leaf(leaf, path, obj.get(key), &mut problems),
        }
    }
    problems
}

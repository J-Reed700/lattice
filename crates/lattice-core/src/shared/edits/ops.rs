//! JSON-Patch subset execution and inversion against a `FieldSpec` form.

use serde_json::{Map, Value};

use super::types::{EditOp, FieldProblem, FieldSpec};
use crate::shared::error::{AppError, Result};

enum PathTarget<'a> {
    TopLeaf {
        key: &'a str,
        spec: &'a FieldSpec,
    },
    ListLeaf {
        list_key: &'a str,
        index: usize,
        leaf_key: &'a str,
        spec: &'a FieldSpec,
    },
}

enum ListSlot<'a> {
    Append {
        list_key: &'a str,
        item: &'a [FieldSpec],
    },
    At {
        list_key: &'a str,
        index: usize,
        item: &'a [FieldSpec],
    },
}

fn bad_path(path: &str, detail: &str) -> AppError {
    AppError::InvalidInput(format!("Invalid edit at `{path}`: {detail}"))
}

fn parse_index(segment: &str, path: &str) -> Result<usize> {
    if segment.is_empty() || !segment.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad_path(path, "expected a list index."));
    }
    segment
        .parse::<usize>()
        .map_err(|_| bad_path(path, "list index is out of range."))
}

fn key_of(spec: &FieldSpec, path: &str) -> Result<&str> {
    spec.path()
        .strip_prefix('/')
        .ok_or_else(|| bad_path(path, "field descriptor path is malformed."))
}

impl FieldSpec {
    pub(crate) fn path(&self) -> &str {
        match self {
            Self::Text { path, .. }
            | Self::Flag { path, .. }
            | Self::Choice { path, .. }
            | Self::Number { path, .. }
            | Self::List { path, .. } => path,
        }
    }
}

fn resolve_leaf<'a>(form: &'a [FieldSpec], path: &str) -> Result<PathTarget<'a>> {
    for spec in form {
        match spec {
            FieldSpec::List {
                path: list_path,
                item,
                ..
            } => {
                let Some(rest) = path
                    .strip_prefix(list_path.as_str())
                    .and_then(|s| s.strip_prefix('/'))
                else {
                    continue;
                };
                let Some((idx_seg, leaf_rest)) = rest.split_once('/') else {
                    continue;
                };
                let leaf_path = format!("/{leaf_rest}");
                for leaf_spec in item {
                    if !matches!(leaf_spec, FieldSpec::List { .. }) && leaf_spec.path() == leaf_path
                    {
                        let list_key = key_of(spec, path)?;
                        let leaf_key = key_of(leaf_spec, path)?;
                        let index = parse_index(idx_seg, path)?;
                        return Ok(PathTarget::ListLeaf {
                            list_key,
                            index,
                            leaf_key,
                            spec: leaf_spec,
                        });
                    }
                }
            }
            leaf_spec => {
                if leaf_spec.path() == path {
                    let key = key_of(leaf_spec, path)?;
                    return Ok(PathTarget::TopLeaf {
                        key,
                        spec: leaf_spec,
                    });
                }
            }
        }
    }
    Err(bad_path(path, "path is not a leaf in the form."))
}

fn resolve_list_slot<'a>(
    form: &'a [FieldSpec],
    path: &str,
    allow_append: bool,
) -> Result<ListSlot<'a>> {
    for spec in form {
        let FieldSpec::List {
            path: list_path,
            item,
            ..
        } = spec
        else {
            continue;
        };
        let Some(rest) = path
            .strip_prefix(list_path.as_str())
            .and_then(|s| s.strip_prefix('/'))
        else {
            continue;
        };
        if rest.contains('/') {
            continue;
        }
        let list_key = key_of(spec, path)?;
        if allow_append && rest == "-" {
            return Ok(ListSlot::Append { list_key, item });
        }
        let index = parse_index(rest, path)?;
        return Ok(ListSlot::At {
            list_key,
            index,
            item,
        });
    }
    Err(bad_path(path, "path does not name a list member slot."))
}

fn normalize_leaf(spec: &FieldSpec, value: &Value, path: &str) -> Result<Value> {
    match spec {
        FieldSpec::Text { .. } => {
            let text = value
                .as_str()
                .ok_or_else(|| bad_path(path, "expected a text value."))?;
            Ok(Value::String(text.replace("\r\n", "\n")))
        }
        FieldSpec::Flag { .. } => {
            let flag = value
                .as_bool()
                .ok_or_else(|| bad_path(path, "expected a boolean flag."))?;
            Ok(Value::Bool(flag))
        }
        FieldSpec::Choice { options, .. } => {
            let index = value
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|&n| n < options.len())
                .ok_or_else(|| bad_path(path, "choice index is out of range."))?;
            Ok(Value::Number(serde_json::Number::from(index)))
        }
        FieldSpec::Number { min, .. } => {
            let n = value
                .as_i64()
                .filter(|&n| n >= *min)
                .ok_or_else(|| bad_path(path, &format!("expected a whole number >= {min}.")))?;
            Ok(Value::Number(serde_json::Number::from(n)))
        }
        FieldSpec::List { .. } => Err(bad_path(path, "list cannot be replaced directly.")),
    }
}

fn normalize_item(item_specs: &[FieldSpec], value: &Value, path: &str) -> Result<Value> {
    let Some(map) = value.as_object() else {
        return Err(bad_path(path, "added list item must be an object."));
    };
    if map.len() != item_specs.len() {
        return Err(bad_path(
            path,
            "added list item must contain exactly the item's leaf keys.",
        ));
    }
    let mut normalized = Map::new();
    for spec in item_specs {
        if matches!(spec, FieldSpec::List { .. }) {
            return Err(bad_path(path, "nested lists are not supported."));
        }
        let key = key_of(spec, path)?;
        let raw = map
            .get(key)
            .ok_or_else(|| bad_path(path, &format!("missing item key `{key}`.")))?;
        let leaf_path = format!("{path}/{key}");
        normalized.insert(key.to_owned(), normalize_leaf(spec, raw, &leaf_path)?);
    }
    Ok(Value::Object(normalized))
}

fn apply_one(doc: &mut Value, form: &[FieldSpec], op: &EditOp) -> Result<EditOp> {
    let doc_obj = doc
        .as_object_mut()
        .ok_or_else(|| bad_path("/", "document root must be an object."))?;
    match op {
        EditOp::Replace { path, value } => match resolve_leaf(form, path)? {
            PathTarget::TopLeaf { key, spec } => {
                let normalized = normalize_leaf(spec, value, path)?;
                let slot = doc_obj
                    .get_mut(key)
                    .ok_or_else(|| bad_path(path, "field is missing from document."))?;
                let old = slot.clone();
                *slot = normalized;
                Ok(EditOp::Replace {
                    path: path.clone(),
                    value: old,
                })
            }
            PathTarget::ListLeaf {
                list_key,
                index,
                leaf_key,
                spec,
            } => {
                let normalized = normalize_leaf(spec, value, path)?;
                let arr = doc_obj
                    .get_mut(list_key)
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| bad_path(path, "list field is missing from document."))?;
                let member = arr
                    .get_mut(index)
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| bad_path(path, "list member index is out of range."))?;
                let slot = member
                    .get_mut(leaf_key)
                    .ok_or_else(|| bad_path(path, "item field is missing from member."))?;
                let old = slot.clone();
                *slot = normalized;
                Ok(EditOp::Replace {
                    path: path.clone(),
                    value: old,
                })
            }
        },
        EditOp::Add { path, value } => {
            let (list_key, index_opt, item_specs) = match resolve_list_slot(form, path, true)? {
                ListSlot::Append { list_key, item } => (list_key, None, item),
                ListSlot::At {
                    list_key,
                    index,
                    item,
                } => (list_key, Some(index), item),
            };
            let normalized = normalize_item(item_specs, value, path)?;
            let arr = doc_obj
                .get_mut(list_key)
                .and_then(Value::as_array_mut)
                .ok_or_else(|| bad_path(path, "list field is missing from document."))?;
            let insert_at = match index_opt {
                Some(index) if index <= arr.len() => index,
                Some(_) => return Err(bad_path(path, "list insertion index is out of range.")),
                None => arr.len(),
            };
            arr.insert(insert_at, normalized);
            Ok(EditOp::Remove {
                path: format!("/{list_key}/{insert_at}"),
            })
        }
        EditOp::Remove { path } => {
            let ListSlot::At {
                list_key, index, ..
            } = resolve_list_slot(form, path, false)?
            else {
                return Err(bad_path(path, "remove requires an existing list index."));
            };
            let arr = doc_obj
                .get_mut(list_key)
                .and_then(Value::as_array_mut)
                .ok_or_else(|| bad_path(path, "list field is missing from document."))?;
            if index >= arr.len() {
                return Err(bad_path(path, "list removal index is out of range."));
            }
            let removed = arr.remove(index);
            Ok(EditOp::Add {
                path: format!("/{list_key}/{index}"),
                value: removed,
            })
        }
    }
}

fn check_list_bounds(doc: &Value, form: &[FieldSpec]) -> Result<()> {
    let Some(obj) = doc.as_object() else {
        return Err(bad_path("/", "document root must be an object."));
    };
    for spec in form {
        let FieldSpec::List {
            path,
            label,
            min_items,
            max_items,
            ..
        } = spec
        else {
            continue;
        };
        let key = key_of(spec, path)?;
        let len = obj
            .get(key)
            .and_then(Value::as_array)
            .ok_or_else(|| bad_path(path, "list field is missing from document."))?
            .len();
        if len < usize::try_from(*min_items).unwrap_or(usize::MAX) {
            return Err(AppError::InvalidInput(format!(
                "{label} ({path}) needs at least {min_items}."
            )));
        }
        if let Some(max) = max_items {
            if len > usize::try_from(*max).unwrap_or(usize::MAX) {
                return Err(AppError::InvalidInput(format!(
                    "{label} ({path}) can have at most {max}."
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn apply_ops(
    document: &Value,
    form: &[FieldSpec],
    ops: &[EditOp],
) -> Result<(Value, Vec<EditOp>)> {
    let mut next = document.clone();
    let mut inverses = Vec::with_capacity(ops.len());
    for op in ops {
        inverses.push(apply_one(&mut next, form, op)?);
    }
    check_list_bounds(&next, form)?;
    inverses.reverse();
    Ok((next, inverses))
}

fn op_path(op: &EditOp) -> &str {
    match op {
        EditOp::Replace { path, .. } | EditOp::Add { path, .. } | EditOp::Remove { path } => path,
    }
}

pub(crate) fn apply_ops_preview(
    document: &Value,
    form: &[FieldSpec],
    ops: &[EditOp],
) -> (Value, Vec<FieldProblem>) {
    let mut next = document.clone();
    let mut problems = Vec::new();
    for op in ops {
        if let Err(err) = apply_one(&mut next, form, op) {
            problems.push(FieldProblem {
                path: Some(op_path(op).to_owned()),
                message: err.to_string(),
            });
        }
    }
    (next, problems)
}

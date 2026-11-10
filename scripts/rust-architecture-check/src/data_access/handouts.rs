//! Rule 5's test entry points on the checker before rule 5: no module scopes, every file at the crate root.
use std::collections::HashMap;

pub(super) type ModuleScopes = HashMap<Vec<String>, Vec<String>>;

pub(super) fn module_of(_path: &str) -> Vec<String> {
    Vec::new()
}

pub(super) fn module_scopes<'a>(
    _files: impl IntoIterator<Item = (&'a [String], &'a syn::File)>,
) -> ModuleScopes {
    ModuleScopes::new()
}

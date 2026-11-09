//! Rule 4: sqlx's unit-of-work types are named only in data modules; everything
//! else takes `WriteTx` or `ReadTx`.
use super::{use_leaves, Violation};
use crate::{
    item_attrs,
    ratchet::{impl_item_attrs, token_paths, trait_item_attrs},
    test_only,
};
use syn::{
    visit::{self, Visit},
    ImplItem, Item, TraitItem,
};

const TYPES: [&str; 7] = [
    "Transaction",
    "SqliteTransaction",
    "SqliteConnection",
    "PoolConnection",
    "Executor",
    "SqliteExecutor",
    "Acquire",
];

/// The index of the unit-of-work type in a sqlx path, and the path as written up to it.
fn named(segments: &[String]) -> Option<(usize, String)> {
    if segments.first()? != "sqlx" {
        return None;
    }
    let index = segments
        .iter()
        .position(|segment| TYPES.contains(&segment.as_str()))?;
    Some((index, segments.get(..=index)?.join("::")))
}

struct Scan<'a> {
    path: &'a str,
    found: Vec<Violation>,
}

impl Scan<'_> {
    fn add(&mut self, line: usize, what: &str) {
        self.found.push(Violation {
            rule: 4,
            path: self.path.to_string(),
            line,
            message: format!(
                "names {what} outside a data module; use WriteTx or ReadTx, \
                 or move the code into a data module"
            ),
        });
    }
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if !item_attrs(item).is_some_and(test_only) {
            visit::visit_item(self, item);
        }
    }
    fn visit_impl_item(&mut self, item: &'ast ImplItem) {
        if !impl_item_attrs(item).is_some_and(test_only) {
            visit::visit_impl_item(self, item);
        }
    }
    fn visit_trait_item(&mut self, item: &'ast TraitItem) {
        if !trait_item_attrs(item).is_some_and(test_only) {
            visit::visit_trait_item(self, item);
        }
    }
    fn visit_stmt(&mut self, stmt: &'ast syn::Stmt) {
        if let syn::Stmt::Local(local) = stmt {
            if test_only(&local.attrs) {
                return;
            }
        }
        visit::visit_stmt(self, stmt);
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut leaves = Vec::new();
        use_leaves(&item.tree, Vec::new(), &mut leaves);
        let mut names: Vec<String> = Vec::new();
        for (leaf, _) in &leaves {
            if let Some((_, name)) = named(leaf) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        if !names.is_empty() {
            self.add(item.use_token.span.start().line, &names.join(", "));
        }
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        if let Some((index, name)) = named(&segments) {
            let line = path
                .segments
                .iter()
                .nth(index)
                .map_or(1, |segment| segment.ident.span().start().line);
            self.add(line, &name);
        }
        visit::visit_path(self, path);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        visit::visit_macro(self, mac);
        for (segments, _, line) in token_paths(mac.tokens.clone()) {
            if let Some((_, name)) = named(&segments) {
                self.add(line, &name);
            }
        }
    }
}

pub fn violations(path: &str, ast: &syn::File) -> Vec<Violation> {
    let mut scan = Scan {
        path,
        found: Vec::new(),
    };
    scan.visit_file(ast);
    scan.found
}

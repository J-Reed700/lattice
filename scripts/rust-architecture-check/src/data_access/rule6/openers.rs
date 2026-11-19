//! Rule 6(b): only the openers name a pool or its options, connect, or create a `Database`.
use super::OPENS;
use crate::{
    data_access::{use_leaves, Violation},
    item_attrs,
    ratchet::{impl_item_attrs, token_paths, trait_item_attrs},
    test_only,
};
use syn::{
    visit::{self, Visit},
    ImplItem, Item, TraitItem,
};

const NAMES: [&str; 4] = [
    "SqlitePool",
    "SqlitePoolOptions",
    "SqliteConnectOptions",
    "PoolOptions",
];

/// `(type, function)` pairs that open a database or a connection.
const CALLS: [(&str, &str); 5] = [
    ("Database", "new"),
    ("SqliteConnection", "connect"),
    ("SqliteConnection", "connect_with"),
    ("Connection", "connect"),
    ("Connection", "connect_with"),
];

/// Whether a path names one of the pool types or ends in a call that opens a database.
fn opens(segments: &[String]) -> bool {
    segments
        .iter()
        .any(|segment| NAMES.contains(&segment.as_str()))
        || matches!(segments, [.., kind, function]
            if CALLS.iter().any(|(k, f)| kind == k && function == f))
}

struct Scan<'a> {
    path: &'a str,
    found: Vec<Violation>,
}

impl Scan<'_> {
    fn add(&mut self, line: usize) {
        self.found.push(Violation {
            rule: 6,
            path: self.path.to_string(),
            line,
            message: OPENS.to_string(),
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
        for (leaf, line) in leaves {
            if leaf.iter().any(|segment| NAMES.contains(&segment.as_str())) {
                self.add(line);
            }
        }
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        if opens(&segments) {
            let at = path
                .segments
                .iter()
                .find(|segment| NAMES.contains(&segment.ident.to_string().as_str()))
                .or_else(|| path.segments.iter().rev().nth(1))
                .or_else(|| path.segments.first());
            self.add(at.map_or(1, |segment| segment.ident.span().start().line));
        }
        visit::visit_path(self, path);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        visit::visit_macro(self, mac);
        for (segments, _, line) in token_paths(mac.tokens.clone()) {
            if opens(&segments) {
                self.add(line);
            }
        }
    }
}

/// The places that open a database; everything else takes the `Database` it is given.
pub(super) fn violations(path: &str, ast: &syn::File) -> Vec<Violation> {
    let mut scan = Scan {
        path,
        found: Vec::new(),
    };
    scan.visit_file(ast);
    scan.found
}

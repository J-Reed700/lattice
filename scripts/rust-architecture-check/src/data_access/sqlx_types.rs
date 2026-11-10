//! Rule 4: sqlx and the driver trait are named only in data modules; everything else
//! holds a `Database` or takes `WriteTx` or `ReadTx`.
use super::{handouts::import_leaves, Violation};
use crate::{
    item_attrs,
    ratchet::{impl_item_attrs, token_paths, trait_item_attrs},
    test_only,
};
use proc_macro2::TokenStream;
use syn::{
    visit::{self, Visit},
    ImplItem, Item, TraitItem,
};

/// Whether a path names the driver trait: `DriverPool`, the `persistence::driver` module,
/// or a `sqlx_pool` item.
fn driver_path(segments: &[String]) -> bool {
    segments.iter().any(|segment| segment == "DriverPool")
        || segments
            .windows(2)
            .any(|pair| matches!(pair, [a, b] if a == "persistence" && b == "driver"))
        || (segments.len() >= 2 && segments.last().is_some_and(|last| last == "sqlx_pool"))
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
                "names {what} outside a data module; hold a Database, take WriteTx or \
                 ReadTx, or move the code into a data module"
            ),
        });
    }

    fn tokens(&mut self, stream: TokenStream) {
        for (segments, _, line) in token_paths(stream) {
            let sqlx = segments.len() >= 2 && segments.first().is_some_and(|first| first == "sqlx");
            if sqlx || driver_path(&segments) {
                self.add(line, &segments.join("::"));
            }
        }
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
        let (mut sqlx, mut driver): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
        for leaf in import_leaves(&item.tree) {
            let name = leaf.text();
            let names = if leaf.path.first().is_some_and(|first| first == "sqlx") {
                &mut sqlx
            } else if driver_path(&leaf.path) {
                &mut driver
            } else {
                continue;
            };
            if !names.contains(&name) {
                names.push(name);
            }
        }
        for names in [sqlx, driver] {
            if !names.is_empty() {
                self.add(item.use_token.span.start().line, &names.join(", "));
            }
        }
    }
    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        if item.ident == "sqlx" {
            self.add(item.ident.span().start().line, "sqlx");
        }
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        if let Some(first) = path.segments.first() {
            if first.ident == "sqlx" || driver_path(&segments) {
                self.add(first.ident.span().start().line, &segments.join("::"));
            }
        }
        visit::visit_path(self, path);
    }
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        let path = attribute.path();
        if let Some(first) = path.segments.first() {
            if first.ident == "sqlx" {
                let segments: Vec<String> =
                    path.segments.iter().map(|s| s.ident.to_string()).collect();
                self.add(first.ident.span().start().line, &segments.join("::"));
            }
        }
        if let syn::Meta::List(list) = &attribute.meta {
            self.tokens(list.tokens.clone());
        }
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        visit::visit_macro(self, mac);
        self.tokens(mac.tokens.clone());
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

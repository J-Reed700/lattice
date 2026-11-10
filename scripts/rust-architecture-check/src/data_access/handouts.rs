//! Rule 5: a data module hands no driver handle out to code outside data
//! modules, and only a data module unwraps a `Database` with `sqlx_pool()`.
use super::{Exemption, Violation, DATA_NAMES, DATA_SUFFIXES, STORE_CRATE};
use crate::{item_attrs, test_only};
use std::collections::HashSet;
use syn::{Ident, Item, UseTree, Visibility};

mod imports;
mod index;
mod items;
mod pool_calls;

pub(super) use index::{module_scopes, ModuleScopes};
pub(super) use pool_calls::calls;

const HANDLES: &[&str] = &[
    "Pool",
    "SqlitePool",
    "PgPool",
    "AnyPool",
    "PoolConnection",
    "SqliteConnection",
    "PgConnection",
    "AnyConnection",
    "Transaction",
    "SqliteTransaction",
    "PgTransaction",
    "PoolOptions",
    "SqlitePoolOptions",
    "PgPoolOptions",
    "SqliteConnectOptions",
    "PgConnectOptions",
    "Executor",
    "SqliteExecutor",
    "Acquire",
];

/// One name a `use` brings in.
pub(super) struct Leaf {
    pub path: Vec<String>,
    pub alias: String,
    pub glob: bool,
    pub line: usize,
}

impl Leaf {
    pub fn text(&self) -> String {
        let mut text = self.path.join("::");
        if self.glob {
            text.push_str("::*");
        }
        text
    }
}

pub(super) fn import_leaves(tree: &UseTree) -> Vec<Leaf> {
    let mut out = Vec::new();
    collect_leaves(tree, Vec::new(), &mut out);
    out
}

fn collect_leaves(tree: &UseTree, mut prefix: Vec<String>, out: &mut Vec<Leaf>) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_leaves(&path.tree, prefix, out);
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_leaves(item, prefix.clone(), out);
            }
        }
        UseTree::Name(name) => named_leaf(prefix, &name.ident, None, out),
        UseTree::Rename(rename) => named_leaf(prefix, &rename.ident, Some(&rename.rename), out),
        UseTree::Glob(glob) => out.push(Leaf {
            path: prefix,
            alias: "*".to_string(),
            glob: true,
            line: glob.star_token.span.start().line,
        }),
    }
}

fn named_leaf(mut prefix: Vec<String>, ident: &Ident, alias: Option<&Ident>, out: &mut Vec<Leaf>) {
    let line = alias.unwrap_or(ident).span().start().line;
    if ident != "self" {
        prefix.push(ident.to_string());
    }
    let alias = match alias {
        Some(alias) => alias.to_string(),
        None => prefix.last().cloned().unwrap_or_default(),
    };
    out.push(Leaf {
        path: prefix,
        alias,
        glob: false,
        line,
    });
}

/// The module a visibility written inside `base` reaches: the item is visible in it and below.
fn resolve(vis: &Visibility, base: &[String]) -> Vec<String> {
    match vis {
        Visibility::Public(_) => Vec::new(),
        Visibility::Restricted(restricted) => {
            let mut scope = base.to_vec();
            for segment in &restricted.path.segments {
                match segment.ident.to_string().as_str() {
                    "crate" => scope.clear(),
                    "self" => {}
                    "super" => {
                        scope.pop();
                    }
                    name => scope.push(name.to_string()),
                }
            }
            scope
        }
        _ => base.to_vec(),
    }
}

fn data_scope(scope: &[String]) -> bool {
    scope.iter().any(|segment| {
        DATA_NAMES.contains(&segment.as_str())
            || DATA_SUFFIXES
                .iter()
                .any(|suffix| segment.ends_with(*suffix))
    })
}

fn exempted(entry: &Exemption, owner: &str, item: &str) -> bool {
    entry.1 == owner && (entry.2 == item || entry.2 == "*")
}

/// The module path of a repository-relative file path, for callers that have no source root.
#[cfg(test)]
pub(super) fn module_of(path: &str) -> Vec<String> {
    let relative = path.split_once("/src/").map_or(path, |(_, rest)| rest);
    let stem = relative.strip_suffix(".rs").unwrap_or(relative);
    let mut parts: Vec<String> = stem.split('/').map(str::to_string).collect();
    let root = parts.len() == 1
        && parts
            .first()
            .is_some_and(|name| name == "lib" || name == "main");
    if root {
        parts.clear();
    } else if parts.last().is_some_and(|name| name == "mod") {
        parts.pop();
    }
    parts
}

fn violation(path: &str, line: usize, handle: &str, kind: &str, name: &str) -> Violation {
    Violation {
        rule: 5,
        path: path.to_string(),
        line,
        message: format!(
            "hands out {handle} from {kind} {name}; return a Database or keep it \
             private to data modules"
        ),
    }
}

struct Reexports<'a> {
    path: &'a str,
    store: bool,
    modules: &'a ModuleScopes,
    found: Vec<Violation>,
}

impl Reexports<'_> {
    fn declaration(&mut self, item: &syn::ItemUse, module: &[String]) {
        if matches!(item.vis, Visibility::Inherited) {
            return;
        }
        let site = self.modules.narrowed(resolve(&item.vis, module), module);
        let public = matches!(item.vis, Visibility::Public(_));
        for leaf in import_leaves(&item.tree) {
            if leaf.path.first().is_some_and(|first| first == "sqlx") {
                if self.modules.reaches(self.store, &site, public, module) {
                    let (text, alias) = (leaf.text(), leaf.alias.as_str());
                    self.found
                        .push(violation(self.path, leaf.line, &text, "re-export", alias));
                }
                continue;
            }
            let Some((target, name)) = self.modules.target(module, &leaf.path, leaf.glob) else {
                continue;
            };
            let mut seen = HashSet::new();
            let mut reported = HashSet::new();
            for hand in self.modules.reexported(&target, name.as_deref(), &mut seen) {
                let reaches =
                    self.modules
                        .reexport_reaches(self.store, (&site, public), module, hand);
                let shown = if leaf.glob { &hand.name } else { &leaf.alias };
                if reaches && reported.insert(shown.clone()) {
                    self.found.push(violation(
                        self.path,
                        leaf.line,
                        &hand.handle,
                        "re-export",
                        shown,
                    ));
                    if !leaf.glob {
                        break;
                    }
                }
            }
        }
    }

    fn items(&mut self, items: &[Item], module: &mut Vec<String>) {
        for item in items {
            if item_attrs(item).is_some_and(test_only) {
                continue;
            }
            match item {
                Item::Use(item) => self.declaration(item, module),
                Item::Mod(item) => {
                    if let Some((_, inner)) = &item.content {
                        module.push(item.ident.to_string());
                        self.items(inner, module);
                        module.pop();
                    }
                }
                _ => {}
            }
        }
    }
}

/// Rule 5(b): the handles a data module's items hand to code outside data modules, and the
/// re-exports that carry them there.
pub(super) fn handouts(
    path: &str,
    module: &[String],
    ast: &syn::File,
    modules: &ModuleScopes,
    exemptions: &[Exemption],
) -> Vec<Violation> {
    let store = path.starts_with(STORE_CRATE);
    let imports = imports::Imports::of(ast);
    let exempt = |owner: &str, item: &str| {
        exemptions
            .iter()
            .any(|entry| entry.0 == path && exempted(entry, owner, item))
    };
    let mut found: Vec<Violation> = items::hands(ast, module, &imports, &exempt)
        .iter()
        .filter(|hand| modules.reaches_hand(store, hand))
        .map(|hand| violation(path, hand.line, &hand.handle, hand.kind, &hand.name))
        .collect();
    let mut reexports = Reexports {
        path,
        store,
        modules,
        found: Vec::new(),
    };
    reexports.items(&ast.items, &mut module.to_vec());
    found.extend(reexports.found);
    found
}

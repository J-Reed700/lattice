//! Rule 5: a data module hands no driver handle out to code outside data
//! modules, and only a data module unwraps a `Database` with `sqlx_pool()`.
use super::{
    is_group, is_punct, walk_tokens, Exemption, Violation, DATA_NAMES, DATA_SUFFIXES, STORE_CRATE,
};
use crate::{
    item_attrs,
    ratchet::{impl_item_attrs, trait_item_attrs},
    test_only,
};
use proc_macro2::{Delimiter, TokenTree};
use std::collections::{HashMap, HashSet};
use syn::{
    visit::{self, Visit},
    Ident, ImplItem, Item, Signature, TraitItem, UseTree, Visibility,
};

/// For each declared module path, the module its `mod` declaration makes it visible in.
pub(super) type ModuleScopes = HashMap<Vec<String>, Vec<String>>;

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

/// Where each `mod` declared in the files is visible, keyed by the module's path.
pub(super) fn module_scopes<'a>(
    files: impl IntoIterator<Item = (&'a [String], &'a syn::File)>,
) -> ModuleScopes {
    let mut scopes = ModuleScopes::new();
    for (module, ast) in files {
        declared(&ast.items, module, &mut scopes);
    }
    scopes
}

fn declared(items: &[Item], module: &[String], scopes: &mut ModuleScopes) {
    for item in items {
        let Item::Mod(declaration) = item else {
            continue;
        };
        if test_only(&declaration.attrs) {
            continue;
        }
        let mut inner = module.to_vec();
        inner.push(declaration.ident.to_string());
        scopes.insert(inner.clone(), resolve(&declaration.vis, module));
        if let Some((_, items)) = &declaration.content {
            declared(items, &inner, scopes);
        }
    }
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

/// The deepest of the item's own scope and the scopes its enclosing modules are declared in.
fn effective_scope(vis: &Visibility, module: &[String], modules: &ModuleScopes) -> Vec<String> {
    let mut scope = resolve(vis, module);
    for end in 1..=module.len() {
        let declared = module.get(..end).and_then(|prefix| modules.get(prefix));
        if let Some(declared) = declared {
            if declared.len() > scope.len() {
                scope = declared.clone();
            }
        }
    }
    scope
}

fn data_scope(store: bool, scope: &[String]) -> bool {
    store
        || scope.iter().any(|segment| {
            DATA_NAMES.contains(&segment.as_str())
                || DATA_SUFFIXES
                    .iter()
                    .any(|suffix| segment.ends_with(*suffix))
        })
}

/// What the file imports from sqlx, what it imports from elsewhere, and what it defines.
#[derive(Default)]
struct Imports {
    named: HashMap<String, Vec<String>>,
    foreign: HashSet<String>,
    globs: usize,
    local: HashSet<String>,
}

impl Imports {
    fn is_handle(&self, segments: &[String]) -> bool {
        let Some(first) = segments.first() else {
            return false;
        };
        if let Some(target) = self.named.get(first) {
            return target
                .iter()
                .chain(segments.iter().skip(1))
                .last()
                .is_some_and(|last| HANDLES.contains(&last.as_str()));
        }
        let Some(last) = segments.last() else {
            return false;
        };
        HANDLES.contains(&last.as_str())
            && (first == "sqlx"
                || (segments.len() == 1
                    && self.globs > 0
                    && !self.foreign.contains(first)
                    && !self.local.contains(first)))
    }
}

impl<'ast> Visit<'ast> for Imports {
    fn visit_item(&mut self, item: &'ast Item) {
        if item_attrs(item).is_some_and(test_only) {
            return;
        }
        let defined = match item {
            Item::Struct(item) => Some(&item.ident),
            Item::Enum(item) => Some(&item.ident),
            Item::Union(item) => Some(&item.ident),
            Item::Type(item) => Some(&item.ident),
            Item::Trait(item) => Some(&item.ident),
            _ => None,
        };
        if let Some(ident) = defined {
            self.local.insert(ident.to_string());
        }
        visit::visit_item(self, item);
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        for leaf in import_leaves(&item.tree) {
            if leaf.path.first().is_some_and(|first| first == "sqlx") {
                if leaf.glob {
                    self.globs += 1;
                } else {
                    self.named.insert(leaf.alias, leaf.path);
                }
            } else {
                self.foreign.insert(leaf.alias);
            }
        }
    }
}

/// The first handle type named anywhere in a type, and its line.
struct Finder<'a> {
    imports: &'a Imports,
    found: Option<(String, usize)>,
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        if self.found.is_none() && self.imports.is_handle(&segments) {
            let line = path
                .segments
                .first()
                .map_or(1, |segment| segment.ident.span().start().line);
            self.found = Some((segments.join("::"), line));
        }
        visit::visit_path(self, path);
    }
}

fn find_handle(imports: &Imports, ty: &syn::Type) -> Option<(String, usize)> {
    let mut finder = Finder {
        imports,
        found: None,
    };
    finder.visit_type(ty);
    finder.found
}

struct Handouts<'a> {
    path: &'a str,
    store: bool,
    modules: &'a ModuleScopes,
    exemptions: &'a [Exemption<'a>],
    imports: Imports,
    found: Vec<Violation>,
}

impl Handouts<'_> {
    fn reachable(&self, vis: &Visibility, module: &[String]) -> bool {
        !data_scope(self.store, &effective_scope(vis, module, self.modules))
    }

    fn report(&mut self, line: usize, handle: &str, kind: &str, name: &str) {
        self.found.push(Violation {
            rule: 5,
            path: self.path.to_string(),
            line,
            message: format!(
                "hands out {handle} from {kind} {name}; return a Database or keep it \
                 private to data modules"
            ),
        });
    }

    fn function(&mut self, vis: &Visibility, signature: &Signature, module: &[String]) {
        let syn::ReturnType::Type(_, ty) = &signature.output else {
            return;
        };
        let exempt = self
            .exemptions
            .iter()
            .any(|(file, name, _)| *file == self.path && signature.ident == *name);
        if exempt {
            return;
        }
        let Some((handle, _)) = find_handle(&self.imports, ty) else {
            return;
        };
        if self.reachable(vis, module) {
            let line = signature.ident.span().start().line;
            self.report(line, &handle, "fn", &signature.ident.to_string());
        }
    }

    fn typed(
        &mut self,
        vis: &Visibility,
        ty: &syn::Type,
        kind: &str,
        name: &Ident,
        module: &[String],
    ) {
        if let Some((handle, line)) = find_handle(&self.imports, ty) {
            if self.reachable(vis, module) {
                self.report(line, &handle, kind, &name.to_string());
            }
        }
    }

    fn fields(
        &mut self,
        fields: &syn::Fields,
        owner: &str,
        shared: Option<&Visibility>,
        module: &[String],
    ) {
        for (index, field) in fields.iter().enumerate() {
            let Some((handle, line)) = find_handle(&self.imports, &field.ty) else {
                continue;
            };
            if self.reachable(shared.unwrap_or(&field.vis), module) {
                let name = match &field.ident {
                    Some(ident) => format!("{owner}.{ident}"),
                    None => format!("{owner}.{index}"),
                };
                self.report(line, &handle, "field", &name);
            }
        }
    }

    fn uses(&mut self, item: &syn::ItemUse, module: &[String]) {
        if !self.reachable(&item.vis, module) {
            return;
        }
        for leaf in import_leaves(&item.tree) {
            if leaf.path.first().is_some_and(|first| first == "sqlx") {
                self.report(leaf.line, &leaf.text(), "re-export", &leaf.alias);
            }
        }
    }

    fn items(&mut self, items: &[Item], module: &mut Vec<String>) {
        for item in items {
            if item_attrs(item).is_some_and(test_only) {
                continue;
            }
            match item {
                Item::Fn(item) => self.function(&item.vis, &item.sig, module),
                Item::Impl(item) if item.trait_.is_none() => {
                    for member in &item.items {
                        if impl_item_attrs(member).is_some_and(test_only) {
                            continue;
                        }
                        match member {
                            ImplItem::Fn(f) => self.function(&f.vis, &f.sig, module),
                            ImplItem::Const(c) => {
                                self.typed(&c.vis, &c.ty, "const", &c.ident, module);
                            }
                            _ => {}
                        }
                    }
                }
                Item::Trait(item) => {
                    for member in &item.items {
                        if trait_item_attrs(member).is_some_and(test_only) {
                            continue;
                        }
                        if let TraitItem::Fn(f) = member {
                            self.function(&item.vis, &f.sig, module);
                        }
                    }
                }
                Item::Struct(item) => {
                    self.fields(&item.fields, &item.ident.to_string(), None, module);
                }
                Item::Enum(item) => {
                    for variant in &item.variants {
                        let owner = format!("{}::{}", item.ident, variant.ident);
                        self.fields(&variant.fields, &owner, Some(&item.vis), module);
                    }
                }
                Item::Type(item) => self.typed(&item.vis, &item.ty, "alias", &item.ident, module),
                Item::Const(item) => self.typed(&item.vis, &item.ty, "const", &item.ident, module),
                Item::Static(item) => {
                    self.typed(&item.vis, &item.ty, "static", &item.ident, module);
                }
                Item::Use(item) => self.uses(item, module),
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

/// Rule 5(b): the handles a data module's items hand to code outside data modules.
pub(super) fn handouts(
    path: &str,
    module: &[String],
    ast: &syn::File,
    modules: &ModuleScopes,
    exemptions: &[Exemption],
) -> Vec<Violation> {
    let mut imports = Imports::default();
    imports.visit_file(ast);
    let mut walk = Handouts {
        path,
        store: path.starts_with(STORE_CRATE),
        modules,
        exemptions,
        imports,
        found: Vec::new(),
    };
    walk.items(&ast.items, &mut module.to_vec());
    walk.found
}

struct Calls<'a> {
    path: &'a str,
    found: Vec<Violation>,
}

impl Calls<'_> {
    fn add(&mut self, line: usize) {
        self.found.push(Violation {
            rule: 5,
            path: self.path.to_string(),
            line,
            message: "calls sqlx_pool() outside a data module; pass the Database to a \
                      data-module function"
                .to_string(),
        });
    }
}

impl<'ast> Visit<'ast> for Calls<'_> {
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
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "sqlx_pool" {
            self.add(call.method.span().start().line);
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(function) = &*call.func {
            if let Some(last) = function.path.segments.last() {
                if last.ident == "sqlx_pool" {
                    self.add(last.ident.span().start().line);
                }
            }
        }
        visit::visit_expr_call(self, call);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        visit::visit_macro(self, mac);
        let mut lines = Vec::new();
        walk_tokens(mac.tokens.clone(), &mut |tokens, index| {
            let Some(TokenTree::Ident(ident)) = tokens.get(index) else {
                return;
            };
            if ident != "sqlx_pool" || !is_group(tokens.get(index + 1), Delimiter::Parenthesis) {
                return;
            }
            let before = index.checked_sub(1).and_then(|i| tokens.get(i));
            if is_punct(before, '.') || is_punct(before, ':') {
                lines.push(ident.span().start().line);
            }
        });
        for line in lines {
            self.add(line);
        }
    }
}

/// Rule 5(c): the calls of `sqlx_pool()` in a file that is not a data module.
pub(super) fn calls(path: &str, ast: &syn::File) -> Vec<Violation> {
    let mut scan = Calls {
        path,
        found: Vec::new(),
    };
    scan.visit_file(ast);
    scan.found
}

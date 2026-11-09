//! The four data-access rules, held at zero: one way to begin a transaction,
//! SQL only in data modules, one transaction for a function that runs several statements
//! or is a unit of work, and sqlx's unit-of-work types only in data modules.
use crate::{
    item_attrs,
    ratchet::{impl_item_attrs, production_files, token_paths, trait_item_attrs},
    test_only,
};
mod rule3;
mod sqlx_types;
mod unit_of_work;
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use std::{collections::HashSet, fmt, path::Path};
use syn::{
    visit::{self, Visit},
    Expr, ImplItem, Item, TraitItem, UseTree,
};

type Error = Box<dyn std::error::Error>;

const TRANSACTIONS: &str = "crates/lattice-core/src/shared/persistence/transactions.rs";
const POSTGRES: &str = "crates/lattice-sync/";
const STORE_CRATE: &str = "crates/lattice-store/";
const DATA_NAMES: [&str; 7] = [
    "repository",
    "repositories",
    "persistence",
    "store",
    "storage",
    "database",
    "migrations",
];
const DATA_SUFFIXES: [&str; 2] = ["_repository", "_store"];
const SQL_FUNCTIONS: [&str; 7] = [
    "query",
    "query_as",
    "query_scalar",
    "query_with",
    "query_as_with",
    "query_scalar_with",
    "raw_sql",
];
const SQL_MACROS: [&str; 3] = ["query", "query_as", "query_scalar"];
const DEFERRED: &str = "begins a deferred transaction; use begin_write() or begin_read()";
const BEGIN_WITH: &str = "begins with begin_with(); use begin_write() or begin_read()";

/// `(file, function, reason)`: a function rule 3 would flag, with the reason it must not be one transaction.
pub const EXCEPTIONS: &[(&str, &str, &str)] = &[
    (
        "crates/lattice-desktop/src/infrastructure/persistence/database/migrate.rs",
        "run_migrations",
        "sqlx's migrator runs each migration in its own transaction, and PRAGMA optimize after it refreshes planner statistics only",
    ),
    (
        "crates/lattice-desktop/src/features/indexing/library_owner.rs",
        "adopt",
        "The owner file is created (create_new, never replacing) while the owner id is uncommitted: a file that cannot be created, or that another instance created first, rolls the id back, so the id and the file always agree.",
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: u8,
    pub path: String,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "rule {}: {}:{} {}",
            self.rule, self.path, self.line, self.message
        )
    }
}

/// Whether a repository-relative path is a data module: the store crate, or a
/// segment or file stem that names data access or ends in `_repository` or `_store`.
pub fn is_data_module(path: &str) -> bool {
    path.starts_with(STORE_CRATE)
        || path.split('/').any(|segment| {
            let stem = segment.strip_suffix(".rs").unwrap_or(segment);
            DATA_NAMES.contains(&stem) || DATA_SUFFIXES.iter().any(|suffix| stem.ends_with(*suffix))
        })
}

pub fn file_violations(path: &str, ast: &syn::File) -> Vec<Violation> {
    file_violations_with(path, ast, EXCEPTIONS)
}

pub fn file_violations_with(
    path: &str,
    ast: &syn::File,
    exceptions: &[(&str, &str, &str)],
) -> Vec<Violation> {
    if test_only(&ast.attrs) {
        return Vec::new();
    }
    let data = is_data_module(path);
    let postgres = path.starts_with(POSTGRES);
    let mut scan = Scan {
        path,
        begins: path != TRANSACTIONS && !postgres,
        sql: !data,
        found: Vec::new(),
    };
    scan.visit_file(ast);
    let mut found = scan.found;
    if !data && !postgres {
        found.extend(sqlx_types::violations(path, ast));
    }
    found.extend(rule3::violations(path, ast, exceptions, data));
    found.extend(unit_of_work::violations(path, ast, exceptions));
    found.sort_by_key(|violation| (violation.rule, violation.line));
    found
}

/// Checks the production files below every source root; a root named twice counts once.
pub fn check(roots: &[&Path], failures: &mut usize) -> Result<(), Error> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for root in roots {
        let canonical = root
            .canonicalize()
            .map_err(|e| format!("{}: {e}", root.display()))?;
        if !seen.insert(canonical.clone()) {
            continue;
        }
        let label = repository_label(&canonical)?;
        let (files, _) = production_files(root)?;
        for file in &files {
            let path = format!("{label}/{}", file.relative);
            found.extend(file_violations(&path, &file.ast));
        }
    }
    found.sort_by(|a, b| (&a.path, a.rule, a.line).cmp(&(&b.path, b.rule, b.line)));
    for violation in &found {
        eprintln!("{violation}");
        *failures += 1;
    }
    Ok(())
}

/// `crates/<crate>/src`: the last three components of a source root.
fn repository_label(source_root: &Path) -> Result<String, Error> {
    let parts: Vec<String> = source_root
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    let tail = parts
        .len()
        .checked_sub(3)
        .and_then(|start| parts.get(start..))
        .ok_or_else(|| format!("{}: too short to name a crate", source_root.display()))?;
    Ok(tail.join("/"))
}

fn is_sql_function(name: &str) -> bool {
    SQL_FUNCTIONS.contains(&name) || name.starts_with("query_file")
}

fn is_sql_macro(name: &str) -> bool {
    SQL_MACROS.contains(&name) || name.starts_with("query_file")
}

/// The segment that makes a path SQL, and what to call it.
fn path_sql(segments: &[String]) -> Option<(usize, String)> {
    if let [first, second, ..] = segments {
        if first == "sqlx" && is_sql_function(second) {
            return Some((0, format!("sqlx::{second}")));
        }
    }
    segments
        .iter()
        .position(|segment| segment == "QueryBuilder")
        .map(|index| (index, "QueryBuilder".to_string()))
}

fn use_leaves(tree: &UseTree, mut prefix: Vec<String>, out: &mut Vec<(Vec<String>, usize)>) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            use_leaves(&path.tree, prefix, out);
        }
        UseTree::Group(group) => {
            for item in &group.items {
                use_leaves(item, prefix.clone(), out);
            }
        }
        UseTree::Name(name) => {
            prefix.push(name.ident.to_string());
            out.push((prefix, name.ident.span().start().line));
        }
        UseTree::Rename(rename) => {
            prefix.push(rename.ident.to_string());
            out.push((prefix, rename.ident.span().start().line));
        }
        UseTree::Glob(glob) => out.push((prefix, glob.star_token.span.start().line)),
    }
}

/// Calls `visit` with every token of the stream and of the groups inside it.
fn walk_tokens(stream: TokenStream, visit: &mut impl FnMut(&[TokenTree], usize)) {
    let tokens: Vec<TokenTree> = stream.into_iter().collect();
    for index in 0..tokens.len() {
        visit(&tokens, index);
    }
    for token in &tokens {
        if let TokenTree::Group(group) = token {
            walk_tokens(group.stream(), visit);
        }
    }
}

fn is_punct(token: Option<&TokenTree>, character: char) -> bool {
    matches!(token, Some(TokenTree::Punct(punct)) if punct.as_char() == character)
}

fn is_group(token: Option<&TokenTree>, delimiter: Delimiter) -> bool {
    matches!(token, Some(TokenTree::Group(group)) if group.delimiter() == delimiter)
}

/// Whether the identifier at `index` follows `sqlx::`.
fn follows_sqlx(tokens: &[TokenTree], index: usize) -> bool {
    index
        .checked_sub(3)
        .and_then(|start| tokens.get(start..index))
        .is_some_and(|before| {
            matches!(
                before,
                [TokenTree::Ident(first), TokenTree::Punct(a), TokenTree::Punct(b)]
                    if first.to_string() == "sqlx" && a.as_char() == ':' && b.as_char() == ':'
            )
        })
}

/// Rules 1 and 2: how a transaction begins, and where SQL is built.
struct Scan<'a> {
    path: &'a str,
    begins: bool,
    sql: bool,
    found: Vec<Violation>,
}

impl Scan<'_> {
    fn add(&mut self, rule: u8, line: usize, message: String) {
        self.found.push(Violation {
            rule,
            path: self.path.to_string(),
            line,
            message,
        });
    }

    fn deferred(&mut self, line: usize) {
        if self.begins {
            self.add(1, line, DEFERRED.to_string());
        }
    }

    fn begin_with(&mut self, line: usize) {
        if self.begins {
            self.add(1, line, BEGIN_WITH.to_string());
        }
    }

    fn sql_found(&mut self, line: usize, what: String) {
        if self.sql {
            self.add(2, line, format!("builds SQL outside a data module: {what}"));
        }
    }

    fn tokens(&mut self, stream: TokenStream) {
        for (segments, _, line) in token_paths(stream.clone()) {
            if let Some((_, what)) = path_sql(&segments) {
                self.sql_found(line, what);
            }
        }
        walk_tokens(stream, &mut |tokens, index| {
            let Some(TokenTree::Ident(ident)) = tokens.get(index) else {
                return;
            };
            let name = ident.to_string();
            let line = ident.span().start().line;
            let next = tokens.get(index + 1);
            let called = is_group(next, Delimiter::Parenthesis);
            if name == "begin_with" && called {
                self.begin_with(line);
            }
            let empty_call = matches!(next, Some(TokenTree::Group(group))
                if group.delimiter() == Delimiter::Parenthesis && group.stream().is_empty());
            let awaited = is_punct(tokens.get(index + 2), '.')
                && matches!(tokens.get(index + 3), Some(TokenTree::Ident(word)) if word.to_string() == "await");
            let dotted = index
                .checked_sub(1)
                .is_some_and(|before| is_punct(tokens.get(before), '.'));
            if name == "begin" && empty_call && dotted && awaited {
                self.deferred(line);
            }
            if follows_sqlx(tokens, index) {
                return;
            }
            if name == "raw_sql" && called {
                self.sql_found(line, "raw_sql(".to_string());
            }
            if is_sql_macro(&name)
                && is_punct(next, '!')
                && is_group(tokens.get(index + 2), Delimiter::Parenthesis)
            {
                self.sql_found(line, format!("{name}!("));
            }
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
    fn visit_expr_await(&mut self, expr: &'ast syn::ExprAwait) {
        if let Expr::MethodCall(call) = &*expr.base {
            if call.method == "begin" && call.args.is_empty() {
                self.deferred(call.method.span().start().line);
            }
        }
        visit::visit_expr_await(self, expr);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "begin_with" {
            self.begin_with(call.method.span().start().line);
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(function) = &*call.func {
            let first = function.path.segments.first();
            if let Some(last) = function.path.segments.last() {
                let line = last.ident.span().start().line;
                if last.ident == "begin_with" {
                    self.begin_with(line);
                }
                if last.ident == "raw_sql" && !first.is_some_and(|first| first.ident == "sqlx") {
                    self.sql_found(line, "raw_sql(".to_string());
                }
            }
        }
        visit::visit_expr_call(self, call);
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        if let Some((index, what)) = path_sql(&segments) {
            let line = path
                .segments
                .iter()
                .nth(index)
                .map_or(1, |segment| segment.ident.span().start().line);
            self.sql_found(line, what);
        }
        visit::visit_path(self, path);
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut leaves = Vec::new();
        use_leaves(&item.tree, Vec::new(), &mut leaves);
        let mut reported = false;
        for (leaf, line) in leaves {
            if leaf.iter().any(|segment| segment == "QueryBuilder") {
                self.sql_found(line, "QueryBuilder".to_string());
            } else if let (false, [first, name, ..]) = (reported, leaf.as_slice()) {
                if first == "sqlx" && is_sql_function(name) {
                    reported = true;
                    self.sql_found(
                        item.use_token.span.start().line,
                        format!("use sqlx::{name}"),
                    );
                }
            }
        }
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        visit::visit_macro(self, mac);
        let first = mac.path.segments.first();
        if let Some(last) = mac.path.segments.last() {
            let name = last.ident.to_string();
            if !first.is_some_and(|first| first.ident == "sqlx")
                && is_sql_macro(&name)
                && matches!(mac.delimiter, syn::MacroDelimiter::Paren(_))
            {
                self.sql_found(last.ident.span().start().line, format!("{name}!("));
            }
        }
        self.tokens(mac.tokens.clone());
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod unit_of_work_tests;

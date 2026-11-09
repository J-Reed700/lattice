//! The three data-access rules, held at zero: one way to begin a transaction,
//! SQL only in data modules, and one transaction per multi-statement write.
use crate::{
    item_attrs,
    ratchet::{impl_item_attrs, production_files, token_paths, trait_item_attrs},
    test_only,
};
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use std::{collections::HashSet, fmt, path::Path};
use syn::{
    __private::ToTokens,
    visit::{self, Visit},
    Expr, ImplItem, Item, Pat, TraitItem, UseTree,
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
const STATEMENTS: [&str; 7] = [
    "execute",
    "execute_many",
    "fetch",
    "fetch_one",
    "fetch_all",
    "fetch_optional",
    "fetch_many",
];
const WRITES: [&str; 2] = ["execute", "execute_many"];
const DEFERRED: &str = "begins a deferred transaction; use begin_write() or begin_read()";
const BEGIN_WITH: &str = "begins with begin_with(); use begin_write() or begin_read()";

/// `(file, function, reason)` for a function SQLite refuses to run in a transaction.
pub const EXCEPTIONS: &[(&str, &str, &str)] = &[];

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
    let mut scan = Scan {
        path,
        begins: path != TRANSACTIONS && !path.starts_with(POSTGRES),
        sql: !data,
        found: Vec::new(),
    };
    scan.visit_file(ast);
    let mut found = scan.found;
    if data {
        let mut functions = Functions {
            path,
            exceptions,
            found: Vec::new(),
        };
        functions.visit_file(ast);
        found.extend(functions.found);
    }
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

/// A statement call: whether it writes, and the tokens of its arguments.
struct Call {
    write: bool,
    squashed: String,
    words: HashSet<String>,
}

/// Every statement call in a function body, and the connections it checks out.
#[derive(Default)]
struct Statements {
    acquired: HashSet<String>,
    calls: Vec<Call>,
}

impl Statements {
    fn call(&mut self, method: &str, arguments: &str) {
        if STATEMENTS.contains(&method) {
            self.calls.push(Call {
                write: WRITES.contains(&method),
                squashed: arguments.split_whitespace().collect::<String>(),
                words: arguments
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .filter(|word| !word.is_empty())
                    .map(str::to_string)
                    .collect(),
            });
        }
    }

    fn on_pool(&self, call: &Call) -> bool {
        call.squashed.contains("pool")
            || call.squashed.contains("acquire()")
            || !call.words.is_disjoint(&self.acquired)
    }
}

fn binding(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(ident) => Some(ident.ident.to_string()),
        Pat::Type(typed) => binding(&typed.pat),
        Pat::TupleStruct(wrapper) if wrapper.path.is_ident("Ok") && wrapper.elems.len() == 1 => {
            wrapper.elems.first().and_then(binding)
        }
        _ => None,
    }
}

impl<'ast> Visit<'ast> for Statements {
    fn visit_item(&mut self, item: &'ast Item) {
        if !item_attrs(item).is_some_and(test_only) {
            visit::visit_item(self, item);
        }
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if test_only(&local.attrs) {
            return;
        }
        if let Some(init) = &local.init {
            let text: String = init
                .expr
                .to_token_stream()
                .to_string()
                .split_whitespace()
                .collect();
            if text.contains("acquire()") {
                if let Some(name) = binding(&local.pat) {
                    self.acquired.insert(name);
                }
            }
        }
        visit::visit_local(self, local);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let method = call.method.to_string();
        self.call(&method, &call.args.to_token_stream().to_string());
        visit::visit_expr_method_call(self, call);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        walk_tokens(mac.tokens.clone(), &mut |tokens, index| {
            let Some(TokenTree::Ident(method)) = tokens.get(index) else {
                return;
            };
            let dotted = index
                .checked_sub(1)
                .is_some_and(|before| is_punct(tokens.get(before), '.'));
            if let (true, Some(TokenTree::Group(arguments))) = (dotted, tokens.get(index + 1)) {
                if arguments.delimiter() == Delimiter::Parenthesis {
                    self.call(&method.to_string(), &arguments.stream().to_string());
                }
            }
        });
    }
}

/// Rule 3: every function with a body in a data module.
struct Functions<'a> {
    path: &'a str,
    exceptions: &'a [(&'a str, &'a str, &'a str)],
    found: Vec<Violation>,
}

impl Functions<'_> {
    fn function(&mut self, signature: &syn::Signature, block: &syn::Block) {
        let name = signature.ident.to_string();
        let mut statements = Statements::default();
        statements.visit_block(block);
        let on_pool: Vec<&Call> = statements
            .calls
            .iter()
            .filter(|call| statements.on_pool(call))
            .collect();
        let writes = on_pool.iter().filter(|call| call.write).count();
        let excepted = self
            .exceptions
            .iter()
            .any(|(file, function, _)| *file == self.path && *function == name);
        if on_pool.len() >= 2 && writes >= 1 && !excepted {
            self.found.push(Violation {
                rule: 3,
                path: self.path.to_string(),
                line: signature.fn_token.span.start().line,
                message: format!(
                    "{name} runs {} statements on the pool, {writes} of them writes, \
                     outside a transaction",
                    on_pool.len()
                ),
            });
        }
    }
}

impl<'ast> Visit<'ast> for Functions<'_> {
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
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.function(&item.sig, &item.block);
        visit::visit_item_fn(self, item);
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.function(&item.sig, &item.block);
        visit::visit_impl_item_fn(self, item);
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if let Some(block) = &item.default {
            self.function(&item.sig, block);
        }
        visit::visit_trait_item_fn(self, item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVICE: &str = "crates/lattice-learning/src/lessons/service.rs";
    const REPOSITORY: &str = "crates/lattice-learning/src/lessons/repository.rs";
    const TRANSACTIONS: &str = "crates/lattice-core/src/shared/persistence/transactions.rs";
    const DEFERRED: &str = "begins a deferred transaction; use begin_write() or begin_read()";
    const BEGIN_WITH: &str = "begins with begin_with(); use begin_write() or begin_read()";

    fn found(path: &str, source: &str) -> Vec<String> {
        let ast = syn::parse_file(source).unwrap();
        file_violations(path, &ast)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn sql(path: &str, line: usize, what: &str) -> String {
        format!("rule 2: {path}:{line} builds SQL outside a data module: {what}")
    }

    fn pool(path: &str, line: usize, name: &str, statements: usize, writes: usize) -> String {
        format!(
            "rule 3: {path}:{line} {name} runs {statements} statements on the pool, \
             {writes} of them writes, outside a transaction"
        )
    }

    #[test]
    fn a_deferred_begin_is_flagged_and_the_two_explicit_begins_are_not() {
        let source = r#"async fn start(pool: &SqlitePool) {
    let tx = pool
        .begin()
        .await
        .unwrap();
    let write = pool.begin_write().await.unwrap();
    let read = pool.begin_read().await.unwrap();
    let builder = connection.begin();
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![format!("rule 1: {SERVICE}:3 {DEFERRED}")]
        );
    }

    #[test]
    fn begin_with_is_flagged_everywhere_but_transactions_rs() {
        let source = r#"async fn start(pool: &SqlitePool) {
    let tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![format!("rule 1: {SERVICE}:2 {BEGIN_WITH}")]
        );
        assert_eq!(found(TRANSACTIONS, source), Vec::<String>::new());
    }

    #[test]
    fn the_sync_server_may_begin_a_postgres_transaction() {
        let source = r#"async fn start(pool: &PgPool) {
    let tx = pool.begin().await.unwrap();
    let other = pool.begin_with("BEGIN").await.unwrap();
}
"#;
        assert_eq!(
            found("crates/lattice-sync/src/service.rs", source),
            Vec::<String>::new()
        );
        assert_eq!(
            found(SERVICE, source),
            vec![
                format!("rule 1: {SERVICE}:2 {DEFERRED}"),
                format!("rule 1: {SERVICE}:3 {BEGIN_WITH}"),
            ]
        );
    }

    #[test]
    fn a_begin_inside_a_macro_body_is_flagged() {
        let source = r#"async fn start(pool: &SqlitePool) {
    tokio::select! { tx = pool.begin().await => {} }
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![format!("rule 1: {SERVICE}:2 {DEFERRED}")]
        );
    }

    #[test]
    fn sql_in_a_service_a_di_file_and_a_trait_impl_is_flagged() {
        let call = r#"async fn load(pool: &SqlitePool) {
    sqlx::query("SELECT 1").fetch_one(pool).await.unwrap();
}
"#;
        assert_eq!(found(SERVICE, call), vec![sql(SERVICE, 2, "sqlx::query")]);
        let di = "crates/lattice-desktop/src/interfaces/di/memory.rs";
        assert_eq!(found(di, call), vec![sql(di, 2, "sqlx::query")]);
        let port = "crates/lattice-core/src/application/ports/lessons.rs";
        let implementation = r#"impl Port for Adapter {
    async fn get(&self) {
        sqlx::query_as::<_, Row>("SELECT 1").fetch_all(&self.pool).await.unwrap();
    }
}
"#;
        assert_eq!(
            found(port, implementation),
            vec![sql(port, 3, "sqlx::query_as")]
        );
    }

    #[test]
    fn the_same_sql_in_a_data_module_is_fine() {
        let call = r#"async fn load(pool: &SqlitePool) {
    sqlx::query("SELECT 1").fetch_one(pool).await.unwrap();
}
"#;
        assert_eq!(found(SERVICE, call), vec![sql(SERVICE, 2, "sqlx::query")]);
        for path in [
            "crates/lattice-learning/src/lessons/repository.rs",
            "crates/lattice-learning/src/lessons/repository/claims.rs",
            "crates/lattice-index/src/embedding_store.rs",
            "crates/lattice-store/src/lib.rs",
            "crates/lattice-desktop/src/features/x/persistence/mod.rs",
        ] {
            assert_eq!(found(path, call), Vec::<String>::new(), "{path}");
        }
    }

    #[test]
    fn sql_inside_a_macro_body_is_flagged() {
        let source = r#"async fn load(pool: &SqlitePool) {
    tokio::try_join!(
        sqlx::query("a").execute(pool),
        query_scalar!("SELECT 1").fetch_one(pool),
    )
    .unwrap();
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![
                sql(SERVICE, 3, "sqlx::query"),
                sql(SERVICE, 4, "query_scalar!(")
            ]
        );
    }

    #[test]
    fn a_use_of_a_query_function_is_flagged_and_other_imports_are_not() {
        let source = "use sqlx::{query, Row};
use sqlx::query_as;
use sqlx::{Row, SqlitePool};
use sqlx::query::Query;
use sqlx::sqlite::SqliteRow;
";
        assert_eq!(
            found(SERVICE, source),
            vec![
                sql(SERVICE, 1, "use sqlx::query"),
                sql(SERVICE, 2, "use sqlx::query_as"),
                sql(SERVICE, 4, "use sqlx::query"),
            ]
        );
    }

    #[test]
    fn a_query_builder_is_flagged_where_it_is_imported_and_where_it_is_named() {
        let source = r#"use sqlx::QueryBuilder;
fn build() {
    let mut b = QueryBuilder::<Sqlite>::new("INSERT");
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![
                sql(SERVICE, 1, "QueryBuilder"),
                sql(SERVICE, 3, "QueryBuilder")
            ]
        );
    }

    #[test]
    fn a_call_to_raw_sql_is_flagged_and_so_is_its_import() {
        let source = r#"use sqlx::raw_sql;
async fn run(pool: &SqlitePool) {
    raw_sql("PRAGMA x").execute(pool).await.unwrap();
}
"#;
        assert_eq!(
            found(SERVICE, source),
            vec![
                sql(SERVICE, 1, "use sqlx::raw_sql"),
                sql(SERVICE, 3, "raw_sql(")
            ]
        );
    }

    #[test]
    fn test_only_items_are_not_production_code() {
        let source = r#"#[cfg(test)]
mod tests {
    fn f() { sqlx::query("x"); pool.begin().await; }
}
#[cfg(test)]
fn helper() { sqlx::query("y"); }
fn kept() { sqlx::query("z"); }
"#;
        assert_eq!(found(SERVICE, source), vec![sql(SERVICE, 7, "sqlx::query")]);
    }

    #[test]
    fn two_statements_on_the_pool_with_a_write_are_flagged() {
        let source = r#"pub async fn rename(pool: &SqlitePool) {
    sqlx::query("UPDATE a").execute(pool).await.unwrap();
    sqlx::query("SELECT b").fetch_one(pool).await.unwrap();
}
"#;
        assert_eq!(
            found(REPOSITORY, source),
            vec![pool(REPOSITORY, 1, "rename", 2, 1)]
        );
    }

    #[test]
    fn two_statements_on_a_connection_from_acquire_are_flagged() {
        let source = r#"pub async fn move_it(pool: &SqlitePool) {
    let mut conn = pool.acquire().await.unwrap();
    sqlx::query("UPDATE a").execute(&mut *conn).await.unwrap();
    sqlx::query("SELECT b").fetch_one(&mut *conn).await.unwrap();
}
"#;
        assert_eq!(
            found(REPOSITORY, source),
            vec![pool(REPOSITORY, 1, "move_it", 2, 1)]
        );
    }

    #[test]
    fn statements_on_a_transaction_or_a_borrowed_connection_are_not_flagged() {
        let on_pool = r#"pub async fn move_it(pool: &SqlitePool) {
    sqlx::query("UPDATE a").execute(pool).await.unwrap();
    sqlx::query("UPDATE b").execute(pool).await.unwrap();
}
"#;
        assert_eq!(
            found(REPOSITORY, on_pool),
            vec![pool(REPOSITORY, 1, "move_it", 2, 2)]
        );
        let on_transaction = r#"pub async fn move_it(pool: &SqlitePool) {
    let mut tx = pool.begin_write().await.unwrap();
    sqlx::query("UPDATE a").execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE b").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
pub async fn move_in(conn: &mut SqliteConnection) {
    sqlx::query("UPDATE a").execute(&mut *conn).await.unwrap();
    sqlx::query("UPDATE b").execute(&mut *conn).await.unwrap();
}
"#;
        assert_eq!(found(REPOSITORY, on_transaction), Vec::<String>::new());
    }

    #[test]
    fn one_statement_or_two_reads_are_not_flagged() {
        let one = r#"pub async fn rename(pool: &SqlitePool) {
    sqlx::query("UPDATE a").execute(pool).await.unwrap();
}
"#;
        let reads = r#"pub async fn both(pool: &SqlitePool) {
    sqlx::query("SELECT a").fetch_all(pool).await.unwrap();
    sqlx::query("SELECT b").fetch_optional(pool).await.unwrap();
}
"#;
        let write_and_read = r#"pub async fn rename(pool: &SqlitePool) {
    sqlx::query("SELECT b").fetch_optional(pool).await.unwrap();
    sqlx::query_scalar::<_, i64>("SELECT c").fetch_one(pool).await.unwrap();
    sqlx::query("UPDATE a").execute_many(pool).await.unwrap();
}
"#;
        assert_eq!(found(REPOSITORY, one), Vec::<String>::new());
        assert_eq!(found(REPOSITORY, reads), Vec::<String>::new());
        assert_eq!(
            found(REPOSITORY, write_and_read),
            vec![pool(REPOSITORY, 1, "rename", 3, 1)]
        );
    }

    #[test]
    fn a_listed_function_is_excepted() {
        let source = r#"pub async fn compact(pool: &SqlitePool) {
    sqlx::query("VACUUM").execute(pool).await.unwrap();
    sqlx::query("PRAGMA wal_checkpoint").fetch_one(pool).await.unwrap();
}
"#;
        let ast = syn::parse_file(source).unwrap();
        let listed = [(REPOSITORY, "compact", "VACUUM runs outside a transaction")];
        assert_eq!(
            file_violations_with(REPOSITORY, &ast, &[]).len(),
            1,
            "unlisted"
        );
        assert!(file_violations_with(REPOSITORY, &ast, &listed).is_empty());
        let other_file = [("crates/x/src/repository.rs", "compact", "elsewhere")];
        assert_eq!(file_violations_with(REPOSITORY, &ast, &other_file).len(), 1);
    }

    #[test]
    fn statements_inside_a_macro_body_count() {
        let source = r#"pub async fn load(pool: &SqlitePool) {
    tokio::try_join!(
        sqlx::query("UPDATE a").execute(pool),
        sqlx::query("SELECT b").fetch_all(pool),
    )
    .unwrap();
}
"#;
        assert_eq!(
            found(REPOSITORY, source),
            vec![pool(REPOSITORY, 1, "load", 2, 1)]
        );
    }

    #[test]
    fn methods_count_and_services_and_test_functions_are_not_checked() {
        let source = r#"impl Store {
    pub async fn rename(&self) {
        sqlx::query("UPDATE a").execute(&self.pool).await.unwrap();
        sqlx::query("SELECT b").fetch_one(self.pool()).await.unwrap();
    }
}
#[cfg(test)]
async fn helper(pool: &SqlitePool) {
    sqlx::query("UPDATE a").execute(pool).await.unwrap();
    sqlx::query("SELECT b").fetch_one(pool).await.unwrap();
}
"#;
        assert_eq!(
            found(REPOSITORY, source),
            vec![pool(REPOSITORY, 2, "rename", 2, 1)]
        );
        let service = r#"async fn rename(pool: &SqlitePool) {
    foo.execute(pool).await.unwrap();
    foo.fetch_one(pool).await.unwrap();
}
"#;
        assert_eq!(found(SERVICE, service), Vec::<String>::new());
        assert_eq!(
            found(REPOSITORY, service),
            vec![pool(REPOSITORY, 1, "rename", 2, 1)]
        );
    }

    #[test]
    fn data_modules_are_matched_by_segment_or_stem() {
        for path in [
            "crates/lattice-store/src/lib.rs",
            "crates/lattice-learning/src/lessons/repository.rs",
            "crates/lattice-learning/src/lessons/repository/claims.rs",
            "crates/lattice-learning/src/repositories/claims.rs",
            "crates/lattice-index/src/embedding_store.rs",
            "crates/lattice-index/src/embedding_store/rows.rs",
            "crates/lattice-index/src/lessons_repository.rs",
            "crates/lattice-core/src/shared/persistence/mod.rs",
            "crates/lattice-core/src/shared/storage.rs",
            "crates/lattice-core/src/shared/database/pool.rs",
            "crates/lattice-core/src/shared/migrations.rs",
            "crates/lattice-core/src/store/mod.rs",
        ] {
            assert!(is_data_module(path), "{path}");
        }
        for path in [
            "crates/lattice-learning/src/lessons/service.rs",
            "crates/lattice-learning/src/lessons/restore.rs",
            "crates/lattice-learning/src/lessons/storefront.rs",
            "crates/lattice-learning/src/lessons/store_things.rs",
            "crates/lattice-learning/src/lessons/repository_helpers.rs",
            "crates/lattice-desktop/src/interfaces/di/memory.rs",
        ] {
            assert!(!is_data_module(path), "{path}");
        }
    }
}

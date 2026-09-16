//! Extract production SQL literals with Rust's parser (ignores comments and test fixtures).
use std::{fs, path::Path};
use syn::{
    visit::{self, Visit},
    Attribute, Item,
};

fn test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.parse_args::<syn::Path>()
                .is_ok_and(|p| p.is_ident("test"))
    })
}
struct Sql<'a> {
    file: &'a str,
    statements: &'a mut Vec<serde_json::Value>,
    fragment: bool,
}
impl Sql<'_> {
    fn literal(&mut self, lit: &syn::LitStr) {
        let sql = lit.value();
        let text = sql.trim_start();
        if ![
            "SELECT ",
            "SELECT\n",
            "WITH ",
            "INSERT ",
            "UPDATE ",
            "DELETE ",
            "REPLACE ",
            "PRAGMA ",
            "CREATE TABLE ",
            "CREATE INDEX ",
            "CREATE UNIQUE INDEX ",
        ]
        .iter()
        .any(|prefix| text.starts_with(prefix))
        {
            return;
        }
        self.statements.push(serde_json::json!({"file":self.file,"line":lit.span().start().line,"sql":sql,"fragment":self.fragment}));
    }
    fn tokens(&mut self, tokens: proc_macro2::TokenStream) {
        for token in tokens {
            match token {
                proc_macro2::TokenTree::Literal(lit) => {
                    if let Ok(string) = syn::parse_str::<syn::LitStr>(&lit.to_string()) {
                        // Parsing a standalone token resets its span; restore the original location.
                        let mut string = string;
                        string.set_span(lit.span());
                        self.literal(&string);
                    }
                }
                proc_macro2::TokenTree::Group(group) => self.tokens(group.stream()),
                _ => {}
            }
        }
    }
}
impl<'ast> Visit<'ast> for Sql<'_> {
    fn visit_attribute(&mut self, _: &'ast Attribute) {} // Documentation is not executable SQL.
    fn visit_item(&mut self, item: &'ast Item) {
        let attrs = match item {
            Item::Fn(i) => &i.attrs,
            Item::Mod(i) => &i.attrs,
            Item::Impl(i) => &i.attrs,
            Item::Const(i) => &i.attrs,
            Item::Static(i) => &i.attrs,
            _ => {
                visit::visit_item(self, item);
                return;
            }
        };
        if !test_only(attrs) {
            visit::visit_item(self, item);
        }
    }
    fn visit_lit_str(&mut self, lit: &'ast syn::LitStr) {
        self.literal(lit);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.tokens(mac.tokens.clone());
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        let old = self.fragment;
        if let syn::Expr::Path(p) = call.func.as_ref() {
            self.fragment |= p.path.segments.iter().any(|s| s.ident == "QueryBuilder");
        }
        visit::visit_expr_call(self, call);
        self.fragment = old;
    }
}
fn scan(path: &Path, rows: &mut Vec<serde_json::Value>) -> Result<(), Box<dyn std::error::Error>> {
    if path.is_dir() {
        if path
            .file_name()
            .is_some_and(|s| s == "tests" || s == "mocks")
        {
            return Ok(());
        }
        let mut entries = fs::read_dir(path)?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort();
        for child in entries {
            scan(&child, rows)?;
        }
        return Ok(());
    }
    if path.extension().is_none_or(|s| s != "rs") {
        return Ok(());
    }
    let name = path.file_stem().unwrap().to_string_lossy();
    if name == "tests" || name == "mocks" || name.ends_with("_tests") || name.ends_with("_test") {
        return Ok(());
    }
    let ast = syn::parse_file(&fs::read_to_string(path)?)?;
    if !test_only(&ast.attrs) {
        Sql {
            file: &path.to_string_lossy(),
            statements: rows,
            fragment: false,
        }
        .visit_file(&ast);
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .ok_or("usage: lattice-data-contract-check <Rust source root>")?;
    let mut rows = Vec::new();
    scan(Path::new(&root), &mut rows)?;
    println!("{}", serde_json::to_string(&rows)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_executable_sql_and_excludes_documentation_and_tests() {
        let ast = syn::parse_file(
            r#"
            /// SELECT obsolete_column FROM obsolete_table
            fn query() {
                sqlx::query("SELECT file_name FROM documents");
                sqlx::query!("SELECT id FROM text_chunks");
                let builder = sqlx::QueryBuilder::new("SELECT id FROM documents WHERE id IN (");
                let formatted = format!("SELECT {COLUMNS} FROM documents");
            }
            #[cfg(test)] mod tests {
                fn fixture() { sqlx::query("SELECT name FROM files"); }
            }
        "#,
        )
        .unwrap();
        let mut rows = Vec::new();
        Sql {
            file: "fixture.rs",
            statements: &mut rows,
            fragment: false,
        }
        .visit_file(&ast);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["sql"], "SELECT file_name FROM documents");
        assert_eq!(rows[1]["sql"], "SELECT id FROM text_chunks");
        assert_eq!(rows[2]["fragment"], true);
        assert_eq!(rows[3]["sql"], "SELECT {COLUMNS} FROM documents");
    }
}

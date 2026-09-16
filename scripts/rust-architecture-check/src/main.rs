//! Source-level dependency guard. This is not a substitute for separate crates:
//! macro expansion and aliases exported elsewhere still require compiler boundaries.
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use syn::{
    visit::{self, Visit},
    Attribute, Item, UseTree,
};

fn test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.parse_args::<syn::Path>()
                .is_ok_and(|p| p.is_ident("test"))
    })
}

struct Dependencies<'a> {
    forbidden: &'a [&'a str],
    drivers: &'a [&'a str],
    violations: Vec<String>,
    aliases: Vec<String>,
}
impl Dependencies<'_> {
    fn check(&mut self, segments: &[String]) {
        if (segments.first().is_some_and(|s| s == "crate")
            && segments
                .get(1)
                .is_some_and(|s| self.forbidden.contains(&s.as_str())))
            || segments
                .first()
                .is_some_and(|s| self.drivers.contains(&s.as_str()))
        {
            self.violations.push(segments.join("::"));
        }
    }
    fn use_tree(&mut self, tree: &UseTree, mut prefix: Vec<String>) {
        match tree {
            UseTree::Path(p) => {
                prefix.push(p.ident.to_string());
                self.use_tree(&p.tree, prefix);
            }
            UseTree::Group(g) => {
                for item in &g.items {
                    self.use_tree(item, prefix.clone());
                }
            }
            UseTree::Name(n) => {
                prefix.push(n.ident.to_string());
                self.check(&prefix);
            }
            UseTree::Rename(n) => {
                prefix.push(n.ident.to_string());
                self.check(&prefix);
            }
            UseTree::Glob(_) => self.check(&prefix),
        }
    }
}
impl<'ast> Visit<'ast> for Dependencies<'_> {
    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        if attr.path().is_ident("derive") {
            if let Ok(paths) = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            ) {
                for path in paths {
                    self.check(
                        &path
                            .segments
                            .iter()
                            .map(|s| s.ident.to_string())
                            .collect::<Vec<_>>(),
                    );
                }
            }
        }
        visit::visit_attribute(self, attr);
    }
    fn visit_item(&mut self, item: &'ast Item) {
        let attrs = match item {
            Item::Const(i) => &i.attrs,
            Item::Enum(i) => &i.attrs,
            Item::ExternCrate(i) => &i.attrs,
            Item::Fn(i) => &i.attrs,
            Item::ForeignMod(i) => &i.attrs,
            Item::Impl(i) => &i.attrs,
            Item::Macro(i) => &i.attrs,
            Item::Mod(i) => &i.attrs,
            Item::Static(i) => &i.attrs,
            Item::Struct(i) => &i.attrs,
            Item::Trait(i) => &i.attrs,
            Item::TraitAlias(i) => &i.attrs,
            Item::Type(i) => &i.attrs,
            Item::Union(i) => &i.attrs,
            Item::Use(i) => &i.attrs,
            _ => {
                visit::visit_item(self, item);
                return;
            }
        };
        if !test_only(attrs) {
            visit::visit_item(self, item);
        }
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.use_tree(&item.tree, Vec::new());
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.check(
            &path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>(),
        );
        visit::visit_path(self, path);
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        for attr in &item.attrs {
            if attr.path().is_ident("path") {
                if let syn::Meta::NameValue(value) = &attr.meta {
                    if let syn::Expr::Lit(expr) = &value.value {
                        if let syn::Lit::Str(path) = &expr.lit {
                            self.aliases.push(path.value());
                        }
                    }
                }
            }
        }
        visit::visit_item_mod(self, item);
    }
}

#[cfg(test)]
fn inspect(source: &str, forbidden: &[&str]) -> Result<(Vec<String>, Vec<String>), syn::Error> {
    inspect_with_drivers(source, forbidden, &["sqlx", "axum", "tauri"])
}

fn inspect_with_drivers(
    source: &str,
    forbidden: &[&str],
    drivers: &[&str],
) -> Result<(Vec<String>, Vec<String>), syn::Error> {
    let ast = syn::parse_file(source)?;
    if test_only(&ast.attrs) {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut visitor = Dependencies {
        forbidden,
        drivers,
        violations: Vec::new(),
        aliases: Vec::new(),
    };
    visitor.visit_file(&ast);
    Ok((visitor.violations, visitor.aliases))
}

fn scan(
    path: &Path,
    forbidden: &[&str],
    visited: &mut HashSet<PathBuf>,
    failures: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    scan_with_drivers(
        path,
        forbidden,
        &["sqlx", "axum", "tauri"],
        visited,
        failures,
    )
}

fn scan_with_drivers(
    path: &Path,
    forbidden: &[&str],
    drivers: &[&str],
    visited: &mut HashSet<PathBuf>,
    failures: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.canonicalize()?;
    if !visited.insert(path.clone()) {
        return Ok(());
    }
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let child = entry?.path();
            if child.is_dir() || child.extension().is_some_and(|e| e == "rs") {
                scan_with_drivers(&child, forbidden, drivers, visited, failures)?;
            }
        }
        return Ok(());
    }
    let source = fs::read_to_string(&path)?;
    let (violations, aliases) = inspect_with_drivers(&source, forbidden, drivers)
        .map_err(|e| format!("{}: {}", path.display(), e))?;
    for dependency in violations {
        eprintln!("VIOLATION: {} imports {}", path.display(), dependency);
        *failures += 1;
    }
    for alias in aliases {
        let target = path.parent().unwrap().join(alias);
        if target.file_name().is_some_and(|n| n == "mod.rs") {
            // Fail closed for a missing entry point, even if its directory exists.
            target.canonicalize()?;
            scan_with_drivers(
                target.parent().unwrap(),
                forbidden,
                drivers,
                visited,
                failures,
            )?;
        } else {
            scan_with_drivers(&target, forbidden, drivers, visited, failures)?;
        }
    }
    // Ordinary out-of-line modules are part of the same boundary. Moving a
    // query into a child module must not make it invisible to the checker.
    let ast = syn::parse_file(&source)?;
    if !test_only(&ast.attrs) {
        let parent = path.parent().unwrap();
        let module_dir = match path.file_stem().and_then(|s| s.to_str()) {
            Some("mod" | "lib" | "main") => parent.to_path_buf(),
            Some(stem) => parent.join(stem),
            None => parent.to_path_buf(),
        };
        for item in ast.items {
            if let Item::Mod(module) = item {
                if module.content.is_none()
                    && !test_only(&module.attrs)
                    && !module.attrs.iter().any(|a| a.path().is_ident("path"))
                {
                    let file = module_dir.join(format!("{}.rs", module.ident));
                    let target = if file.exists() {
                        file
                    } else {
                        module_dir.join(module.ident.to_string()).join("mod.rs")
                    };
                    scan_with_drivers(&target, forbidden, drivers, visited, failures)?;
                }
            }
        }
    }
    Ok(())
}

fn check_feature_entrypoints(
    path: &Path,
    failures: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(path)? {
        let child = entry?.path();
        if child.is_dir() {
            check_feature_entrypoints(&child, failures)?;
        } else if child
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.starts_with("plugin") && name.ends_with(".rs"))
        {
            scan_with_drivers(&child, &[], &["sqlx"], &mut HashSet::new(), failures)?;
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected Rust source root")?,
    );
    let mut failures = 0;
    check_feature_entrypoints(&root.join("features"), &mut failures)?;
    for workflow in ["branching.rs", "synthesis.rs"] {
        scan_with_drivers(
            &root.join("features/conversation").join(workflow),
            &[],
            &["sqlx"],
            &mut HashSet::new(),
            &mut failures,
        )?;
    }
    scan_with_drivers(
        &root.join("features/conversation/repository/workspace"),
        &["interfaces"],
        &["tauri", "axum"],
        &mut HashSet::new(),
        &mut failures,
    )?;
    for (layer, forbidden) in [
        (
            "domain",
            &["application", "features", "infrastructure", "interfaces"][..],
        ),
        (
            "application",
            &["features", "infrastructure", "interfaces"][..],
        ),
    ] {
        scan(
            &root.join(layer),
            forbidden,
            &mut HashSet::new(),
            &mut failures,
        )?;
    }
    if let Some(api) = std::env::args_os().nth(2) {
        let api = PathBuf::from(api);
        for part in ["sync", "error.rs"] {
            scan(
                &api.join(part),
                &["http", "persistence"],
                &mut HashSet::new(),
                &mut failures,
            )?;
        }
    }
    if failures != 0 {
        return Err(format!("{failures} layer violations").into());
    }
    println!("Rust layer boundary check: clean (syntax-aware).");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resumes_after_test_items_and_ignores_comments_and_strings() {
        let (bad, _) = inspect(
            r#"
            // crate::infrastructure::Comment
            const TEXT: &str = "crate::infrastructure::String";
            #[cfg(test)] mod tests { use crate::infrastructure::Allowed; }
            use crate::{domain::Allowed, infrastructure::{Bad, AlsoBad as Alias}};
            fn production() { crate::infrastructure::run(); }
        "#,
            &["infrastructure"],
        )
        .unwrap();
        assert_eq!(bad.len(), 3);
    }
    #[test]
    fn only_production_path_aliases_are_followed() {
        let (_, paths) = inspect(
            r#"
            #[cfg(test)] #[path = "tests.rs"] mod tests;
            #[path = "owned.rs"] mod owned;
        "#,
            &[],
        )
        .unwrap();
        assert_eq!(paths, vec!["owned.rs"]);
    }
    #[test]
    fn invalid_rust_fails_closed() {
        assert!(inspect("fn broken(", &[]).is_err());
    }

    #[test]
    fn driver_derives_are_dependencies_too() {
        let (bad, _) = inspect("#[derive(sqlx::FromRow)] struct Record { id: i64 }", &[]).unwrap();
        assert_eq!(bad, vec!["sqlx::FromRow"]);
    }

    #[test]
    fn file_level_test_module_is_excluded() {
        let (bad, aliases) = inspect(
            "#![cfg(test)] use sqlx::Pool; #[path=\"fixture.rs\"] mod fixture;",
            &[],
        )
        .unwrap();
        assert!(bad.is_empty());
        assert!(aliases.is_empty());
    }
    #[test]
    fn plugin_boundary_catches_grouped_aliases_and_allows_transport() {
        let source = r#"
            use tauri::Window;
            #[cfg(test)] mod tests { use sqlx::query; }
            use sqlx::{query as select, QueryBuilder};
            #[derive(sqlx::FromRow)] struct Row { id: String }
        "#;
        let (violations, _) = inspect_with_drivers(source, &[], &["sqlx"]).unwrap();
        assert_eq!(violations.len(), 3);
    }

    #[test]
    fn plugin_boundary_follows_ordinary_child_modules() {
        let root =
            std::env::temp_dir().join(format!("lattice-architecture-{}", std::process::id()));
        fs::create_dir_all(root.join("plugin_impl")).unwrap();
        fs::write(root.join("plugin_impl.rs"), "mod hidden;\n").unwrap();
        fs::write(
            root.join("plugin_impl/hidden.rs"),
            "use sqlx::query as select;\n",
        )
        .unwrap();
        let mut failures = 0;
        scan_with_drivers(
            &root.join("plugin_impl.rs"),
            &[],
            &["sqlx"],
            &mut HashSet::new(),
            &mut failures,
        )
        .unwrap();
        assert_eq!(failures, 1);
        fs::remove_dir_all(root).unwrap();
    }
}

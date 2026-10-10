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

mod ratchet;

fn test_only(attrs: &[Attribute]) -> bool {
    fn excluded_from_production(meta: &syn::Meta) -> bool {
        match meta {
            syn::Meta::Path(path) => path.is_ident("test") || path.is_ident("doc"),
            syn::Meta::List(list) if list.path.is_ident("any") || list.path.is_ident("all") => {
                let Ok(items) = list.parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                ) else {
                    return false;
                };
                if list.path.is_ident("any") {
                    items.iter().all(excluded_from_production)
                } else {
                    items.iter().any(excluded_from_production)
                }
            }
            _ => false,
        }
    }
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .is_ok_and(|meta| excluded_from_production(&meta))
    })
}

/// Attributes of an item that can carry `#[cfg(...)]`.
fn item_attrs(item: &Item) -> Option<&[Attribute]> {
    Some(match item {
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
        _ => return None,
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
            || self.drivers.iter().any(|driver| {
                let prefix: Vec<_> = driver.split("::").collect();
                segments.len() >= prefix.len()
                    && segments
                        .iter()
                        .zip(prefix)
                        .all(|(actual, expected)| actual == expected)
            })
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
        if !item_attrs(item).is_some_and(test_only) {
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

// Empty tests inflate the pass count without exercising a contract.
fn empty_tests(source: &str) -> Result<Vec<String>, syn::Error> {
    #[derive(Default)]
    struct EmptyTests(Vec<String>);
    impl<'ast> Visit<'ast> for EmptyTests {
        fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
            let is_test = item
                .attrs
                .iter()
                .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"));
            let no_op = match item.block.stmts.as_slice() {
                [] => true,
                [syn::Stmt::Expr(syn::Expr::Call(call), _)] => {
                    matches!(&*call.func, syn::Expr::Path(path) if path.path.is_ident("Ok"))
                        && matches!(call.args.first(), Some(syn::Expr::Tuple(tuple)) if tuple.elems.is_empty())
                        && call.args.len() == 1
                }
                _ => false,
            };
            if is_test && no_op {
                self.0.push(item.sig.ident.to_string());
            }
            visit::visit_item_fn(self, item);
        }
    }
    let mut visitor = EmptyTests::default();
    visitor.visit_file(&syn::parse_file(source)?);
    Ok(visitor.0)
}

fn check_integration_tests(
    root: &Path,
    failures: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let tests = root
        .parent()
        .ok_or("source root has no parent")?
        .join("tests");
    let mut reachable = HashSet::new();
    for entry in fs::read_dir(&tests)? {
        let path = entry?.path();
        let target = if path.is_dir() {
            path.join("main.rs")
        } else {
            path
        };
        if target.is_file()
            && target
                .extension()
                .is_some_and(|extension| extension == "rs")
        {
            collect_test_modules(&target, true, &mut reachable)?;
        }
    }
    let mut directories = vec![tests];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                directories.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                if !reachable.contains(&path.canonicalize()?) {
                    eprintln!(
                        "VIOLATION: {} is not reachable from a Cargo test target",
                        path.display()
                    );
                    *failures += 1;
                }
                for name in empty_tests(&fs::read_to_string(&path)?)? {
                    eprintln!("VIOLATION: {} test {name} has no behavior", path.display());
                    *failures += 1;
                }
            }
        }
    }
    Ok(())
}

fn test_module_files(items: &[Item], module_dir: &Path, attribute_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for item in items {
        let Item::Mod(module) = item else { continue };
        let name = module.ident.to_string();
        if let Some((_, items)) = &module.content {
            files.extend(test_module_files(
                items,
                &module_dir.join(&name),
                &attribute_dir.join(&name),
            ));
            continue;
        }
        let explicit = module.attrs.iter().find_map(|attribute| {
            if !attribute.path().is_ident("path") {
                return None;
            }
            let syn::Meta::NameValue(value) = &attribute.meta else {
                return None;
            };
            let syn::Expr::Lit(value) = &value.value else {
                return None;
            };
            let syn::Lit::Str(value) = &value.lit else {
                return None;
            };
            Some(attribute_dir.join(value.value()))
        });
        files.push(explicit.unwrap_or_else(|| {
            let flat = module_dir.join(format!("{name}.rs"));
            if flat.exists() {
                flat
            } else {
                module_dir.join(name).join("mod.rs")
            }
        }));
    }
    files
}

fn collect_test_modules(
    path: &Path,
    crate_root: bool,
    reachable: &mut HashSet<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.canonicalize()?;
    if !reachable.insert(path.clone()) {
        return Ok(());
    }
    let parent = path.parent().ok_or("test source has no parent")?;
    let module_dir = if crate_root || path.file_stem().is_some_and(|name| name == "mod") {
        parent.to_path_buf()
    } else {
        parent.join(path.file_stem().ok_or("test source has no name")?)
    };
    let source = syn::parse_file(&fs::read_to_string(&path)?)?;
    for child in test_module_files(&source.items, &module_dir, parent) {
        collect_test_modules(&child, false, reachable)?;
    }
    Ok(())
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
    let mut write_baseline = false;
    let mut roots = Vec::new();
    for argument in std::env::args_os().skip(1) {
        if argument == "--write-baseline" {
            write_baseline = true;
        } else if argument.to_string_lossy().starts_with("--") {
            return Err(format!("unknown flag {}", argument.to_string_lossy()).into());
        } else {
            roots.push(PathBuf::from(argument));
        }
    }
    let root = roots.first().ok_or("expected Rust source root")?.clone();
    let mut failures = 0;
    ratchet::check(
        &root,
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("baseline.txt"),
        write_baseline,
        &mut failures,
    )?;
    check_integration_tests(&root, &mut failures)?;
    check_feature_entrypoints(&root.join("features"), &mut failures)?;
    let mut chat_workflows: Vec<PathBuf> = [
        "chat/turn_record.rs",
        "chat/tool_loop.rs",
        "chat/routing.rs",
        "chat/tools.rs",
    ]
    .iter()
    .map(|workflow| root.join("features/conversation").join(workflow))
    .collect();
    // The turn is split into stage modules; every stage is a workflow file.
    let mut turn_stages: Vec<PathBuf> = fs::read_dir(root.join("features/conversation/chat/turn"))
        .map_err(|error| format!("chat/turn stages: {error}"))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .filter(|path| path.file_name().is_some_and(|name| name != "tests.rs"))
        .collect();
    turn_stages.sort();
    chat_workflows.extend(turn_stages);
    for workflow in &chat_workflows {
        scan_with_drivers(workflow, &[], &["tauri"], &mut HashSet::new(), &mut failures)?;
    }
    scan_with_drivers(
        &root.join("features/learning/lessons/generation_jobs.rs"),
        &[],
        &["tauri"],
        &mut HashSet::new(),
        &mut failures,
    )?;
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
    scan_with_drivers(
        &root.join("shared/error"),
        &[
            "domain",
            "application",
            "features",
            "infrastructure",
            "interfaces",
        ],
        &["sqlx", "keyring", "ndarray", "tauri"],
        &mut HashSet::new(),
        &mut failures,
    )?;
    scan_with_drivers(
        &root.join("domain"),
        &[],
        &["std::fs", "std::env", "tokio::fs", "crate::shared::fs"],
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
    if let Some(api) = roots.get(1) {
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
    fn test_inventory_rejects_orphans_and_follows_nested_and_explicit_modules() {
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let scratch = Scratch(std::env::temp_dir().join(format!(
            "lattice-test-inventory-{}-{unique}",
            std::process::id()
        )));
        let source = scratch.0.join("src");
        let tests = scratch.0.join("tests");
        for directory in [
            &source,
            &tests.join("helpers"),
            &tests.join("inline"),
            &tests.join("support"),
        ] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            tests.join("suite.rs"),
            "mod helpers; mod inline { mod case; } #[path = \"support/custom.rs\"] mod mapped;",
        )
        .unwrap();
        fs::write(tests.join("helpers/mod.rs"), "mod fixtures;").unwrap();
        fs::write(tests.join("helpers/fixtures.rs"), "const FIXTURE: u32 = 1;").unwrap();
        fs::write(
            tests.join("inline/case.rs"),
            "#[test] fn real() { assert_eq!(1, 1); }",
        )
        .unwrap();
        fs::write(tests.join("support/custom.rs"), "const VALUE: bool = true;").unwrap();
        let orphan = tests.join("support/unreferenced.rs");
        fs::write(&orphan, "#[test] fn never_runs() { assert_eq!(1, 1); }").unwrap();
        let mut failures = 0;
        check_integration_tests(&source, &mut failures).unwrap();
        assert_eq!(failures, 1);
        fs::remove_file(orphan).unwrap();
        let mut failures = 0;
        check_integration_tests(&source, &mut failures).unwrap();
        assert_eq!(failures, 0);
    }

    #[test]
    fn rejects_empty_test_bodies_but_keeps_exercised_contracts() {
        assert_eq!(empty_tests("#[tokio::test] async fn stub() -> Result<()> { Ok(()) } #[test] fn empty() {} #[test] fn real() { exercise().unwrap(); }").unwrap(), vec!["stub", "empty"]);
    }

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
    fn domain_io_rule_checks_qualified_paths_and_grouped_imports() {
        let (bad, _) = inspect_with_drivers(
            r#"use std::{fs as disk, env};
                fn resolve() { crate::shared::fs::confinement::confine_to_root(); }
                #[cfg(test)] mod tests { use std::fs; }"#,
            &[],
            &["std::fs", "std::env", "crate::shared::fs"],
        )
        .unwrap();
        assert_eq!(bad.len(), 3);
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

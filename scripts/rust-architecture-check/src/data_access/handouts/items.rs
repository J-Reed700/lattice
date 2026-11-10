//! The handles the items of a file hand out, and where each is visible.
use super::{
    imports::{closure_handle, find_handle, Imports},
    resolve,
};
use crate::{
    item_attrs,
    ratchet::{impl_item_attrs, trait_item_attrs},
    test_only,
};
use quote::ToTokens;
use std::collections::HashMap;
use syn::{Ident, ImplItem, Item, Signature, TraitItem, Type, Visibility};

/// Where an item is visible, before the modules that enclose it narrow that.
#[derive(Clone)]
pub(super) enum Reach {
    /// Its type is declared in another file.
    Everywhere,
    Vis {
        scope: Vec<String>,
        public: bool,
    },
}

impl Reach {
    fn of(vis: &Visibility, module: &[String]) -> Self {
        Self::Vis {
            scope: resolve(vis, module),
            public: matches!(vis, Visibility::Public(_)),
        }
    }

    /// Where both are visible.
    fn meet(self, other: Self) -> Self {
        match (self, other) {
            (Self::Everywhere, other) | (other, Self::Everywhere) => other,
            (
                Self::Vis {
                    scope: first,
                    public: first_public,
                },
                Self::Vis {
                    scope: second,
                    public: second_public,
                },
            ) => Self::Vis {
                scope: if second.len() > first.len() {
                    second
                } else {
                    first
                },
                public: first_public && second_public,
            },
        }
    }
}

/// One handle an item hands out.
pub(super) struct Hand {
    pub module: Vec<String>,
    pub reach: Reach,
    pub handle: String,
    pub line: usize,
    pub kind: &'static str,
    pub name: String,
    /// The name a `use` reaches the item by.
    pub key: String,
}

/// Whether an owner's item is exempt: the owner, then the item.
pub(super) type Exempt<'a> = &'a dyn Fn(&str, &str) -> bool;

/// Where an item sits: its reach, its module, its owner for exemptions and its key.
struct Site<'a> {
    reach: Reach,
    module: &'a [String],
    owner: String,
    key: String,
}

struct Walker<'a> {
    imports: &'a Imports,
    exempt: Exempt<'a>,
    types: HashMap<(Vec<String>, String), Reach>,
    hands: Vec<Hand>,
}

/// Every handle the file's items hand out, wherever they are visible.
pub(super) fn hands(
    ast: &syn::File,
    module: &[String],
    imports: &Imports,
    exempt: Exempt,
) -> Vec<Hand> {
    let mut walk = Walker {
        imports,
        exempt,
        types: HashMap::new(),
        hands: Vec::new(),
    };
    let mut path = module.to_vec();
    walk.declare(&ast.items, &mut path);
    walk.items(&ast.items, &mut path);
    walk.hands
}

fn type_name(ty: &Type) -> String {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_default(),
        Type::Reference(reference) => type_name(&reference.elem),
        other => other.to_token_stream().to_string(),
    }
}

impl Walker<'_> {
    fn declare(&mut self, items: &[Item], module: &mut Vec<String>) {
        for item in items {
            if item_attrs(item).is_some_and(test_only) {
                continue;
            }
            let named = match item {
                Item::Struct(item) => Some((&item.ident, &item.vis)),
                Item::Enum(item) => Some((&item.ident, &item.vis)),
                Item::Union(item) => Some((&item.ident, &item.vis)),
                Item::Type(item) => Some((&item.ident, &item.vis)),
                Item::Trait(item) => Some((&item.ident, &item.vis)),
                Item::Mod(item) => {
                    if let Some((_, inner)) = &item.content {
                        module.push(item.ident.to_string());
                        self.declare(inner, module);
                        module.pop();
                    }
                    None
                }
                _ => None,
            };
            if let Some((ident, vis)) = named {
                let reach = Reach::of(vis, module);
                self.types
                    .insert((module.clone(), ident.to_string()), reach);
            }
        }
    }

    fn push(&mut self, site: &Site, found: (String, usize), kind: &'static str, name: String) {
        self.hands.push(Hand {
            module: site.module.to_vec(),
            reach: site.reach.clone(),
            handle: found.0,
            line: found.1,
            kind,
            name,
            key: site.key.clone(),
        });
    }

    fn exempted(&self, site: &Site, item: &str) -> bool {
        (self.exempt)(site.owner.as_str(), item)
    }

    fn function(&mut self, site: &Site, signature: &Signature) {
        let name = signature.ident.to_string();
        if self.exempted(site, &name) {
            return;
        }
        let returned = match &signature.output {
            syn::ReturnType::Type(_, ty) => find_handle(self.imports, ty),
            syn::ReturnType::Default => None,
        };
        let found = returned.or_else(|| closure_handle(self.imports, signature));
        if let Some((handle, _)) = found {
            let line = signature.ident.span().start().line;
            self.push(site, (handle, line), "fn", name);
        }
    }

    fn typed(&mut self, site: &Site, ty: &Type, kind: &'static str, ident: &Ident) {
        let name = ident.to_string();
        if self.exempted(site, &name) {
            return;
        }
        if let Some(found) = find_handle(self.imports, ty) {
            self.push(site, found, kind, name);
        }
    }

    fn fields(
        &mut self,
        fields: &syn::Fields,
        names: (&str, &str),
        shared: Option<&Visibility>,
        module: &[String],
    ) {
        let (key, owner) = names;
        for (index, field) in fields.iter().enumerate() {
            let Some(found) = find_handle(self.imports, &field.ty) else {
                continue;
            };
            let site = Site {
                reach: Reach::of(shared.unwrap_or(&field.vis), module),
                module,
                owner: String::new(),
                key: key.to_string(),
            };
            let name = match &field.ident {
                Some(ident) => format!("{owner}.{ident}"),
                None => format!("{owner}.{index}"),
            };
            self.push(&site, found, "field", name);
        }
    }

    fn declaration(&mut self, item: &syn::ItemTrait, module: &[String]) {
        let site = Site {
            reach: Reach::of(&item.vis, module),
            module,
            owner: format!("trait {}", item.ident),
            key: item.ident.to_string(),
        };
        for member in &item.items {
            if trait_item_attrs(member).is_some_and(test_only) {
                continue;
            }
            if let TraitItem::Fn(f) = member {
                self.function(&site, &f.sig);
            }
        }
    }

    /// Where an impl's items are visible: where its self type, and its trait when the
    /// file declares it, are.
    fn impl_reach(&self, item: &syn::ItemImpl, path: &syn::Path, module: &[String]) -> Reach {
        let declared = |candidate: &syn::Path| -> Option<Reach> {
            if candidate.segments.len() != 1 {
                return None;
            }
            let segment = candidate.segments.first()?;
            self.types
                .get(&(module.to_vec(), segment.ident.to_string()))
                .cloned()
        };
        let own = match &*item.self_ty {
            Type::Path(ty) if ty.qself.is_none() => declared(&ty.path),
            _ => None,
        }
        .unwrap_or(Reach::Everywhere);
        match declared(path) {
            Some(declared_trait) => own.meet(declared_trait),
            None => own,
        }
    }

    /// The handles a trait impl names outside its items: the trait's generic arguments
    /// (a `From` or `TryFrom` consumes its argument) and, for a conversion, its self type.
    fn named_by_impl(&mut self, item: &syn::ItemImpl, path: &syn::Path, site: &Site) {
        let Some(last) = path.segments.last() else {
            return;
        };
        if self.exempted(site, "*") {
            return;
        }
        let label = format!("{} for {}", last.ident, site.key);
        if last.ident == "From" || last.ident == "TryFrom" {
            if let Some(found) = find_handle(self.imports, &item.self_ty) {
                self.push(site, found, "impl", label);
            }
            return;
        }
        let syn::PathArguments::AngleBracketed(arguments) = &last.arguments else {
            return;
        };
        for argument in &arguments.args {
            if let syn::GenericArgument::Type(ty) = argument {
                if let Some(found) = find_handle(self.imports, ty) {
                    self.push(site, found, "impl", label.clone());
                }
            }
        }
    }

    fn implementation(&mut self, item: &syn::ItemImpl, module: &[String]) {
        let target = type_name(&item.self_ty);
        let (owner, shared) = match &item.trait_ {
            None => (format!("impl {target}"), None),
            Some((_, path, _)) => {
                let name = path
                    .segments
                    .last()
                    .map(|segment| segment.ident.to_string())
                    .unwrap_or_default();
                (
                    format!("impl {name} for {target}"),
                    Some(self.impl_reach(item, path, module)),
                )
            }
        };
        let site = |reach: Reach| Site {
            reach,
            module,
            owner: owner.clone(),
            key: target.clone(),
        };
        for member in &item.items {
            if impl_item_attrs(member).is_some_and(test_only) {
                continue;
            }
            let own = |vis: &Visibility| shared.clone().unwrap_or_else(|| Reach::of(vis, module));
            match member {
                ImplItem::Fn(f) => self.function(&site(own(&f.vis)), &f.sig),
                ImplItem::Const(c) => self.typed(&site(own(&c.vis)), &c.ty, "const", &c.ident),
                ImplItem::Type(t) => self.typed(&site(own(&t.vis)), &t.ty, "type", &t.ident),
                _ => {}
            }
        }
        if let (Some((_, path, _)), Some(reach)) = (&item.trait_, shared) {
            self.named_by_impl(item, path, &site(reach));
        }
    }

    fn items(&mut self, items: &[Item], module: &mut Vec<String>) {
        for item in items {
            if item_attrs(item).is_some_and(test_only) {
                continue;
            }
            match item {
                Item::Fn(item) => {
                    let site = Site {
                        reach: Reach::of(&item.vis, module),
                        module,
                        owner: String::new(),
                        key: item.sig.ident.to_string(),
                    };
                    self.function(&site, &item.sig);
                }
                Item::Impl(item) => self.implementation(item, module),
                Item::Trait(item) => self.declaration(item, module),
                Item::Struct(item) => {
                    let name = item.ident.to_string();
                    self.fields(&item.fields, (name.as_str(), name.as_str()), None, module);
                }
                Item::Enum(item) => {
                    let key = item.ident.to_string();
                    for variant in &item.variants {
                        let owner = format!("{}::{}", item.ident, variant.ident);
                        self.fields(
                            &variant.fields,
                            (key.as_str(), owner.as_str()),
                            Some(&item.vis),
                            module,
                        );
                    }
                }
                Item::Type(item) => {
                    self.item_typed(&item.vis, &item.ty, "alias", &item.ident, module)
                }
                Item::Const(item) => {
                    self.item_typed(&item.vis, &item.ty, "const", &item.ident, module)
                }
                Item::Static(item) => {
                    self.item_typed(&item.vis, &item.ty, "static", &item.ident, module);
                }
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

    fn item_typed(
        &mut self,
        vis: &Visibility,
        ty: &Type,
        kind: &'static str,
        ident: &Ident,
        module: &[String],
    ) {
        let site = Site {
            reach: Reach::of(vis, module),
            module,
            owner: String::new(),
            key: ident.to_string(),
        };
        self.typed(&site, ty, kind, ident);
    }
}

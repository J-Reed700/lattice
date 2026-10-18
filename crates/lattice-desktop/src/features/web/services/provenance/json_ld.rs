//! schema.org JSON-LD blocks: who published a page, who wrote it, when, and
//! what kind of thing it says it is (§4).
//!
//! Every block is parsed as JSON; a block that does not parse is skipped, a
//! value of an unexpected shape is ignored, and nothing a block says is
//! followed anywhere: `@id` references resolve within the page only.

use std::collections::HashMap;

use lattice_core::application::contracts::web::NamedEntity;
use scraper::{Html, Selector};
use serde_json::{Map, Value};

use super::metadata::{absolute_http, clean_text};

type Node = Map<String, Value>;

/// The script type JSON-LD is declared with.
const JSON_LD_TYPE: &str = "application/ld+json";

/// Prefixes a type may be written with; the local name is what is kept.
const SCHEMA_PREFIXES: &[&str] = &["https://schema.org/", "http://schema.org/", "schema:"];

/// schema.org's direct `Organization` subtypes whose names do not end in
/// "Organization" (those that do are recognised by the suffix).
const ORGANIZATION_TYPES: &[&str] = &[
    "Airline",
    "Consortium",
    "Corporation",
    "FundingScheme",
    "LibrarySystem",
    "LocalBusiness",
    "NGO",
    "OnlineBusiness",
    "PerformingGroup",
    "PoliticalParty",
    "Project",
    "WorkersUnion",
];

/// Fields that mark a node as the page's content (an article, a post, a
/// report) rather than the site around it.
const CONTENT_FIELDS: &[&str] = &["datePublished", "headline", "author", "dateCreated"];

/// The JSON-LD of one page, its nodes in document order with content nodes
/// first.
pub(super) struct JsonLd {
    nodes: Vec<Node>,
    by_id: HashMap<String, Value>,
}

impl JsonLd {
    pub(super) fn read(document: &Html) -> Self {
        let mut nodes = Vec::new();
        let mut by_id = HashMap::new();
        if let Ok(selector) = Selector::parse("script[type]") {
            for script in document.select(&selector) {
                let declared = script.value().attr("type").unwrap_or_default();
                if !declared
                    .split(';')
                    .next()
                    .is_some_and(|kind| kind.trim().eq_ignore_ascii_case(JSON_LD_TYPE))
                {
                    continue;
                }
                let body: String = script.text().collect();
                if let Ok(value) = serde_json::from_str::<Value>(unwrap_markup(&body)) {
                    collect_nodes(&value, &mut nodes);
                    index_ids(&value, &mut by_id);
                }
            }
        }
        let (content, rest): (Vec<Node>, Vec<Node>) = nodes
            .into_iter()
            .partition(|node| CONTENT_FIELDS.iter().any(|field| node.contains_key(*field)));
        Self {
            nodes: content.into_iter().chain(rest).collect(),
            by_id,
        }
    }

    /// Every declared `@type`, local names, first occurrence order.
    pub(super) fn types(&self) -> Vec<String> {
        let mut types: Vec<String> = Vec::new();
        for kind in self.nodes.iter().flat_map(node_types) {
            if !types.contains(&kind) {
                types.push(kind);
            }
        }
        types
    }

    /// The first node's value for `field` that yields something.
    fn first<T>(&self, field: &str, read: impl Fn(&Value) -> Option<T>) -> Option<T> {
        self.nodes
            .iter()
            .filter_map(|node| node.get(field))
            .find_map(|value| read(self.resolve(value)))
    }

    pub(super) fn publisher(&self, base: &url::Url) -> Option<NamedEntity> {
        self.first("publisher", |value| self.entity(value, base))
    }

    pub(super) fn author(&self, base: &url::Url) -> Option<NamedEntity> {
        self.first("author", |value| self.entity(value, base))
            .or_else(|| self.first("creator", |value| self.entity(value, base)))
    }

    /// The first node typed as an organisation: a site-wide publisher
    /// declaration when no content node names one.
    pub(super) fn organization(&self, base: &url::Url) -> Option<NamedEntity> {
        self.nodes
            .iter()
            .find(|node| node_types(node).iter().any(|kind| is_organization(kind)))
            .and_then(|node| node_entity(node, base))
    }

    /// The raw text of a date field, as written.
    pub(super) fn date(&self, field: &str) -> Option<String> {
        self.first(field, text_value)
    }

    pub(super) fn headline(&self) -> Option<String> {
        self.first("headline", text_value)
    }

    pub(super) fn language(&self) -> Option<String> {
        self.first("inLanguage", |value| {
            text_value(value).or_else(|| value.get("name").and_then(text_value))
        })
    }

    pub(super) fn license(&self, base: &url::Url) -> Option<String> {
        self.first("license", |value| {
            text_value(value)
                .or_else(|| value.get("url").and_then(text_value))
                .or_else(|| value.get("name").and_then(text_value))
                .map(|license| absolute_http(base, &license).unwrap_or(license))
        })
    }

    /// The name of the `WebSite` node, the site's own name.
    pub(super) fn site_name(&self) -> Option<String> {
        self.nodes
            .iter()
            .find(|node| node_types(node).iter().any(|kind| kind == "WebSite"))
            .and_then(|node| node.get("name"))
            .and_then(text_value)
    }

    /// An untyped reference (`{"@id": …}`, perhaps with a name), resolved
    /// to the typed node it names on this page; anything else as it is.
    fn resolve<'a>(&'a self, value: &'a Value) -> &'a Value {
        match value.as_object() {
            Some(object) if !object.contains_key("@type") => object
                .get("@id")
                .and_then(Value::as_str)
                .and_then(|id| self.by_id.get(id))
                .unwrap_or(value),
            _ => value,
        }
    }

    fn entity(&self, value: &Value, base: &url::Url) -> Option<NamedEntity> {
        match value {
            Value::String(name) => {
                let name = clean_text(name)?;
                Some(NamedEntity {
                    url: absolute_http(base, &name).filter(|_| looks_like_url(&name)),
                    name,
                    kind: None,
                })
            }
            Value::Object(node) => node_entity(node, base),
            Value::Array(values) => values
                .iter()
                .find_map(|value| self.entity(self.resolve(value), base)),
            _ => None,
        }
    }
}

fn node_entity(node: &Node, base: &url::Url) -> Option<NamedEntity> {
    let name = node
        .get("name")
        .and_then(text_value)
        .or_else(|| node.get("legalName").and_then(text_value))
        .or_else(|| {
            let given = node.get("givenName").and_then(text_value)?;
            let family = node.get("familyName").and_then(text_value);
            clean_text(&format!("{given} {}", family.unwrap_or_default()))
        })?;
    let url = node
        .get("url")
        .and_then(text_value)
        .and_then(|url| absolute_http(base, &url));
    Some(NamedEntity {
        name,
        url,
        kind: node_types(node).into_iter().next(),
    })
}

fn looks_like_url(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn is_organization(kind: &str) -> bool {
    kind.ends_with("Organization") || ORGANIZATION_TYPES.contains(&kind)
}

/// A string, a language-tagged `{"@value": …}`, or the first of an array.
fn text_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => clean_text(text),
        Value::Object(object) => object.get("@value").and_then(text_value),
        Value::Array(values) => values.iter().find_map(text_value),
        _ => None,
    }
}

fn node_types(node: &Node) -> Vec<String> {
    let local = |kind: &str| {
        let kind = SCHEMA_PREFIXES
            .iter()
            .find_map(|prefix| kind.strip_prefix(prefix))
            .unwrap_or(kind);
        clean_text(kind)
    };
    match node.get("@type") {
        Some(Value::String(kind)) => local(kind).into_iter().collect(),
        Some(Value::Array(kinds)) => kinds
            .iter()
            .filter_map(Value::as_str)
            .filter_map(local)
            .collect(),
        _ => Vec::new(),
    }
}

/// Top-level nodes: an object, each object of an array, and each object of
/// a `@graph`.
fn collect_nodes(value: &Value, nodes: &mut Vec<Node>) {
    match value {
        Value::Array(values) => values.iter().for_each(|value| collect_nodes(value, nodes)),
        Value::Object(object) => {
            if let Some(graph) = object.get("@graph") {
                collect_nodes(graph, nodes);
            }
            if object
                .keys()
                .any(|key| key != "@context" && key != "@graph")
            {
                nodes.push(object.clone());
            }
        }
        _ => {}
    }
}

/// Every typed object anywhere in the block that names itself with `@id`,
/// so a reference elsewhere can be resolved. The first typed declaration of
/// an id wins.
fn index_ids(value: &Value, by_id: &mut HashMap<String, Value>) {
    match value {
        Value::Array(values) => values.iter().for_each(|value| index_ids(value, by_id)),
        Value::Object(object) => {
            if let Some(id) = object.get("@id").and_then(Value::as_str) {
                if object.contains_key("@type") && !by_id.contains_key(id) {
                    by_id.insert(id.to_string(), value.clone());
                }
            }
            object.values().for_each(|value| index_ids(value, by_id));
        }
        _ => {}
    }
}

/// Some sites wrap a block in an HTML comment or CDATA section.
fn unwrap_markup(body: &str) -> &str {
    let mut text = body.trim();
    for (open, close) in [
        ("<!--", "-->"),
        ("//<![CDATA[", "//]]>"),
        ("<![CDATA[", "]]>"),
    ] {
        if let Some(inner) = text
            .strip_prefix(open)
            .and_then(|rest| rest.strip_suffix(close))
        {
            text = inner.trim();
        }
    }
    text
}

//! What a page's markup says about itself: `<head>` tags, OpenGraph,
//! Dublin Core, microdata and JSON-LD (§4).
//!
//! Everything here is parsed from elements and attributes, never guessed
//! from prose, and every value is kept as data: a page that calls itself
//! official, or writes instructions into its description, is recorded saying
//! so and nothing more (§14).

use lattice_core::application::contracts::web::{DateBasis, NamedEntity};
use scraper::{ElementRef, Html, Selector};
use url::Url;

use super::dates::{self, ChosenDate, DateField};
use super::json_ld::JsonLd;

/// `meta` keys that name the date a page was published, after JSON-LD and
/// `article:published_time`: microdata, Dublin Core, Highwire Press.
const PUBLISHED_META_KEYS: &[&str] = &[
    "datepublished",
    "date",
    "dc.date",
    "dc.date.issued",
    "dcterms.date",
    "dcterms.issued",
    "dcterms.created",
    "citation_publication_date",
    "citation_date",
    "pubdate",
    "publishdate",
];
const MODIFIED_META_KEYS: &[&str] = &[
    "datemodified",
    "dc.date.modified",
    "dcterms.modified",
    "http-equiv:last-modified",
];
const AUTHOR_META_KEYS: &[&str] = &["author", "dc.creator", "dcterms.creator", "citation_author"];
const PUBLISHER_META_KEYS: &[&str] = &["dc.publisher", "dcterms.publisher", "citation_publisher"];
/// Language declarations after `<html lang>`, `og:locale` and JSON-LD
/// `inLanguage`.
const LANGUAGE_META_KEYS: &[&str] = &[
    "http-equiv:content-language",
    "dc.language",
    "dcterms.language",
    "language",
];

/// What the page's markup declared. Absent fields were not declared.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PageMetadata {
    pub title: Option<String>,
    pub site_name: Option<String>,
    pub publisher: Option<NamedEntity>,
    pub author: Option<NamedEntity>,
    pub declared_types: Vec<String>,
    pub language: Option<String>,
    pub generator: Option<String>,
    pub license: Option<String>,
    pub description: Option<String>,
    /// `rel=canonical`, absolute.
    pub canonical: Option<String>,
    /// `og:url`, absolute.
    pub og_url: Option<String>,
    pub published: ChosenDate,
    pub modified: ChosenDate,
}

/// Read the metadata of `document`, served from `page_url`; relative URLs
/// resolve against it.
pub(crate) fn read_metadata(document: &Html, page_url: &Url) -> PageMetadata {
    let meta = MetaTags::read(document);
    let json_ld = JsonLd::read(document);
    let mut declared_types = json_ld.types();
    if let Some(og_type) = meta.first(&["og:type"]) {
        if !declared_types.contains(&og_type) {
            declared_types.push(og_type);
        }
    }
    PageMetadata {
        title: first_text(document, "head title")
            .or_else(|| first_text(document, "title"))
            .or_else(|| meta.first(&["og:title", "twitter:title"]))
            .or_else(|| json_ld.headline()),
        site_name: meta
            .first(&["og:site_name", "application-name"])
            .or_else(|| json_ld.site_name()),
        publisher: json_ld
            .publisher(page_url)
            .or_else(|| meta.entity(PUBLISHER_META_KEYS, page_url))
            .or_else(|| microdata_entity(document, "publisher", page_url))
            .or_else(|| json_ld.organization(page_url)),
        author: json_ld
            .author(page_url)
            .or_else(|| meta.entity(AUTHOR_META_KEYS, page_url))
            .or_else(|| meta.entity(&["article:author"], page_url))
            .or_else(|| microdata_entity(document, "author", page_url))
            .or_else(|| rel_author(document, page_url)),
        declared_types,
        language: first_attr(document, "html[lang]", "lang")
            .or_else(|| meta.first(&["og:locale"]))
            .or_else(|| json_ld.language())
            .or_else(|| meta.first(LANGUAGE_META_KEYS)),
        generator: meta.first(&["generator"]),
        license: json_ld
            .license(page_url)
            .or_else(|| rel_href(document, "license", page_url))
            .or_else(|| meta.first(&["dcterms.license"])),
        description: meta.first(&["description", "og:description", "twitter:description"]),
        canonical: rel_href(document, "canonical", page_url),
        og_url: meta
            .first(&["og:url"])
            .and_then(|url| absolute_http(page_url, &url)),
        published: dates::choose(published_fields(&meta, &json_ld, document)),
        modified: dates::choose(modified_fields(&meta, &json_ld, document)),
    }
}

fn published_fields(meta: &MetaTags, json_ld: &JsonLd, document: &Html) -> Vec<DateField> {
    let mut fields = Vec::new();
    push_field(
        &mut fields,
        "json_ld:datePublished",
        json_ld.date("datePublished"),
        DateBasis::JsonLdDatePublished,
    );
    push_meta_fields(
        &mut fields,
        meta,
        &["article:published_time"],
        &DateBasis::ArticlePublishedTime,
    );
    push_meta_fields(&mut fields, meta, PUBLISHED_META_KEYS, &DateBasis::MetaDate);
    for css in [
        "time[itemprop~=datePublished][datetime]",
        "time[pubdate][datetime]",
        "article time[datetime]",
    ] {
        push_field(
            &mut fields,
            css,
            first_attr(document, css, "datetime"),
            DateBasis::TimeElement,
        );
    }
    fields
}

fn modified_fields(meta: &MetaTags, json_ld: &JsonLd, document: &Html) -> Vec<DateField> {
    let mut fields = Vec::new();
    push_field(
        &mut fields,
        "json_ld:dateModified",
        json_ld.date("dateModified"),
        DateBasis::JsonLdDateModified,
    );
    push_meta_fields(
        &mut fields,
        meta,
        &["article:modified_time", "og:updated_time"],
        &DateBasis::ArticleModifiedTime,
    );
    push_meta_fields(&mut fields, meta, MODIFIED_META_KEYS, &DateBasis::MetaDate);
    let css = "time[itemprop~=dateModified][datetime]";
    push_field(
        &mut fields,
        css,
        first_attr(document, css, "datetime"),
        DateBasis::TimeElement,
    );
    fields
}

fn push_field(fields: &mut Vec<DateField>, field: &str, raw: Option<String>, basis: DateBasis) {
    if let Some(raw) = raw {
        fields.push(DateField {
            field: field.to_string(),
            raw,
            basis,
        });
    }
}

fn push_meta_fields(
    fields: &mut Vec<DateField>,
    meta: &MetaTags,
    keys: &[&str],
    basis: &DateBasis,
) {
    for key in keys {
        push_field(
            fields,
            &format!("meta:{key}"),
            meta.first(&[key]),
            basis.clone(),
        );
    }
}

/// Every `meta` tag's content by key: `name`, `property` and `itemprop`
/// lower-cased, `http-equiv` as `http-equiv:<value>`. Document order.
struct MetaTags(Vec<(String, String)>);

impl MetaTags {
    fn read(document: &Html) -> Self {
        let mut tags = Vec::new();
        for element in select(document, "meta[content]") {
            let Some(content) = element.value().attr("content") else {
                continue;
            };
            let attrs = element.value();
            let keys = ["name", "property", "itemprop"]
                .into_iter()
                .filter_map(|attr| attrs.attr(attr).map(|key| key.trim().to_ascii_lowercase()))
                .chain(
                    attrs
                        .attr("http-equiv")
                        .map(|key| format!("http-equiv:{}", key.trim().to_ascii_lowercase())),
                );
            for key in keys {
                tags.push((key, content.to_string()));
            }
        }
        Self(tags)
    }

    /// The first non-empty value under any of `keys`, by key order.
    fn first(&self, keys: &[&str]) -> Option<String> {
        keys.iter().find_map(|key| {
            self.0
                .iter()
                .filter(|(name, _)| name == key)
                .find_map(|(_, content)| clean_text(content))
        })
    }

    /// A named entity from a text tag; a value that is a URL is kept as the
    /// entity's URL as well (`article:author` is often a profile link).
    fn entity(&self, keys: &[&str], base: &Url) -> Option<NamedEntity> {
        let name = self.first(keys)?;
        let url = (name.starts_with("http://") || name.starts_with("https://"))
            .then(|| absolute_http(base, &name))
            .flatten();
        Some(NamedEntity {
            name,
            url,
            kind: None,
        })
    }
}

/// A microdata entity, `itemprop="author"` / `"publisher"`: its `name`
/// property, else its own text when it has no nested properties.
fn microdata_entity(document: &Html, property: &str, base: &Url) -> Option<NamedEntity> {
    let element = select(document, &format!("[itemprop~={property}]"))
        .into_iter()
        .next()?;
    let nested = |name: &str| {
        select_in(element, &format!("[itemprop~={name}]"))
            .into_iter()
            .next()
    };
    let name = match nested("name") {
        Some(name) => property_value(name),
        None if element.value().name() == "meta" => {
            element.value().attr("content").and_then(clean_text)
        }
        None if nested_properties(element) => None,
        None => clean_text(&element.text().collect::<String>()),
    }?;
    let url = nested("url")
        .and_then(|url| {
            url.value()
                .attr("href")
                .or_else(|| url.value().attr("content"))
        })
        .or_else(|| element.value().attr("href"))
        .and_then(|href| absolute_http(base, href));
    let kind = element
        .value()
        .attr("itemtype")
        .and_then(|kind| kind.split(['/', ' ']).rfind(|part| !part.is_empty()))
        .and_then(clean_text);
    Some(NamedEntity { name, url, kind })
}

fn nested_properties(element: ElementRef<'_>) -> bool {
    !select_in(element, "[itemprop]").is_empty()
}

fn property_value(element: ElementRef<'_>) -> Option<String> {
    element
        .value()
        .attr("content")
        .and_then(clean_text)
        .or_else(|| clean_text(&element.text().collect::<String>()))
}

/// `<a rel="author">`: its text names the author, its target is their page.
fn rel_author(document: &Html, base: &Url) -> Option<NamedEntity> {
    rel_elements(document, "author")
        .into_iter()
        .find_map(|element| {
            let name = element
                .value()
                .attr("title")
                .and_then(clean_text)
                .or_else(|| clean_text(&element.text().collect::<String>()))?;
            let url = element
                .value()
                .attr("href")
                .and_then(|href| absolute_http(base, href));
            Some(NamedEntity {
                name,
                url,
                kind: None,
            })
        })
}

/// The first absolute http(s) target of a `link` or `a` with `rel` token
/// `rel`.
fn rel_href(document: &Html, rel: &str, base: &Url) -> Option<String> {
    rel_elements(document, rel)
        .into_iter()
        .filter_map(|element| element.value().attr("href"))
        .find_map(|href| absolute_http(base, href))
}

/// `link` and `a` elements whose space-separated `rel` holds `token`,
/// compared without case as HTML does.
fn rel_elements<'a>(document: &'a Html, token: &str) -> Vec<ElementRef<'a>> {
    select(document, "link[rel], a[rel]")
        .into_iter()
        .filter(|element| {
            element.value().attr("rel").is_some_and(|rel| {
                rel.split_ascii_whitespace()
                    .any(|value| value.eq_ignore_ascii_case(token))
            })
        })
        .collect()
}

fn select<'a>(document: &'a Html, css: &str) -> Vec<ElementRef<'a>> {
    Selector::parse(css)
        .map(|selector| document.select(&selector).collect())
        .unwrap_or_default()
}

fn select_in<'a>(element: ElementRef<'a>, css: &str) -> Vec<ElementRef<'a>> {
    Selector::parse(css)
        .map(|selector| element.select(&selector).collect())
        .unwrap_or_default()
}

fn first_text(document: &Html, css: &str) -> Option<String> {
    select(document, css)
        .into_iter()
        .find_map(|element| clean_text(&element.text().collect::<String>()))
}

fn first_attr(document: &Html, css: &str, attr: &str) -> Option<String> {
    select(document, css)
        .into_iter()
        .find_map(|element| element.value().attr(attr).and_then(clean_text))
}

/// A declared value as data: control characters removed, whitespace
/// collapsed, `None` when nothing is left. Nothing else is changed.
pub(super) fn clean_text(text: &str) -> Option<String> {
    let visible: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let collapsed = visible.split_whitespace().collect::<Vec<_>>().join(" ");
    (!collapsed.is_empty()).then_some(collapsed)
}

/// `href` resolved against `base`, kept only when it is http(s).
pub(super) fn absolute_http(base: &Url, href: &str) -> Option<String> {
    let url = base.join(href.trim()).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

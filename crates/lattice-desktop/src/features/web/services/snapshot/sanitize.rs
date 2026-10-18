//! The sanitizer (§6.3, §6.4, §8.3): the rule table both passes read, pass 1
//! (collect resource URLs, read-only) and pass 2 (rewrite into an inert,
//! self-contained document). Both run on lol_html, which passes everything
//! it is not told to touch through byte-for-byte.
//!
//! Pass 2 is [`sanitize_with_resources`]: no network, so it is unit-tested
//! with a hand-built [`ResolvedResources`].

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use chrono::{DateTime, Utc};
use lol_html::html_content::{ContentType, Element, TextChunk};
use lol_html::{doc_comments, doc_text, doctype, element, rewrite_str, text, RewriteStrSettings};
use url::Url;

use super::clutter;
use super::css::{self, CssInliner, CssRefKind};
use super::resources::{
    resolve_reference, ResolvedResource, ResolvedResources, ResourceKind, ResourceRef, CSS_MIME,
};
use super::{MAX_EMITTED_BYTES, PLACEHOLDER_DATA_URL_LEN};
use crate::features::web::traits::page_snapshot::{CaptureRequest, DataUri};

mod archival;
mod collect;
mod srcset;

pub use archival::synthesize_archival_html;
pub(super) use collect::collect;
pub(super) use srcset::choose_srcset;

const SVG_NS: &str = "http://www.w3.org/2000/svg";

// ---------------------------------------------------------------------------
// The rule table
// ---------------------------------------------------------------------------

/// Removed with everything inside them (§6.4). `link`, `meta`, `template`,
/// `dialog`, `form` and `noscript` have their own rules in [`element_action`].
const REMOVED_WITH_CONTENT: &[&str] = &[
    "script",
    "iframe",
    "frame",
    "frameset",
    "object",
    "embed",
    "applet",
    "portal",
    "fencedframe",
    "input",
    "button",
    "select",
    "textarea",
    "datalist",
    "option",
    "base",
    "audio",
    "source",
    "track",
    "canvas",
    "marquee",
    "blink",
    // SVG (tag names arrive lowercased): SMIL animation and foreign content.
    "animate",
    "animatemotion",
    "animatetransform",
    "set",
    "discard",
    "foreignobject",
];

/// `meta[name]` values only the envelope may declare.
const ENVELOPE_META_NAMES: &[&str] = &[
    "referrer",
    "lattice-snapshot",
    "memento-datetime",
    "color-scheme",
];

/// Removed from every element (§6.4), besides every `on*`.
const REMOVED_ATTRIBUTES: &[&str] = &[
    "ping",
    "target",
    "download",
    "formaction",
    "formmethod",
    "autofocus",
    "contenteditable",
    "draggable",
    "nonce",
    "integrity",
    "crossorigin",
    "referrerpolicy",
    "is",
    "srcdoc",
    "manifest",
    "background",
    "dynsrc",
    "lowsrc",
    "codebase",
    "data",
    "archive",
    "classid",
];

/// Attributes that carry a URL. Always removed; the element rules below
/// write back the ones that survive (`data:` images, rewritten links).
const URL_ATTRIBUTES: &[&str] = &[
    "src",
    "href",
    "xlink:href",
    "poster",
    "cite",
    "longdesc",
    "action",
    "srcset",
];

/// Lazy-loading attributes on `img` (§6.3), consumed and removed.
const LAZY_SRC: &[&str] = &["data-src", "data-lazy-src", "data-original"];
const LAZY_SRCSET: &[&str] = &["data-srcset", "data-lazy-srcset"];
/// Loading hints removed from `img` once `src` is chosen.
const IMG_HINTS: &[&str] = &["sizes", "loading", "decoding", "fetchpriority"];

/// srcset choice (§6.3): the largest `w` up to this, else the largest `x` up to [`SRCSET_MAX_X`].
const SRCSET_MAX_W: u32 = 1600;
const SRCSET_MAX_X: f32 = 2.0;

const CSP: &str = "default-src 'none'; img-src data:; media-src data:; font-src data:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'";
const BASE_LAYER: &str = ":root { color-scheme: light; }\n\
    :where(html) { color-scheme: light; background-color: #ffffff; color: #111827; scrollbar-gutter: stable; }\n\
    html, body { visibility: visible !important; opacity: 1 !important; }\n\
    img[data-lattice-missing] { background: repeating-linear-gradient(45deg, #eee 0 6px, #f6f6f6 6px 12px); min-height: 2em; }\n\
    #__next > div[style*=\"visibility:hidden\"], #__next > div[style*=\"visibility: hidden\"], \
    [data-reactroot] > div[style*=\"visibility:hidden\"], [data-reactroot] > div[style*=\"visibility: hidden\"], \
    #root > div[style*=\"visibility:hidden\"], #root > div[style*=\"visibility: hidden\"], \
    #app > div[style*=\"visibility:hidden\"], #app > div[style*=\"visibility: hidden\"], \
    body > div[style*=\"visibility:hidden\"][style*=\"100vh\"], body > div[style*=\"visibility: hidden\"][style*=\"100vh\"], \
    main[style*=\"opacity:0\"], main[style*=\"opacity: 0\"], \
    article[style*=\"opacity:0\"], article[style*=\"opacity: 0\"], \
    [data-w-id][style*=\"opacity:0\"], [data-w-id][style*=\"opacity: 0\"], \
    [data-aos][style*=\"opacity:0\"], [data-aos][style*=\"opacity: 0\"] { visibility: visible !important; opacity: 1 !important; }";

/// Reads an attribute by lowercase name, entity-decoded.
type AttrFn<'a> = &'a dyn Fn(&str) -> Option<String>;

/// What happens to an element as a whole.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Keep,
    Remove,
    /// Drop the tag, keep the children (`form`, `noscript`).
    Unwrap,
    /// `dialog[open]` becomes a plain `div`.
    IntoDiv,
    /// `link`: a stylesheet becomes `<style>` in place, anything else goes.
    Link,
}

fn element_action(tag: &str, attr: AttrFn<'_>) -> Action {
    if REMOVED_WITH_CONTENT.contains(&tag) {
        return Action::Remove;
    }
    match tag {
        "meta" if attr("http-equiv").is_some() || attr("charset").is_some() => Action::Remove,
        "meta"
            if attr("name").is_some_and(|n| {
                ENVELOPE_META_NAMES.contains(&n.trim().to_ascii_lowercase().as_str())
            }) =>
        {
            Action::Remove
        }
        "template" if attr("shadowrootmode").is_none() => Action::Remove,
        "dialog" if attr("open").is_some() => Action::IntoDiv,
        "dialog" => Action::Remove,
        "form" | "noscript" => Action::Unwrap,
        "link" => Action::Link,
        _ => Action::Keep,
    }
}

/// The one table both passes read: the resource an element loads, as written.
fn element_resource(tag: &str, svg: bool, attr: AttrFn<'_>) -> Option<(String, ResourceKind)> {
    match (tag, svg) {
        ("img", false) => image_source(attr).map(|url| (url, ResourceKind::Image)),
        ("video", false) => non_empty(attr("poster")).map(|url| (url, ResourceKind::Image)),
        ("image", true) => svg_href(attr)
            .filter(|url| !url.starts_with('#'))
            .map(|url| (url, ResourceKind::Image)),
        ("link", _) => link_resource(attr),
        _ => None,
    }
}

/// `link[rel~=stylesheet]` (not alternate) or `link[rel~=preload][as=style]` or `link[rel~=icon]` / `apple-touch-icon`.
fn link_resource(attr: AttrFn<'_>) -> Option<(String, ResourceKind)> {
    let rel = attr("rel").unwrap_or_default().to_ascii_lowercase();
    let tokens: Vec<&str> = rel.split_ascii_whitespace().collect();
    let href = non_empty(attr("href"))?;
    let is_preload_style = tokens.contains(&"preload")
        && attr("as").is_some_and(|a| a.trim().eq_ignore_ascii_case("style"));
    if (tokens.contains(&"stylesheet") && !tokens.contains(&"alternate")) || is_preload_style {
        Some((href, ResourceKind::Stylesheet))
    } else if tokens.contains(&"icon") || tokens.contains(&"apple-touch-icon") {
        Some((href, ResourceKind::Favicon))
    } else {
        None
    }
}

/// `href`, else `xlink:href`.
fn svg_href(attr: AttrFn<'_>) -> Option<String> {
    non_empty(attr("href")).or_else(|| non_empty(attr("xlink:href")))
}

/// Which URL an `img` shows: the srcset choice, else a real `src`, else a
/// lazy attribute when `src` is absent, empty or a small `data:` placeholder.
pub(super) fn image_source(attr: AttrFn<'_>) -> Option<String> {
    if let Some(chosen) = non_empty(attr("srcset")).and_then(|set| choose_srcset(&set)) {
        return Some(chosen);
    }
    let src = non_empty(attr("src"));
    if src.as_deref().is_some_and(|src| !is_placeholder(src)) {
        return src;
    }
    let lazy_set = LAZY_SRCSET
        .iter()
        .find_map(|name| non_empty(attr(name)).and_then(|set| choose_srcset(&set)));
    lazy_set
        .or_else(|| LAZY_SRC.iter().find_map(|name| non_empty(attr(name))))
        .or(src)
}

fn is_placeholder(src: &str) -> bool {
    src.len() < PLACEHOLDER_DATA_URL_LEN && super::resources::is_data_url(src)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Decode the character references lol_html leaves in attribute values and
/// RCDATA: the five XML ones, `&nbsp;` and numeric references.
pub(super) fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        match decode_reference(rest) {
            Some((ch, len)) => {
                out.push(ch);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// One reference at the start of `text` (which begins with `&`): the
/// character and the bytes it used.
fn decode_reference(text: &str) -> Option<(char, usize)> {
    let semi = text.bytes().take(12).position(|b| b == b';')?;
    let name = &text[1..semi];
    let ch = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)?
        }
    };
    Some((ch, semi + 1))
}

/// Read an attribute of a lol_html element, entity-decoded.
fn attribute(el: &Element<'_, '_>, name: &str) -> Option<String> {
    el.get_attribute(name).map(|value| decode_entities(&value))
}

/// Write an attribute; lol_html escapes `"`, so only `&` is escaped here.
fn set_attribute(el: &mut Element<'_, '_>, name: &str, value: &str) {
    let _ = el.set_attribute(name, &value.replace('&', "&amp;"));
}

fn is_svg(el: &Element<'_, '_>) -> bool {
    el.namespace_uri() == SVG_NS
}

/// The URL relative references resolve against: the first `base[href]`
/// with an `http(s)` target, else the document URL. `base` itself is removed.
pub(super) fn effective_base(html: &str, doc_url: &Url) -> Url {
    let found: RefCell<Option<Url>> = RefCell::new(None);
    let settings = RewriteStrSettings::new()
        .append_element_content_handler(element!("base[href]", |el| {
            let mut found = found.borrow_mut();
            if found.is_none() {
                *found = attribute(el, "href")
                    .and_then(|href| doc_url.join(href.trim()).ok())
                    .filter(|url| matches!(url.scheme(), "http" | "https"));
            }
            Ok(())
        }))
        .with_strict(false)
        .with_enable_esi_tags(false);
    let _ = rewrite_str(html, settings);
    found.into_inner().unwrap_or_else(|| doc_url.clone())
}

/// Accumulate a text node's chunks; returns the whole text on its last
/// chunk. With `remove`, every chunk but the last is removed from the
/// output (the caller replaces the last).
fn whole_text(chunk: &mut TextChunk<'_>, buffer: &RefCell<String>, remove: bool) -> Option<String> {
    buffer.borrow_mut().push_str(chunk.as_str());
    if !chunk.last_in_text_node() {
        if remove {
            chunk.remove();
        }
        return None;
    }
    Some(buffer.take())
}

/// Rule N: lol_html hands `noscript` content over as raw text (its parser
/// treats `noscript` like a scripting-enabled browser does). Keep only the
/// `img` and `picture` elements in it; `None` when there are none.
pub(super) fn noscript_images(text: &str) -> Option<String> {
    let found = Cell::new(false);
    let settings = RewriteStrSettings::new()
        .append_element_content_handler(element!("*", |el| {
            let tag = el.tag_name();
            let keep = matches!(tag.as_str(), "img" | "picture")
                || (tag == "link"
                    && matches!(
                        link_resource(&|name| attribute(el, name)),
                        Some((_, ResourceKind::Stylesheet))
                    ));
            if keep {
                found.set(true);
            } else {
                el.remove_and_keep_content();
            }
            Ok(())
        }))
        .append_document_content_handler(doc_text!(|chunk| {
            chunk.remove();
            Ok(())
        }))
        .append_document_content_handler(doc_comments!(|comment| {
            comment.remove();
            Ok(())
        }))
        .with_strict(false)
        .with_enable_esi_tags(false);
    let images = rewrite_str(text, settings).ok()?;
    found.get().then_some(images)
}

// ---------------------------------------------------------------------------
// Pass 2: rewrite
// ---------------------------------------------------------------------------

/// The finished document and anything pass 2 had to give up.
#[derive(Debug)]
pub(super) struct SanitizedDocument {
    pub html: String,
    pub partial_reasons: Vec<String>,
}

/// What pass 2 reads, and the emitted-bytes budget it charges.
struct Context<'a> {
    base: Url,
    resources: &'a ResolvedResources,
    snapshot_id: &'a str,
    emitted: Cell<usize>,
    over_budget: Cell<bool>,
    inlined_sheets: RefCell<HashSet<String>>,
}

/// Pass 2: apply §6.4 and §8.3 to `html`, substitute the fetched resources,
/// and write the envelope of rule E first in `<head>` (a `<head>` is
/// supplied when the page has none).
pub(super) fn sanitize_with_resources(
    html: &str,
    request: &CaptureRequest,
    doc_url: &Url,
    resources: &ResolvedResources,
    captured_at: DateTime<Utc>,
) -> SanitizedDocument {
    let context = Context {
        base: effective_base(html, doc_url),
        resources,
        snapshot_id: &request.snapshot_id,
        emitted: Cell::new(0),
        over_budget: Cell::new(false),
        inlined_sheets: RefCell::new(HashSet::new()),
    };
    let envelope = envelope(doc_url, captured_at);
    let written = Cell::new(false);
    let mut partial_reasons = Vec::new();
    let body = rewrite(html, &context, Some((envelope.as_str(), &written))).unwrap_or_else(|_| {
        partial_reasons.push("rewrite_failed".to_string());
        String::new()
    });
    let mut out = String::with_capacity(body.len() + envelope.len() + 32);
    out.push_str("<!doctype html>\n");
    if !written.get() {
        out.push_str(&format!("<head>\n{envelope}</head>\n"));
    }
    out.push_str(&body);
    if context.over_budget.get() {
        partial_reasons.push("over_budget".to_string());
    }
    SanitizedDocument {
        html: out,
        partial_reasons,
    }
}

/// The rule-E envelope. Our `http-equiv` CSP is the only one in the output.
fn envelope(doc_url: &Url, captured_at: DateTime<Utc>) -> String {
    format!(
        "<meta charset=\"utf-8\">\n\
         <meta http-equiv=\"Content-Security-Policy\" content=\"{CSP}\">\n\
         <meta name=\"referrer\" content=\"no-referrer\">\n\
         <meta name=\"lattice-snapshot\" content=\"1\">\n\
         <meta name=\"memento-datetime\" content=\"{}\">\n\
         <link rel=\"original\" href=\"{}\">\n\
         <style id=\"lattice-base\">{BASE_LAYER}</style>\n",
        captured_at.format("%a, %d %b %Y %H:%M:%S GMT"),
        escape_attribute(doc_url.as_str()),
    )
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Run pass 2 over a document (with the envelope) or a fragment (without).
fn rewrite(
    html: &str,
    context: &Context<'_>,
    envelope: Option<(&str, &Cell<bool>)>,
) -> Result<String, String> {
    let style_text = RefCell::new(String::new());
    let noscript_text = RefCell::new(String::new());
    let mut settings = RewriteStrSettings::new()
        .append_document_content_handler(doctype!(|doctype| {
            doctype.remove();
            Ok(())
        }))
        .append_document_content_handler(doc_comments!(|comment| {
            comment.remove();
            Ok(())
        }))
        .append_element_content_handler(element!("*", |el| {
            rewrite_element(el, context);
            Ok(())
        }))
        .append_element_content_handler(element!("noscript *", |el| {
            // Only reached if a parser ever hands noscript content over as elements.
            let tag = el.tag_name();
            let keep = matches!(tag.as_str(), "img" | "picture")
                || (tag == "link"
                    && matches!(
                        link_resource(&|name| attribute(el, name)),
                        Some((_, ResourceKind::Stylesheet))
                    ));
            if !keep {
                el.remove_and_keep_content();
            }
            Ok(())
        }))
        .append_element_content_handler(text!("style", |chunk| {
            if let Some(css_text) = whole_text(chunk, &style_text, true) {
                let rewritten = css::rewrite_sheet(&css_text, &context.base, context, 0);
                chunk.replace(&escape_style_text(&rewritten), ContentType::Html);
            }
            Ok(())
        }))
        .append_element_content_handler(text!("noscript", |chunk| {
            if let Some(content) = whole_text(chunk, &noscript_text, true) {
                match noscript_images(&content)
                    .and_then(|images| rewrite(&images, context, None).ok())
                {
                    Some(images) => chunk.replace(&images, ContentType::Html),
                    None => chunk.remove(),
                }
            }
            Ok(())
        }))
        .with_strict(false)
        .with_enable_esi_tags(false);
    if let Some((envelope, written)) = envelope {
        settings = settings.append_element_content_handler(element!("head", move |el| {
            if !written.get() {
                el.prepend(envelope, ContentType::Html);
                written.set(true);
            }
            Ok(())
        }));
    }
    rewrite_str(html, settings).map_err(|e| e.to_string())
}

/// `</` cannot appear inside a `<style>`: it could close the element. In CSS
/// `<\/` reads the same inside a string and is inert outside one.
fn escape_style_text(css_text: &str) -> String {
    css_text.replace("</", "<\\/")
}

fn rewrite_element(el: &mut Element<'_, '_>, context: &Context<'_>) {
    let tag = el.tag_name();
    if clutter::is_clutter(&tag, |name| el.get_attribute(name)) {
        el.remove();
        return;
    }
    let action = element_action(&tag, &|name| attribute(el, name));
    match action {
        Action::Remove => return el.remove(),
        Action::Unwrap => return el.remove_and_keep_content(),
        Action::Link => return rewrite_link(el, context),
        Action::IntoDiv => {
            let _ = el.set_tag_name("div");
            el.remove_attribute("open");
        }
        Action::Keep => {}
    }
    rewrite_attributes(el, &tag, context);
}

/// A stylesheet link becomes `<style>` with the rewritten sheet (inside
/// `@media` when it has one); every other link, and a sheet that was not
/// fetched, is removed.
fn rewrite_link(el: &mut Element<'_, '_>, context: &Context<'_>) {
    let attr = |name: &str| attribute(el, name);
    let sheet = match link_resource(&attr) {
        Some((href, ResourceKind::Stylesheet)) => {
            if let Ok(resolved) = resolve_reference(&context.base, &href) {
                if context.inlined_sheets.borrow().contains(&resolved) {
                    el.remove();
                    return;
                }
            }
            if let Some((text, url)) = context.stylesheet(&href, &context.base) {
                context.inlined_sheets.borrow_mut().insert(url.to_string());
                let css_text = css::rewrite_sheet(&text, &url, context, 0);
                let raw_media = attr("media").unwrap_or_default();
                let media =
                    if raw_media.trim().eq_ignore_ascii_case("print") && attr("onload").is_some() {
                        ""
                    } else {
                        &raw_media
                    };
                css::wrap_media(css_text, media)
            } else {
                None
            }
        }
        _ => None,
    };
    match sheet {
        Some(css_text) => el.replace(
            &format!("<style>{}</style>", escape_style_text(&css_text)),
            ContentType::Html,
        ),
        None => el.remove(),
    }
}

/// Strip what §6.4 removes, then write back what survives: `data:` images,
/// rewritten links, same-document SVG references and the rewritten `style`.
fn rewrite_attributes(el: &mut Element<'_, '_>, tag: &str, context: &Context<'_>) {
    let svg = is_svg(el);
    let attr = |name: &str| attribute(el, name);
    let resource = element_resource(tag, svg, &attr);
    let link = matches!(tag, "a" | "area")
        .then(|| svg_href(&attr))
        .flatten();
    let use_fragment = (svg && tag == "use")
        .then(|| svg_href(&attr))
        .flatten()
        .filter(|h| h.starts_with('#'));
    let style = attr("style");
    let names: Vec<String> = el.attributes().iter().map(|a| a.name()).collect();
    for name in &names {
        if is_removed_attribute(el, tag, name) {
            el.remove_attribute(name);
        }
    }
    if let Some(style) = style {
        let rewritten = css::rewrite_declarations(&style, &context.base, context);
        if !rewritten.is_empty() {
            set_attribute(el, "style", &rewritten);
        }
    }
    if let Some((raw, ResourceKind::Image)) = resource {
        write_image(el, tag, &raw, context);
    }
    if let Some(raw) = link {
        write_link(el, &raw, context);
    }
    if let Some(fragment) = use_fragment {
        set_attribute(el, "href", &fragment);
    }
}

/// Whether one attribute goes: `on*`, the removal list, every URL attribute
/// (written back by the element rules), `style` (rewritten), the consumed
/// lazy and loading attributes of `img`, and any other value with an
/// external `url(` (SVG presentation attributes are CSS).
fn is_removed_attribute(el: &Element<'_, '_>, tag: &str, name: &str) -> bool {
    name.starts_with("on")
        || name == "style"
        || REMOVED_ATTRIBUTES.contains(&name)
        || URL_ATTRIBUTES.contains(&name)
        || (tag == "img"
            && (LAZY_SRC.contains(&name)
                || LAZY_SRCSET.contains(&name)
                || IMG_HINTS.contains(&name)))
        || attribute(el, name).is_some_and(|value| css::has_external_url(&value))
}

/// An image source becomes its `data:` URL; a dropped `img` keeps its box
/// and says why in `data-lattice-missing`.
fn write_image(el: &mut Element<'_, '_>, tag: &str, raw: &str, context: &Context<'_>) {
    let target = match tag {
        "video" => "poster",
        "image" => "href",
        _ => "src",
    };
    match context.image(raw) {
        Ok(data_url) => set_attribute(el, target, &data_url),
        Err(reason) if tag == "img" => set_attribute(el, "data-lattice-missing", &reason),
        Err(_) => {}
    }
}

/// §8.3: fragment links stay; `http(s)` links go through `/l` with the
/// original in `data-lattice-href`; anything else is left unlinked.
fn write_link(el: &mut Element<'_, '_>, raw: &str, context: &Context<'_>) {
    let raw = raw.trim();
    if raw.starts_with('#') {
        return set_attribute(el, "href", raw);
    }
    let Some(target) = context
        .base
        .join(raw)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https"))
    else {
        return;
    };
    let routed = format!(
        "/l?u={}&s={}",
        urlencoding::encode(target.as_str()),
        urlencoding::encode(context.snapshot_id)
    );
    set_attribute(el, "href", &routed);
    set_attribute(el, "data-lattice-href", target.as_str());
}

impl Context<'_> {
    /// Look up `raw` (resolved against `base`); its type must satisfy `fits`.
    /// The error is the reason to record (`timeout` when it was never fetched).
    fn lookup(
        &self,
        raw: &str,
        base: &Url,
        fits: impl Fn(&str) -> bool,
    ) -> Result<(String, &DataUri), String> {
        let url = resolve_reference(base, raw).map_err(str::to_string)?;
        match self.resources.get(&url) {
            Some(ResolvedResource::Inlined(data)) if fits(data.mime.as_str()) => Ok((url, data)),
            Some(ResolvedResource::Inlined(_)) => Err("bad_type".to_string()),
            Some(ResolvedResource::Dropped { reason }) => Err(reason.clone()),
            None => Err("timeout".to_string()),
        }
    }

    /// A `data:` URL for an image, charged to the emitted budget.
    fn image(&self, raw: &str) -> Result<String, String> {
        self.data_url_for(raw, &self.base, |mime| mime.starts_with("image/"))
    }

    fn data_url_for(
        &self,
        raw: &str,
        base: &Url,
        fits: impl Fn(&str) -> bool,
    ) -> Result<String, String> {
        let (_, data) = self.lookup(raw, base, fits)?;
        let data_url = data.to_data_url();
        if self.charge(data_url.len()) {
            Ok(data_url)
        } else {
            Err("over_budget".to_string())
        }
    }

    /// A fetched stylesheet's text and URL, charged to the emitted budget.
    fn stylesheet(&self, raw: &str, base: &Url) -> Option<(String, Url)> {
        let (url, data) = self.lookup(raw, base, |mime| mime == CSS_MIME).ok()?;
        let url = Url::parse(&url).ok()?;
        self.charge(data.bytes.len())
            .then(|| (String::from_utf8_lossy(&data.bytes).into_owned(), url))
    }

    /// Count `bytes` against [`MAX_EMITTED_BYTES`]; `false` once it is spent.
    fn charge(&self, bytes: usize) -> bool {
        let total = self.emitted.get().saturating_add(bytes);
        if total > MAX_EMITTED_BYTES {
            self.over_budget.set(true);
            return false;
        }
        self.emitted.set(total);
        true
    }
}

impl CssInliner for Context<'_> {
    fn data_url(&self, raw: &str, base: &Url) -> Option<String> {
        let fits = |mime: &str| {
            mime != CSS_MIME
                && (mime.starts_with("image/")
                    || mime.starts_with("font/")
                    || mime.starts_with("application/"))
        };
        self.data_url_for(raw, base, fits).ok()
    }

    fn import(&self, raw: &str, base: &Url) -> Option<(String, Url)> {
        self.stylesheet(raw, base)
    }
}

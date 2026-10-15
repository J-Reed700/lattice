//! Pass 1: read the page once, without changing it, and list every resource
//! URL the rewrite will need, with its role, plus the title and favicon.

use super::*;

/// What pass 1 found.
#[derive(Debug, Default)]
pub(in super::super) struct Collected {
    pub refs: Vec<ResourceRef>,
    /// The first icon link, absolute.
    pub favicon: Option<String>,
    pub title: Option<String>,
}

#[derive(Default)]
struct CollectState {
    found: RefCell<Collected>,
    in_title: Cell<bool>,
    title_seen: Cell<bool>,
    title_text: RefCell<String>,
    style_text: RefCell<String>,
    noscript_text: RefCell<String>,
}

/// Pass 1: every resource URL the rewrite will look up, with its role.
pub(in super::super) fn collect(html: &str, doc_url: &Url) -> Collected {
    let base = effective_base(html, doc_url);
    collect_with_base(html, &base)
}

fn collect_with_base(html: &str, base: &Url) -> Collected {
    let state = CollectState::default();
    let settings = RewriteStrSettings::new()
        .append_element_content_handler(element!("*", |el| {
            collect_element(el, base, &state);
            Ok(())
        }))
        .append_element_content_handler(text!("title", |chunk| {
            collect_title(chunk, &state);
            Ok(())
        }))
        .append_element_content_handler(text!("style", |chunk| {
            if let Some(css) = whole_text(chunk, &state.style_text, false) {
                collect_css(&css, base, 0, &state);
            }
            Ok(())
        }))
        .append_element_content_handler(text!("noscript", |chunk| {
            if let Some(fragment) =
                whole_text(chunk, &state.noscript_text, false).and_then(|t| noscript_images(&t))
            {
                let inner = collect_with_base(&fragment, base);
                state.found.borrow_mut().refs.extend(inner.refs);
            }
            Ok(())
        }))
        .with_strict(false)
        .with_enable_esi_tags(false);
    let _ = rewrite_str(html, settings);
    state.found.into_inner()
}

fn collect_element(el: &mut Element<'_, '_>, base: &Url, state: &CollectState) {
    let tag = el.tag_name();
    let attr = |name: &str| attribute(el, name);
    if tag == "title" && !is_svg(el) && !state.title_seen.get() {
        state.in_title.set(true);
    }
    if let Some((raw, kind)) = element_resource(&tag, is_svg(el), &attr) {
        if let Ok(url) = resolve_reference(base, &raw) {
            let mut found = state.found.borrow_mut();
            match kind {
                ResourceKind::Favicon => {
                    found.favicon.get_or_insert(url);
                }
                _ => found.refs.push(ResourceRef {
                    url,
                    kind,
                    depth: 0,
                }),
            }
        }
    }
    if let Some(style) = attr("style") {
        collect_css(&style, base, 0, state);
    }
}

/// CSS references, resolved against `base`; `@import` is one level deeper.
fn collect_css(css_text: &str, base: &Url, depth: u8, state: &CollectState) {
    let mut found = state.found.borrow_mut();
    for reference in css::collect_refs(css_text) {
        let (kind, depth) = match reference.kind {
            CssRefKind::Import => (ResourceKind::Stylesheet, depth + 1),
            CssRefKind::Font => (ResourceKind::Font, depth),
            CssRefKind::Image => (ResourceKind::Image, depth),
        };
        if let Ok(url) = resolve_reference(base, &reference.url) {
            found.refs.push(ResourceRef { url, kind, depth });
        }
    }
}

fn collect_title(chunk: &mut TextChunk<'_>, state: &CollectState) {
    if !state.in_title.get() {
        return;
    }
    state.title_text.borrow_mut().push_str(chunk.as_str());
    if chunk.last_in_text_node() {
        state.in_title.set(false);
        state.title_seen.set(true);
        let text = decode_entities(&state.title_text.take());
        let title = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !title.is_empty() {
            state.found.borrow_mut().title = Some(title);
        }
    }
}

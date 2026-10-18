//! Capture-time clutter removal (§6.6): advertising containers that are not
//! content, so removing them irreversibly is fine. Cookie banners, modals and
//! paywall overlays are only *hidden*, by the viewer's clean layer, and are
//! not touched here.
//!
//! Matching is by whole token, never by substring, so `address`, `header`,
//! `loader` and `shadow` can never match `ad`.

/// Class or id tokens that mark an advertising container.
const AD_TOKENS: &[&str] = &[
    "ad",
    "ads",
    "advert",
    "advertisement",
    "advertising",
    "ad-slot",
    "ad-unit",
    "ad-wrapper",
    "ad-container",
    "sponsored-content",
    "taboola",
    "outbrain",
    "mgid",
    "dfp",
];

/// Elements that are never removed as clutter, whatever their tokens say: a
/// site that puts `class="ads"` on `body` must not lose its whole page.
const NEVER_CLUTTER: &[&str] = &["html", "head", "body", "main", "article"];

/// Whether an element is advertising clutter to remove with its content.
/// `attr` returns the element's attribute value by (lowercase) name.
pub(super) fn is_clutter(tag: &str, attr: impl Fn(&str) -> Option<String>) -> bool {
    if NEVER_CLUTTER.contains(&tag) {
        return false;
    }
    if tag == "amp-ad" || tag == "amp-embed" {
        return true;
    }
    if attr("data-ad-slot").is_some() || attr("data-ad-client").is_some() {
        return true;
    }
    if attr("aria-label").is_some_and(|label| label.trim().eq_ignore_ascii_case("advertisement")) {
        return true;
    }
    let class = attr("class").unwrap_or_default();
    if class_tokens(&class).any(|token| token == "adsbygoogle" || is_ad_token(token)) {
        return true;
    }
    attr("id").is_some_and(|id| id_is_clutter(&id))
}

/// `[id^="google_ads_"]`, `[id^="div-gpt-ad"]`, or an ad token as the whole id
/// or as one of its `-`/`_` separated parts.
fn id_is_clutter(id: &str) -> bool {
    let id = id.trim();
    id.starts_with("google_ads_")
        || id.starts_with("div-gpt-ad")
        || is_ad_token(id)
        || id.split(['-', '_']).any(is_ad_token)
}

/// Class tokens: the attribute split on ASCII whitespace only.
fn class_tokens(class: &str) -> impl Iterator<Item = &str> {
    class.split_ascii_whitespace()
}

/// Exact, case-insensitive membership in [`AD_TOKENS`].
fn is_ad_token(token: &str) -> bool {
    AD_TOKENS.iter().any(|ad| ad.eq_ignore_ascii_case(token))
}

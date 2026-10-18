//! Finding candidates (§7.3): Wikimedia Commons' keyless search API, and the
//! picture a page offers for itself.

use lattice_core::application::contracts::images::ImageOrigin;
use serde_json::Value;

use super::{extension_for_mime, COMMONS_THUMB_WIDTH};

const COMMONS_API: &str = "https://commons.wikimedia.org/w/api.php";

/// A picture worth fetching, with what its source says about it. Declared
/// sizes and types are hints for skipping; the fetched bytes are checked.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub image_url: String,
    pub caption: String,
    pub source_url: String,
    pub source_title: Option<String>,
    pub origin: ImageOrigin,
    pub license: Option<String>,
    pub attribution: Option<String>,
}

/// The search request for `query`: files in namespace 6, bitmaps only,
/// twice `limit` candidates so a few rejections still leave enough, with
/// the image info and the licence fields a caption chip needs.
pub fn search_url(query: &str, limit: usize) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("action", "query")
        .append_pair("format", "json")
        .append_pair("formatversion", "2")
        .append_pair("generator", "search")
        .append_pair("gsrnamespace", "6")
        .append_pair("gsrlimit", &(limit * 2).to_string())
        .append_pair("gsrsearch", &format!("filetype:bitmap {}", query.trim()))
        .append_pair("prop", "imageinfo")
        .append_pair("iiprop", "url|size|mime|extmetadata")
        .append_pair("iiurlwidth", &COMMONS_THUMB_WIDTH.to_string())
        .append_pair(
            "iiextmetadatafilter",
            "ImageDescription|LicenseShortName|Artist|Credit",
        )
        .finish();
    format!("{COMMONS_API}?{query}")
}

/// The candidates in a search response, in search order, skipping any
/// whose declared type is not a stored format. Missing fields are
/// tolerated; a page without a fetchable URL is skipped.
pub fn parse_search(response: &Value) -> Vec<Candidate> {
    let Some(pages) = response.pointer("/query/pages").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut ranked: Vec<(u64, Candidate)> = pages
        .iter()
        .filter_map(|page| {
            let info = page.pointer("/imageinfo/0")?;
            let mime = info.get("mime").and_then(Value::as_str)?;
            extension_for_mime(mime)?;
            let image_url = text(info, "thumburl").or_else(|| text(info, "url"))?;
            let title = text(page, "title");
            let source_url = text(info, "descriptionurl").or_else(|| {
                title
                    .as_ref()
                    .map(|t| format!("https://commons.wikimedia.org/wiki/{}", t.replace(' ', "_")))
            })?;
            let caption = metadata(info, "ImageDescription")
                .or_else(|| title.as_deref().map(title_caption))
                .unwrap_or_default();
            let index = page
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or(u64::MAX);
            Some((
                index,
                Candidate {
                    image_url,
                    caption,
                    source_url,
                    source_title: title,
                    origin: ImageOrigin::WikimediaCommons,
                    license: metadata(info, "LicenseShortName"),
                    attribution: metadata(info, "Artist").or_else(|| metadata(info, "Credit")),
                },
            ))
        })
        .collect();
    // formatversion=2 lists generator pages by id; `index` is the search rank.
    ranked.sort_by_key(|(index, _)| *index);
    ranked.into_iter().map(|(_, candidate)| candidate).collect()
}

/// The candidate a page offers: its lead picture, resolved against the
/// page's final URL. Caption is the page title; there is no licence.
pub fn page_candidate(html: &str, page_url: &str, page_title: Option<String>) -> Option<Candidate> {
    let lead = crate::features::web::services::capture::page_lead_image(html)?;
    let image_url = url::Url::parse(page_url).ok()?.join(&lead).ok()?;
    Some(Candidate {
        image_url: image_url.into(),
        caption: page_title.clone().unwrap_or_default(),
        source_url: page_url.to_string(),
        source_title: page_title,
        origin: ImageOrigin::Page,
        license: None,
        attribution: None,
    })
}

/// The `<title>` of a page, collapsed, if it has one.
pub fn page_title(html: &str) -> Option<String> {
    let document = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("title").ok()?;
    let title = document
        .select(&selector)
        .next()?
        .text()
        .collect::<String>();
    let title = collapse(&title);
    (!title.is_empty()).then_some(title)
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// An `extmetadata` field's value as plain text: Commons writes HTML there.
fn metadata(info: &Value, field: &str) -> Option<String> {
    let html = info
        .pointer(&format!("/extmetadata/{field}/value"))
        .and_then(Value::as_str)?;
    let plain = strip_html(html);
    (!plain.is_empty()).then_some(plain)
}

/// HTML to text: tags dropped, entities decoded, whitespace collapsed.
pub fn strip_html(html: &str) -> String {
    let fragment = scraper::Html::parse_fragment(html);
    collapse(&fragment.root_element().text().collect::<String>())
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `File:Grey heron (Ardea cinerea).jpg` → `Grey heron (Ardea cinerea)`.
fn title_caption(title: &str) -> String {
    let name = title.strip_prefix("File:").unwrap_or(title);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    collapse(&stem.replace('_', " "))
}

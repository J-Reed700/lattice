//! Archival fallback: a clean, self-contained document built from extracted text when the page itself cannot be captured.

use chrono::{DateTime, Utc};
use url::Url;

use super::envelope;

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Builds a clean, self-contained archival HTML document from extracted text.
/// `None` when there is no text to keep or `url` does not parse.
pub fn synthesize_archival_html(
    url: &str,
    title: Option<&str>,
    text: &str,
    snapshot_id: &str,
    captured_at: DateTime<Utc>,
) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    let doc_url = Url::parse(url).ok()?;
    let envelope = envelope(&doc_url, captured_at);
    let title_text = title
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| doc_url.as_str());
    let escaped_title = escape_html(title_text);
    let escaped_url = escape_html(doc_url.as_str());
    let routed_href = format!(
        "/l?u={}&amp;s={}",
        urlencoding::encode(doc_url.as_str()),
        urlencoding::encode(snapshot_id)
    );

    let mut article = String::new();
    for para in text.split("\n\n") {
        let trimmed = para.trim();
        if !trimmed.is_empty() {
            article.push_str("<p>");
            article.push_str(&escape_html(trimmed));
            article.push_str("</p>\n");
        }
    }

    Some(format!(
        "<!doctype html>\n\
         <html>\n\
         <head>\n\
         {envelope}\
         <title>{escaped_title}</title>\n\
         <style>\
         body {{ margin: 0; padding: 2.5rem 1.5rem; background: #ffffff; color: #111827; font-family: -apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, Georgia, serif; line-height: 1.7; }} \
         main {{ max-width: 44rem; margin: 0 auto; }} \
         header {{ margin-bottom: 2rem; padding-bottom: 1rem; border-bottom: 1px solid #e5e7eb; }} \
         h1 {{ font-size: 1.75rem; line-height: 1.25; margin: 0 0 0.5rem; color: #111827; }} \
         .meta {{ font-size: 0.8125rem; color: #6b7280; word-break: break-all; }} \
         p {{ margin: 0 0 1.15rem; font-size: 1.03125rem; white-space: pre-wrap; }}\
         </style>\n\
         </head>\n\
         <body>\n\
         <main>\n\
         <header>\n\
         <h1>{escaped_title}</h1>\n\
         <div class=\"meta\"><a href=\"{routed_href}\" data-lattice-href=\"{escaped_url}\">{escaped_url}</a></div>\n\
         </header>\n\
         <article>\n\
         {article}\
         </article>\n\
         </main>\n\
         </body>\n\
         </html>\n"
    ))
}

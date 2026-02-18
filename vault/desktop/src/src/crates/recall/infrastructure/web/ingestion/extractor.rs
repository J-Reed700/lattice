use crate::infrastructure::web::ingestion::config::WebIngestionConfig;
use crate::infrastructure::web::ingestion::error::{Result, WebIngestionError};
use crate::infrastructure::web::ingestion::types::WebContent;
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use std::collections::HashMap;
use tracing::{debug, warn};

// Compiled regex patterns for class/id scoring (used in hot path)
// These patterns are hardcoded and guaranteed to be valid
static POSITIVE_CLASS_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(article|body|content|entry|hentry|main|page|post|text|blog|story)")
        .expect("Safe: hardcoded regex pattern is valid")
});

static NEGATIVE_CLASS_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(combx|comment|com-|contact|foot|footer|footnote|masthead|media|meta|outbrain|promo|related|scroll|share|shoutbox|sidebar|skyscraper|sponsor|shopping|tags|tool|widget|ad-|advertisement|banner|nav|navigation)")
        .expect("Safe: hardcoded regex pattern is valid")
});

// Compiled regex pattern for whitespace normalization
static WHITESPACE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\s+")
        .expect("Safe: hardcoded regex pattern is valid")
});

pub struct ContentExtractor {
    config: WebIngestionConfig,
}

impl ContentExtractor {
    pub fn new(config: WebIngestionConfig) -> Self {
        Self { config }
    }

    pub fn extract(&self, html: &str, url: &str) -> Result<WebContent> {
        let document = Html::parse_document(html);

        let title = self.extract_title(&document);
        debug!("Extracted title: {}", title);

        self.remove_unlikely_candidates(&document);

        let scored_nodes = self.score_nodes(&document);

        let best_candidate = self.find_best_candidate(&scored_nodes)?;
        debug!("Best candidate score: {}", best_candidate.1);

        let content_html = self.extract_content_html(&document, best_candidate.0);

        let cleaned_text = self.clean_text(&content_html);

        if cleaned_text.len() < self.config.min_text_length {
            return Err(WebIngestionError::no_content_found(url));
        }

        let byline = self.extract_byline(&document);

        let lang = self.extract_language(&document);

        let mut content = WebContent::new(title, cleaned_text);

        if let Some(byline) = byline {
            content = content.with_byline(byline);
        }

        if let Some(lang) = lang {
            content = content.with_lang(lang);
        }

        Ok(content)
    }

    fn extract_title(&self, document: &Html) -> String {
        if let Ok(selector) = Selector::parse("title") {
            if let Some(title_element) = document.select(&selector).next() {
                let title = title_element.text().collect::<String>().trim().to_string();
                if !title.is_empty() {
                    return title;
                }
            }
        }

        if let Ok(selector) = Selector::parse("h1") {
            if let Some(h1) = document.select(&selector).next() {
                let h1_text = h1.text().collect::<String>().trim().to_string();
                if !h1_text.is_empty() {
                    return h1_text;
                }
            }
        }

        "Untitled".to_string()
    }

    fn extract_byline(&self, document: &Html) -> Option<String> {
        let selectors = [
            "[rel='author']",
            "[itemprop='author']",
            ".author",
            ".byline",
            "[class*='author']",
        ];

        for selector_str in &selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                if let Some(element) = document.select(&selector).next() {
                    let text = element.text().collect::<String>().trim().to_string();
                    if !text.is_empty() && text.len() < 100 {
                        return Some(text);
                    }
                }
            }
        }

        None
    }

    fn extract_language(&self, document: &Html) -> Option<String> {
        if let Ok(selector) = Selector::parse("html") {
            if let Some(html_element) = document.select(&selector).next() {
                if let Some(lang) = html_element.value().attr("lang") {
                    return Some(lang.to_string());
                }
            }
        }

        None
    }

    fn remove_unlikely_candidates(&self, _document: &Html) {
        // In a real implementation, we would modify the DOM here
        // For scraper, we'll handle this during scoring instead
    }

    fn score_nodes(&self, document: &Html) -> HashMap<String, f64> {
        let mut scores: HashMap<String, f64> = HashMap::new();

        let content_selectors = ["p", "td", "pre", "article", "section"];

        for selector_str in &content_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                for element in document.select(&selector) {
                    let score = self.calculate_node_score(&element);

                    let id = self.get_element_id(&element);
                    scores.insert(id.clone(), score);

                    self.bubble_score(&element, score * 0.5, &mut scores);
                }
            }
        }

        scores
    }

    fn calculate_node_score(&self, element: &ElementRef) -> f64 {
        let mut score = 0.0;

        let text_content = element.text().collect::<String>();
        let text_length = text_content.len();

        if text_length < self.config.min_text_length {
            return 0.0;
        }

        score += 1.0;

        score += (text_content.split(',').count() as f64).min(10.0);

        score += (text_length as f64 / 100.0).min(3.0);

        if let Some(class) = element.value().attr("class") {
            score += self.get_class_weight(class);
        }

        if let Some(id) = element.value().attr("id") {
            score += self.get_class_weight(id);
        }

        let tag_name = element.value().name();
        match tag_name {
            "article" => score += 10.0,
            "section" => score += 5.0,
            "div" => score += 5.0,
            "p" => score += 3.0,
            "td" => score += 1.0,
            _ => {}
        }

        let link_density = self.calculate_link_density(element);
        score -= link_density * 25.0;

        score.max(0.0).min(self.config.max_node_score)
    }

    fn get_class_weight(&self, class_or_id: &str) -> f64 {
        let mut weight = 0.0;

        if POSITIVE_CLASS_PATTERN.is_match(class_or_id) {
            weight += 25.0;
        }

        if NEGATIVE_CLASS_PATTERN.is_match(class_or_id) {
            weight -= 25.0;
        }

        weight
    }

    fn calculate_link_density(&self, element: &ElementRef) -> f64 {
        let text_content = element.text().collect::<String>();
        let text_length = text_content.len() as f64;

        if text_length == 0.0 {
            return 0.0;
        }

        let mut link_length = 0.0;

        if let Ok(selector) = Selector::parse("a") {
            for link in element.select(&selector) {
                let link_text = link.text().collect::<String>();
                link_length += link_text.len() as f64;
            }
        }

        link_length / text_length
    }

    fn bubble_score(&self, element: &ElementRef, score: f64, scores: &mut HashMap<String, f64>) {
        let mut current_level = 0;
        let mut current_element = *element;

        while current_level < 3 {
            if let Some(parent) = current_element.parent() {
                if let Some(parent_element) = ElementRef::wrap(parent) {
                    let parent_id = self.get_element_id(&parent_element);
                    *scores.entry(parent_id).or_insert(0.0) += score / (current_level as f64 + 1.0);

                    current_element = parent_element;
                    current_level += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn get_element_id(&self, element: &ElementRef) -> String {
        if let Some(id) = element.value().attr("id") {
            return format!("#{}", id);
        }

        if let Some(class) = element.value().attr("class") {
            return format!(".{}", class.split_whitespace().next().unwrap_or("unknown"));
        }

        format!("{}", element.value().name())
    }

    fn find_best_candidate(&self, scores: &HashMap<String, f64>) -> Result<(&String, &f64)> {
        scores
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or_else(|| WebIngestionError::internal_error("No scored nodes found"))
    }

    fn extract_content_html(&self, document: &Html, _best_id: &str) -> String {
        let content_selectors = [
            "article",
            "[role='main']",
            "main",
            ".content",
            "#content",
            ".post",
            ".entry",
        ];

        for selector_str in &content_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                if let Some(element) = document.select(&selector).next() {
                    return element.html();
                }
            }
        }

        if let Ok(selector) = Selector::parse("body") {
            if let Some(body) = document.select(&selector).next() {
                return body.html();
            }
        }

        document.html()
    }

    fn clean_text(&self, html: &str) -> String {
        let document = Html::parse_fragment(html);

        let elements_to_remove = [
            "script", "style", "noscript", "iframe", "embed", "object",
            "nav", "footer", "aside", "form", "button",
        ];

        let mut text_parts = Vec::new();

        self.extract_text_from_node(document.root_element(), &elements_to_remove, &mut text_parts);

        let text = text_parts.join("\n");

        let cleaned = WHITESPACE_PATTERN.replace_all(&text, " ");

        cleaned.trim().to_string()
    }

    fn extract_text_from_node(
        &self,
        element: ElementRef,
        skip_tags: &[&str],
        text_parts: &mut Vec<String>,
    ) {
        let tag_name = element.value().name();

        if skip_tags.contains(&tag_name) {
            return;
        }

        for child in element.children() {
            match child.value() {
                Node::Text(text) => {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        text_parts.push(trimmed.to_string());
                    }
                }
                Node::Element(_) => {
                    if let Some(child_element) = ElementRef::wrap(child) {
                        self.extract_text_from_node(child_element, skip_tags, text_parts);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title() {
        let config = WebIngestionConfig::default();
        let extractor = ContentExtractor::new(config);

        let html = r#"
        <html>
        <head><title>Test Article</title></head>
        <body></body>
        </html>
        "#;

        let document = Html::parse_document(html);
        let title = extractor.extract_title(&document);

        assert_eq!(title, "Test Article");
    }

    #[test]
    fn test_calculate_link_density() {
        let config = WebIngestionConfig::default();
        let extractor = ContentExtractor::new(config);

        let html = r#"
        <div>
            This is some text with a <a href="#">link</a> in it.
        </div>
        "#;

        let document = Html::parse_fragment(html);
        let element = document.root_element();

        let density = extractor.calculate_link_density(&element);
        assert!(density > 0.0 && density < 1.0);
    }

    #[test]
    fn test_class_weight() {
        let config = WebIngestionConfig::default();
        let extractor = ContentExtractor::new(config);

        let positive_weight = extractor.get_class_weight("article-content");
        assert!(positive_weight > 0.0);

        let negative_weight = extractor.get_class_weight("advertisement-banner");
        assert!(negative_weight < 0.0);

        let neutral_weight = extractor.get_class_weight("random-class");
        assert_eq!(neutral_weight, 0.0);
    }

    #[test]
    fn test_clean_text() {
        let config = WebIngestionConfig::default();
        let extractor = ContentExtractor::new(config);

        let html = r#"
        <div>
            <p>This is a paragraph.</p>
            <script>alert('bad');</script>
            <p>Another paragraph.</p>
            <nav>Navigation here</nav>
        </div>
        "#;

        let cleaned = extractor.clean_text(html);

        assert!(cleaned.contains("This is a paragraph"));
        assert!(cleaned.contains("Another paragraph"));
        assert!(!cleaned.contains("alert"));
        assert!(!cleaned.contains("Navigation here"));
    }
}

//! Reference extraction retains structure discarded by short chat-page reads.
use crate::shared::error::{AppError, Result};
use scraper::{ElementRef, Html, Selector};

fn children(element: ElementRef<'_>, out: &mut String, depth: usize) -> Result<()> {
    for child in element.children() {
        if let Some(text) = child.value().as_text() {
            out.push_str(text);
        } else if let Some(element) = ElementRef::wrap(child) {
            render(element, out, depth + 1)?;
        }
    }
    Ok(())
}
fn render(element: ElementRef<'_>, out: &mut String, depth: usize) -> Result<()> {
    if depth > 256 {
        return Err(AppError::InvalidInput("Reference HTML is too deeply nested to capture safely. Supply a simpler page or document.".into()));
    }
    let name = element.value().name();
    if matches!(
        name,
        "script" | "style" | "nav" | "header" | "footer" | "aside" | "button" | "svg" | "noscript"
    ) || element.value().attr("hidden").is_some()
        || element.value().attr("aria-hidden") == Some("true")
    {
        return Ok(());
    }
    if name == "pre" {
        let code = element.text().collect::<String>();
        let language = element
            .children()
            .filter_map(ElementRef::wrap)
            .find(|e| e.value().name() == "code")
            .and_then(|e| e.value().attr("class"))
            .and_then(|classes| {
                classes
                    .split_whitespace()
                    .find_map(|c| c.strip_prefix("language-"))
            })
            .unwrap_or("text");
        let fence = if code.contains("```") { "~~~~" } else { "```" };
        out.push_str(&format!("\n\n{fence}{language}\n{code}\n{fence}\n\n"));
        return Ok(());
    }
    if name == "ol" {
        let mut number = element
            .value()
            .attr("start")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(1);
        out.push('\n');
        for item in element
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|e| e.value().name() == "li")
        {
            number = item
                .value()
                .attr("value")
                .and_then(|v| v.parse().ok())
                .unwrap_or(number);
            out.push_str(&format!("\n{number}. "));
            children(item, out, depth + 1)?;
            number = number.saturating_add(1);
        }
        out.push('\n');
        return Ok(());
    }
    let heading = match name {
        "h1" => 1,
        "h2" => 2,
        "h3" => 3,
        "h4" => 4,
        "h5" => 5,
        "h6" => 6,
        _ => 0,
    };
    let block = matches!(
        name,
        "p" | "div"
            | "section"
            | "article"
            | "blockquote"
            | "ul"
            | "table"
            | "tr"
            | "dl"
            | "dt"
            | "dd"
    );
    if heading > 0 {
        out.push_str(&format!("\n\n{} ", "#".repeat(heading)));
    } else if block {
        out.push('\n');
    } else if name == "li" {
        out.push_str("\n- ");
    } else if name == "br" {
        out.push('\n');
    } else if matches!(name, "td" | "th") {
        out.push_str(" | ");
    }
    if name == "img" {
        if let Some(alt) = element.value().attr("alt").filter(|alt| !alt.is_empty()) {
            out.push_str(&format!("[Image description: {alt}]"));
        }
    } else {
        children(element, out, depth)?;
    }
    if block || heading > 0 {
        out.push('\n');
    }
    Ok(())
}

pub(super) fn extract(html: &str) -> Result<(Option<String>, String)> {
    let document = Html::parse_document(html);
    let title = Selector::parse("title")
        .ok()
        .and_then(|s| document.select(&s).next())
        .map(|e| e.text().collect::<String>().trim().to_owned());
    let root = Selector::parse("main, [role=main]")
        .ok()
        .and_then(|s| {
            document
                .select(&s)
                .max_by_key(|e| e.text().map(str::len).sum::<usize>())
        })
        .or_else(|| {
            Selector::parse("body")
                .ok()
                .and_then(|s| document.select(&s).next())
        })
        .unwrap_or_else(|| document.root_element());
    let mut text = String::new();
    render(root, &mut text, 0)?;
    Ok((title, text.trim().to_owned()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn deeply_nested_reference_html_is_rejected_without_partial_capture() {
        let html = format!("{}reference{}", "<div>".repeat(300), "</div>".repeat(300));
        assert!(super::extract(&html).is_err());
    }

    #[test]
    fn references_keep_headings_lists_tables_and_literal_code_whitespace() {
        let (title,text)=super::extract("<title>Reference</title><nav>Ignore menu</nav><main><h1>Ownership</h1><p>A <em>borrow</em> keeps ownership.</p><pre><code class='language-rust'>fn main() {\n    let x = 1;\n}</code></pre><ol start='3'><li>Mix flour</li><li>Chill dough</li></ol><table><tr><th>Structure</th><th>Function</th></tr><tr><td>Chloroplast</td><td>Photosynthesis</td></tr></table><script>ignore()</script></main>").expect("readable reference");
        assert_eq!(title.as_deref(), Some("Reference"));
        for expected in [
            "# Ownership",
            "A borrow keeps ownership.",
            "```rust\nfn main() {\n    let x = 1;\n}",
            "3. Mix flour",
            "4. Chill dough",
            " | Structure | Function",
            " | Chloroplast | Photosynthesis",
        ] {
            assert!(text.contains(expected), "Missing {expected}: {text}");
        }
        assert!(!text.contains("Ignore menu"));
        assert!(!text.contains("ignore()"));
    }
}

//! Reference extraction retains document structure discarded by short chat-page reads.
use crate::shared::error::{AppError, Result};
use scraper::{ElementRef, Html, Selector};
use url::Url;

pub(crate) const EXTRACTION_VERSION: &str = "web_reference_markdown_v2";
const MAX_NESTING_DEPTH: usize = 256;

fn invalid_depth() -> AppError {
    AppError::InvalidInput(
        "Reference HTML is too deeply nested to capture safely. Supply a simpler page or document."
            .into(),
    )
}

fn is_hidden(element: ElementRef<'_>) -> bool {
    matches!(
        element.value().name(),
        "script"
            | "style"
            | "nav"
            | "header"
            | "footer"
            | "aside"
            | "button"
            | "svg"
            | "noscript"
            | "template"
    ) || element.value().attr("hidden").is_some()
        || element.value().attr("aria-hidden") == Some("true")
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "div"
            | "dl"
            | "fieldset"
            | "figure"
            | "figcaption"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hr"
            | "main"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "ul"
    )
}

fn escape_text(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut escaped = String::with_capacity(text.len());
    for (index, &character) in characters.iter().enumerate() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            // CommonMark never reads an underscore between two alphanumerics as
            // emphasis, so `snake_case` identifiers stay quotable and searchable.
            '_' if is_intraword(&characters, index) => escaped.push(character),
            '\\' | '`' | '*' | '_' | '[' | ']' | '|' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }
    escaped
}

fn is_intraword(characters: &[char], index: usize) -> bool {
    let before = index
        .checked_sub(1)
        .and_then(|previous| characters.get(previous));
    let after = characters.get(index + 1);
    before.is_some_and(|c| c.is_alphanumeric()) && after.is_some_and(|c| c.is_alphanumeric())
}

fn append_collapsed(out: &mut String, text: &str) {
    let leading_space = text.chars().next().is_some_and(char::is_whitespace);
    let trailing_space = text.chars().next_back().is_some_and(char::is_whitespace);
    if leading_space && !out.is_empty() && !out.ends_with([' ', '\n']) {
        out.push(' ');
    }
    for (index, word) in text.split_whitespace().enumerate() {
        if index > 0 && !out.ends_with([' ', '\n']) {
            out.push(' ');
        }
        out.push_str(&escape_text(word));
    }
    if trailing_space && !out.is_empty() && !out.ends_with([' ', '\n']) {
        out.push(' ');
    }
}

fn inline_code(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let longest_run = text
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let delimiter = "`".repeat((longest_run + 1).max(1));
    if text.starts_with('`') || text.ends_with('`') {
        format!("{delimiter} {text} {delimiter}")
    } else {
        format!("{delimiter}{text}{delimiter}")
    }
}

fn safe_link(href: &str, base_url: Option<&Url>) -> Option<String> {
    let parsed = Url::parse(href)
        .ok()
        .or_else(|| base_url.and_then(|base| base.join(href).ok()))?;
    matches!(parsed.scheme(), "http" | "https").then(|| {
        parsed
            .as_str()
            .replace(' ', "%20")
            .replace('(', "%28")
            .replace(')', "%29")
    })
}

fn inline_children(
    element: ElementRef<'_>,
    out: &mut String,
    depth: usize,
    base_url: Option<&Url>,
) -> Result<()> {
    if depth > MAX_NESTING_DEPTH {
        return Err(invalid_depth());
    }
    for child in element.children() {
        if let Some(text) = child.value().as_text() {
            append_collapsed(out, text);
        } else if let Some(child) = ElementRef::wrap(child) {
            render_inline(child, out, depth + 1, base_url)?;
        }
    }
    Ok(())
}

fn render_inline(
    element: ElementRef<'_>,
    out: &mut String,
    depth: usize,
    base_url: Option<&Url>,
) -> Result<()> {
    if depth > MAX_NESTING_DEPTH {
        return Err(invalid_depth());
    }
    if is_hidden(element) {
        return Ok(());
    }
    match element.value().name() {
        "br" => out.push_str("  \n"),
        "code" => out.push_str(&inline_code(&element.text().collect::<String>())),
        "strong" | "b" => {
            let mut inner = String::new();
            inline_children(element, &mut inner, depth, base_url)?;
            if !inner.trim().is_empty() {
                out.push_str("**");
                out.push_str(inner.trim());
                out.push_str("**");
            }
        }
        "em" | "i" => {
            let mut inner = String::new();
            inline_children(element, &mut inner, depth, base_url)?;
            if !inner.trim().is_empty() {
                out.push('*');
                out.push_str(inner.trim());
                out.push('*');
            }
        }
        "del" | "s" => {
            let mut inner = String::new();
            inline_children(element, &mut inner, depth, base_url)?;
            if !inner.trim().is_empty() {
                out.push_str("~~");
                out.push_str(inner.trim());
                out.push_str("~~");
            }
        }
        "a" => {
            let mut label = String::new();
            inline_children(element, &mut label, depth, base_url)?;
            let label = label.trim();
            if let Some(url) = element
                .value()
                .attr("href")
                .and_then(|href| safe_link(href, base_url))
                .filter(|_| !label.is_empty())
            {
                out.push('[');
                out.push_str(label);
                out.push_str("](");
                out.push_str(&url);
                out.push(')');
            } else {
                out.push_str(label);
            }
        }
        "img" => {
            if let Some(alt) = element
                .value()
                .attr("alt")
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                out.push_str("*Image: ");
                out.push_str(&escape_text(alt));
                out.push('*');
            }
        }
        _ => inline_children(element, out, depth, base_url)?,
    }
    Ok(())
}

fn push_block(blocks: &mut Vec<String>, block: String) {
    let block = block.trim();
    if !block.is_empty() {
        blocks.push(block.to_owned());
    }
}

fn flush_inline(blocks: &mut Vec<String>, inline: &mut String) {
    push_block(blocks, std::mem::take(inline));
}

fn render_container(
    element: ElementRef<'_>,
    blocks: &mut Vec<String>,
    depth: usize,
    base_url: Option<&Url>,
) -> Result<()> {
    if depth > MAX_NESTING_DEPTH {
        return Err(invalid_depth());
    }
    let mut inline = String::new();
    for child in element.children() {
        if let Some(text) = child.value().as_text() {
            append_collapsed(&mut inline, text);
            continue;
        }
        let Some(child) = ElementRef::wrap(child) else {
            continue;
        };
        if is_hidden(child) {
            continue;
        }
        if is_block(child.value().name()) {
            flush_inline(blocks, &mut inline);
            render_block(child, blocks, depth + 1, base_url)?;
        } else {
            render_inline(child, &mut inline, depth + 1, base_url)?;
        }
    }
    flush_inline(blocks, &mut inline);
    Ok(())
}

fn code_block(element: ElementRef<'_>) -> String {
    let raw = element.text().collect::<String>();
    let code = raw.trim_matches('\n');
    let language = element
        .children()
        .filter_map(ElementRef::wrap)
        .find(|child| child.value().name() == "code")
        .and_then(|code| code.value().attr("class"))
        .and_then(|classes| {
            classes.split_whitespace().find_map(|class| {
                class
                    .strip_prefix("language-")
                    .or_else(|| class.strip_prefix("lang-"))
            })
        })
        .filter(|language| {
            language.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '_')
            })
        })
        .unwrap_or("text");
    let longest_run = code
        .lines()
        .flat_map(|line| line.split(|character| character != '`'))
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat((longest_run + 1).max(3));
    format!("{fence}{language}\n{code}\n{fence}")
}

fn render_list_lines(
    element: ElementRef<'_>,
    ordered: bool,
    depth: usize,
    base_url: Option<&Url>,
) -> Result<Vec<String>> {
    if depth > MAX_NESTING_DEPTH {
        return Err(invalid_depth());
    }
    let mut lines = Vec::new();
    let mut number = element
        .value()
        .attr("start")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(1);
    for item in element
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|child| child.value().name() == "li")
    {
        if ordered {
            number = item
                .value()
                .attr("value")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(number);
        }
        let mut content = String::new();
        let mut nested = Vec::new();
        for child in item.children() {
            if let Some(text) = child.value().as_text() {
                append_collapsed(&mut content, text);
                continue;
            }
            let Some(child) = ElementRef::wrap(child) else {
                continue;
            };
            if is_hidden(child) {
                continue;
            }
            match child.value().name() {
                "ul" => nested.extend(render_list_lines(child, false, depth + 1, base_url)?),
                "ol" => nested.extend(render_list_lines(child, true, depth + 1, base_url)?),
                _ => render_inline(child, &mut content, depth + 1, base_url)?,
            }
        }
        let marker = if ordered {
            format!("{number}.")
        } else {
            "-".to_owned()
        };
        lines.push(format!("{marker} {}", content.trim()));
        for line in nested {
            lines.push(format!("    {line}"));
        }
        if ordered {
            number = number.saturating_add(1);
        }
    }
    Ok(lines)
}

fn table_markdown(element: ElementRef<'_>, base_url: Option<&Url>) -> Result<Option<String>> {
    let row_selector = Selector::parse("tr")
        .map_err(|error| AppError::InternalError(format!("Invalid table selector: {error}")))?;
    let mut rows = Vec::<Vec<String>>::new();
    for row in element.select(&row_selector) {
        let mut cells = Vec::new();
        for cell in row
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|cell| matches!(cell.value().name(), "th" | "td"))
        {
            let mut value = String::new();
            inline_children(cell, &mut value, 0, base_url)?;
            cells.push(value.replace('\n', " ").trim().to_owned());
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    let Some(width) = rows.iter().map(Vec::len).max() else {
        return Ok(None);
    };
    for row in &mut rows {
        row.resize(width, String::new());
    }
    let render_row = |row: &[String]| format!("| {} |", row.join(" | "));
    let Some(header) = rows.first() else {
        return Ok(None);
    };
    let mut lines = vec![render_row(header)];
    lines.push(format!(
        "| {} |",
        std::iter::repeat_n("---", width)
            .collect::<Vec<_>>()
            .join(" | ")
    ));
    lines.extend(rows.iter().skip(1).map(|row| render_row(row)));
    Ok(Some(lines.join("\n")))
}

fn render_block(
    element: ElementRef<'_>,
    blocks: &mut Vec<String>,
    depth: usize,
    base_url: Option<&Url>,
) -> Result<()> {
    if depth > MAX_NESTING_DEPTH {
        return Err(invalid_depth());
    }
    if is_hidden(element) {
        return Ok(());
    }
    match element.value().name() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = element
                .value()
                .name()
                .trim_start_matches('h')
                .parse::<usize>()
                .unwrap_or(1);
            let mut heading = String::new();
            inline_children(element, &mut heading, depth, base_url)?;
            push_block(blocks, format!("{} {}", "#".repeat(level), heading.trim()));
        }
        "p" | "figcaption" => {
            let mut paragraph = String::new();
            inline_children(element, &mut paragraph, depth, base_url)?;
            push_block(blocks, paragraph);
        }
        "pre" => push_block(blocks, code_block(element)),
        "ul" => push_block(
            blocks,
            render_list_lines(element, false, 0, base_url)?.join("\n"),
        ),
        "ol" => push_block(
            blocks,
            render_list_lines(element, true, 0, base_url)?.join("\n"),
        ),
        "table" => {
            if let Some(table) = table_markdown(element, base_url)? {
                push_block(blocks, table);
            }
        }
        "blockquote" => {
            let mut quote_blocks = Vec::new();
            render_container(element, &mut quote_blocks, depth + 1, base_url)?;
            let quote = quote_blocks
                .join("\n\n")
                .lines()
                .map(|line| {
                    if line.is_empty() {
                        ">".to_owned()
                    } else {
                        format!("> {line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            push_block(blocks, quote);
        }
        "hr" => push_block(blocks, "---".into()),
        "dl" => {
            let mut term = String::new();
            for child in element.children().filter_map(ElementRef::wrap) {
                match child.value().name() {
                    "dt" => {
                        term.clear();
                        inline_children(child, &mut term, depth + 1, base_url)?;
                    }
                    "dd" => {
                        let mut definition = String::new();
                        inline_children(child, &mut definition, depth + 1, base_url)?;
                        push_block(
                            blocks,
                            format!("**{}**\n\n{}", term.trim(), definition.trim()),
                        );
                    }
                    _ => {}
                }
            }
        }
        _ => render_container(element, blocks, depth + 1, base_url)?,
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn extract(html: &str) -> Result<(Option<String>, String)> {
    extract_with_url(html, None)
}

pub(super) fn extract_with_url(
    html: &str,
    page_url: Option<&str>,
) -> Result<(Option<String>, String)> {
    let document = Html::parse_document(html);
    let title = Selector::parse("title")
        .ok()
        .and_then(|selector| document.select(&selector).next())
        .map(|element| element.text().collect::<String>().trim().to_owned());
    let root = Selector::parse("main, [role=main]")
        .ok()
        .and_then(|selector| {
            document
                .select(&selector)
                .max_by_key(|element| element.text().map(str::len).sum::<usize>())
        })
        .or_else(|| {
            Selector::parse("body")
                .ok()
                .and_then(|selector| document.select(&selector).next())
        })
        .unwrap_or_else(|| document.root_element());
    let base_url = page_url.and_then(|url| Url::parse(url).ok());
    let mut blocks = Vec::new();
    render_container(root, &mut blocks, 0, base_url.as_ref())?;
    Ok((title, blocks.join("\n\n")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn deeply_nested_reference_html_is_rejected_without_partial_capture() {
        let html = format!("{}reference{}", "<div>".repeat(300), "</div>".repeat(300));
        assert!(super::extract(&html).is_err());
    }

    #[test]
    fn identifiers_keep_their_underscores_while_emphasis_markers_stay_escaped() {
        let (_, text) = super::extract(
            "<html><body><article><p>Set MAX_PAGE_BYTES and snake_case_name, not _this_ or *that*.</p></article></body></html>",
        )
        .unwrap();
        assert_eq!(
            text,
            "Set MAX_PAGE_BYTES and snake_case_name, not \\_this\\_ or \\*that\\*."
        );
    }

    #[test]
    fn references_become_clean_structured_markdown() {
        let html = r#"
            <title>Reference</title><nav>Ignore menu</nav><main>
              <h1>Ownership</h1>
              <p>A <em>borrow</em> keeps <strong>ownership</strong>. Use <code>Vec&lt;T&gt;</code>.</p>
              <p>Read the <a href="/guide">complete guide</a>.</p>
              <pre><code class="language-rust">fn main() {
    let x = 1;
}</code></pre>
              <ol start="3"><li>Mix flour</li><li>Chill dough<ul><li>Keep it cold</li></ul></li></ol>
              <table><tr><th>Structure</th><th>Function</th></tr><tr><td>Chloroplast</td><td>Photosynthesis</td></tr></table>
              <blockquote><p>Evidence should remain attributable.</p></blockquote>
              <script>ignore()</script>
            </main>
        "#;
        let (title, text) = super::extract_with_url(html, Some("https://example.org/book/"))
            .expect("readable reference");
        assert_eq!(title.as_deref(), Some("Reference"));
        assert_eq!(
            text,
            "# Ownership\n\nA *borrow* keeps **ownership**. Use `Vec<T>`.\n\nRead the [complete guide](https://example.org/guide).\n\n```rust\nfn main() {\n    let x = 1;\n}\n```\n\n3. Mix flour\n4. Chill dough\n    - Keep it cold\n\n| Structure | Function |\n| --- | --- |\n| Chloroplast | Photosynthesis |\n\n> Evidence should remain attributable."
        );
        assert!(!text.contains("Ignore menu"));
        assert!(!text.contains("ignore()"));
        assert!(!text.contains("\n\n\n"));
    }

    #[test]
    fn layout_divs_do_not_turn_source_wrapping_into_separate_paragraphs() {
        let (_, text) = super::extract(
            "<main><div><div>Cargo stores output in the target directory. By default,\n this is inside the workspace.</div></div><div><p>Next paragraph.</p></div></main>",
        )
        .expect("readable reference");
        assert_eq!(
            text,
            "Cargo stores output in the target directory. By default, this is inside the workspace.\n\nNext paragraph."
        );
    }
}

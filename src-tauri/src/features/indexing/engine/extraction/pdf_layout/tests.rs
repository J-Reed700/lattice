//! Tests for run spacing, heading tiers and table detection on built PDF pages.

use super::fonts::{font_widths, glyph_width, looks_bold};
use super::*;
use lopdf::dictionary;

fn run(x: f32, y: f32, size: f32, text: &str) -> Run {
    Run {
        x,
        y,
        width: text.chars().map(glyph_width).sum::<f32>() * size,
        size,
        bold: false,
        measured: false,
        text: text.into(),
    }
}

fn page(lines: Vec<Line>) -> PageLayout {
    PageLayout {
        page: 1,
        lines,
        fallback_text: None,
        has_image: false,
    }
}

#[test]
fn body_size_is_the_size_that_sets_the_most_characters() {
    let pages = vec![page(group_lines(vec![
        run(50.0, 700.0, 24.0, "Title"),
        run(50.0, 660.0, 10.0, "a paragraph of ordinary body text"),
        run(50.0, 640.0, 10.0, "another paragraph of body text here"),
    ]))];

    assert_eq!(body_size(&pages), 10.0);
}

#[test]
fn runs_on_the_same_baseline_become_one_line_in_reading_order() {
    let lines = group_lines(vec![
        run(200.0, 700.0, 10.0, "second"),
        run(50.0, 700.3, 10.0, "first"),
        run(50.0, 680.0, 10.0, "next line"),
    ]);

    assert_eq!(lines.len(), 2);
    assert_eq!(lines.first().map(Line::text), Some("first second".into()));
    assert_eq!(lines.get(1).map(Line::text), Some("next line".into()));
}

#[test]
fn a_three_column_block_becomes_a_markdown_table() {
    let pages = vec![page(group_lines(vec![
        run(50.0, 700.0, 10.0, "Name"),
        run(200.0, 700.0, 10.0, "Role"),
        run(350.0, 700.0, 10.0, "Team"),
        run(50.0, 685.0, 10.0, "Ada"),
        run(200.0, 685.0, 10.0, "Engineer"),
        run(350.0, 685.0, 10.0, "Core"),
    ]))];

    let rendered = render(&pages).first().cloned().unwrap_or_default();

    assert_eq!(
        rendered,
        "| Name | Role | Team |\n| --- | --- | --- |\n| Ada | Engineer | Core |"
    );
}

#[test]
fn a_single_wide_row_is_not_a_table() {
    let pages = vec![page(group_lines(vec![
        run(50.0, 700.0, 10.0, "one"),
        run(200.0, 700.0, 10.0, "two"),
        run(350.0, 700.0, 10.0, "three"),
        run(50.0, 685.0, 10.0, "an ordinary sentence follows the header"),
    ]))];

    let rendered = render(&pages).first().cloned().unwrap_or_default();

    assert!(!rendered.contains("---"), "unexpected table: {rendered}");
}

#[test]
fn size_tiers_become_nested_markdown_headings() {
    let pages = vec![page(group_lines(vec![
        run(50.0, 700.0, 24.0, "Document Title"),
        run(50.0, 660.0, 14.0, "First Section"),
        run(
            50.0,
            640.0,
            10.0,
            "Body text that outweighs every heading on the page.",
        ),
        run(
            50.0,
            620.0,
            10.0,
            "More body text, so ten point is clearly the body.",
        ),
    ]))];

    let rendered = render(&pages).first().cloned().unwrap_or_default();

    assert!(rendered.starts_with("## Document Title\n"));
    assert!(rendered.contains("### First Section\n"));
    assert!(!rendered.contains("# Body text"));
}

#[test]
fn a_bold_line_at_body_size_is_still_a_heading() {
    let mut bold = run(50.0, 660.0, 10.0, "Bold Subheading");
    bold.bold = true;
    let pages = vec![page(group_lines(vec![
        run(50.0, 700.0, 20.0, "Title"),
        bold,
        run(
            50.0,
            640.0,
            10.0,
            "Body text that outweighs every heading here.",
        ),
        run(
            50.0,
            620.0,
            10.0,
            "More body text so ten point wins the mode.",
        ),
    ]))];

    let rendered = render(&pages).first().cloned().unwrap_or_default();

    assert!(rendered.contains("### Bold Subheading"), "{rendered}");
}

#[test]
fn an_overlong_line_is_never_promoted_to_a_heading() {
    let long = "word ".repeat(40);
    let pages = vec![page(group_lines(vec![
        run(50.0, 700.0, 24.0, &long),
        run(
            50.0,
            660.0,
            10.0,
            "Body text that outweighs the giant line.",
        ),
        run(
            50.0,
            640.0,
            10.0,
            "More body text so ten point wins the mode.",
        ),
    ]))];

    let rendered = render(&pages).first().cloned().unwrap_or_default();

    assert!(!rendered.contains('#'), "{rendered}");
}

#[test]
fn simple_font_widths_come_from_the_widths_array() {
    let doc = Document::new();
    let font = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "FirstChar" => 65,
        "Widths" => vec![722.into(), 500.into(), 250.into()],
    };

    let widths = font_widths(&doc, &font);

    assert!(widths.measured);
    assert!(!widths.two_byte);
    // "AB" plus a space the font does not list.
    let (width, glyphs, spaces) = widths.measure(b"AB ");
    assert!((width - (0.722 + 0.5 + 0.5)).abs() < 0.001, "{width}");
    assert_eq!((glyphs, spaces), (3, 1));
}

#[test]
fn cid_font_widths_parse_both_forms_of_the_w_array() {
    let mut doc = Document::new();
    let descendant = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "DW" => 1000,
        // 1 => [600 700], then the range 10..=11 => 300.
        "W" => vec![
            1.into(),
            vec![600.into(), 700.into()].into(),
            10.into(),
            11.into(),
            300.into(),
        ],
    });
    let font = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![descendant.into()],
    };

    let widths = font_widths(&doc, &font);

    assert!(widths.measured);
    assert!(widths.two_byte);
    // Codes 1, 2 and 10 as two-byte big-endian, then an unlisted code.
    let (width, glyphs, spaces) = widths.measure(&[0, 1, 0, 2, 0, 10, 0, 99]);
    assert!((width - (0.6 + 0.7 + 0.3 + 1.0)).abs() < 0.001, "{width}");
    // Word spacing never applies to two-byte codes, so no space is counted.
    assert_eq!((glyphs, spaces), (4, 0));
}

#[test]
fn a_font_without_metrics_falls_back_to_estimates() {
    let doc = Document::new();
    let font = dictionary! { "Type" => "Font", "Subtype" => "Type1" };

    assert!(!font_widths(&doc, &font).measured);
}

#[test]
fn bold_names_are_recognised_through_subset_prefixes() {
    assert!(looks_bold(b"ABCDEF+Helvetica-Bold"));
    assert!(!looks_bold(b"Helvetica"));
}

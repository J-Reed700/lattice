//! PDF extraction tests over PDFs generated in-test with lopdf.
//!
//! Building the fixtures from operators keeps the test honest about what the
//! extractor actually reads: font sizes, text positions and image XObjects,
//! rather than a checked-in binary nobody can inspect.

use super::*;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use std::path::PathBuf;

/// Font resource names installed on every generated page.
const REGULAR: &str = "F1";
const BOLD: &str = "F2";

/// Ten megabytes: every fixture is a few hundred bytes.
const NO_SIZE_LIMIT: u64 = 10 * 1024 * 1024;

struct TestPage {
    operations: Vec<Operation>,
    image: bool,
}

impl TestPage {
    fn text(operations: Vec<Operation>) -> Self {
        Self {
            operations,
            image: false,
        }
    }

    /// A scan: one image painted over the whole page, no text.
    fn scan() -> Self {
        Self {
            operations: image_operations(),
            image: true,
        }
    }
}

fn image_operations() -> Vec<Operation> {
    vec![
        Operation::new("q", vec![]),
        Operation::new(
            "cm",
            vec![
                612.into(),
                0.into(),
                0.into(),
                792.into(),
                0.into(),
                0.into(),
            ],
        ),
        Operation::new("Do", vec!["Im1".into()]),
        Operation::new("Q", vec![]),
    ]
}

/// One line of text set at `size` at the given page position.
fn line(x: f32, y: f32, size: f32, font: &str, text: &str) -> Vec<Operation> {
    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![font.into(), size.into()]),
        Operation::new("Td", vec![x.into(), y.into()]),
        Operation::new("Tj", vec![Object::string_literal(text)]),
        Operation::new("ET", vec![]),
    ]
}

/// Enough body copy that ten point is unambiguously the document's body size.
fn body_paragraphs(start_y: f32) -> Vec<Operation> {
    (0..4)
        .flat_map(|index| {
            line(
                72.0,
                start_y - index as f32 * 15.0,
                10.0,
                REGULAR,
                "Ordinary body copy that carries the bulk of the characters on this page.",
            )
        })
        .collect()
}

fn write_pdf(dir: &std::path::Path, name: &str, pages: Vec<TestPage>) -> PathBuf {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let regular = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let bold = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
    });
    // A one-pixel grey image is still an image XObject, which is all the
    // textless-page rule looks for.
    let image = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 1,
            "Height" => 1,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
        },
        vec![0u8],
    ));

    let mut page_ids = Vec::new();
    for page in pages {
        let mut resources = dictionary! {
            "Font" => dictionary! {
                REGULAR => regular,
                BOLD => bold,
            },
        };
        if page.image {
            resources.set("XObject", dictionary! { "Im1" => image });
        }
        let resources_id = doc.add_object(resources);
        let content = Content {
            operations: page.operations,
        };
        let content_id = doc.add_object(Stream::new(
            dictionary! {},
            content.encode().expect("content encodes"),
        ));
        page_ids.push(doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }));
    }

    let count = page_ids.len() as i64;
    let kids: Vec<Object> = page_ids.into_iter().map(Object::Reference).collect();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let path = dir.join(name);
    doc.save(&path).expect("fixture saves");
    path
}

/// An OCR provider that answers with a fixed phrase per page.
struct StubOcr;

#[async_trait::async_trait]
impl OcrPort for StubOcr {
    async fn recognize_page(
        &self,
        _document: &Path,
        page: u32,
    ) -> std::result::Result<String, OcrError> {
        Ok(format!("Recognized scan of page {page}"))
    }
}

/// Every page range must start where the previous one ended and the last must
/// reach the end of the text, or `embedding_input` rejects the extraction.
fn assert_ranges_cover(text: &str, ranges: &[PageRange]) {
    let mut covered = 0;
    for (_, start, end) in ranges {
        assert_eq!(
            *start, covered,
            "page ranges are not contiguous: {ranges:?}"
        );
        assert!(end >= start, "inverted range in {ranges:?}");
        covered = *end;
    }
    assert_eq!(covered, text.len(), "page ranges do not cover the text");
}

#[tokio::test]
async fn type_sizes_become_markdown_headings_for_the_chunker() {
    let dir = tempfile::tempdir().unwrap();
    let mut operations = line(72.0, 720.0, 24.0, REGULAR, "Annual Report");
    operations.extend(line(72.0, 690.0, 14.0, REGULAR, "Financial Summary"));
    operations.extend(body_paragraphs(660.0));
    let path = write_pdf(dir.path(), "report.pdf", vec![TestPage::text(operations)]);

    let extracted = extract_pdf(&path, NO_SIZE_LIMIT, None).await.unwrap();

    assert!(
        extracted.text.contains("## Annual Report"),
        "no top heading in:\n{}",
        extracted.text
    );
    assert!(
        extracted.text.contains("### Financial Summary"),
        "no second-tier heading in:\n{}",
        extracted.text
    );
    assert!(
        !extracted.text.contains("# Ordinary body copy"),
        "body copy was promoted to a heading:\n{}",
        extracted.text
    );
    assert!(extracted.needs_ocr.is_empty());
    assert_ranges_cover(&extracted.text, &extracted.page_ranges);
}

#[tokio::test]
async fn a_bold_line_at_body_size_is_a_heading_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut operations = line(72.0, 720.0, 24.0, REGULAR, "Annual Report");
    operations.extend(line(72.0, 690.0, 10.0, BOLD, "Risk Factors"));
    operations.extend(body_paragraphs(660.0));
    let path = write_pdf(dir.path(), "bold.pdf", vec![TestPage::text(operations)]);

    let extracted = extract_pdf(&path, NO_SIZE_LIMIT, None).await.unwrap();

    assert!(
        extracted.text.contains("# Risk Factors"),
        "bold subheading was not marked up:\n{}",
        extracted.text
    );
}

#[tokio::test]
async fn aligned_columns_become_a_markdown_table() {
    let dir = tempfile::tempdir().unwrap();
    let mut operations = body_paragraphs(720.0);
    let rows = [
        ["Region", "Revenue", "Growth"],
        ["North", "412", "9%"],
        ["South", "318", "4%"],
    ];
    for (index, row) in rows.iter().enumerate() {
        let y = 620.0 - index as f32 * 15.0;
        for (column, cell) in row.iter().enumerate() {
            let x = 72.0 + column as f32 * 150.0;
            operations.extend(line(x, y, 10.0, REGULAR, cell));
        }
    }
    let path = write_pdf(dir.path(), "table.pdf", vec![TestPage::text(operations)]);

    let extracted = extract_pdf(&path, NO_SIZE_LIMIT, None).await.unwrap();

    assert!(
        extracted.text.contains("| Region | Revenue | Growth |"),
        "no header row in:\n{}",
        extracted.text
    );
    assert!(
        extracted.text.contains("| --- | --- | --- |"),
        "no separator row in:\n{}",
        extracted.text
    );
    assert!(
        extracted.text.contains("| North | 412 | 9% |"),
        "no body row in:\n{}",
        extracted.text
    );
}

#[tokio::test]
async fn a_scanned_page_is_reported_instead_of_failing_the_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_pdf(
        dir.path(),
        "mixed.pdf",
        vec![
            TestPage::text(body_paragraphs(720.0)),
            TestPage::scan(),
            TestPage::text(body_paragraphs(720.0)),
        ],
    );

    let extracted = extract_pdf(&path, NO_SIZE_LIMIT, None).await.unwrap();

    assert_eq!(extracted.needs_ocr, vec![2]);
    assert_eq!(extracted.page_ranges.len(), 3);
    assert!(extracted.text.contains("Ordinary body copy"));
    assert_ranges_cover(&extracted.text, &extracted.page_ranges);
}

#[tokio::test]
async fn a_fully_scanned_pdf_still_asks_for_ocr() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_pdf(
        dir.path(),
        "scanned.pdf",
        vec![TestPage::scan(), TestPage::scan()],
    );

    let without_provider = extract_pdf(&path, NO_SIZE_LIMIT, None)
        .await
        .unwrap_err()
        .to_string();
    let noop: Arc<dyn OcrPort> = Arc::new(crate::application::ports::NoopOcr);
    let with_noop = extract_pdf(&path, NO_SIZE_LIMIT, Some(&noop))
        .await
        .unwrap_err()
        .to_string();

    assert!(without_provider.contains("OCR"), "{without_provider}");
    assert!(with_noop.contains("OCR"), "{with_noop}");
}

#[tokio::test]
async fn recognized_text_lands_inside_its_own_page_range() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_pdf(
        dir.path(),
        "ocr.pdf",
        vec![
            TestPage::text(body_paragraphs(720.0)),
            TestPage::scan(),
            TestPage::text(body_paragraphs(720.0)),
        ],
    );
    let ocr: Arc<dyn OcrPort> = Arc::new(StubOcr);

    let extracted = extract_pdf(&path, NO_SIZE_LIMIT, Some(&ocr)).await.unwrap();

    assert!(extracted.needs_ocr.is_empty());
    assert_ranges_cover(&extracted.text, &extracted.page_ranges);
    let (_, start, end) = extracted
        .page_ranges
        .iter()
        .find(|(page, _, _)| *page == 2)
        .copied()
        .expect("page two has a range");
    assert_eq!(&extracted.text[start..end], "\nRecognized scan of page 2");
}

#[test]
fn failed_page_does_not_return_a_partial_document() {
    let result = collect_pdf_pages(Path::new("mpep.pdf"), 1..=3, |page| {
        if page == 2 {
            Err(IndexingError::Other("Unreadable font".into()))
        } else {
            Ok(format!("Text from page {page}"))
        }
    });
    let error = result.unwrap_err().to_string();
    assert!(error.contains("mpep.pdf"));
    assert!(error.contains("page 2"));
    assert!(error.contains("Unreadable font"));
    assert!(error.contains("not indexed"));
}

#[test]
fn successful_pages_keep_their_citation_ranges() {
    let pages = vec![
        PageText {
            page: 1,
            text: "Patent".into(),
            needs_ocr: false,
        },
        PageText {
            page: 2,
            text: "Examination".into(),
            needs_ocr: false,
        },
    ];

    let (text, ranges) = assemble(&pages);

    assert_eq!(text, "Patent\nExamination");
    assert_eq!(ranges, vec![(1, 0, 6), (2, 6, 18)]);
}

#[test]
fn only_an_entirely_textless_document_is_unreadable() {
    let scan = |page| PageText {
        page,
        text: String::new(),
        needs_ocr: true,
    };
    let readable = PageText {
        page: 1,
        text: "Examination guidance".into(),
        needs_ocr: false,
    };

    assert!(is_unreadable("", &[scan(1), scan(2)]));
    assert!(is_unreadable(
        "Page 2",
        &[scan(1), scan(2)],
        // Text that is only folio numbers still means every page is a scan.
    ));
    assert!(!is_unreadable("Examination guidance", &[readable, scan(2)]));
}

#[test]
fn a_running_header_does_not_hide_a_scanned_page() {
    let scan_with_header = "Rev. 01.2024, November 2024\nMPEP CHAPTER 2800\n2800-3";

    assert!(body_alphanumerics(scan_with_header) < MIN_PAGE_ALPHANUMERICS);
    assert!(
        body_alphanumerics("Header\nReal body text that the page genuinely carries.\nFooter")
            >= MIN_PAGE_ALPHANUMERICS
    );
}

#[test]
fn recognized_text_is_appended_to_whatever_the_page_did_have() {
    assert_eq!(splice("", "Recognized"), "Recognized");
    assert_eq!(splice("12", "Recognized"), "12\nRecognized");
}

/// Smoke test against a real PDF, following the `live_tests` convention used
/// elsewhere in indexing: opt-in, read-only, and never part of the gate.
///
/// Run with `LATTICE_LAYOUT_PDF=/path/to.pdf cargo test --lib
/// real_pdf_keeps_its_structure -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "requires LATTICE_LAYOUT_PDF (read-only)"]
async fn real_pdf_keeps_its_structure_and_page_ranges() {
    let path = PathBuf::from(std::env::var("LATTICE_LAYOUT_PDF").unwrap());

    let extracted = extract_pdf(&path, 200 * 1024 * 1024, None).await.unwrap();

    assert_ranges_cover(&extracted.text, &extracted.page_ranges);
    let headings: Vec<&str> = extracted
        .text
        .lines()
        .filter(|line| line.starts_with('#'))
        .take(40)
        .collect();
    let tables: usize = extracted
        .text
        .lines()
        .filter(|line| line.starts_with("| --- "))
        .count();
    // Print one page in full: the first that carries a real body of text, or
    // the page named by LATTICE_LAYOUT_PAGE.
    let wanted: Option<usize> = std::env::var("LATTICE_LAYOUT_PAGE")
        .ok()
        .and_then(|page| page.parse().ok());
    let sample = extracted
        .page_ranges
        .iter()
        .find(|(page, start, end)| match wanted {
            Some(wanted) => *page == wanted,
            None => end - start > 1500,
        })
        .map(|(page, start, end)| format!("--- page {page} ---\n{}", &extracted.text[*start..*end]))
        .unwrap_or_default();
    println!(
        "pages={} chars={} needs_ocr={:?} tables={tables}\nheadings:\n{}\n{sample}",
        extracted.page_ranges.len(),
        extracted.text.chars().count(),
        extracted.needs_ocr,
        headings.join("\n"),
    );
}

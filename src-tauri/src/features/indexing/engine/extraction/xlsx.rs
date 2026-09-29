//! Excel (XLSX) extraction.

use super::archive_budget::{run_archive_work, ArchiveBudget, MAX_ARCHIVE_MEMBERS};
use super::markup::{attr, decode_entities, Markup, MarkupCursor};
use super::types::{ContentMetadata, ExtractedContent};
use crate::features::indexing::engine::error::{IndexingError, Result};
use std::borrow::Cow;
use std::fs::File;
use std::path::Path;
use zip::ZipArchive;

const MAX_WORKSHEETS: usize = 256;
const MAX_ROWS_PER_WORKSHEET: usize = 100_000;
const MAX_COLUMNS: usize = 16_384;
const MAX_GRID_CELLS: usize = 1_000_000;
const MAX_EXPANDED_CELL_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SHARED_STRINGS: usize = 250_000;
const MAX_SHARED_STRING_BYTES: usize = 8 * 1024 * 1024;
const MAX_OUTPUT_TEXT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SHEET_NAME_BYTES: usize = 256;

#[derive(Default)]
struct XlsxBudget {
    grid_cells: usize,
    expanded_text_bytes: usize,
    output_bytes: usize,
}

impl XlsxBudget {
    fn allocate_grid_cells(&mut self, count: usize, path: &Path) -> Result<()> {
        let total = self.grid_cells.saturating_add(count);
        if total > MAX_GRID_CELLS {
            return Err(IndexingError::ContentExtraction {
                path: path.display().to_string(),
                reason: format!("worksheet grid exceeds {MAX_GRID_CELLS} allocated cells"),
            });
        }
        self.grid_cells = total;
        Ok(())
    }

    fn add_expanded_text(&mut self, bytes: usize, path: &Path) -> Result<()> {
        let total = self.expanded_text_bytes.saturating_add(bytes);
        if total > MAX_EXPANDED_CELL_TEXT_BYTES {
            return Err(IndexingError::ContentExtraction {
                path: path.display().to_string(),
                reason: format!(
                    "expanded worksheet cell text exceeds {MAX_EXPANDED_CELL_TEXT_BYTES} bytes"
                ),
            });
        }
        self.expanded_text_bytes = total;
        Ok(())
    }

    fn add_output_bytes(&mut self, bytes: usize, path: &Path) -> Result<()> {
        let total = self.output_bytes.saturating_add(bytes);
        if total > MAX_OUTPUT_TEXT_BYTES {
            return Err(IndexingError::ContentExtraction {
                path: path.display().to_string(),
                reason: format!("expanded XLSX output exceeds {MAX_OUTPUT_TEXT_BYTES} bytes"),
            });
        }
        self.output_bytes = total;
        Ok(())
    }
}

/// Extract content from XLSX files.
pub async fn extract_xlsx(path: &Path, max_file_size: u64) -> Result<ExtractedContent> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| IndexingError::Io {
            message: e.to_string(),
            kind: format!("{:?}", e.kind()),
        })?;

    let file_size = metadata.len();
    if file_size > max_file_size {
        return Err(IndexingError::FileTooLarge {
            path: path.display().to_string(),
            size_bytes: file_size,
            max_size_bytes: max_file_size,
        });
    }

    let path_clone = path.to_path_buf();
    let text = run_archive_work(path.to_path_buf(), move || extract_xlsx_sync(&path_clone)).await?;

    let metadata = ContentMetadata {
        page_count: None,
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        language: None,
    };

    Ok(ExtractedContent {
        text,
        mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
        metadata,
        page_ranges: vec![],
        needs_ocr: Vec::new(),
    })
}

fn extract_xlsx_sync(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;

    let mut archive = ZipArchive::new(file).map_err(|e| IndexingError::ContentExtraction {
        path: path.display().to_string(),
        reason: format!("Failed to open XLSX as ZIP: {}", e),
    })?;
    let mut budget = ArchiveBudget::default();
    budget.check_members(&archive, path)?;

    let shared = match read_optional_part(&mut archive, "xl/sharedStrings.xml", path, &mut budget)?
    {
        Some(xml) => shared_strings(&xml, path)?,
        None => Vec::new(),
    };

    let mut sections = Vec::new();
    let mut xlsx_budget = XlsxBudget::default();
    for (name, part) in sheet_parts(&mut archive, path, &mut budget)? {
        let Some(xml) = read_optional_part(&mut archive, &part, path, &mut budget)? else {
            continue;
        };
        let rows = format_rows(
            &sheet_rows(&xml, &shared, path, &mut xlsx_budget)?,
            path,
            &mut xlsx_budget,
        )?;
        if !rows.is_empty() {
            xlsx_budget.add_output_bytes(name.len().saturating_add(10), path)?;
            // A heading per sheet gives each sheet its own section span.
            sections.push(format!("# {name}\n{rows}"));
        }
    }

    Ok(sections.join("\n\n"))
}

fn read_optional_part(
    archive: &mut ZipArchive<File>,
    name: &str,
    path: &Path,
    budget: &mut ArchiveBudget,
) -> Result<Option<String>> {
    match archive.by_name(name) {
        Ok(_) => {}
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => {
            return Err(IndexingError::ContentExtraction {
                path: path.display().to_string(),
                reason: format!("Failed to open {name}: {e}"),
            })
        }
    }
    Ok(Some(budget.read_part(archive, name, path)?))
}

/// `(sheet name, worksheet part)` in the workbook's tab order. The workbook
/// lists sheets in tab order and its relationships map each to a part; a file
/// missing either falls back to the worksheet parts by number.
fn sheet_parts(
    archive: &mut ZipArchive<File>,
    path: &Path,
    budget: &mut ArchiveBudget,
) -> Result<Vec<(String, String)>> {
    let workbook = read_optional_part(archive, "xl/workbook.xml", path, budget)?;
    let rels = read_optional_part(archive, "xl/_rels/workbook.xml.rels", path, budget)?;
    if let (Some(workbook), Some(rels)) = (workbook, rels) {
        let targets: std::collections::HashMap<String, String> = MarkupCursor::new(&rels)
            .filter_map(|event| match event {
                Markup::Open {
                    name: "Relationship",
                    attrs,
                    ..
                } => Some((attr(attrs, "Id")?, attr(attrs, "Target")?)),
                _ => None,
            })
            .collect();
        let sheets: Vec<(String, String)> = MarkupCursor::new(&workbook)
            .filter_map(|event| match event {
                Markup::Open {
                    name: "sheet",
                    attrs,
                    ..
                } => {
                    let target = targets.get(&attr(attrs, "r:id")?)?;
                    let part = match target.strip_prefix('/') {
                        Some(absolute) => absolute.to_owned(),
                        None => format!("xl/{target}"),
                    };
                    Some((attr(attrs, "name")?, part))
                }
                _ => None,
            })
            .collect();
        if !sheets.is_empty() {
            if sheets.len() > MAX_WORKSHEETS {
                return Err(IndexingError::ContentExtraction {
                    path: path.display().to_string(),
                    reason: format!(
                        "workbook has {} sheets; limit is {MAX_WORKSHEETS}",
                        sheets.len()
                    ),
                });
            }
            if sheets
                .iter()
                .any(|(name, _)| name.len() > MAX_SHEET_NAME_BYTES)
            {
                return Err(IndexingError::ContentExtraction {
                    path: path.display().to_string(),
                    reason: format!("worksheet name exceeds {MAX_SHEET_NAME_BYTES} bytes"),
                });
            }
            return Ok(sheets);
        }
    }
    let mut numbered: Vec<(u32, String)> = archive
        .file_names()
        .filter_map(|name| {
            let number = name
                .strip_prefix("xl/worksheets/sheet")?
                .strip_suffix(".xml")?
                .parse()
                .ok()?;
            Some((number, name.to_owned()))
        })
        .collect();
    numbered.sort();
    if numbered.len() > MAX_WORKSHEETS || numbered.len() > MAX_ARCHIVE_MEMBERS {
        return Err(IndexingError::ContentExtraction {
            path: path.display().to_string(),
            reason: format!(
                "workbook has too many worksheets ({}); limit is {MAX_WORKSHEETS}",
                numbered.len()
            ),
        });
    }
    Ok(numbered
        .into_iter()
        .map(|(number, part)| (format!("Sheet{number}"), part))
        .collect())
}

/// The shared string table, one entry per `<si>`. Rich text splits a string
/// across several `<r><t>` runs, which join back into one value; `<rPh>`
/// holds phonetic guides, not cell text.
fn shared_strings(xml: &str, path: &Path) -> Result<Vec<String>> {
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut total_bytes = 0usize;
    let mut in_text = false;
    let mut in_phonetic = false;
    for event in MarkupCursor::new(xml) {
        match event {
            Markup::Open { name: "si", .. } => current.clear(),
            Markup::Close { name: "si" } => {
                if strings.len() >= MAX_SHARED_STRINGS {
                    return Err(IndexingError::ContentExtraction {
                        path: path.display().to_string(),
                        reason: format!("shared string table exceeds {MAX_SHARED_STRINGS} entries"),
                    });
                }
                total_bytes = total_bytes.saturating_add(current.len());
                if total_bytes > MAX_SHARED_STRING_BYTES {
                    return Err(IndexingError::ContentExtraction {
                        path: path.display().to_string(),
                        reason: format!(
                            "shared string text exceeds {MAX_SHARED_STRING_BYTES} bytes"
                        ),
                    });
                }
                strings.push(std::mem::take(&mut current));
            }
            Markup::Open {
                name: "rPh", empty, ..
            } => in_phonetic = !empty,
            Markup::Close { name: "rPh" } => in_phonetic = false,
            Markup::Open {
                name: "t", empty, ..
            } => in_text = !empty,
            Markup::Close { name: "t" } => in_text = false,
            Markup::Text(text) if in_text && !in_phonetic => {
                current.push_str(&decode_entities(text))
            }
            _ => {}
        }
    }
    Ok(strings)
}

/// Rows of cell values, each placed at its column from the `r="B7"` reference
/// so empty cells (which Excel omits) do not shift later values left.
fn sheet_rows(
    xml: &str,
    shared: &[String],
    path: &Path,
    budget: &mut XlsxBudget,
) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell_type = String::new();
    let mut column = 0usize;
    let mut value = String::new();
    let mut in_value = false;

    for event in MarkupCursor::new(xml) {
        match event {
            Markup::Open {
                name: "row",
                attrs,
                empty,
            } => {
                if let Some(reference) = attr(attrs, "r") {
                    let index = reference.parse::<usize>().map_err(|_| {
                        IndexingError::ContentExtraction {
                            path: path.display().to_string(),
                            reason: format!("row reference {reference} is malformed"),
                        }
                    })?;
                    if index == 0 || index > MAX_ROWS_PER_WORKSHEET {
                        return Err(IndexingError::ContentExtraction {
                            path: path.display().to_string(),
                            reason: format!("row reference {reference} exceeds the {MAX_ROWS_PER_WORKSHEET}-row extraction limit"),
                        });
                    }
                }
                if rows.len() >= MAX_ROWS_PER_WORKSHEET {
                    return Err(IndexingError::ContentExtraction {
                        path: path.display().to_string(),
                        reason: format!("worksheet exceeds {MAX_ROWS_PER_WORKSHEET} rows"),
                    });
                }
                row.clear();
                column = 0;
                if empty {
                    rows.push(Vec::new());
                }
            }
            Markup::Close { name: "row" } => rows.push(std::mem::take(&mut row)),
            Markup::Open {
                name: "c",
                attrs,
                empty,
            } => {
                if let Some(reference) = attr(attrs, "r") {
                    let index = column_index(&reference).ok_or_else(|| {
                        IndexingError::ContentExtraction {
                            path: path.display().to_string(),
                            reason: format!("cell reference {reference} is malformed or overflows"),
                        }
                    })?;
                    if index >= MAX_COLUMNS {
                        return Err(IndexingError::ContentExtraction {
                            path: path.display().to_string(),
                            reason: format!("cell reference {reference} exceeds Excel's {MAX_COLUMNS}-column limit"),
                        });
                    }
                    column = index;
                }
                if column >= MAX_COLUMNS {
                    return Err(IndexingError::ContentExtraction {
                        path: path.display().to_string(),
                        reason: format!("worksheet uses more than {MAX_COLUMNS} columns"),
                    });
                }
                cell_type = attr(attrs, "t").unwrap_or_default();
                value.clear();
                if empty {
                    column += 1;
                }
            }
            Markup::Close { name: "c" } => {
                let text = cell_text(&cell_type, &value, shared);
                if !text.is_empty() {
                    if row.len() <= column {
                        let new_slots = column + 1 - row.len();
                        budget.allocate_grid_cells(new_slots, path)?;
                        row.resize(column + 1, String::new());
                    }
                    if let Some(cell) = row.get_mut(column) {
                        budget.add_expanded_text(text.len(), path)?;
                        *cell = text.into_owned();
                    }
                }
                column += 1;
            }
            // `<v>` holds numbers, booleans, shared-string indexes and cached
            // formula results; `<is><t>` holds an inline string.
            Markup::Open {
                name: "v" | "t",
                empty,
                ..
            } => in_value = !empty,
            Markup::Close { name: "v" | "t" } => in_value = false,
            Markup::Text(text) if in_value => value.push_str(&decode_entities(text)),
            _ => {}
        }
    }
    Ok(rows)
}

fn cell_text<'a>(cell_type: &str, value: &'a str, shared: &'a [String]) -> Cow<'a, str> {
    let value = value.trim();
    match cell_type {
        "s" => value
            .parse::<usize>()
            .ok()
            .and_then(|i| shared.get(i))
            .map(|value| Cow::Borrowed(value.as_str()))
            .unwrap_or(Cow::Borrowed("")),
        "b" => match value {
            "1" => Cow::Borrowed("TRUE"),
            "0" => Cow::Borrowed("FALSE"),
            other => Cow::Borrowed(other),
        },
        _ => Cow::Borrowed(value),
    }
}

/// Zero-based column of a cell reference: `A1` → 0, `AB12` → 27.
fn column_index(reference: &str) -> Option<usize> {
    let split = reference
        .bytes()
        .position(|byte| !byte.is_ascii_alphabetic())?;
    if !reference
        .as_bytes()
        .get(split)
        .is_some_and(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let mut letters = reference.bytes().take(split);
    let first = letters.next()?;
    let mut index = usize::from(first.to_ascii_uppercase() - b'A' + 1);
    for letter in letters {
        index = index
            .checked_mul(26)?
            .checked_add(usize::from(letter.to_ascii_uppercase() - b'A' + 1))?;
    }
    Some(index - 1)
}

/// `header: value` lines, one per row, as the CSV extractor writes them: a
/// row read on its own still says what each number means. The first
/// non-empty row is the header; a column without one is named by its letter.
fn format_rows(rows: &[Vec<String>], path: &Path, budget: &mut XlsxBudget) -> Result<String> {
    let mut remaining_rows = rows
        .iter()
        .skip_while(|row| row.iter().all(String::is_empty));
    let Some(header) = remaining_rows.next() else {
        return Ok(String::new());
    };
    let data_rows = remaining_rows.filter(|row| row.iter().any(|v| !v.is_empty()));
    // Estimate the fully expanded representation before formatting. Reusing
    // a large shared-string header on many rows can be much larger than the
    // worksheet XML itself.
    let mut estimate = 0usize;
    let mut line_count = 0usize;
    for row in data_rows.clone() {
        for (parts, (index, value)) in row
            .iter()
            .enumerate()
            .filter(|(_, value)| !value.is_empty())
            .enumerate()
        {
            let label_len = header
                .get(index)
                .filter(|label| !label.is_empty())
                .map_or_else(|| column_name(index).len(), String::len);
            estimate = estimate
                .saturating_add(label_len)
                .saturating_add(2) // ": "
                .saturating_add(value.len());
            if parts > 0 {
                estimate = estimate.saturating_add(2); // ", "
            }
        }
        line_count += 1;
    }
    if line_count == 0 {
        let values: Vec<&String> = header.iter().filter(|value| !value.is_empty()).collect();
        estimate = values.iter().map(|value| value.len()).sum();
        estimate = estimate.saturating_add(values.len().saturating_sub(1).saturating_mul(2));
    } else {
        estimate = estimate.saturating_add(line_count.saturating_sub(1));
    }
    budget.add_output_bytes(estimate, path)?;

    let mut lines = Vec::new();
    for row in data_rows {
        let parts: Vec<String> = row
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.is_empty())
            .map(|(i, v)| match header.get(i).filter(|h| !h.is_empty()) {
                Some(h) => format!("{h}: {v}"),
                None => format!("{}: {v}", column_name(i)),
            })
            .collect();
        lines.push(parts.join(", "));
    }
    if lines.is_empty() {
        // A sheet with a single row has no header to pair; keep the values.
        return Ok(header
            .iter()
            .filter(|v| !v.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(", "));
    }
    Ok(lines.join("\n"))
}

fn column_name(mut index: usize) -> String {
    let mut name = Vec::new();
    loop {
        name.push(b'A' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    name.reverse();
    String::from_utf8_lossy(&name).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const WORKBOOK: &str = r#"<?xml version="1.0"?><workbook xmlns:r="r"><sheets><sheet name="Budget" sheetId="1" r:id="rId2"/><sheet name="Notes" sheetId="2" r:id="rId1"/></sheets></workbook>"#;
    const RELS: &str = r#"<?xml version="1.0"?><Relationships><Relationship Id="rId1" Type="worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="worksheet" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Type="styles" Target="styles.xml"/></Relationships>"#;
    const SHARED: &str = r#"<?xml version="1.0"?><sst count="5" uniqueCount="5"><si><t>Item</t></si><si><t>Cost</t></si><si><r><t>Rent</t></r><r><rPr><b/></rPr><t xml:space="preserve"> &amp; utilities</t></r><rPh><t>ignored</t></rPh></si><si><t>Food</t></si><si><t>Reminder</t></si></sst>"#;
    // Header row, a shared-string column, a numeric column, a skipped cell
    // (D2 has no C2 before it), a boolean and an inline string.
    const BUDGET: &str = r#"<?xml version="1.0"?><worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1" t="inlineStr"><is><t>Paid</t></is></c></row><row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>1250.5</v></c><c r="D2" t="b"><v>1</v></c></row><row r="3"><c r="A3" t="s"><v>3</v></c><c r="B3" s="1"><f>SUM(B1:B2)</f><v>300</v></c></row></sheetData></worksheet>"#;
    const NOTES: &str = r#"<?xml version="1.0"?><worksheet><sheetData><row r="1"><c r="A1" t="s"><v>4</v></c></row></sheetData></worksheet>"#;

    #[tokio::test]
    async fn rows_keep_numbers_and_pair_each_value_with_its_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("budget.xlsx");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, body) in [
            ("xl/workbook.xml", WORKBOOK),
            ("xl/_rels/workbook.xml.rels", RELS),
            ("xl/sharedStrings.xml", SHARED),
            ("xl/worksheets/sheet1.xml", NOTES),
            ("xl/worksheets/sheet2.xml", BUDGET),
        ] {
            zip.start_file(name, zip::write::FileOptions::default())
                .unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();

        let text = extract_xlsx(&path, 1 << 20).await.unwrap().text;
        assert_eq!(
            text,
            "# Budget\nItem: Rent & utilities, Cost: 1250.5, D: TRUE\nItem: Food, Cost: 300\n\n# Notes\nReminder"
        );
    }

    #[test]
    fn column_references_round_trip() {
        assert_eq!(column_index("A1"), Some(0));
        assert_eq!(column_index("AB12"), Some(27));
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_name(3), "D");
    }

    #[test]
    fn worksheet_rejects_columns_beyond_excel_dimensions_and_too_many_rows() {
        let path = Path::new("oversized.xlsx");
        let mut budget = XlsxBudget::default();
        assert!(sheet_rows(
            r#"<worksheet><sheetData><row><c r="XFE1"><v>1</v></c></row></sheetData></worksheet>"#,
            &[],
            path,
            &mut budget,
        )
        .is_err());
        let mut budget = XlsxBudget::default();
        assert!(sheet_rows(
            r#"<worksheet><sheetData><row r="100001"><c r="A100001"><v>1</v></c></row></sheetData></worksheet>"#,
            &[],
            path,
            &mut budget,
        )
        .is_err());

        let mut xml = String::from("<worksheet><sheetData>");
        for _ in 0..=MAX_ROWS_PER_WORKSHEET {
            xml.push_str("<row></row>");
        }
        xml.push_str("</sheetData></worksheet>");
        assert!(sheet_rows(&xml, &[], path, &mut XlsxBudget::default()).is_err());
    }

    #[test]
    fn sparse_grid_and_repeated_shared_strings_obey_expanded_memory_budgets() {
        let path = Path::new("sparse.xlsx");
        let mut xml = String::from("<worksheet><sheetData>");
        for row in 1..=64 {
            xml.push_str(&format!(
                "<row r=\"{row}\"><c r=\"XFD{row}\"><v>1</v></c></row>"
            ));
        }
        xml.push_str("</sheetData></worksheet>");
        assert!(sheet_rows(&xml, &[], path, &mut XlsxBudget::default()).is_err());

        let shared = vec!["x".repeat(MAX_EXPANDED_CELL_TEXT_BYTES - 32)];
        let repeated = r#"<worksheet><sheetData><row><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>0</v></c></row></sheetData></worksheet>"#;
        assert!(sheet_rows(repeated, &shared, path, &mut XlsxBudget::default()).is_err());

        let mut budget = XlsxBudget::default();
        let rows = sheet_rows(
            r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row><row r="2"><c r="A2"><v>1</v></c></row><row r="3"><c r="A3"><v>1</v></c></row><row r="4"><c r="A4"><v>1</v></c></row></sheetData></worksheet>"#,
            &shared,
            path,
            &mut budget,
        )
        .unwrap();
        assert!(format_rows(&rows, path, &mut budget).is_err());
    }
}

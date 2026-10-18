//! Date parsing, one rule each. The link scanner is core's
//! (`domain::provenance::markdown_links`), tested there.

use super::dates::parse_date;
use chrono::DateTime;

fn at(text: &str) -> i64 {
    DateTime::parse_from_rfc3339(text).unwrap().timestamp()
}

#[test]
fn page_dates_parse_in_the_forms_pages_write_them() {
    let noon = at("2026-03-14T12:00:00Z");
    for written in [
        "2026-03-14T12:00:00Z",
        "2026-03-14T08:00:00-04:00",
        "2026-03-14T12:00:00.000+0000",
        "2026-03-14T12:00:00",
        "2026-03-14 12:00",
        "Sat, 14 Mar 2026 12:00:00 GMT",
    ] {
        assert_eq!(parse_date(written), Some(noon), "{written}");
    }
    assert_eq!(parse_date("2026/03/14"), Some(at("2026-03-14T00:00:00Z")));
    for unparsed in ["", "last Tuesday", "2026-13-45", "14/03/2026"] {
        assert_eq!(parse_date(unparsed), None, "{unparsed}");
    }
}

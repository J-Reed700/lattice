use super::*;

fn allowed(url: &str) -> bool {
    navigation_allowed(&Url::parse(url).unwrap())
}

#[test]
fn only_the_public_web_is_reachable() {
    assert!(allowed(
        "https://www.thespruce.com/best-houseplants-for-sun-4147670"
    ));
    assert!(allowed("http://example.com/"));
    assert!(allowed("https://93.184.216.34/"));
    assert!(allowed("about:blank"));
    assert!(allowed("about:srcdoc"));

    for blocked in [
        "file:///etc/passwd",
        "ftp://example.com/",
        "tauri://localhost/",
        "javascript:alert(1)",
        "http://localhost:1420/",
        "http://app.localhost/",
        "http://127.0.0.1:8080/",
        "http://10.0.0.1/",
        "http://192.168.1.1/",
        "http://169.254.169.254/latest/meta-data/",
        "http://[::1]/",
        "http://[fd00::1]/",
        "http://[::ffff:127.0.0.1]/",
        "http://100.64.0.1/",
        "http://0.0.0.0/",
        "http://printer.local/",
        "http://router/",
    ] {
        assert!(!allowed(blocked), "{blocked} should be refused");
    }
}

fn report(text: &str) -> Report {
    Report {
        title: "Title".to_string(),
        text: text.to_string(),
        blocked: false,
        part: 0,
        parts: 1,
        snapshot: "first".to_string(),
    }
}

#[test]
fn a_page_is_located_by_the_browser_not_by_its_report() {
    // The page's script claims to be Wikipedia; the webview says otherwise.
    let payload = serde_json::json!({
        "url": "https://en.wikipedia.org/wiki/Rust",
        "title": "Rust",
        "text": "forged"
    })
    .to_string();
    let posted = Url::parse(&format!(
        "https://{REPORT_HOST}/#{}",
        urlencoding::encode(&payload)
    ))
    .unwrap();
    let forged = decode_report(&posted).expect("decodes");
    let landed = Url::parse("https://attacker.example/page").unwrap();

    let page = page_from_report(forged, Some(landed)).unwrap();
    assert_eq!(page.final_url, "https://attacker.example/page");
    assert_eq!(page.text, "forged");
}

#[test]
fn a_page_the_browser_cannot_place_is_not_returned() {
    assert!(page_from_report(report("text"), None).is_err());
    for landed in ["about:blank", "data:text/html,hi", "http://192.168.1.1/"] {
        let landed = Url::parse(landed).unwrap();
        assert!(page_from_report(report("text"), Some(landed)).is_err());
    }
    let blocked = Report {
        blocked: true,
        ..report("text")
    };
    let landed = Url::parse("https://example.com/").unwrap();
    assert!(page_from_report(blocked, Some(landed)).is_err());
}

#[test]
fn a_report_is_decoded_from_the_fragment() {
    let payload = serde_json::json!({
        "title": "A — page",
        "text": "First paragraph.\n\nSecond, with #hash & ünïcode."
    })
    .to_string();
    let url = Url::parse(&format!(
        "https://{REPORT_HOST}/#{}",
        urlencoding::encode(&payload)
    ))
    .unwrap();
    let report = decode_report(&url).expect("decodes");
    assert_eq!(report.title, "A — page");
    assert!(report.text.contains("ünïcode"));
}

fn piece(part: usize, parts: usize, snapshot: &str, text: &str) -> Report {
    Report {
        part,
        parts,
        snapshot: snapshot.to_string(),
        ..report(text)
    }
}

/// A page's text goes in parts, which arrive in any order and more than once,
/// and come back together whole.
#[test]
fn a_report_sent_in_parts_comes_back_whole() {
    let mut assembly = Assembly::default();
    assert!(assembly.add(piece(2, 3, "a", "three")).is_none());
    assert!(assembly.add(piece(0, 3, "a", "one ")).is_none());
    assert!(assembly.add(piece(2, 3, "a", "ignored repeat")).is_none());
    let whole = assembly
        .add(piece(1, 3, "a", "two "))
        .expect("every part is in");
    assert_eq!(whole.text, "one two three");
    assert_eq!(whole.title, "Title");

    let mut single = Assembly::default();
    assert_eq!(
        single.add(report("short")).map(|r| r.text).as_deref(),
        Some("short")
    );
    assert!(Assembly::default()
        .add(piece(7, 3, "a", "out of range"))
        .is_none());
}

/// The part count is the page's to write. A page claiming 10^15 parts made
/// the reader allocate a slot for each and abort the app; it is refused
/// before anything is kept, and the refusal is remembered for the error.
#[test]
fn a_report_claiming_more_parts_than_the_reader_takes_is_refused_without_allocating() {
    let mut assembly = Assembly::default();
    assert!(assembly
        .add(piece(0, 1_000_000_000_000_000, "a", "x"))
        .is_none());
    assert_eq!(assembly.refused, Some(1_000_000_000_000_000));
    assert!(assembly.received.is_empty());
    let too_long = "x".repeat(PART_CHARS + 1);
    assert!(assembly.add(piece(0, 2, "a", &too_long)).is_none());
    assert!(assembly.received.is_empty());
}

/// A page that replaced its document mid-transfer sent parts of two
/// different texts with the same count; they were joined. A new reading
/// starts the assembly over.
#[test]
fn parts_of_two_readings_are_never_joined() {
    let mut assembly = Assembly::default();
    assert!(assembly.add(piece(0, 2, "old", "old start ")).is_none());
    assert!(assembly.add(piece(1, 2, "new", "new end")).is_none());
    let whole = assembly
        .add(piece(0, 2, "new", "new start "))
        .expect("the new reading is whole");
    assert_eq!(whole.text, "new start new end");
}

/// When time runs out partway through a long page, what arrived is kept, and
/// each missing part is marked where it belongs.
#[test]
fn what_arrived_before_the_timeout_is_kept_with_the_gaps_marked() {
    let mut assembly = Assembly::default();
    assert!(assembly.partial().is_none());
    assembly.add(piece(0, 3, "a", "first "));
    assembly.add(piece(2, 3, "a", "third"));
    let partial = assembly.partial().expect("two parts arrived");
    assert!(partial.text.starts_with("first "));
    assert!(partial.text.ends_with("third"));
    assert!(partial
        .text
        .contains("[Part 2 of 3 of this page's text did not arrive"));
}

/// The check script's cutting and sending, run in a JavaScript engine against
/// a stand-in page: a text longer than one part, with a surrogate pair on
/// the cut, comes back whole after the parts go round, and every part of
/// one reading carries the same snapshot.
#[cfg(feature = "learning-labs")]
#[test]
fn the_check_script_sends_a_long_page_in_parts_that_rejoin_exactly() {
    use rquickjs::{Context, Runtime};
    let text = format!(
        "{}\u{1F600}{}",
        "a".repeat(PART_CHARS - 1),
        "b".repeat(PART_CHARS / 2)
    );
    let runtime = Runtime::new().unwrap();
    let context = Context::full(&runtime).unwrap();
    let mut assembly = Assembly::default();
    let mut snapshots = std::collections::HashSet::new();
    let whole = context.with(|ctx| {
        ctx.globals().set("pageText", text.as_str()).unwrap();
        ctx.eval::<(), _>(
            r#"
            var window = {};
            var location = { protocol: 'https:', href: '' };
            var block = { tagName: 'P', textContent: pageText, parentElement: null, closest: () => null };
            var document = {
              readyState: 'complete', title: 'Long page', body: { textContent: pageText },
              querySelector: () => null,
              querySelectorAll: (selector) => selector === 'article' ? [] : [block],
            };
            "#,
        )
        .unwrap();
        let script = check_script(false, true);
        for _ in 0..6 {
            ctx.eval::<(), _>(script.as_str()).unwrap();
            let href: String = ctx.eval("location.href").unwrap();
            let report = decode_report(&Url::parse(&href).unwrap()).expect("a report");
            snapshots.insert(report.snapshot.clone());
            if let Some(whole) = assembly.add(report) {
                return Some(whole);
            }
        }
        None
    });
    let whole = whole.expect("the parts came round and rejoined");
    assert_eq!(whole.text, text, "nothing lost or split mid-character");
    assert_eq!(whole.title, "Long page");
    assert_eq!(snapshots.len(), 1);
}

#[test]
fn the_check_script_carries_its_deadlines_and_no_placeholders() {
    let early = check_script(false, false);
    assert!(early.contains("if (false) report(true, '')"));
    let late = check_script(true, true);
    assert!(late.contains("if (true) report(true, '')"));
    assert!(!late.contains("__"));
    assert!(late.contains(REPORT_HOST));
}

/// A web service with no reader attached reports this for every browser
/// read, so its callers keep the HTTP result.
#[test]
fn without_a_reader_a_read_is_unavailable() {
    let error = unavailable();
    assert!(matches!(error, AppError::ServiceNotAvailable(_)));
    assert!(error.to_string().contains("no browser"));
}

//! General specification consistency checks shared across lesson validation
//! and quiz authoring/review (SMART FHIR scopes, code fence completeness, and
//! cross-section constant consistency).

use lattice_core::shared::text::segmentation::code_fences;
use lattice_core::shared::text::sentences;
use serde_json::Value;
use std::collections::HashSet;

#[cfg(test)]
#[path = "spec_checks_tests.rs"]
mod tests;

const SMART_CONTEXTS: &[&str] = &["system", "patient", "user"];
const NON_SCOPE_EXTS: &[&str] = &[
    "json", "ts", "js", "jsx", "tsx", "py", "md", "txt", "yml", "yaml", "xml", "html", "css",
    "sql", "csv", "pdf", "png", "jpg", "env", "lock", "toml", "sh",
];

const TS_LANGS: &[&str] = &["ts", "typescript"];
const JS_TS_LANGS: &[&str] = &["ts", "typescript", "js", "javascript"];

const BUILTIN_TS_TYPES: &[&str] = &[
    "String",
    "Number",
    "Boolean",
    "Object",
    "Array",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Promise",
    "Date",
    "Error",
    "Record",
    "Partial",
    "Required",
    "Readonly",
    "Pick",
    "Omit",
    "Buffer",
    "URL",
    "URLSearchParams",
    "Headers",
    "Request",
    "Response",
    "AbortSignal",
    "Logger",
];

const NEGATIVE_SCOPE_CUES: &[&str] = &[
    "does not exist",
    "doesn't exist",
    "not exist",
    "invalid",
    "not a valid",
    "not valid",
    "unsupported",
    "nonexistent",
    "non-existent",
    "never uses .w",
    "instead of .w",
    "rather than .w",
    "instead of `.w`",
    "rather than `.w`",
];

/// Checks `text` for invalid SMART on FHIR scope syntax (such as `.w` or `.rw`
/// when SMART v1 uses `.read`/`.write` and SMART v2 uses `.c`/`.r`/`.u`/`.d`/`.s`)
/// that is not already quoted in `sources_text` and not presented explicitly as
/// nonexistent/invalid.
pub(crate) fn smart_scope_faults(text: &str, sources_text: &[&str]) -> Vec<String> {
    let mut faults = Vec::new();
    for (start, end) in sentences(text) {
        let Some(sentence) = text.get(start..end) else {
            continue;
        };
        let lower = sentence.replace('\u{2019}', "'").to_ascii_lowercase();
        let has_negative_cue = NEGATIVE_SCOPE_CUES.iter().any(|cue| lower.contains(cue));
        if has_negative_cue
            && !lower.contains("fall back to")
            && !lower.contains("write access")
            && !lower.contains("allows writing")
        {
            continue;
        }
        for token in extract_smart_tokens(sentence) {
            let Some((_, suffix)) = token.rsplit_once('.') else {
                continue;
            };
            if is_valid_smart_suffix(suffix) {
                continue;
            }
            if sources_text.iter().any(|src| src.contains(&token)) {
                continue;
            }
            let message = format!(
                "Uses invalid SMART FHIR scope `{token}` (SMART v1 uses `.read`/`.write`; SMART v2 uses `.c`/`.r`/`.u`/`.d`/`.s`, not `.{suffix}`)."
            );
            if !faults.contains(&message) {
                faults.push(message);
            }
        }
    }

    let full_lower = text.replace('\u{2019}', "'").to_ascii_lowercase();
    let mentions_smart_context = full_lower.contains(".cruds")
        || full_lower.contains("smart")
        || !extract_smart_tokens(text).is_empty();
    if mentions_smart_context {
        const WRITE_W_PHRASES: &[&str] = &[
            "'w' (write)",
            "`.w` (write)",
            "the 'w' permission",
            "`.w` permission",
            ".w permission",
        ];
        for (start, end) in sentences(text) {
            let Some(sentence) = text.get(start..end) else {
                continue;
            };
            let lower = sentence.replace('\u{2019}', "'").to_ascii_lowercase();
            let has_negative_cue = NEGATIVE_SCOPE_CUES.iter().any(|cue| lower.contains(cue));
            if !has_negative_cue
                && WRITE_W_PHRASES.iter().any(|phrase| lower.contains(phrase))
                && !sources_text
                    .iter()
                    .any(|src| src.to_ascii_lowercase().contains("'w'"))
            {
                let message = "Claims `.w` / `'w'` is a SMART write permission; SMART v2 uses `.c` (create) and `.u` (update), while SMART v1 uses `.write`."
                    .to_owned();
                if !faults.contains(&message) {
                    faults.push(message);
                }
            }
        }
    }
    faults
}

/// Checks lesson blocks for internal TypeScript/code-fence and protocol naming
/// contradictions (undeclared injected types, `refreshToken` naming on
/// `client_credentials`, and mismatched `expires_in` buffer seconds across prose
/// and code).
pub(crate) fn block_consistency_faults(
    lesson: &Value,
    sources_text: &[&str],
) -> Vec<(usize, String)> {
    let Some(blocks) = lesson.get("blocks").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut declared_types: HashSet<String> = HashSet::new();
    let mut prose_buffers: Vec<usize> = Vec::new();

    for block in blocks {
        let body = block
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        for fence in code_fences(body) {
            let lang = fence.language.trim().to_ascii_lowercase();
            if JS_TS_LANGS.contains(&lang.as_str()) {
                let code = body.get(fence.text.clone()).unwrap_or_default();
                collect_declared_types(code, &mut declared_types);
            }
        }
        collect_prose_expiry_buffers(body, &mut prose_buffers);
    }

    let mut issues = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let body = block
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default();
        for fault in smart_scope_faults(body, sources_text) {
            issues.push((index, fault));
        }

        let section_has_client_creds = body.contains("client_credentials")
            && !body.contains("'refresh_token'")
            && !body.contains("\"refresh_token\"");

        for fence in code_fences(body) {
            let lang = fence.language.trim().to_ascii_lowercase();
            let code = body.get(fence.text.clone()).unwrap_or_default();
            if TS_LANGS.contains(&lang.as_str()) {
                for missing in undeclared_injected_types(code, &declared_types) {
                    issues.push((
                        index,
                        format!(
                            "TypeScript code references `{missing}` without importing or defining `{missing}` in the lesson."
                        ),
                    ));
                }
            }
            if JS_TS_LANGS.contains(&lang.as_str()) {
                if (section_has_client_creds || code.contains("client_credentials"))
                    && !code.contains("refresh_token")
                {
                    for ident in ["refreshToken", "refreshPromises", "refresh_token"] {
                        if contains_ident(code, ident) {
                            issues.push((
                                index,
                                format!(
                                    "Code uses `grant_type: 'client_credentials'` (which issues no refresh tokens) while naming `{ident}`; rename to `requestAccessToken` / `inflightTokenRequests`."
                                ),
                            ));
                            break;
                        }
                    }
                }
                if let Some(mismatch) = code_buffer_mismatch(code, &prose_buffers) {
                    issues.push((index, mismatch));
                }
            }
        }
    }
    issues
}

fn extract_smart_tokens(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for raw in text.split_whitespace() {
        let trimmed = raw
            .trim_matches(|ch: char| {
                matches!(
                    ch,
                    '`' | '\'' | '"' | '(' | ')' | '[' | ']' | ',' | ';' | ':'
                )
            })
            .trim_end_matches('.');
        let Some((ctx, rest)) = trimmed.split_once('/') else {
            continue;
        };
        if !SMART_CONTEXTS.contains(&ctx) {
            continue;
        }
        let Some((resource, suffix)) = rest.split_once('.') else {
            continue;
        };
        let valid_resource = resource == "*"
            || (resource
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
                && resource.chars().all(|c| c.is_ascii_alphanumeric()));
        let valid_suffix_chars =
            !suffix.is_empty() && (suffix == "*" || suffix.chars().all(|c| c.is_ascii_lowercase()));
        if valid_resource && valid_suffix_chars && !NON_SCOPE_EXTS.contains(&suffix) {
            found.push(trimmed.to_owned());
        }
    }
    found
}

fn is_valid_smart_suffix(suffix: &str) -> bool {
    if matches!(suffix, "*" | "read" | "write") {
        return true;
    }
    const CRUDS: &[u8] = b"cruds";
    let mut prev: Option<usize> = None;
    for byte in suffix.bytes() {
        let Some(pos) = CRUDS.iter().position(|&allowed| allowed == byte) else {
            return false;
        };
        if prev.is_some_and(|prior| pos <= prior) {
            return false;
        }
        prev = Some(pos);
    }
    !suffix.is_empty()
}

fn collect_declared_types(code: &str, out: &mut HashSet<String>) {
    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            for word in trimmed.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
                if word.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    out.insert(word.to_owned());
                }
            }
        }
        for kw in [
            "class ",
            "interface ",
            "type ",
            "enum ",
            "export class ",
            "export interface ",
            "export type ",
        ] {
            if let Some(rest) = trimmed.strip_prefix(kw) {
                if let Some(name) = rest
                    .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                    .find(|part| !part.is_empty())
                {
                    out.insert(name.to_owned());
                }
            }
        }
    }
}

fn undeclared_injected_types(code: &str, declared: &HashSet<String>) -> Vec<String> {
    let mut missing = Vec::new();
    let Some(ctor_at) = code.find("constructor(") else {
        return missing;
    };
    let rest = &code[ctor_at + "constructor(".len()..];
    let Some(close_at) = rest.find(')') else {
        return missing;
    };
    let params = &rest[..close_at];
    for segment in params.split(',') {
        let Some((_, ty_raw)) = segment.split_once(':') else {
            continue;
        };
        let ty = ty_raw
            .trim()
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .next()
            .unwrap_or_default();
        if ty.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && !BUILTIN_TS_TYPES.contains(&ty)
            && !declared.contains(ty)
            && !missing.iter().any(|seen| seen == ty)
        {
            missing.push(ty.to_owned());
        }
    }
    missing
}

fn contains_ident(code: &str, ident: &str) -> bool {
    let bytes = code.as_bytes();
    let mut from = 0;
    while let Some(rel) = code[from..].find(ident) {
        let start = from + rel;
        let end = start + ident.len();
        let left_ok =
            start == 0 || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
        let right_ok =
            end == bytes.len() || !(bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_');
        if left_ok && right_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn collect_prose_expiry_buffers(body: &str, out: &mut Vec<usize>) {
    let fences = code_fences(body);
    let mut prose = String::new();
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        if !fences.iter().any(|fence| fence.whole().contains(&offset)) {
            prose.push_str(line);
        }
        offset += line.len();
    }
    let normalized = prose.replace('`', "");
    let mut search = normalized.as_str();
    while let Some(at) = search.find("expires_in - ") {
        let after = &search[at + "expires_in - ".len()..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(val) = digits.parse::<usize>() {
            if !out.contains(&val) {
                out.push(val);
            }
        }
        search = after;
    }
}

fn code_buffer_mismatch(code: &str, prose_buffers: &[usize]) -> Option<String> {
    if prose_buffers.len() != 1 {
        return None;
    }
    let expected = prose_buffers[0];
    let normalized = code.replace(' ', "");
    for needle in ["expires_in-"] {
        let mut search = normalized.as_str();
        while let Some(at) = search.find(needle) {
            let after = &search[at + needle.len()..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(val) = digits.parse::<usize>() {
                if val > 0 && val < 300 && val != expected {
                    return Some(format!(
                        "Code expiry buffer (`expires_in - {val}`) disagrees with lesson prose (`expires_in - {expected}`); use the same buffer value across sections."
                    ));
                }
            }
            search = after;
        }
    }
    None
}

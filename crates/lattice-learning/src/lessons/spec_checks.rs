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
const FHIR_RESOURCES: &[&str] = &[
    "AllergyIntolerance",
    "AuditEvent",
    "Binary",
    "Bundle",
    "CapabilityStatement",
    "CarePlan",
    "Claim",
    "Communication",
    "Condition",
    "Consent",
    "Coverage",
    "Device",
    "DiagnosticReport",
    "DocumentReference",
    "Encounter",
    "ExplanationOfBenefit",
    "Goal",
    "ImagingStudy",
    "Immunization",
    "Location",
    "Medication",
    "MedicationRequest",
    "Observation",
    "OperationOutcome",
    "Organization",
    "Patient",
    "Practitioner",
    "Procedure",
    "Provenance",
    "Questionnaire",
    "QuestionnaireResponse",
    "ServiceRequest",
    "Specimen",
];
const NON_SCOPE_EXTS: &[&str] = &[
    "json", "ts", "js", "jsx", "tsx", "mjs", "cjs", "py", "md", "txt", "yml", "yaml", "xml",
    "html", "css", "scss", "sql", "csv", "pdf", "png", "jpg", "jpeg", "svg", "env", "lock", "toml",
    "sh", "bash", "zsh", "pem", "key", "crt", "cer", "der", "p12", "pfx", "pub", "cfg", "conf",
    "ini", "log", "db", "sqlite", "sqlite3", "rb", "kt", "kts", "php", "swift", "go", "java", "rs",
    "c", "h", "cpp", "hpp", "cs", "ex", "exs", "pl", "lua", "r", "wasm", "bin", "tmp", "bak",
    "out", "exe", "dll", "so", "dylib", "jar", "war", "class", "pyc", "pyo", "o", "obj", "dat",
    "raw", "tar", "gz", "zip",
];

/// Returns `text` with fenced code blocks omitted so prose checks skip code.
pub(crate) fn prose_without_fences(text: &str) -> String {
    let fences = code_fences(text);
    if fences.is_empty() {
        return text.to_owned();
    }
    let mut prose = String::with_capacity(text.len());
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if !fences.iter().any(|fence| fence.whole().contains(&offset)) {
            prose.push_str(line);
        }
        offset += line.len();
    }
    prose
}

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
    smart_scope_fault_entries(text, sources_text)
        .into_iter()
        .map(|(_, message)| message)
        .collect()
}

pub(crate) fn smart_scope_fault_entries(
    text: &str,
    sources_text: &[&str],
) -> Vec<(String, String)> {
    if !mentions_smart_fhir_context(text, sources_text) {
        return Vec::new();
    }
    let mut faults: Vec<(String, String)> = Vec::new();
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
            if !faults.iter().any(|(_, existing)| existing == &message) {
                faults.push((sentence.trim().to_owned(), message));
            }
        }
    }

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
            if !faults.iter().any(|(_, existing)| existing == &message) {
                faults.push((sentence.trim().to_owned(), message));
            }
        }
    }
    faults
}

/// Structured block-level specification or code consistency defect, scoped to
/// the narrow `passage` (sentence or fenced code block) that repair may edit.
pub(crate) struct BlockConsistencyFault {
    pub(crate) block_index: usize,
    pub(crate) subject: String,
    pub(crate) passage: String,
    pub(crate) message: String,
}

/// Checks lesson blocks for internal TypeScript/code-fence and protocol naming
/// contradictions (undeclared injected types, `refreshToken` naming on
/// `client_credentials`, and mismatched `expires_in` buffer seconds across prose
/// and code).
#[cfg(test)]
pub(crate) fn block_consistency_faults(
    lesson: &Value,
    sources_text: &[&str],
) -> Vec<(usize, String)> {
    block_consistency_fault_entries(lesson, sources_text)
        .into_iter()
        .map(|fault| (fault.block_index, fault.message))
        .collect()
}

pub(crate) fn block_consistency_fault_entries(
    lesson: &Value,
    sources_text: &[&str],
) -> Vec<BlockConsistencyFault> {
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
        for (target, fault) in smart_scope_fault_entries(body, sources_text) {
            issues.push(BlockConsistencyFault {
                block_index: index,
                subject: target.clone(),
                passage: target,
                message: fault,
            });
        }

        let section_has_client_creds = body.contains("client_credentials")
            && !body.contains("'refresh_token'")
            && !body.contains("\"refresh_token\"");

        for fence in code_fences(body) {
            let lang = fence.language.trim().to_ascii_lowercase();
            let code = body.get(fence.text.clone()).unwrap_or_default();
            let fence_passage = body.get(fence.whole()).unwrap_or(code).to_owned();
            if TS_LANGS.contains(&lang.as_str()) {
                for missing in undeclared_injected_types(code, &declared_types) {
                    issues.push(BlockConsistencyFault {
                        block_index: index,
                        subject: missing.clone(),
                        passage: fence_passage.clone(),
                        message: format!(
                            "TypeScript code references `{missing}` without importing or defining `{missing}` in the lesson."
                        ),
                    });
                }
            }
            if JS_TS_LANGS.contains(&lang.as_str()) {
                if (section_has_client_creds || code.contains("client_credentials"))
                    && !code.contains("refresh_token")
                {
                    for ident in ["refreshToken", "refreshPromises", "refresh_token"] {
                        if contains_ident(code, ident) {
                            issues.push(BlockConsistencyFault {
                                block_index: index,
                                subject: ident.to_owned(),
                                passage: fence_passage.clone(),
                                message: format!(
                                    "Code uses `grant_type: 'client_credentials'` (which issues no refresh tokens) while naming `{ident}`; rename to `requestAccessToken` / `inflightTokenRequests`."
                                ),
                            });
                            break;
                        }
                    }
                }
                if let Some((snippet, mismatch)) = code_buffer_mismatch(code, &prose_buffers) {
                    issues.push(BlockConsistencyFault {
                        block_index: index,
                        subject: snippet,
                        passage: fence_passage,
                        message: mismatch,
                    });
                }
            }
        }
    }
    issues
}

fn mentions_smart_fhir_context(text: &str, sources_text: &[&str]) -> bool {
    let text_lower = text.replace('\u{2019}', "'").to_ascii_lowercase();
    let has_explicit_cue = |haystack: &str| {
        haystack.contains("fhir")
            || haystack.contains("smart")
            || haystack.contains("hl7")
            || haystack.contains(".cruds")
            || haystack.contains("launch/patient")
            || haystack.contains("launch/encounter")
    };
    if has_explicit_cue(&text_lower)
        || sources_text
            .iter()
            .any(|src| has_explicit_cue(&src.to_ascii_lowercase()))
    {
        return true;
    }
    let mentions_scope_word =
        contains_ascii_word(&text_lower, "scope") || contains_ascii_word(&text_lower, "scopes");
    extract_smart_tokens(text).into_iter().any(|token| {
        let Some((_, rest)) = token.split_once('/') else {
            return false;
        };
        let Some((resource, suffix)) = rest.split_once('.') else {
            return false;
        };
        FHIR_RESOURCES.contains(&resource)
            || matches!(suffix, "read" | "write")
            || (mentions_scope_word
                && (suffix == "*"
                    || (!suffix.is_empty()
                        && suffix.len() <= 5
                        && suffix
                            .bytes()
                            .all(|byte| matches!(byte, b'c' | b'r' | b'u' | b'd' | b's' | b'w')))))
    })
}

fn contains_ascii_word(haystack: &str, word: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(rel) = haystack[from..].find(word) {
        let start = from + rel;
        let end = start + word.len();
        let left_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let right_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if left_ok && right_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn extract_smart_tokens(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for raw in text.split_whitespace() {
        let trimmed = raw.trim_matches(|ch: char| {
            matches!(
                ch,
                '`' | '\'' | '"' | '(' | ')' | '[' | ']' | ',' | ';' | ':' | '.' | '?' | '!'
            )
        });
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
    let mut in_import = false;
    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if in_import
            && [
                "@",
                "class ",
                "interface ",
                "type ",
                "enum ",
                "export ",
                "function ",
                "const ",
                "let ",
                "var ",
                "abstract ",
            ]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix))
        {
            in_import = false;
        }
        if trimmed.starts_with("import ") || trimmed.starts_with("import{") {
            in_import = true;
        }
        if in_import {
            let before_from = split_import_from(trimmed);
            for word in before_from.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
                if word.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    out.insert(word.to_owned());
                }
            }
            if import_statement_ends(trimmed) {
                in_import = false;
            }
            continue;
        }
        for kw in [
            "class ",
            "interface ",
            "type ",
            "enum ",
            "abstract class ",
            "export class ",
            "export abstract class ",
            "export interface ",
            "export type ",
            "export enum ",
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

fn split_import_from(line: &str) -> &str {
    if line.starts_with("from ") {
        return "";
    }
    if let Some((before, _)) = line.split_once(" from ") {
        return before;
    }
    if let Some((before, _)) = line.split_once("}from ") {
        return before;
    }
    if line.starts_with("import '") || line.starts_with("import \"") {
        return "";
    }
    line
}

fn import_statement_ends(trimmed: &str) -> bool {
    let without_comment = trimmed.split("//").next().unwrap_or(trimmed).trim_end();
    if without_comment.ends_with(';') {
        return true;
    }
    let has_from = without_comment.starts_with("from ")
        || without_comment.contains(" from ")
        || without_comment.contains("}from ");
    let side_effect =
        without_comment.starts_with("import '") || without_comment.starts_with("import \"");
    (has_from || side_effect) && (without_comment.ends_with('\'') || without_comment.ends_with('"'))
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
    let normalized = prose_without_fences(body).replace('`', "");
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

fn code_buffer_mismatch(code: &str, prose_buffers: &[usize]) -> Option<(String, String)> {
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
                    return Some((
                        format!("expires_in - {val}"),
                        format!(
                            "Code expiry buffer (`expires_in - {val}`) disagrees with lesson prose (`expires_in - {expected}`); use the same buffer value across sections."
                        ),
                    ));
                }
            }
            search = after;
        }
    }
    None
}

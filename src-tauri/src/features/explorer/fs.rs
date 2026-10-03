//! Read-only filesystem work inside a scope: listing, reading, searching and
//! finding files. Nothing here writes to the folder.
//!
//! Walking honours `.gitignore` (inside a git repository or not) and always
//! skips `.git`. Symlinks are never followed by a walk; a listing shows one
//! only when it resolves inside the scope.

use super::dto::{
    ExplorerEntryDto, ExplorerEntryKind, ExplorerFileDto, ExplorerListingDto,
    ExplorerSearchMatchDto, ExplorerSearchResultDto,
};
use super::scope::{Scope, ScopedPath};
use crate::shared::{AppError, Result};
use ignore::WalkBuilder;
use regex::{Regex, RegexBuilder};
use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Files over this are not opened as text: the viewer and the model get the
/// size and a `too_large` flag instead.
pub const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;

/// A NUL byte in this prefix marks a file as binary, as git decides it.
const BINARY_SNIFF_BYTES: usize = 8 * 1024;

const PREVIEW_CHARS: usize = 200;

/// How far one search or find may go before it stops and says `truncated`.
#[derive(Debug, Clone, Copy)]
pub struct WalkBounds {
    pub max_files: usize,
    pub max_bytes_per_file: u64,
    pub time_budget: Duration,
}

impl Default for WalkBounds {
    fn default() -> Self {
        Self {
            max_files: 20_000,
            max_bytes_per_file: 1024 * 1024,
            time_budget: Duration::from_secs(2),
        }
    }
}

fn walker(start: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(start);
    // Dotfiles are ordinary files here (`.github`, `.env.example`); only
    // `.gitignore` decides what is dimmed. `require_git(false)` honours a
    // `.gitignore` in a folder that is not a repository.
    builder.hidden(false).require_git(false).follow_links(false);
    builder
}

pub(super) fn is_git_dir(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == ".git")
}

/// Whether `target` (a lexical path under the root) is ignored, itself or
/// through an ignored ancestor. Walks only the chain of directories leading to
/// it, so the cost is one directory read per level.
pub(super) fn is_ignored(scope: &Scope, target: &Path) -> bool {
    if target == scope.root() {
        return false;
    }
    let wanted = target.to_path_buf();
    let mut builder = walker(scope.root());
    builder
        .filter_entry(move |entry| !is_git_dir(entry.path()) && wanted.starts_with(entry.path()));
    !builder
        .build()
        .filter_map(|entry| entry.ok())
        .any(|entry| entry.path() == target)
}

pub fn list_dir(scope: &Scope, path: &str) -> Result<ExplorerListingDto> {
    let dir = scope.resolve(path)?;
    if !dir.canonical.is_dir() {
        return Err(AppError::InvalidInput(format!(
            "Not a folder: {}",
            dir.relative
        )));
    }
    // Children of an ignored directory are ignored too, though no pattern
    // names them; the walker alone would call them clean.
    let parent_ignored = is_ignored(scope, &dir.lexical);
    let kept: HashSet<OsString> = {
        let mut builder = walker(&dir.canonical);
        builder.max_depth(Some(1));
        builder
            .build()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.depth() == 1)
            .map(|entry| entry.file_name().to_os_string())
            .collect()
    };

    let read = std::fs::read_dir(&dir.canonical).map_err(|error| AppError::FileRead {
        path: dir.relative.clone(),
        reason: error.to_string(),
    })?;
    let mut entries = Vec::new();
    for entry in read.filter_map(|entry| entry.ok()) {
        let file_name = entry.file_name();
        if file_name == ".git" {
            continue;
        }
        let name = file_name.to_string_lossy().into_owned();
        let relative = join_relative(&dir.relative, &name);
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        // A symlink is listed by what it points at, and only when that is
        // inside the scope; one pointing out could not be opened anyway.
        let metadata = if file_type.is_symlink() {
            match scope.resolve(&relative) {
                Ok(target) => std::fs::metadata(&target.canonical).ok(),
                Err(_) => None,
            }
        } else {
            entry.metadata().ok()
        };
        let Some(metadata) = metadata else {
            continue;
        };
        let kind = if metadata.is_dir() {
            ExplorerEntryKind::Directory
        } else {
            ExplorerEntryKind::File
        };
        entries.push(ExplorerEntryDto {
            size: (kind == ExplorerEntryKind::File).then_some(metadata.len()),
            ignored: parent_ignored || !kept.contains(&file_name),
            name,
            path: relative,
            kind,
        });
    }
    entries.sort_by(|a, b| {
        (a.kind != ExplorerEntryKind::Directory)
            .cmp(&(b.kind != ExplorerEntryKind::Directory))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(ExplorerListingDto {
        root: scope.root_string(),
        path: dir.relative,
        entries,
    })
}

fn join_relative(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

pub(super) fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(BINARY_SNIFF_BYTES).any(|&byte| byte == 0)
}

/// Reads up to `limit` bytes, and says whether the file had more.
pub(super) fn read_prefix(path: &Path, limit: u64) -> std::io::Result<(Vec<u8>, bool)> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    let cut = bytes.len() as u64 > limit;
    bytes.truncate(limit as usize);
    Ok((bytes, cut))
}

pub fn read_file(scope: &Scope, path: &str) -> Result<ExplorerFileDto> {
    let file = scope.resolve(path)?;
    let read_error = |error: std::io::Error| AppError::FileRead {
        path: file.relative.clone(),
        reason: error.to_string(),
    };
    let metadata = std::fs::metadata(&file.canonical).map_err(read_error)?;
    if !metadata.is_file() {
        return Err(AppError::InvalidInput(format!(
            "Not a file: {}",
            file.relative
        )));
    }
    let size_bytes = metadata.len();
    let language = language_for(&file.relative);
    let too_large = size_bytes > MAX_TEXT_BYTES;
    let limit = if too_large {
        BINARY_SNIFF_BYTES as u64
    } else {
        MAX_TEXT_BYTES
    };
    let (bytes, _) = read_prefix(&file.canonical, limit).map_err(read_error)?;
    let binary = looks_binary(&bytes);
    let text = (!too_large && !binary).then(|| String::from_utf8_lossy(&bytes).into_owned());
    let line_count = text
        .as_deref()
        .map_or(0, |text| text.lines().count() as u32);

    Ok(ExplorerFileDto {
        root: scope.root_string(),
        path: file.relative,
        text,
        line_count,
        size_bytes,
        language,
        binary,
        too_large,
    })
}

/// A search over the scope's text files.
#[derive(Debug, Clone)]
pub struct SearchRequest<'a> {
    pub query: &'a str,
    pub regex: bool,
    pub path_prefix: Option<&'a str>,
    pub max_results: usize,
}

/// Lowercase queries match any case; one with a capital matches exactly, the
/// way editors' "smart case" behaves.
fn build_matcher(query: &str, regex: bool) -> Result<Regex> {
    if query.trim().is_empty() {
        return Err(AppError::InvalidInput("Search for something.".to_string()));
    }
    let pattern = if regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!query.chars().any(char::is_uppercase))
        .size_limit(1 << 20)
        .build()
        .map_err(|error| AppError::InvalidInput(format!("Invalid search pattern: {error}")))
}

/// The walk start for an optional sub-path, refused if it leaves the scope.
pub(super) fn walk_start(scope: &Scope, path_prefix: Option<&str>) -> Result<Option<ScopedPath>> {
    match path_prefix
        .map(str::trim)
        .filter(|prefix| !prefix.is_empty())
    {
        Some(prefix) => scope.resolve(prefix).map(Some),
        None => Ok(None),
    }
}

/// Every non-ignored regular file under `start` (or the whole scope), walked
/// from the root so the ignore rules of the folders above `start` apply too.
/// Stops when `visit` returns false.
pub(super) fn walk_files(
    scope: &Scope,
    start: Option<&ScopedPath>,
    mut visit: impl FnMut(&Path) -> bool,
) {
    let start: PathBuf = start.map_or_else(|| scope.root().to_path_buf(), |s| s.lexical.clone());
    let along = start.clone();
    let mut builder = walker(scope.root());
    builder.filter_entry(move |entry| {
        !is_git_dir(entry.path())
            && (entry.path().starts_with(&along) || along.starts_with(entry.path()))
    });
    // The parallel walker would be faster on large trees, but the bounds and
    // ordering are simpler to keep honest on one thread, and the time cap
    // already keeps a turn from waiting long.
    for entry in builder.build().filter_map(|entry| entry.ok()) {
        let is_file = entry.file_type().is_some_and(|kind| kind.is_file());
        if is_file && entry.path().starts_with(&start) && !visit(entry.path()) {
            return;
        }
    }
}

pub fn search(scope: &Scope, request: &SearchRequest<'_>) -> Result<ExplorerSearchResultDto> {
    search_with_bounds(scope, request, WalkBounds::default())
}

pub fn search_with_bounds(
    scope: &Scope,
    request: &SearchRequest<'_>,
    bounds: WalkBounds,
) -> Result<ExplorerSearchResultDto> {
    let matcher = build_matcher(request.query, request.regex)?;
    let start = walk_start(scope, request.path_prefix)?;
    let max_results = request.max_results.max(1);
    let deadline = Instant::now() + bounds.time_budget;
    let mut matches = Vec::new();
    let mut files_scanned = 0usize;
    let mut truncated = false;

    walk_files(scope, start.as_ref(), |path| {
        if files_scanned >= bounds.max_files || Instant::now() >= deadline {
            truncated = true;
            return false;
        }
        files_scanned += 1;
        let Ok((bytes, cut)) = read_prefix(path, bounds.max_bytes_per_file) else {
            return true;
        };
        if looks_binary(&bytes) {
            return true;
        }
        truncated |= cut;
        let Some(relative) = scope.relative_of(path) else {
            return true;
        };
        let text = String::from_utf8_lossy(&bytes);
        for (index, line) in text.lines().enumerate() {
            if let Some(found) = matcher.find(line) {
                matches.push(ExplorerSearchMatchDto {
                    path: relative.clone(),
                    line: index as u32 + 1,
                    column: line[..found.start()].chars().count() as u32 + 1,
                    preview: preview(line, found.start()),
                });
                if matches.len() >= max_results {
                    truncated = true;
                    return false;
                }
            }
        }
        true
    });

    Ok(ExplorerSearchResultDto {
        matches,
        truncated,
        files_scanned: files_scanned as u32,
    })
}

/// The matched line, trimmed, and cut to a window around the match when long.
fn preview(line: &str, match_start: usize) -> String {
    let leading = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    let start_chars =
        line[..match_start.max(leading)].chars().count() - line[..leading].chars().count();
    let total = trimmed.chars().count();
    if total <= PREVIEW_CHARS {
        return trimmed.to_string();
    }
    let from = start_chars.saturating_sub(40).min(total - PREVIEW_CHARS);
    let window: String = trimmed.chars().skip(from).take(PREVIEW_CHARS).collect();
    let head = if from > 0 { "…" } else { "" };
    let tail = if from + PREVIEW_CHARS < total {
        "…"
    } else {
        ""
    };
    format!("{head}{window}{tail}")
}

/// Paths found by name, with whether a bound cut the list short.
#[derive(Debug, Clone, Default)]
pub struct FoundFiles {
    pub paths: Vec<String>,
    pub truncated: bool,
}

/// Files whose name or relative path matches `pattern`: a glob when it holds
/// `*` or `?`, otherwise a case-insensitive substring.
pub fn find_files(scope: &Scope, pattern: &str, max_results: usize) -> Result<FoundFiles> {
    find_files_with_bounds(scope, pattern, max_results, WalkBounds::default())
}

pub fn find_files_with_bounds(
    scope: &Scope,
    pattern: &str,
    max_results: usize,
    bounds: WalkBounds,
) -> Result<FoundFiles> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err(AppError::InvalidInput(
            "Give a name or pattern to find.".to_string(),
        ));
    }
    let matches_path = name_matcher(pattern)?;
    let deadline = Instant::now() + bounds.time_budget;
    let mut found = FoundFiles::default();
    let mut visited = 0usize;

    walk_files(scope, None, |path| {
        visited += 1;
        if visited > bounds.max_files || Instant::now() >= deadline {
            found.truncated = true;
            return false;
        }
        let Some(relative) = scope.relative_of(path) else {
            return true;
        };
        if matches_path(&relative) {
            found.paths.push(relative);
            if found.paths.len() >= max_results.max(1) {
                found.truncated = true;
                return false;
            }
        }
        true
    });
    Ok(found)
}

/// Most files a reference that names no exact path is matched to.
const MAX_LOCATED: usize = 20;

/// The files a path written in an answer most likely means.
///
/// A model that has read `contracts/effect_api.hpp` often cites it as
/// `effect_api.hpp:12`, or with the folder's absolute path. The path itself
/// wins when it is a file in the folder. Otherwise the files whose relative
/// path ends with it, segment by segment, come first, then any file with the
/// same name; each group shortest path first, case ignored. Empty when
/// nothing in the folder matches.
pub fn locate_file(scope: &Scope, path: &str) -> Result<Vec<String>> {
    let root = format!("{}/", scope.root_string().trim_end_matches('/'));
    let wanted = path.trim();
    let wanted = wanted.strip_prefix(root.as_str()).unwrap_or(wanted);
    let wanted = wanted.trim_start_matches("./").trim_start_matches('/');
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    if let Ok(found) = scope.resolve(wanted) {
        if found.canonical.is_file() {
            return Ok(vec![found.relative]);
        }
    }

    let wanted = wanted.to_lowercase();
    let suffix = format!("/{wanted}");
    let name = wanted.rsplit('/').next().unwrap_or(&wanted).to_string();
    let name_suffix = format!("/{name}");
    let deadline = Instant::now() + WalkBounds::default().time_budget;
    let mut visited = 0usize;
    let (mut by_path, mut by_name) = (Vec::new(), Vec::new());
    walk_files(scope, None, |file| {
        visited += 1;
        if visited > WalkBounds::default().max_files || Instant::now() >= deadline {
            return false;
        }
        let Some(relative) = scope.relative_of(file) else {
            return true;
        };
        let lower = relative.to_lowercase();
        if lower == wanted || lower.ends_with(&suffix) {
            by_path.push(relative);
        } else if lower == name || lower.ends_with(&name_suffix) {
            by_name.push(relative);
        }
        true
    });
    let shortest_first = |paths: &mut Vec<String>| {
        paths.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    };
    shortest_first(&mut by_path);
    shortest_first(&mut by_name);
    by_path.extend(by_name);
    by_path.truncate(MAX_LOCATED);
    Ok(by_path)
}

type PathMatcher = Box<dyn Fn(&str) -> bool>;

fn name_matcher(pattern: &str) -> Result<PathMatcher> {
    if !pattern.contains(['*', '?']) {
        let needle = pattern.to_lowercase();
        return Ok(Box::new(move |relative: &str| {
            relative.to_lowercase().contains(&needle)
        }));
    }
    let glob = glob_regex(pattern)?;
    // A pattern without a `/` names a file wherever it sits (`*.rs`); one with
    // a `/` is matched against the whole relative path (`src/**/*.rs`).
    let by_name = !pattern.contains('/');
    Ok(Box::new(move |relative: &str| {
        let subject = if by_name {
            relative.rsplit('/').next().unwrap_or(relative)
        } else {
            relative
        };
        glob.is_match(subject)
    }))
}

fn glob_regex(pattern: &str) -> Result<Regex> {
    let mut out = String::from("(?i)^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    out.push_str("(?:.*/)?");
                } else {
                    out.push_str(".*");
                }
            }
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            other => out.push_str(&regex::escape(&other.to_string())),
        }
    }
    out.push('$');
    Regex::new(&out).map_err(|error| AppError::InvalidInput(format!("Invalid pattern: {error}")))
}

/// A language name for the viewer and the prompt's code fences.
pub fn language_for(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name {
        "Dockerfile" => return Some("dockerfile".to_string()),
        "Makefile" | "makefile" | "GNUmakefile" => return Some("makefile".to_string()),
        _ => {}
    }
    let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
    let language = match extension.as_str() {
        "rs" => Some("rust"),
        "ts" | "mts" | "cts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsx" => Some("jsx"),
        "py" | "pyi" => Some("python"),
        "go" => Some("go"),
        "java" => Some("java"),
        "kt" | "kts" => Some("kotlin"),
        "swift" => Some("swift"),
        "c" | "h" => Some("c"),
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => Some("cpp"),
        "cs" => Some("csharp"),
        "rb" => Some("ruby"),
        "php" => Some("php"),
        "sh" | "bash" | "zsh" => Some("shell"),
        "json" | "jsonc" => Some("json"),
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "md" | "markdown" | "mdx" => Some("markdown"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "scss" | "sass" => Some("scss"),
        "sql" => Some("sql"),
        "xml" | "svg" | "plist" => Some("xml"),
        "lua" => Some("lua"),
        "dart" => Some("dart"),
        "zig" => Some("zig"),
        "ex" | "exs" => Some("elixir"),
        "hs" => Some("haskell"),
        "scala" => Some("scala"),
        "r" => Some("r"),
        "vue" => Some("vue"),
        "svelte" => Some("svelte"),
        _ => None,
    };
    language.map(str::to_string)
}

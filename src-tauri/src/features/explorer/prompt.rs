//! What an Explorer turn tells the model about the folder.
//!
//! One block: the folder's name and top level, the open file with the
//! selected lines (or its first lines when nothing is selected), the passages
//! the folder index found for the question, and the rule for pointing at
//! lines. The open file and the passages ride along on every turn so that a
//! model without tool calling can still answer about the folder.

use super::dto::ExplorerFocusDto;
use super::fs;
use super::index::search::{FolderHit, FolderSearch};
use super::index::tool::{self as folder_index_tool, TOOL_SEARCH_FOLDER};
use super::repository;
use super::scope::Scope;
use super::tools::{self, number_line, number_width};
use crate::features::function_calling::domain::{FunctionCall, FunctionResult};
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::warn;

/// Top-level entries named in the block before the rest are counted instead.
const TOP_LEVEL_ENTRIES: usize = 40;
/// Passages the folder index puts in the block before generation.
pub const FIRST_STEP_PASSAGES: usize = 5;

/// What the folder index found for the question before generation.
#[derive(Debug, Clone, Default)]
pub struct FolderPassages {
    pub hits: Vec<FolderHit>,
    /// Set while the index is still being built: how far along it is.
    pub progress: Option<String>,
}

/// The folder an Explorer conversation is bound to, opened for one turn, and
/// what the user had in view when they sent.
#[derive(Clone)]
pub struct ExplorerTurn {
    pub scope: Scope,
    focus: Option<ExplorerFocusDto>,
    /// The open folder's index, when it has anything in it.
    index: Option<Arc<FolderSearch>>,
}

impl std::fmt::Debug for ExplorerTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExplorerTurn")
            .field("scope", &self.scope)
            .field("focus", &self.focus)
            .field("index", &self.index.is_some())
            .finish()
    }
}

impl ExplorerTurn {
    /// `None` for an ordinary chat. A folder that has gone missing since it was
    /// chosen also gives `None`: the turn then runs as a plain chat rather than
    /// failing, and the viewer is where the user finds out.
    pub async fn resolve(
        pool: &SqlitePool,
        conversation_id: &str,
        focus: Option<&ExplorerFocusDto>,
    ) -> Option<Self> {
        let root = match repository::conversation_root(pool, conversation_id).await {
            Ok(Some(root)) => root,
            Ok(None) => return None,
            Err(error) => {
                warn!(%error, conversation_id, "Could not read the conversation's Explorer folder");
                return None;
            }
        };
        match Scope::open(&root) {
            Ok(scope) => {
                let index = super::index::manager::search_for_root(scope.root()).await;
                Some(Self {
                    scope,
                    focus: focus.cloned(),
                    index,
                })
            }
            Err(error) => {
                warn!(%error, root, "Explorer folder is unavailable; the turn runs without it");
                None
            }
        }
    }

    /// A turn with an explicit index, for callers that already hold one.
    pub fn with_index(
        scope: Scope,
        focus: Option<ExplorerFocusDto>,
        index: Option<Arc<FolderSearch>>,
    ) -> Self {
        Self {
            scope,
            focus,
            index,
        }
    }

    /// Whether `search_folder` is offered: the folder's index has passages.
    pub fn has_folder_index(&self) -> bool {
        self.index.is_some()
    }

    /// What the folder index finds for `question`, before generation. Empty
    /// when there is no index or the search fails; the turn goes on without.
    pub async fn first_step_passages(&self, question: &str) -> FolderPassages {
        let Some(index) = &self.index else {
            return FolderPassages::default();
        };
        match index.search(question, None, FIRST_STEP_PASSAGES).await {
            Ok(hits) => FolderPassages {
                progress: (!hits.is_empty())
                    .then(|| folder_index_tool::progress_note(index))
                    .flatten(),
                hits,
            },
            Err(error) => {
                warn!(%error, "Folder index search failed; the turn runs without passages");
                FolderPassages::default()
            }
        }
    }

    /// The prompt block, at most about `max_chars` long, with the passages
    /// the folder index finds for `question` (`None` on a closed-book turn,
    /// which searches nothing). Reads the disk, so the rendering runs on the
    /// blocking pool.
    pub async fn context_block(
        &self,
        max_chars: usize,
        tools_offered: bool,
        question: Option<&str>,
    ) -> String {
        let passages = match question {
            Some(question) => self.first_step_passages(question).await,
            None => FolderPassages::default(),
        };
        let scope = self.scope.clone();
        let focus = self.focus.clone();
        let folder_search = self.has_folder_index();
        tokio::task::spawn_blocking(move || {
            render_context(
                &scope,
                focus.as_ref(),
                max_chars,
                tools_offered,
                folder_search,
                &passages,
            )
        })
        .await
        .unwrap_or_default()
    }

    /// Runs one folder tool call: `search_folder` against the index, the
    /// others against the disk.
    pub async fn execute_tool(
        &self,
        call: FunctionCall,
        max_chars: usize,
    ) -> crate::shared::Result<FunctionResult> {
        if call.name != TOOL_SEARCH_FOLDER {
            return tools::execute(&self.scope, call, max_chars).await;
        }
        let Some(index) = &self.index else {
            return Ok(FunctionResult::error(
                "explorer_error",
                "The folder has no index to search. Use search_files instead.",
            ));
        };
        Ok(
            match folder_index_tool::run(index, &call.arguments, max_chars).await {
                Ok(text) => FunctionResult::success(serde_json::Value::String(text)),
                Err(error) => FunctionResult::error("explorer_error", tools::describe(&error)),
            },
        )
    }
}

fn rules(tools_offered: bool, folder_search: bool) -> String {
    let reading = if tools_offered && folder_search {
        "Read more of the folder with list_directory, read_file, search_files and find_files rather than guessing at what a file says. search_folder finds code by meaning when you do not know the exact words."
    } else if tools_offered {
        "Read more of the folder with list_directory, read_file, search_files and find_files rather than guessing at what a file says."
    } else {
        "You cannot open other files this turn. Answer from what is shown here, and say so when the answer needs a file you have not seen."
    };
    format!(
        "How to answer about the folder:\n\
         - {reading}\n\
         - Point at lines with a line reference written as inline code: the path relative to the folder root, a colon, and 1-based line numbers, like `src/main.rs:10-24`, or `src/main.rs:12` for one line. The user sees each one as a link that opens the file at those lines. Only reference lines you have actually seen.\n\
         - The folder is what the user is looking at. Their library and the web are outside knowledge: cite those with bracket numbers like [1] only when numbered passages from them appear in this message, and never use bracket numbers for the folder."
    )
}

/// The passages section, at most `budget` characters; empty without hits.
fn render_passages(passages: &FolderPassages, budget: usize) -> String {
    if passages.hits.is_empty() {
        return String::new();
    }
    let heading = "Passages from the folder that may bear on the question, found by its search index. Each is headed by its line reference; cite one the same way if you use it.\n";
    let note = passages
        .progress
        .as_deref()
        .map(|note| format!("{note}\n"))
        .unwrap_or_default();
    let room = budget.saturating_sub(heading.len() + note.len());
    let body = folder_index_tool::render_passages(&passages.hits, room);
    if body.trim().is_empty() || body.starts_with('[') {
        return String::new();
    }
    format!("{heading}{body}{note}")
}

/// The block for one turn. A focus that does not resolve inside the scope (a
/// stale path, a file deleted since) is dropped without a word: the user did
/// nothing wrong, and the rest of the block still holds.
///
/// Passages from the folder index take at most half of `max_chars`; the open
/// file and selection keep the rest.
pub fn render_context(
    scope: &Scope,
    focus: Option<&ExplorerFocusDto>,
    max_chars: usize,
    tools_offered: bool,
    folder_search: bool,
    passages: &FolderPassages,
) -> String {
    let rules = rules(tools_offered, folder_search);
    let passages = render_passages(passages, max_chars / 2);
    let mut out = format!(
        "Folder open beside this chat: {}. The user is looking at it in Explorer; every path here is relative to its root.\n",
        scope.name()
    );

    if let Ok(listing) = fs::list_dir(scope, "") {
        let visible: Vec<String> = listing
            .entries
            .iter()
            .filter(|entry| !entry.ignored)
            .map(|entry| match entry.size {
                None => format!("{}/", entry.name),
                Some(_) => entry.name.clone(),
            })
            .collect();
        if visible.is_empty() {
            out.push_str("Top level: (empty)\n");
        } else {
            let named: Vec<&str> = visible
                .iter()
                .take(TOP_LEVEL_ENTRIES)
                .map(String::as_str)
                .collect();
            out.push_str(&format!("Top level: {}", named.join(", ")));
            if visible.len() > named.len() {
                out.push_str(&format!(", and {} more", visible.len() - named.len()));
            }
            out.push('\n');
        }
    }

    if let Some(open) = focus.and_then(|focus| focus.open_path.as_deref()) {
        if let Ok(file) = fs::read_file(scope, open) {
            match file.text.as_deref() {
                None => out.push_str(&format!(
                    "Open file: {} ({}; not shown).\n",
                    file.path,
                    if file.binary { "binary" } else { "too large" }
                )),
                Some(text) => {
                    let total = file.line_count;
                    // A selection is taken as sent, clipped to the file; one
                    // that cannot be read as a range falls back to the top.
                    let selection = focus
                        .and_then(|focus| focus.selection)
                        .filter(|range| {
                            range.start_line >= 1
                                && range.start_line <= range.end_line
                                && range.start_line <= total
                        })
                        .map(|range| (range.start_line, range.end_line.min(total)));
                    let (start, end, heading) = match selection {
                        Some((start, end)) => (
                            start,
                            end,
                            format!(
                                "Open file: {} ({total} lines). The user selected lines {start}–{end}:",
                                file.path
                            ),
                        ),
                        None => (
                            1,
                            total,
                            format!("Open file: {} ({total} lines):", file.path),
                        ),
                    };
                    out.push_str(&heading);
                    out.push('\n');
                    let fence = file.language.as_deref().unwrap_or("");
                    out.push_str(&format!("```{fence}\n"));
                    // Whatever the header, listing, passages and rules leave
                    // is the file's; a cut is said, never silent.
                    let budget = max_chars.saturating_sub(rules.len() + passages.len() + 120);
                    let mut last = start.saturating_sub(1);
                    if total > 0 {
                        let width = number_width(end);
                        let numbered = text
                            .lines()
                            .enumerate()
                            .skip(start as usize - 1)
                            .take((end - start + 1) as usize)
                            .map(|(index, line)| number_line(index as u32 + 1, line, width));
                        for line in numbered {
                            if out.len() + line.len() + 1 > budget {
                                break;
                            }
                            out.push_str(&line);
                            out.push('\n');
                            last += 1;
                        }
                    }
                    out.push_str("```\n");
                    if last < end {
                        out.push_str(&format!(
                            "[Lines {}–{end} not shown for length.{}]\n",
                            last + 1,
                            if tools_offered {
                                format!(" read_file reads on from line {}.", last + 1)
                            } else {
                                String::new()
                            }
                        ));
                    }
                }
            }
        }
    }

    if !passages.is_empty() {
        out.push('\n');
        out.push_str(&passages);
    }
    out.push('\n');
    out.push_str(&rules);
    out
}

//! Independent checks receive the public task, never a reference answer.
use super::{
    invalid, validate_files, FileJson, GeneratedPracticalFile, PracticalFileRole,
    PracticalRuntimeContext,
};
use crate::lessons::structured_call::Asking;
use lattice_core::application::services::model_contract::ask::{
    ask_each, ask_new, Alone, Answered, Refusal, Says, Settled,
};
use lattice_core::application::services::model_contract::shape::{
    Authored, Each, Fields, Form, New,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Kind, Scope, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::{application::ports::LLMPort, shared::error::Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub(super) fn check_python_line_quotes(path: &str, content: &str) -> Result<()> {
    let mut triple: Option<&str> = None;
    for (idx, line) in content.lines().enumerate() {
        let mut chars = line.chars().peekable();
        let mut quote: Option<char> = None;
        while let Some(ch) = chars.next() {
            if let Some(delim) = triple {
                let first = delim.chars().next().unwrap_or('"');
                if ch == '\\' {
                    let _ = chars.next();
                } else if ch == first && chars.peek() == Some(&first) {
                    let mut clone = chars.clone();
                    let _ = clone.next();
                    if clone.next() == Some(first) {
                        let _ = chars.next();
                        let _ = chars.next();
                        triple = None;
                    }
                }
                continue;
            }
            if let Some(active) = quote {
                if ch == '\\' {
                    let _ = chars.next();
                } else if ch == active {
                    quote = None;
                }
                continue;
            }
            if ch == '#' {
                break;
            }
            if ch == '\'' || ch == '"' {
                if chars.peek() == Some(&ch) {
                    let mut clone = chars.clone();
                    let _ = clone.next();
                    if clone.next() == Some(ch) {
                        let _ = chars.next();
                        let _ = chars.next();
                        triple = Some(if ch == '\'' { "'''" } else { "\"\"\"" });
                        continue;
                    }
                }
                quote = Some(ch);
            }
        }
        if quote.is_some() && !line.trim_end().ends_with('\\') {
            return Err(invalid(format!(
                "Python file '{path}' has an unterminated string literal on line {} (`{}`): close the string literal with a matching quote, and use double-quoted strings with backslash-escaped quotes instead of single-quoted strings containing double quotes.",
                idx + 1,
                line.trim()
            )));
        }
    }
    if triple.is_some() {
        return Err(invalid(format!(
            "Python file '{path}' has an unterminated triple-quoted string."
        )));
    }
    Ok(())
}

const EVALUATOR_HEAD: &str = "Author only independent evaluator files for this public practical brief and exact executionEnvironment. ";

const EVALUATOR_OWNED: &str = "evaluatorPaths are app-owned required filenames, and evaluatorFiles shows each of them with its id. Write every one of those files: return an entries array with one entry for each, naming it in of with its id and giving the file's full content in content. ";

const EVALUATOR_UNOWNED: &str = "evaluatorPaths are app-owned required filenames; use those paths exactly and include every listed file. Return an items array with one item for each evaluator file, giving its path in path and its full content in content. ";

const EVALUATOR_TAIL: &str = "Every publicFiles path belongs to the learner workspace: never emit, copy or replace it. Import the learner's supplied interface and assert the stated behavior and edge cases with 4 to 6 concise, syntactically valid test cases. A failing check is the learner's feedback, so name each test for the behavior it checks and give each assertion a message stating the expected behavior and the observed value, without revealing how to implement it. In Python files, use double-quoted string literals with backslash-escaped quotes instead of single-quoted strings containing double quotes. Use only supported language versions and available dependencies. Assertions must fail nonzero; zero checks cannot count as success. Do not include a solution or reference implementation, derive expected output from hidden solution text, use networking, or execute a host shell.";

const EVALUATOR_OUTPUT_TOKENS: usize = 2_500;

/// The call that writes the content of each evaluator file the app requires.
pub(super) struct PracticalEvaluator;

impl Contract for PracticalEvaluator {
    const ID: ContractId = ContractId::new("practical-evaluator");
    type Input = Input;
}

/// The call that writes the evaluator files of a runtime whose files the app does not name.
pub(super) struct CustomEvaluator;

impl Contract for CustomEvaluator {
    const ID: ContractId = ContractId::new("practical-evaluator-custom");
    type Input = Input;
}

/// One file of the learner's workspace, as the model reads it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicFile {
    pub(super) path: Told<String>,
    pub(super) content: Told<String>,
    pub(super) editable: Told<bool>,
}

/// A file the app requires the evaluator to hold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct EvaluatorFile {
    pub(super) at: usize,
    pub(super) path: Told<String>,
}

impl Kind for EvaluatorFile {
    const PREFIX: &'static str = "e";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// What one evaluator request says.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Input {
    pub(super) brief: Told<String>,
    pub(super) public_files: Vec<PublicFile>,
    pub(super) environment: Told<Value>,
    pub(super) files: Vec<EvaluatorFile>,
}

impl Input {
    fn holds_public(&self, path: &str) -> bool {
        self.public_files.iter().any(|file| file.path.0 == path)
    }

    fn says(&self, showing: &mut Showing<'_>, owned: bool) -> Result<Says> {
        let shown = showing.show(&self.files)?;
        let mut task = json!({
            "brief":self.brief,"publicFiles":self.public_files,"executionEnvironment":self.environment,
        });
        if let Some(fields) = task.as_object_mut().filter(|_| owned) {
            let listed: Vec<Value> = shown
                .iter()
                .map(|(handle, file)| json!({"id": handle.written(), "path": file.path}))
                .collect();
            fields.insert("evaluatorFiles".into(), json!(listed));
        }
        let paths = if owned {
            EVALUATOR_OWNED
        } else {
            EVALUATOR_UNOWNED
        };
        let mut says = Says::new(
            format!("{EVALUATOR_HEAD}{paths}{EVALUATOR_TAIL}"),
            task.to_string(),
        );
        says.output_tokens = EVALUATOR_OUTPUT_TOKENS;
        Ok(says)
    }
}

/// The content of one required evaluator file.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Content {
    pub(super) content: String,
}

impl Authored for Content {
    fn form() -> Form {
        Form::new().text("content")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            content: fields.text("content")?,
        })
    }
}

impl Answered for PracticalEvaluator {
    type Answer = Each<EvaluatorFile, Content>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        input.says(showing, true)
    }

    fn accept(_input: &Input, file: &Content) -> std::result::Result<(), Refusal> {
        if file.content.trim().is_empty() {
            return Err(Refusal("Write the content of this file.".into()));
        }
        Ok(())
    }
}

/// An evaluator file the model names and writes.
#[derive(Debug, Clone)]
pub(super) struct CheckFile {
    pub(super) path: String,
    pub(super) content: String,
}

impl Authored for CheckFile {
    fn form() -> Form {
        Form::new().text("path").text("content")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            path: fields.text("path")?,
            content: fields.text("content")?,
        })
    }
}

impl Answered for CustomEvaluator {
    type Answer = New<CheckFile>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        input.says(showing, false)
    }

    fn accept(input: &Input, file: &CheckFile) -> std::result::Result<(), Refusal> {
        if file.content.trim().is_empty() {
            return Err(Refusal("Write the content of this file.".into()));
        }
        crate::lab_runtime::validate_lab_path(&file.path)
            .map_err(|error| Refusal(error.to_string()))?;
        if input.holds_public(&file.path) {
            return Err(Refusal(format!("Evaluator file '{}' cannot replace a public practical file; return only independent checks.", file.path)));
        }
        if file.path.ends_with(".py") {
            check_python_line_quotes(&file.path, &file.content)
                .map_err(|error| Refusal(error.to_string()))?;
        }
        Ok(())
    }
}

pub(super) async fn generate(
    llm: &dyn LLMPort,
    brief: &str,
    files: &[GeneratedPracticalFile],
    runtime: &PracticalRuntimeContext,
) -> Result<Vec<GeneratedPracticalFile>> {
    if files.iter().all(|file| !file.editable) {
        return Err(invalid(
            "An executable practical activity needs at least one editable starter file.",
        ));
    }
    let input = Input {
        brief: Told(brief.to_owned()),
        public_files: files
            .iter()
            .map(|file| PublicFile {
                path: Told(file.path.clone()),
                content: Told(file.content.clone()),
                editable: Told(file.editable),
            })
            .collect(),
        environment: Told(
            serde_json::to_value(runtime).map_err(|error| invalid(error.to_string()))?,
        ),
        files: runtime
            .evaluator_paths
            .iter()
            .enumerate()
            .map(|(at, path)| EvaluatorFile {
                at,
                path: Told(path.clone()),
            })
            .collect(),
    };
    let asking = Asking {
        progress: None,
        reasoning: None,
    };
    let call = crate::model_call::answer_options();
    let written: Vec<FileJson> = if input.files.is_empty() {
        let found = ask_new::<CustomEvaluator>(
            llm,
            &asking,
            &mut Scope::for_question(),
            input.clone(),
            |_| None,
            &call,
        )
        .await?;
        if !(1..=16).contains(&found.items.len()) {
            return Err(invalid("Return 1–16 independent evaluator files."));
        }
        found
            .items
            .into_iter()
            .map(|file| FileJson {
                path: file.path,
                content: file.content,
            })
            .collect()
    } else {
        let settled = ask_each::<PracticalEvaluator>(
            llm,
            &asking,
            &mut Scope::for_question(),
            &input.files,
            |open, _| Input {
                files: open.iter().map(|file| (*file).clone()).collect(),
                ..input.clone()
            },
            &call,
        )
        .await?;
        let mut written = Vec::new();
        for (file, settled) in input.files.iter().zip(settled) {
            let Settled::Answered(check) = settled else {
                return Err(invalid(format!(
                    "The evaluator file '{}' could not be written.",
                    file.path.0
                )));
            };
            written.push(FileJson {
                path: file.path.0.clone(),
                content: check.content,
            });
        }
        written
    };
    let checks = validate_files(
        written
            .into_iter()
            .map(|file| (file, PracticalFileRole::Check)),
    )?;
    if let Some(file) = checks.iter().find(|file| input.holds_public(&file.path)) {
        return Err(invalid(format!("Evaluator file '{}' cannot replace a public practical file; return only independent checks at the required evaluator paths.", file.path)));
    }
    Ok(checks)
}

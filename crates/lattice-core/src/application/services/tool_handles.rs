//! The kinds of thing a turn names for the model, and the one place where a
//! tool call's names become the app's own arguments.
//!
//! A result shows a document, a message or the point where a read
//! stopped under a handle; a later call passes that handle back. Ids, offsets,
//! pages and positions never reach the model, so it never has to write one.

use std::sync::Arc;

use crate::application::contracts::tools::ToolDefinition;
use crate::application::services::model_contract::kept::TurnNames;
use crate::application::services::model_contract::reply::Reference;
use crate::application::services::model_contract::shown::{Drawn, Kind};
use crate::application::services::model_contract::tools::{ArgumentError, Parameters};
use crate::shared::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
mod tests;

/// The tool that carries on from where a result stopped.
pub const READ_MORE: &str = "read_more";

const GET_DOCUMENT: &str = "get_document";
const READ_CONVERSATION_HISTORY: &str = "read_conversation_history";

/// A passage of a document, or a web page, a reply may cite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub document_id: String,
    pub chunk_id: String,
}

impl Kind for Source {
    const PREFIX: &'static str = "s";
    const DRAWN: Drawn = Drawn::Number;
    type Ref = (String, String);

    fn reference(&self) -> (String, String) {
        (self.document_id.clone(), self.chunk_id.clone())
    }
}

/// A file of the folder that a tool result showed, by its path from the folder root.
pub struct FolderFile(pub String);

impl Kind for FolderFile {
    const PREFIX: &'static str = "f";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = String;

    fn reference(&self) -> String {
        self.0.clone()
    }
}

/// A document some tool listed or a source of the turn's context.
pub struct Document(pub String);

impl Kind for Document {
    const PREFIX: &'static str = "d";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = String;

    fn reference(&self) -> String {
        self.0.clone()
    }
}

/// A message of this conversation a history tool showed.
pub struct Message(pub String);

impl Kind for Message {
    const PREFIX: &'static str = "h";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = String;

    fn reference(&self) -> String {
        self.0.clone()
    }
}

/// What a picture's reference holds: the request that found it and the app's image line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PictureRef {
    pub request: String,
    pub markdown: String,
}

/// A picture `find_image` stored for a request.
pub struct Picture(pub PictureRef);

impl Kind for Picture {
    const PREFIX: &'static str = "i";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = PictureRef;

    fn reference(&self) -> PictureRef {
        self.0.clone()
    }
}

/// The image line a picture reference stands for, when the picture was found in `request_id`.
pub fn picture_line(reference: &Reference, request_id: &str) -> Option<String> {
    if reference.prefix != Picture::PREFIX {
        return None;
    }
    serde_json::from_value::<PictureRef>(reference.reference.clone())
        .ok()
        .filter(|picture| picture.request == request_id)
        .map(|picture| picture.markdown)
}

/// The call that carries on from where a result stopped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resume {
    pub tool: String,
    pub arguments: Value,
}

/// The point where a result stopped reading.
pub struct More(pub Resume);

impl Kind for More {
    const PREFIX: &'static str = "m";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = Resume;

    fn reference(&self) -> Resume {
        self.0.clone()
    }
}

/// The parameter of `get_document` that names the document.
pub const DOCUMENT_PARAMETER: &str = "document";

const DOCUMENT_DESCRIPTION: &str = "The document to read: the bracketed name of a source in the context, such as [s3], or the bracketed name a tool result showed for it, such as [d2]";

fn document_parameters() -> Parameters {
    Parameters::new().handle::<Document>(DOCUMENT_PARAMETER, DOCUMENT_DESCRIPTION)
}

fn source_parameters() -> Parameters {
    Parameters::new().handle::<Source>(DOCUMENT_PARAMETER, DOCUMENT_DESCRIPTION)
}

fn message_parameters() -> Parameters {
    Parameters::new().handle::<Message>(
        "message",
        "The message to read: the bracketed name a history search or a saved-knowledge result showed for it, such as [h2]",
    )
}

fn read_more_parameters() -> Parameters {
    Parameters::new().handle::<More>(
        "handle",
        "The bracketed name a result gave for where it stopped, such as [m3]",
    )
}

/// The schema property of `get_document`'s document parameter.
pub fn document_property() -> Value {
    property(&document_parameters(), DOCUMENT_PARAMETER)
}

/// The schema property of `read_conversation_history`'s message parameter.
pub fn message_property() -> Value {
    property(&message_parameters(), "message")
}

fn property(parameters: &Parameters, name: &str) -> Value {
    parameters
        .schema()
        .as_json()
        .get("properties")
        .and_then(|properties| properties.get(name))
        .cloned()
        .unwrap_or(Value::Null)
}

/// The `read_more` tool: one way to read on from any result that stopped.
pub fn read_more_definition() -> Result<ToolDefinition> {
    ToolDefinition::new(
        READ_MORE,
        "Read on from where an earlier result stopped. A result that was cut, or that has more pages or matches, names the point it stopped at in square brackets; pass that name here and the next part comes back.",
        read_more_parameters().schema().as_json().clone(),
    )
}

/// The call the app runs for what the model asked: the tool and its own arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct Called {
    pub tool: String,
    pub arguments: Value,
}

/// Everything the turn's tool results have shown, under the names it gave them.
pub struct ToolHandles {
    names: Arc<TurnNames>,
    conversation: Option<String>,
    request: Option<String>,
}

impl Default for ToolHandles {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandles {
    pub fn new() -> Self {
        Self::in_turn(Arc::new(TurnNames::unstored()))
    }

    /// The handles of a turn whose names are `names`.
    pub fn in_turn(names: Arc<TurnNames>) -> Self {
        Self {
            names,
            conversation: None,
            request: None,
        }
    }

    /// The names of a turn of this conversation.
    pub fn in_conversation(mut self, conversation_id: &str) -> Self {
        self.conversation = Some(conversation_id.to_owned());
        self
    }

    /// The names of a turn answering this request.
    pub fn in_request(mut self, request_id: &str) -> Self {
        self.request = Some(request_id.to_owned());
        self
    }

    /// Whether the history tools can read what belongs to `conversation_id`: only this conversation's.
    pub fn reads(&self, conversation_id: &str) -> bool {
        self.conversation.as_deref() == Some(conversation_id)
    }

    fn name<K: Kind>(&self, thing: K) -> Option<String> {
        let things = [thing];
        self.names.with(|scope| {
            let shown = scope.request().show(&things).ok()?;
            shown.handles().first().map(|handle| handle.bracketed())
        })
    }

    /// The bracketed name a document is shown under.
    pub fn document(&self, id: &str) -> Option<String> {
        self.name(Document(id.to_owned()))
    }

    /// The bracketed name a message is shown under.
    pub fn message(&self, id: &str) -> Option<String> {
        self.name(Message(id.to_owned()))
    }

    /// The bracketed name a picture of this request is shown under; `None` without a request.
    pub fn picture(&self, markdown: &str) -> Option<String> {
        let request = self.request.clone()?;
        self.name(Picture(PictureRef {
            request,
            markdown: markdown.to_owned(),
        }))
    }

    /// The bracketed name of the point a call's result stopped at.
    pub fn more(&self, resume: Resume) -> Option<String> {
        self.name(More(resume))
    }

    /// The call the model's arguments mean. A name the model wrote is resolved
    /// here; what the app owns (ids, offsets, pages, positions) is set here
    /// and never taken from the model.
    pub fn read_call(
        &self,
        tool: &str,
        arguments: &Value,
    ) -> std::result::Result<Called, ArgumentError> {
        match tool {
            READ_MORE => self.resume(arguments),
            GET_DOCUMENT => {
                let id = self.document_named(arguments)?;
                let mut called = without_app_owned(tool, arguments);
                if let Some(object) = called.as_object_mut() {
                    object.remove(DOCUMENT_PARAMETER);
                    object.insert("document_id".to_owned(), Value::String(id));
                }
                Ok(Called {
                    tool: tool.to_owned(),
                    arguments: called,
                })
            }
            READ_CONVERSATION_HISTORY => {
                let id = self.message_named(arguments)?;
                let mut called = without_app_owned(tool, arguments);
                if let Some(object) = called.as_object_mut() {
                    object.remove("message");
                    object.insert("message_ids".to_owned(), Value::from(vec![id]));
                }
                Ok(Called {
                    tool: tool.to_owned(),
                    arguments: called,
                })
            }
            _ => Ok(Called {
                tool: tool.to_owned(),
                arguments: without_app_owned(tool, arguments),
            }),
        }
    }

    fn resume(&self, arguments: &Value) -> std::result::Result<Called, ArgumentError> {
        let parameters = read_more_parameters();
        let picked = self
            .names
            .with(|scope| parameters.read(arguments, scope).handle::<More>("handle"))?;
        Ok(Called {
            tool: picked.reference.tool,
            arguments: picked.reference.arguments,
        })
    }

    fn document_named(&self, arguments: &Value) -> std::result::Result<String, ArgumentError> {
        let sources = source_parameters();
        let documents = document_parameters();
        self.names.with(|scope| {
            if let Ok(picked) = sources
                .read(arguments, scope)
                .handle::<Source>(DOCUMENT_PARAMETER)
            {
                return Ok(picked.reference.0);
            }
            documents
                .read(arguments, scope)
                .handle::<Document>(DOCUMENT_PARAMETER)
                .map(|picked| picked.reference)
        })
    }

    fn message_named(&self, arguments: &Value) -> std::result::Result<String, ArgumentError> {
        let parameters = message_parameters();
        let picked = self.names.with(|scope| {
            parameters
                .read(arguments, scope)
                .handle::<Message>("message")
        })?;
        Ok(picked.reference)
    }
}

/// What the app decides for a tool call, so that a call never carries it from the model.
const APP_OWNED: &[(&str, &[&str])] = &[
    ("semantic_search", &["offset"]),
    (GET_DOCUMENT, &["offset", "document_id"]),
    ("list_documents", &["offset"]),
    ("web_search", &["page", "offset"]),
    ("fetch_url_content", &["offset"]),
    ("find_sources", &["page"]),
    ("follow_links", &["near_text"]),
    ("read_course_source", &["offset"]),
    ("search_conversation_history", &["offset"]),
    (
        READ_CONVERSATION_HISTORY,
        &["message_ids", "from_sequence", "to_sequence", "start_byte"],
    ),
];

fn without_app_owned(tool: &str, arguments: &Value) -> Value {
    let mut cleaned = arguments.clone();
    let owned = APP_OWNED
        .iter()
        .find(|(name, _)| *name == tool)
        .map_or(&[][..], |(_, keys)| *keys);
    if let Some(object) = cleaned.as_object_mut() {
        for key in owned {
            object.remove(*key);
        }
    }
    cleaned
}

/// One call as its result is rendered: the call's own arguments and the turn's names.
pub struct CallView<'a> {
    tool: &'a str,
    arguments: &'a Value,
    handles: &'a ToolHandles,
}

impl<'a> CallView<'a> {
    pub fn new(tool: &'a str, arguments: &'a Value, handles: &'a ToolHandles) -> Self {
        Self {
            tool,
            arguments,
            handles,
        }
    }

    pub fn handles(&self) -> &ToolHandles {
        self.handles
    }

    /// The bracketed name of the call that carries on: this call with `changes` applied.
    pub fn read_on(&self, changes: &[(&str, Value)]) -> String {
        let mut arguments = self.arguments.clone();
        if let Some(object) = arguments.as_object_mut() {
            for (key, value) in changes {
                object.insert((*key).to_owned(), value.clone());
            }
        }
        self.handles
            .more(Resume {
                tool: self.tool.to_owned(),
                arguments,
            })
            .unwrap_or_default()
    }

    /// The line that tells the model how to read on, for a name from [`Self::read_on`].
    pub fn read_on_line(&self, name: &str) -> String {
        format!("To read on, call {READ_MORE} with {name}.")
    }
}

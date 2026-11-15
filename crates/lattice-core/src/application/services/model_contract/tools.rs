//! Tools: parameters, results, records.
//!
//! A tool's parameters are built from five things: text, a choice among
//! meanings, a handle, a quantity, a flag. A quantity is the model's own
//! number; the executor clamps it and never refuses it. Tool schemas stay
//! static (a handle is a plain string), so the cached prompt prefix never
//! changes, and no backend enforces a tool parameter: an argument that names
//! something the app showed is resolved by the executor against the scope,
//! and one that does not resolve returns tool-error text saying how to get a
//! valid one. A turn never fails on it.
//!
//! A result is a [`View`]: lines of text, named facts and handles. It has no
//! constructor from a serialisable type, so no storage or IPC type can be
//! dumped into one.

use super::shape::{Schema, Vocabulary};
use super::shown::{Handle, Kind, Picked, Scope};

#[cfg(test)]
mod tests;

/// The parameters of one tool: its schema and the reading of a call's arguments.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Parameters {
    parameters: Vec<Parameter>,
}

#[derive(Debug, Clone, PartialEq)]
struct Parameter {
    name: &'static str,
    description: &'static str,
    kind: ParameterKind,
    required: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum ParameterKind {
    Text,
    Choice(Vec<&'static str>),
    Handle { prefix: &'static str },
    Quantity,
    Flag,
}

impl Parameters {
    pub fn new() -> Self {
        Self::default()
    }

    fn with(
        mut self,
        name: &'static str,
        description: &'static str,
        kind: ParameterKind,
        required: bool,
    ) -> Self {
        self.parameters.push(Parameter {
            name,
            description,
            kind,
            required,
        });
        self
    }

    /// Text the model writes: a query, a URL, a title it knows from the world or the user.
    pub fn text(self, name: &'static str, description: &'static str) -> Self {
        self.with(name, description, ParameterKind::Text, true)
    }

    /// Text the model may leave out.
    pub fn optional_text(self, name: &'static str, description: &'static str) -> Self {
        self.with(name, description, ParameterKind::Text, false)
    }

    /// A choice among meanings.
    pub fn choice<W: Vocabulary>(self, name: &'static str, description: &'static str) -> Self {
        self.with(name, description, ParameterKind::Choice(words::<W>()), true)
    }

    /// A choice the model may leave out.
    pub fn optional_choice<W: Vocabulary>(
        self,
        name: &'static str,
        description: &'static str,
    ) -> Self {
        self.with(
            name,
            description,
            ParameterKind::Choice(words::<W>()),
            false,
        )
    }

    /// The handle of a thing of kind `K` the app showed.
    pub fn handle<K: Kind>(self, name: &'static str, description: &'static str) -> Self {
        let kind = ParameterKind::Handle { prefix: K::PREFIX };
        self.with(name, description, kind, true)
    }

    /// A handle the model may leave out.
    pub fn optional_handle<K: Kind>(self, name: &'static str, description: &'static str) -> Self {
        let kind = ParameterKind::Handle { prefix: K::PREFIX };
        self.with(name, description, kind, false)
    }

    /// The model's own number. Optional: the executor has a default and clamps what is given.
    pub fn quantity(self, name: &'static str, description: &'static str) -> Self {
        self.with(name, description, ParameterKind::Quantity, false)
    }

    /// A flag. Optional: absent is `false`.
    pub fn flag(self, name: &'static str, description: &'static str) -> Self {
        self.with(name, description, ParameterKind::Flag, false)
    }

    /// The parameters as a tool definition carries them.
    pub fn schema(&self) -> Schema {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for parameter in &self.parameters {
            properties.insert(parameter.name.to_owned(), parameter.property());
            if parameter.required {
                required.push(serde_json::Value::from(parameter.name));
            }
        }
        Schema::made(serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        }))
    }

    /// The arguments of one call, read against `scope`.
    pub fn read<'a>(&'a self, arguments: &'a serde_json::Value, scope: &'a Scope) -> Arguments<'a> {
        Arguments {
            parameters: self,
            arguments,
            scope,
        }
    }
}

fn words<W: Vocabulary>() -> Vec<&'static str> {
    W::WORDS.iter().map(|(word, _)| *word).collect()
}

impl Parameter {
    fn property(&self) -> serde_json::Value {
        let description = self.description;
        match &self.kind {
            ParameterKind::Text | ParameterKind::Handle { .. } => {
                serde_json::json!({"type": "string", "description": description})
            }
            ParameterKind::Choice(words) => {
                serde_json::json!({"type": "string", "description": description, "enum": words})
            }
            ParameterKind::Quantity => {
                serde_json::json!({"type": "integer", "description": description})
            }
            ParameterKind::Flag => {
                serde_json::json!({"type": "boolean", "description": description})
            }
        }
    }
}

/// What the model did wrong in a call's arguments, as tool-error text it can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgumentError(pub String);

/// The arguments of one tool call. Every accessor is total; asking for a
/// parameter the tool does not declare, or as another kind than it declares,
/// is a programming error and reads as absent.
pub struct Arguments<'a> {
    parameters: &'a Parameters,
    arguments: &'a serde_json::Value,
    scope: &'a Scope,
}

impl<'a> Arguments<'a> {
    /// The declared parameter called `name` whose kind `is_kind`, if the tool has one.
    fn declared(
        &self,
        name: &str,
        is_kind: impl Fn(&ParameterKind) -> bool,
    ) -> Option<&'a Parameter> {
        self.parameters
            .parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .filter(|parameter| is_kind(&parameter.kind))
    }

    /// What the model wrote for `name`; `null` is the same as leaving it out.
    fn given(&self, name: &str) -> Option<&'a serde_json::Value> {
        self.arguments.get(name).filter(|value| !value.is_null())
    }

    /// The trimmed string the model wrote for `name`, when it wrote a non-empty one.
    fn given_text(&self, name: &str) -> Option<&'a str> {
        self.given(name)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    }

    pub fn text(&self, name: &'static str) -> Result<String, ArgumentError> {
        let parameter = self
            .declared(name, |kind| matches!(kind, ParameterKind::Text))
            .ok_or_else(|| undeclared(name))?;
        self.given_text(name)
            .map(str::to_owned)
            .ok_or_else(|| missing(parameter))
    }

    pub fn optional_text(&self, name: &'static str) -> Option<String> {
        self.declared(name, |kind| matches!(kind, ParameterKind::Text))?;
        self.given_text(name).map(str::to_owned)
    }

    pub fn choice<W: Vocabulary>(&self, name: &'static str) -> Result<W, ArgumentError> {
        let parameter = self
            .declared(name, |kind| matches!(kind, ParameterKind::Choice(_)))
            .ok_or_else(|| undeclared(name))?;
        match self.given(name) {
            None => Err(ArgumentError(format!(
                "{} One of: {}.",
                missing(parameter).0,
                words::<W>().join(", ")
            ))),
            Some(value) => word(name, value),
        }
    }

    /// `Ok(None)` when the model left it out; a word outside the vocabulary is an error.
    pub fn optional_choice<W: Vocabulary>(
        &self,
        name: &'static str,
    ) -> Result<Option<W>, ArgumentError> {
        if self
            .declared(name, |kind| matches!(kind, ParameterKind::Choice(_)))
            .is_none()
        {
            return Ok(None);
        }
        match self.given(name) {
            None => Ok(None),
            Some(value) if value.as_str().is_some_and(|text| text.trim().is_empty()) => Ok(None),
            Some(value) => word(name, value).map(Some),
        }
    }

    /// The shown thing the argument names. Unknown: an error that says how to get a valid handle.
    pub fn handle<K: Kind>(&self, name: &'static str) -> Result<Picked<K>, ArgumentError> {
        let parameter = self
            .declared(name, is_handle_of::<K>)
            .ok_or_else(|| undeclared(name))?;
        let written = self.given_text(name).ok_or_else(|| missing(parameter))?;
        self.resolve(name, written)
    }

    /// `Ok(None)` when the model left it out; one that does not resolve is an error.
    pub fn optional_handle<K: Kind>(
        &self,
        name: &'static str,
    ) -> Result<Option<Picked<K>>, ArgumentError> {
        if self.declared(name, is_handle_of::<K>).is_none() {
            return Ok(None);
        }
        match self.given_text(name) {
            None => Ok(None),
            Some(written) => self.resolve(name, written).map(Some),
        }
    }

    fn resolve<K: Kind>(&self, name: &str, written: &str) -> Result<Picked<K>, ArgumentError> {
        let bare = written
            .strip_prefix('[')
            .and_then(|inner| inner.strip_suffix(']'))
            .unwrap_or(written)
            .trim();
        self.scope.resolve::<K>(bare).ok_or_else(|| {
            ArgumentError(format!(
                "`{name}` was `{written}`, which names nothing shown in this conversation. \
                 A handle is a bracketed name such as [{}4] in an earlier tool result; \
                 pass one of those, with or without the brackets.",
                K::PREFIX
            ))
        })
    }

    /// The model's number within `least..=most`, or `default` when it gave none.
    pub fn quantity(&self, name: &'static str, default: u32, least: u32, most: u32) -> u32 {
        let given = self
            .declared(name, |kind| matches!(kind, ParameterKind::Quantity))
            .and_then(|_| self.given(name))
            .and_then(whole_number);
        let low = least.min(most);
        let high = least.max(most);
        let number = given.unwrap_or_else(|| i128::from(default));
        let within = number.clamp(i128::from(low), i128::from(high));
        u32::try_from(within).unwrap_or(high)
    }

    pub fn flag(&self, name: &'static str) -> bool {
        self.declared(name, |kind| matches!(kind, ParameterKind::Flag))
            .and_then(|_| self.given(name))
            .is_some_and(|value| match value {
                serde_json::Value::Bool(flag) => *flag,
                serde_json::Value::String(text) => text.trim() == "true",
                _ => false,
            })
    }
}

fn is_handle_of<K: Kind>(kind: &ParameterKind) -> bool {
    matches!(kind, ParameterKind::Handle { prefix } if *prefix == K::PREFIX)
}

fn undeclared(name: &str) -> ArgumentError {
    ArgumentError(format!("`{name}` is not one of this tool's parameters."))
}

fn missing(parameter: &Parameter) -> ArgumentError {
    ArgumentError(format!(
        "Give `{}`: {}",
        parameter.name, parameter.description
    ))
}

fn word<W: Vocabulary>(name: &str, value: &serde_json::Value) -> Result<W, ArgumentError> {
    let written = value.as_str().map(str::trim).unwrap_or_default();
    W::WORDS
        .iter()
        .find(|(word, _)| *word == written)
        .map(|(_, meaning)| *meaning)
        .ok_or_else(|| {
            ArgumentError(format!(
                "`{name}` must be exactly one of: {}.",
                words::<W>().join(", ")
            ))
        })
}

/// A JSON integer, or a JSON float or string that is a whole number.
fn whole_number(value: &serde_json::Value) -> Option<i128> {
    match value {
        serde_json::Value::Number(number) => number
            .as_i64()
            .map(i128::from)
            .or_else(|| number.as_u64().map(i128::from))
            .or_else(|| number.as_f64().and_then(whole_float)),
        serde_json::Value::String(text) => {
            let text = text.trim();
            text.parse::<i128>()
                .ok()
                .or_else(|| text.parse::<f64>().ok().and_then(whole_float))
        }
        _ => None,
    }
}

/// `value` as an integer when it is finite and whole; the cap keeps the cast inside `i128`.
fn whole_float(value: f64) -> Option<i128> {
    (value.is_finite() && value.fract() == 0.0 && value.abs() < 1.0e30).then_some(value as i128)
}

/// A tool's result as the model reads it: lines of text, named facts, and a
/// handle only for a thing some tool takes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    lines: Vec<String>,
}

impl View {
    pub fn new() -> Self {
        Self::default()
    }

    /// A line of text.
    pub fn line(mut self, text: impl AsRef<str>) -> Self {
        self.lines.push(text.as_ref().to_owned());
        self
    }

    /// A named fact, in words: `Verified: 3 March 2026`.
    pub fn fact(self, name: &str, value: impl AsRef<str>) -> Self {
        let line = format!("{name}: {}", value.as_ref());
        self.line(line)
    }

    /// A line that starts with the handle of a thing a tool takes.
    pub fn item<K>(self, handle: &Handle<K>, text: impl AsRef<str>) -> Self {
        let line = format!("{} {}", handle.bracketed(), text.as_ref());
        self.line(line)
    }

    /// Another view's lines, indented under this one.
    pub fn nested(mut self, inner: View) -> Self {
        self.lines
            .extend(inner.lines.into_iter().map(|line| format!("  {line}")));
        self
    }

    /// The result's text.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

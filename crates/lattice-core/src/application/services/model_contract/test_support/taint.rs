//! Taint: what is internal never reaches a prompt, and what the model must be told always does.
//!
//! Every internal value of a fixture is a sentinel the test knows, so no pattern for the shape of
//! an id is needed: a UUID, a path, a pointer, a row id and an offset are all caught the same way.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::application::services::model_contract::ask::{Answered, Says};
use crate::application::services::model_contract::shape::{Held, Shape};
use crate::application::services::model_contract::shown::{Internal, Record, Scope, Told};
use crate::application::services::model_contract::tools::{Parameters, View};

/// A recorded string must be this long, so that it does not occur in prose by chance.
const LEAST_SENTINEL_CHARS: usize = 8;

/// A recorded number must be this large, so that it does not occur in prose by chance.
const LEAST_SENTINEL_NUMBER: f64 = 100_000.0;

/// How many names each top-level kind already has in the scope of the second rendering: enough that
/// a written handle in a constant no longer names anything the request shows.
const EARLIER_NAMES: u32 = 50;

/// The sentinel texts recorded so far.
#[derive(Debug, Default)]
pub struct Marks {
    internal: Vec<String>,
    told: Vec<String>,
}

impl Marks {
    pub fn new() -> Self {
        Self::default()
    }

    /// An internal value. Every string and number in its serialised form is recorded as a sentinel.
    pub fn internal<T: Serialize>(&mut self, value: T) -> Internal<T> {
        let found = sentinels(&value, true);
        self.internal.extend(found);
        Internal::of(value)
    }

    /// A value the model must be told. Every string in its serialised form is recorded.
    pub fn told<T: Serialize>(&mut self, value: T) -> Told<T> {
        let found = sentinels(&value, false);
        self.told.extend(found);
        Told(value)
    }

    /// The identity of a shown thing (a `Kind::Ref`): internal as well.
    pub fn reference<T: Serialize + Clone>(&mut self, value: T) -> T {
        let found = sentinels(&value, true);
        self.internal.extend(found);
        value
    }
}

#[allow(clippy::panic)]
fn sentinels<T: Serialize>(value: &T, numbers: bool) -> Vec<String> {
    let json = match serde_json::to_value(value) {
        Ok(json) => json,
        Err(error) => panic!("a sentinel value could not be serialised: {error}"),
    };
    let mut found = Vec::new();
    collect(&json, numbers, &mut found);
    found
}

fn collect(value: &serde_json::Value, numbers: bool, found: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => {
            assert!(
                text.chars().count() >= LEAST_SENTINEL_CHARS,
                "`{text}` is not a sentinel: a recorded string holds at least {LEAST_SENTINEL_CHARS} characters"
            );
            found.push(text.clone());
        }
        serde_json::Value::Number(number) if numbers => {
            let magnitude = number.as_f64().map_or(0.0, f64::abs);
            assert!(
                magnitude >= LEAST_SENTINEL_NUMBER,
                "`{number}` is not a sentinel: a recorded number is at least {LEAST_SENTINEL_NUMBER}"
            );
            found.push(number.to_string());
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect(item, numbers, found);
            }
        }
        serde_json::Value::Object(fields) => {
            for item in fields.values() {
                collect(item, numbers, found);
            }
        }
        _ => {}
    }
}

/// Everything `says` writes, as one text. A passage's id is internal and is not part of it.
fn said_by(says: &Says) -> String {
    let mut parts = vec![
        says.instructions.clone(),
        says.task.clone(),
        says.evidence_heading.clone(),
    ];
    for passage in &says.evidence {
        parts.push(passage.label.clone());
        parts.push(passage.text.clone());
    }
    parts.push(says.closing.clone());
    parts.extend(says.history.iter().map(|item| item.content.clone()));
    parts.join("\n")
}

/// What one rendering of a request said and showed.
struct Rendering {
    said: String,
    /// Every handle the request shows, as written.
    shown: BTreeSet<String>,
    /// The prefixes of the things the request minted at the top level.
    top_prefixes: BTreeSet<String>,
    /// Every prefix the request minted, top level or within an item.
    prefixes: BTreeSet<String>,
}

#[allow(clippy::panic)]
fn render<C: Answered>(input: &C::Input, mut scope: Scope, known: &BTreeSet<String>) -> Rendering {
    let mut said;
    let mut shown = BTreeSet::new();
    {
        let mut showing = scope.request();
        let says = match C::says(input, &mut showing) {
            Ok(says) => says,
            Err(error) => panic!("{} could not say its request: {error}", C::ID.as_str()),
        };
        said = said_by(&says);
        for pointers in [Held::Enums, Held::Plain] {
            if let Some(schema) = <C::Answer as Shape>::schema(&showing, pointers) {
                said.push('\n');
                said.push_str(&schema.as_json().to_string());
            }
        }
        for prefix in known {
            shown.extend(showing.written(prefix));
            shown.extend(showing.written_under(prefix));
        }
    }
    let minted = scope.fresh();
    Rendering {
        said,
        shown,
        top_prefixes: minted
            .iter()
            .filter(|record| record.under.is_none())
            .map(|record| record.prefix.clone())
            .collect(),
        prefixes: minted.iter().map(|record| record.prefix.clone()).collect(),
    }
}

/// A scope in which every top-level prefix already has [`EARLIER_NAMES`] names.
#[allow(clippy::panic)]
fn crowded(prefixes: &BTreeSet<String>) -> Scope {
    let records = prefixes
        .iter()
        .flat_map(|prefix| {
            (1..=EARLIER_NAMES).map(move |ordinal| Record {
                prefix: prefix.clone(),
                ordinal,
                under: None,
                reference: serde_json::Value::String(format!("earlier-{prefix}-{ordinal}")),
            })
        })
        .collect();
    match Scope::stored(records) {
        Ok(scope) => scope,
        Err(error) => panic!("a scope with earlier names could not be built: {error}"),
    }
}

/// A token of `text` that is a handle written out and that the request does not show.
fn unshown_handle(
    text: &str,
    prefixes: &BTreeSet<String>,
    shown: &BTreeSet<String>,
) -> Option<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .find(|token| {
            prefixes.iter().any(|prefix| {
                token.strip_prefix(prefix.as_str()).is_some_and(|rest| {
                    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
                })
            }) && !shown.contains(*token)
        })
        .map(str::to_owned)
}

#[allow(clippy::panic)]
fn assert_sentinels(of: &str, text: &str, marks: &Marks) {
    for sentinel in &marks.internal {
        assert!(
            !text.contains(sentinel.as_str()),
            "the internal value `{sentinel}` reached what {of} says"
        );
    }
    for sentinel in &marks.told {
        assert!(
            text.contains(sentinel.as_str()),
            "the told value `{sentinel}` is missing from what {of} says"
        );
    }
}

/// Renders `C`'s request for `input` and fails the test if an internal sentinel appears in it or a
/// told one is missing from it, or if a constant of the contract writes a handle out.
#[allow(clippy::panic)]
pub fn holds<C: Answered>(input: &C::Input, marks: &Marks) {
    let id = C::ID.as_str();
    let first = render::<C>(input, Scope::for_question(), &BTreeSet::new());
    assert_sentinels(id, &first.said, marks);
    let second = render::<C>(input, crowded(&first.top_prefixes), &first.prefixes);
    if let Some(token) = unshown_handle(&second.said, &first.top_prefixes, &second.shown) {
        panic!(
            "{id} writes the handle `{token}` in a constant: with {EARLIER_NAMES} earlier names the request does not show it"
        );
    }
}

/// The same for a tool: its parameter schema and one result.
pub fn tool_holds(parameters: &Parameters, view: &View, marks: &Marks) {
    let text = format!("{}\n{}", parameters.schema().as_json(), view.text());
    assert_sentinels("the tool", &text, marks);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::services::model_contract::ask::Alone;
    use crate::application::services::model_contract::shape::Pick;
    use crate::application::services::model_contract::shown::{Drawn, Kind, Showing};
    use crate::application::services::model_contract::test_support::samples::{
        SampleChange, SampleEach, SampleFlagged, SampleLead, SampleNew, SampleOrder, SamplePick,
        SamplePickSome, SampleValue,
    };
    use crate::application::services::model_contract::{Contract, ContractId};
    use crate::shared::error::Result;
    use serde::Deserialize;

    macro_rules! sample_holds {
        ($($sample:ident),+ $(,)?) => {
            $({
                let mut marks = Marks::new();
                let input = $sample::fixture(&mut marks);
                holds::<$sample>(&input, &marks);
            })+
        };
    }

    #[test]
    fn every_sample_holds() {
        sample_holds!(
            SampleValue,
            SamplePick,
            SamplePickSome,
            SampleOrder,
            SampleNew,
            SampleEach,
            SampleFlagged,
            SampleChange,
            SampleLead,
        );
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Stone {
        key: String,
        name: Told<String>,
    }

    impl Kind for Stone {
        const PREFIX: &'static str = "s";
        const DRAWN: Drawn = Drawn::Name;
        type Ref = String;

        fn reference(&self) -> String {
            self.key.clone()
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Stones {
        stones: Vec<Stone>,
        secret: Internal<String>,
    }

    fn stones(marks: &mut Marks) -> Stones {
        let stone = |marks: &mut Marks, key: &str, name: &str| Stone {
            key: marks.reference(key.to_owned()),
            name: marks.told(name.to_owned()),
        };
        Stones {
            stones: vec![
                stone(marks, "stone-key-0001", "grey pebble"),
                stone(marks, "stone-key-0002", "white cobble"),
            ],
            secret: marks.internal("secret-note-7731".to_owned()),
        }
    }

    enum How {
        Honest,
        Leaks,
        DropsTold,
        WritesHandle,
    }

    struct Quarry<const HOW: u8>;

    fn how(code: u8) -> How {
        match code {
            0 => How::Honest,
            1 => How::Leaks,
            2 => How::DropsTold,
            _ => How::WritesHandle,
        }
    }

    impl<const HOW: u8> Contract for Quarry<HOW> {
        const ID: ContractId = ContractId::new("quarry");
        type Input = Stones;
    }

    impl<const HOW: u8> Answered for Quarry<HOW> {
        type Answer = Pick<Stone>;
        const ALONE: Alone = Alone::Pending;

        fn says(input: &Stones, showing: &mut Showing<'_>) -> Result<Says> {
            let shown = showing.show(&input.stones)?;
            let mut task = String::from("Choose a stone.\n");
            let mut instructions = String::from("You sort stones.");
            match how(HOW) {
                How::Honest | How::WritesHandle => {}
                How::Leaks => task.push_str(input.secret.value()),
                How::DropsTold => {}
            }
            if matches!(how(HOW), How::WritesHandle) {
                instructions.push_str(" Start with s2.");
            }
            if !matches!(how(HOW), How::DropsTold) {
                for (handle, stone) in shown.iter() {
                    task.push_str(&format!("{} {}\n", handle.bracketed(), stone.name.0));
                }
            }
            Ok(Says::new(instructions, task))
        }
    }

    fn check<const HOW: u8>() {
        let mut marks = Marks::new();
        let input = stones(&mut marks);
        holds::<Quarry<HOW>>(&input, &marks);
    }

    #[test]
    fn a_contract_that_tells_what_it_must_and_nothing_more_holds() {
        check::<0>();
    }

    #[test]
    #[should_panic(expected = "internal value `secret-note-7731` reached")]
    fn an_internal_value_in_the_prompt_is_caught() {
        check::<1>();
    }

    #[test]
    #[should_panic(expected = "told value `grey pebble` is missing")]
    fn a_told_value_left_out_of_the_prompt_is_caught() {
        check::<2>();
    }

    #[test]
    #[should_panic(expected = "the handle `s2`")]
    fn a_handle_written_in_a_constant_is_caught() {
        check::<3>();
    }

    #[test]
    #[should_panic(expected = "is not a sentinel")]
    fn a_short_string_is_not_a_sentinel() {
        Marks::new().told("pebble".to_owned());
    }

    #[test]
    #[should_panic(expected = "is not a sentinel")]
    fn a_small_number_is_not_a_sentinel() {
        Marks::new().internal(42_u32);
    }
}

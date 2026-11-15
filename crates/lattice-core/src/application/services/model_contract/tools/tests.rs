use super::super::shown::Drawn;
use super::*;
use proptest::prelude::*;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Quick,
    Careful,
}

impl Vocabulary for Mode {
    const WORDS: &'static [(&'static str, Self)] =
        &[("quick", Mode::Quick), ("careful", Mode::Careful)];
}

struct Shape {
    name: &'static str,
}

impl Kind for Shape {
    const PREFIX: &'static str = "s";
    const DRAWN: Drawn = Drawn::Number;
    type Ref = String;

    fn reference(&self) -> String {
        self.name.to_owned()
    }
}

struct Colour {
    name: &'static str,
}

impl Kind for Colour {
    const PREFIX: &'static str = "c";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = String;

    fn reference(&self) -> String {
        self.name.to_owned()
    }
}

const NAMES: [&str; 9] = [
    "query",
    "mode",
    "shape",
    "count",
    "strict",
    "note",
    "second_mode",
    "other_shape",
    "extra",
];

fn parameters() -> Parameters {
    Parameters::new()
        .text("query", "What to look for.")
        .choice::<Mode>("mode", "How to look.")
        .handle::<Shape>("shape", "A shape shown earlier.")
        .quantity("count", "How many.")
        .flag("strict", "Only exact matches.")
        .optional_text("note", "An extra note.")
        .optional_choice::<Mode>("second_mode", "Another way.")
        .optional_handle::<Shape>("other_shape", "Another shape.")
}

fn scope() -> Scope {
    let shapes = [
        Shape { name: "circle" },
        Shape { name: "square" },
        Shape { name: "triangle" },
    ];
    let mut scope = Scope::for_question();
    {
        let mut showing = scope.request();
        showing.show(&shapes).unwrap();
        showing.show(&[Colour { name: "red" }]).unwrap();
    }
    scope
}

#[test]
fn schema_of_one_parameter_of_each_kind() {
    let schema = Parameters::new()
        .text("query", "What to look for.")
        .choice::<Mode>("mode", "How to look.")
        .handle::<Shape>("shape", "A shape shown earlier.")
        .quantity("count", "How many.")
        .flag("strict", "Only exact matches.")
        .schema();
    assert_eq!(
        schema.as_json(),
        &json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "What to look for."},
                "mode": {"type": "string", "description": "How to look.", "enum": ["quick", "careful"]},
                "shape": {"type": "string", "description": "A shape shown earlier."},
                "count": {"type": "integer", "description": "How many."},
                "strict": {"type": "boolean", "description": "Only exact matches."},
            },
            "required": ["query", "mode", "shape"],
            "additionalProperties": false,
        })
    );
}

#[test]
fn schema_lists_properties_in_the_order_added() {
    let schema = parameters().schema();
    let properties = schema.as_json()["properties"].as_object().unwrap();
    let names: Vec<&str> = properties.keys().map(String::as_str).collect();
    assert_eq!(names, NAMES[..8]);
}

#[test]
fn a_handle_has_no_enum_in_its_schema() {
    let schema = parameters().schema();
    assert!(schema.as_json()["properties"]["shape"].get("enum").is_none());
}

#[test]
fn a_tool_with_no_parameters_has_an_empty_object_schema() {
    assert_eq!(
        Parameters::new().schema().as_json(),
        &json!({"type": "object", "properties": {}, "required": [], "additionalProperties": false})
    );
}

#[test]
fn text_reads_a_string_and_trims_it() {
    let parameters = parameters();
    let scope = scope();
    let arguments = json!({"query": "  round things  "});
    let read = parameters.read(&arguments, &scope);
    assert_eq!(read.text("query"), Ok("round things".to_owned()));
}

#[test]
fn text_that_is_empty_missing_null_or_not_a_string_is_an_error() {
    let parameters = parameters();
    let scope = scope();
    for arguments in [
        json!({"query": "   "}),
        json!({}),
        json!({"query": null}),
        json!({"query": 4}),
        json!([1, 2]),
        json!("query"),
    ] {
        let read = parameters.read(&arguments, &scope);
        let error = read.text("query").unwrap_err();
        assert!(
            error.0.starts_with("Give `query`: What to look for."),
            "{error:?}"
        );
    }
}

#[test]
fn optional_text_is_none_when_empty_or_absent() {
    let parameters = parameters();
    let scope = scope();
    let given = json!({"note": " a note "});
    assert_eq!(
        parameters.read(&given, &scope).optional_text("note"),
        Some("a note".to_owned())
    );
    for arguments in [json!({"note": ""}), json!({}), json!({"note": false})] {
        assert_eq!(
            parameters.read(&arguments, &scope).optional_text("note"),
            None
        );
    }
}

#[test]
fn choice_reads_exactly_one_word() {
    let parameters = parameters();
    let scope = scope();
    let arguments = json!({"mode": "careful"});
    assert_eq!(
        parameters.read(&arguments, &scope).choice::<Mode>("mode"),
        Ok(Mode::Careful)
    );
    let loose = json!({"mode": " quick "});
    assert_eq!(
        parameters.read(&loose, &scope).choice::<Mode>("mode"),
        Ok(Mode::Quick)
    );
}

#[test]
fn a_choice_outside_the_words_lists_them() {
    let parameters = parameters();
    let scope = scope();
    for arguments in [
        json!({"mode": "slow"}),
        json!({"mode": "Quick"}),
        json!({"mode": 3}),
        json!({}),
    ] {
        let error = parameters
            .read(&arguments, &scope)
            .choice::<Mode>("mode")
            .unwrap_err();
        assert!(error.0.contains("quick, careful"), "{error:?}");
    }
}

#[test]
fn optional_choice_is_none_when_absent_and_an_error_when_wrong() {
    let parameters = parameters();
    let scope = scope();
    for arguments in [
        json!({}),
        json!({"second_mode": null}),
        json!({"second_mode": ""}),
    ] {
        assert_eq!(
            parameters
                .read(&arguments, &scope)
                .optional_choice::<Mode>("second_mode"),
            Ok(None)
        );
    }
    let good = json!({"second_mode": "quick"});
    assert_eq!(
        parameters
            .read(&good, &scope)
            .optional_choice::<Mode>("second_mode"),
        Ok(Some(Mode::Quick))
    );
    let wrong = json!({"second_mode": "slow"});
    assert!(parameters
        .read(&wrong, &scope)
        .optional_choice::<Mode>("second_mode")
        .is_err());
}

#[test]
fn a_handle_resolves_with_or_without_brackets() {
    let parameters = parameters();
    let scope = scope();
    for written in ["s2", "[s2]", " [s2] ", "[ s2 ]"] {
        let arguments = json!({"shape": written});
        let picked = parameters
            .read(&arguments, &scope)
            .handle::<Shape>("shape")
            .unwrap();
        assert_eq!(picked.reference, "square", "{written}");
        assert_eq!(picked.handle.written(), "s2");
    }
}

#[test]
fn an_unknown_handle_is_an_error_and_not_a_panic() {
    let parameters = parameters();
    let scope = scope();
    for written in [
        "s9", "[s9]", "c1", "s0", "s", "[s2", "s2]", "[[s2]]", "square", "",
    ] {
        let arguments = json!({"shape": written});
        let error = parameters
            .read(&arguments, &scope)
            .handle::<Shape>("shape")
            .unwrap_err();
        assert!(!error.0.is_empty());
        if !written.is_empty() {
            assert!(
                error
                    .0
                    .contains("names nothing shown in this conversation"),
                "{error:?}"
            );
            assert!(error.0.contains("earlier tool result"), "{error:?}");
        }
    }
}

#[test]
fn a_missing_handle_names_the_parameter() {
    let parameters = parameters();
    let scope = scope();
    let arguments = json!({});
    let error = parameters
        .read(&arguments, &scope)
        .handle::<Shape>("shape")
        .unwrap_err();
    assert!(error.0.starts_with("Give `shape`:"), "{error:?}");
}

#[test]
fn an_optional_handle_is_none_when_absent_and_an_error_when_unknown() {
    let parameters = parameters();
    let scope = scope();
    let absent = json!({});
    assert!(matches!(
        parameters
            .read(&absent, &scope)
            .optional_handle::<Shape>("other_shape"),
        Ok(None)
    ));
    let known = json!({"other_shape": "[s3]"});
    let picked = parameters
        .read(&known, &scope)
        .optional_handle::<Shape>("other_shape")
        .unwrap()
        .unwrap();
    assert_eq!(picked.reference, "triangle");
    let unknown = json!({"other_shape": "s7"});
    assert!(parameters
        .read(&unknown, &scope)
        .optional_handle::<Shape>("other_shape")
        .is_err());
}

#[test]
fn quantity_reads_integers_floats_and_strings() {
    let parameters = parameters();
    let scope = scope();
    for (value, expected) in [
        (json!(5), 5),
        (json!(5.0), 5),
        (json!("5"), 5),
        (json!(" 5 "), 5),
        (json!("5.0"), 5),
        (json!(8e0), 8),
    ] {
        let arguments = json!({"count": value});
        assert_eq!(
            parameters
                .read(&arguments, &scope)
                .quantity("count", 3, 1, 10),
            expected,
            "{value}"
        );
    }
}

#[test]
fn quantity_clamps_into_its_range() {
    let parameters = parameters();
    let scope = scope();
    for (value, expected) in [
        (json!(0), 2),
        (json!(-4), 2),
        (json!(99), 10),
        (json!(1e300), 10),
        (json!(-1e300), 2),
        (json!(u64::MAX), 10),
        (json!(i64::MIN), 2),
    ] {
        let arguments = json!({"count": value});
        assert_eq!(
            parameters
                .read(&arguments, &scope)
                .quantity("count", 3, 2, 10),
            expected,
            "{value}"
        );
    }
}

#[test]
fn quantity_defaults_when_not_a_whole_number() {
    let parameters = parameters();
    let scope = scope();
    for arguments in [
        json!({}),
        json!({"count": null}),
        json!({"count": 2.5}),
        json!({"count": "many"}),
        json!({"count": true}),
        json!({"count": [4]}),
    ] {
        assert_eq!(
            parameters
                .read(&arguments, &scope)
                .quantity("count", 3, 1, 10),
            3
        );
    }
}

#[test]
fn a_default_outside_the_range_is_clamped_too() {
    let parameters = parameters();
    let scope = scope();
    let arguments = json!({});
    let read = parameters.read(&arguments, &scope);
    assert_eq!(read.quantity("count", 50, 1, 10), 10);
    assert_eq!(read.quantity("count", 0, 4, 10), 4);
}

#[test]
fn a_reversed_range_does_not_panic() {
    let parameters = parameters();
    let scope = scope();
    let arguments = json!({"count": 7});
    assert_eq!(
        parameters
            .read(&arguments, &scope)
            .quantity("count", 3, 10, 2),
        7
    );
}

#[test]
fn flag_is_true_only_for_true_and_the_string_true() {
    let parameters = parameters();
    let scope = scope();
    for (value, expected) in [
        (json!(true), true),
        (json!("true"), true),
        (json!(false), false),
        (json!("false"), false),
        (json!("yes"), false),
        (json!(1), false),
        (json!(null), false),
    ] {
        let arguments = json!({"strict": value});
        assert_eq!(
            parameters.read(&arguments, &scope).flag("strict"),
            expected,
            "{value}"
        );
    }
    let absent = json!({});
    assert!(!parameters.read(&absent, &scope).flag("strict"));
}

#[test]
fn undeclared_parameters_read_as_absent() {
    let parameters = parameters();
    let scope = scope();
    let arguments = json!({"extra": "x", "other": 4, "flag": true});
    let read = parameters.read(&arguments, &scope);
    assert!(read
        .text("extra")
        .unwrap_err()
        .0
        .contains("not one of this tool's parameters"));
    assert_eq!(read.optional_text("extra"), None);
    assert!(read.choice::<Mode>("extra").is_err());
    assert_eq!(read.optional_choice::<Mode>("extra"), Ok(None));
    assert!(read.handle::<Shape>("extra").is_err());
    assert!(matches!(read.optional_handle::<Shape>("extra"), Ok(None)));
    assert_eq!(read.quantity("other", 3, 1, 10), 3);
    assert!(!read.flag("flag"));
}

#[test]
fn a_parameter_read_as_another_kind_reads_as_absent() {
    let parameters = parameters();
    let scope = scope();
    let arguments =
        json!({"query": "x", "count": 4, "strict": true, "shape": "s1", "mode": "quick"});
    let read = parameters.read(&arguments, &scope);
    assert!(read.text("count").is_err());
    assert_eq!(read.optional_text("shape"), None);
    assert!(read.choice::<Mode>("query").is_err());
    assert!(read.handle::<Shape>("query").is_err());
    assert!(read.handle::<Colour>("shape").is_err());
    assert_eq!(read.quantity("query", 3, 1, 10), 3);
    assert!(!read.flag("count"));
}

fn arbitrary_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        any::<u64>().prop_map(Value::from),
        any::<f64>().prop_map(|number| serde_json::Number::from_f64(number)
            .map(Value::Number)
            .unwrap_or(Value::Null)),
        prop_oneof![
            Just("s1".to_owned()),
            Just("[s2]".to_owned()),
            Just("quick".to_owned()),
            Just("true".to_owned()),
            Just("7".to_owned()),
            Just("1e3".to_owned()),
            ".{0,12}",
        ]
        .prop_map(Value::from),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::btree_map("[a-z_]{1,12}", inner, 0..4)
                .prop_map(|map| Value::Object(map.into_iter().collect())),
        ]
    })
}

fn arguments_object() -> impl Strategy<Value = Value> {
    let names = prop::sample::select(NAMES.to_vec());
    prop::collection::btree_map(names, arbitrary_json(), 0..9).prop_map(|map| {
        Value::Object(
            map.into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        )
    })
}

fn read_everything(arguments: &Value) {
    let parameters = parameters();
    let scope = scope();
    let read = parameters.read(arguments, &scope);
    for name in NAMES {
        let _ = read.text(name);
        let _ = read.optional_text(name);
        let _ = read.choice::<Mode>(name);
        let _ = read.optional_choice::<Mode>(name);
        let _ = read.handle::<Shape>(name);
        let _ = read.handle::<Colour>(name);
        let _ = read.optional_handle::<Shape>(name);
        let count = read.quantity(name, 3, 2, 10);
        assert!((2..=10).contains(&count));
        let _ = read.flag(name);
    }
}

proptest! {
    #[test]
    fn arguments_never_panic_for_any_object(arguments in arguments_object()) {
        read_everything(&arguments);
    }

    #[test]
    fn arguments_never_panic_for_any_json(arguments in arbitrary_json()) {
        read_everything(&arguments);
    }
}

#[test]
fn a_view_is_lines_facts_items_and_nesting() {
    let shapes = [Shape { name: "circle" }];
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let shown = showing.show(&shapes).unwrap();
    let (handle, _) = shown.iter().next().unwrap();

    let inner = View::new().line("round").fact("Sides", "none");
    let view = View::new()
        .line("Found one shape.")
        .fact("Verified", "3 March 2026")
        .item(handle, "a circle")
        .nested(inner);
    assert_eq!(
        view.text(),
        "Found one shape.\nVerified: 3 March 2026\n[s1] a circle\n  round\n  Sides: none"
    );
}

#[test]
fn an_empty_view_is_empty_text() {
    assert_eq!(View::new().text(), "");
}

#[test]
fn nesting_indents_every_level() {
    let deep = View::new().nested(View::new().nested(View::new().line("x")));
    assert_eq!(deep.text(), "    x");
}

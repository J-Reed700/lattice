use serde_json::{json, Map, Value};

use super::revision::{canonical_json, revision_of};

#[test]
fn the_revision_ignores_key_order_and_follows_every_value() {
    let mut left_inner = Map::new();
    left_inner.insert("b".into(), json!(2));
    left_inner.insert("a".into(), json!(1));

    let mut left = Map::new();
    left.insert("z".into(), json!("Quillet"));
    left.insert("nested".into(), Value::Object(left_inner));
    left.insert("arr".into(), json!([{ "y": false, "x": true }]));

    let mut right_inner = Map::new();
    right_inner.insert("a".into(), json!(1));
    right_inner.insert("b".into(), json!(2));

    let mut right = Map::new();
    right.insert("arr".into(), json!([{ "x": true, "y": false }]));
    right.insert("nested".into(), Value::Object(right_inner));
    right.insert("z".into(), json!("Quillet"));

    let left_val = Value::Object(left);
    let right_val = Value::Object(right);

    let c1 = canonical_json(&left_val);
    let c2 = canonical_json(&right_val);
    assert!(!c1.is_empty());
    assert_eq!(c1, c2);
    assert_eq!(revision_of(&left_val), revision_of(&right_val));
    assert!(!revision_of(&left_val).is_empty());

    let changed_leaf = json!({
        "arr": [{ "x": false, "y": false }],
        "nested": { "a": 1, "b": 2 },
        "z": "Quillet"
    });
    assert_ne!(revision_of(&left_val), revision_of(&changed_leaf));
}

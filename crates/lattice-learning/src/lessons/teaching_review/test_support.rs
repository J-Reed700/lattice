//! The seeded lesson content of the teaching review tests, shared with lattice-desktop's live eval.

use serde_json::{json, Value};

pub fn candidate() -> Value {
    json!({"blocks":[{"body":"Rust’s compiler is rustc.\n```rust\nfn main() {\n    println!(\"a  b\");\n}\n```"},{"body":"The cell’s membrane separates the cell interior from its environment."},{"body":"The pastry’s rest comes before rolling, as specified in this recipe."}]})
}

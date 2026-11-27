//! Prints the sentences `flowed_sentences` cuts from each NUL-separated text on
//! stdin, one line per text of `start:end` byte spans. It compiles the app's
//! own segmentation files in place, so the replay cuts what the app cuts.
#![allow(dead_code, unused_imports)]

use std::io::{Read, Write};

#[path = "../../../../../crates/lattice-core/src/shared/markdown_math.rs"]
mod markdown_math;

#[path = "../../../../../crates/lattice-core/src/shared/text/segmentation/mod.rs"]
mod segmentation;

mod shared {
    pub(crate) use super::markdown_math;

    pub(crate) mod text {
        pub(crate) fn floor_char_boundary(_text: &str, _index: usize) -> usize {
            unreachable!("the replay only cuts sentences")
        }
    }
}

fn main() {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("UTF-8 texts on stdin");
    let mut out = std::io::stdout().lock();
    for text in input.split('\0') {
        let spans: Vec<String> = segmentation::flowed_sentences(text)
            .into_iter()
            .map(|(start, end)| format!("{start}:{end}"))
            .collect();
        writeln!(out, "{}", spans.join(" ")).expect("stdout");
    }
}

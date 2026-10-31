#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use super::lexicon::tests::sample;
use super::*;

fn g2p() -> G2p {
    G2p {
        lexicon: sample(),
        fallback: None,
    }
}

#[test]
fn keeps_punctuation_and_spacing() {
    assert_eq!(g2p().phonemize("Hello, cat."), "həlˈQ, kˈat.");
    assert_eq!(g2p().phonemize("(hello)"), "(həlˈQ)");
    assert_eq!(g2p().phonemize("hello—cat"), "həlˈQ—kˈat");
}

#[test]
fn reads_the_before_vowels_and_consonants() {
    assert_eq!(g2p().phonemize("the egg"), "ði ˈɛɡ");
    assert_eq!(g2p().phonemize("the cat"), "ðə kˈat");
}

#[test]
fn reads_amounts_with_their_currency() {
    assert_eq!(g2p().phonemize("$5"), "fˈIv dˈɒləz");
    assert_eq!(g2p().phonemize("50%"), "fˈɪfti pəsˈɛnt");
}

#[test]
fn a_compound_keeps_one_primary_stress() {
    assert_eq!(g2p().phonemize("hello-cat"), "həlˈQkˌat");
}

#[test]
fn an_unreadable_word_is_marked_unknown_without_a_fallback() {
    assert_eq!(g2p().phonemize("zxqv cat"), "❓ kˈat");
}

#[test]
#[ignore = "needs the downloaded speech bundle in LATTICE_KOKORO_DIR"]
fn reads_british_english_with_the_downloaded_bundle() {
    let dir = PathBuf::from(std::env::var_os("LATTICE_KOKORO_DIR").expect("LATTICE_KOKORO_DIR"));
    let g2p = G2p::load(
        &dir.join("misaki-gb_gold.json"),
        &dir.join("misaki-gb_silver.json"),
        &dir.join("misaki-g2p-gb.json"),
        &dir.join("misaki-g2p-gb.safetensors"),
    )
    .unwrap();
    for text in [
        "Hello, this is a test of the British voice.",
        "In 1990, the U.S. spent $5.50 on 3.5 kilograms of tomatoes — about 50% more.",
        "Kokoro reads Lattice's lessons aloud, doesn't it?",
        "Retrieval practice beats rereading.",
    ] {
        eprintln!("{text}\n  {}", g2p.phonemize(text));
    }
    assert_eq!(g2p.phonemize("Hello, world."), "həlˈQ, wˈɜːld.");
    assert!(g2p.phonemize("Kokoro").chars().any(|c| c == 'k'));
}

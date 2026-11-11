use std::path::PathBuf;

use super::lexicon::tests::{american_sample, sample};
use super::*;

fn g2p() -> G2p {
    G2p {
        lexicon: sample(),
        fallback: None,
    }
}

fn american() -> G2p {
    G2p {
        lexicon: american_sample(),
        fallback: None,
    }
}

#[test]
fn a_tap_and_a_glottal_stop_reach_kokoro_as_misaki_hands_them_over() {
    assert_eq!(american().phonemize("water"), "wˈɔTəɹ");
    assert_eq!(american().phonemize("button"), "bˈʌtn");
    assert_eq!(american().phonemize("waited"), "wˈATᵻd");
}

#[test]
fn an_accent_reads_from_its_own_lexicon() {
    assert_eq!(g2p().phonemize("hello"), "həlˈQ");
    assert_eq!(american().phonemize("hello"), "həlˈO");
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
    let g2p = G2p::load(SpeechAccent::British, &dir).unwrap();
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

/// misaki 0.9.4 with the pinned lexicons, on sentences that read the same
/// without its part-of-speech tagger.
const AMERICAN_READINGS: [(&str, &str); 10] = [
    ("Hello, world.", "həlˈO, wˈɜɹld."),
    (
        "Hello, this is a test of the American voice.",
        "həlˈO, ðɪs ɪz ɐ tˈɛst ʌv ði əmˈɛɹəkᵊn vˈYs.",
    ),
    (
        "The water is better than the butter, but neither matters.",
        "ðə wˈɔTəɹ ɪz bˈɛTəɹ ðən ðə bˈʌTəɹ, bˌʌt nˈiðəɹ mˈæTəɹz.",
    ),
    (
        "I waited for the letter, and I wanted to write it better.",
        "ˌI wˈATᵻd fɔɹ ðə lˈɛTəɹ, ænd ˌI wˈɑntᵻd tə ɹˈIt ɪt bˈɛTəɹ.",
    ),
    (
        "She kissed the cats and needed the buses, which were waiting.",
        "ʃˌi kˈɪst ðə kˈæts ænd nˈidᵻd ðə bˈʌsᵻz, wˌɪʧ wɜɹ wˈATɪŋ.",
    ),
    (
        "The button on the mountain was certainly written.",
        "ðə bˈʌtn ˌɔn ðə mˈWntᵊn wʌz sˈɜɹtnli ɹˈɪtn.",
    ),
    (
        "Started, wanted, needed, painted, waited, loaded, voted.",
        "stˈɑɹTᵻd, wˈɑntᵻd, nˈidᵻd, pˈAntᵻd, wˈATᵻd, lˈOdᵻd, vˈOTᵻd.",
    ),
    (
        "Churches, bridges, wishes, boxes, and judges.",
        "ʧˈɜɹʧᵻz, bɹˈɪʤɪz, wˈɪʃᵻz, bˈɑksᵻz, ænd ʤˈʌʤəz.",
    ),
    (
        "Getting started was exciting, and shipping it was rewarding.",
        "ɡˈɛTɪŋ stˈɑɹTᵻd wʌz ɪksˈITɪŋ, ænd ʃˈɪpɪŋ ɪt wʌz ɹəwˈɔɹdɪŋ.",
    ),
    (
        "Mitochondria, zeitgeist, and Lattice.",
        "mˌITəkˈɑndɹiə, zˈItɡˌIst, ænd lˈæTəs.",
    ),
];

#[test]
#[ignore = "needs the voice pack in LATTICE_KOKORO_PACK_DIR"]
fn reads_american_english_as_misaki_does() {
    let dir = PathBuf::from(
        std::env::var_os("LATTICE_KOKORO_PACK_DIR").expect("LATTICE_KOKORO_PACK_DIR"),
    );
    let g2p = G2p::load(SpeechAccent::American, &dir).unwrap();
    for (text, phonemes) in AMERICAN_READINGS {
        assert_eq!(g2p.phonemize(text), phonemes, "{text}");
    }
    assert!(g2p.phonemize("Kokoro").chars().any(|c| c == 'k'));
}

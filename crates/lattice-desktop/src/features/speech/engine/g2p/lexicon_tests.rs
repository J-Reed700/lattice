use super::*;

pub(in super::super) fn sample() -> Lexicon {
    let gold = serde_json::json!({
        "hello": "həlˈQ", "the": "ðə", "to": "tuː", "cat": "kˈat", "walk": "wˈɔːk",
        "run": "ɹˈʌn", "kiss": "kˈɪs", "dollar": "dˈɒlə", "pound": "pˈWnd", "pence": "pˈɛns",
        "cent": "sˈɛnt", "and": "and", "minus": "mˈInəs", "point": "pˈYnt", "O": "ˈQ",
        "zero": "zˈɪəɹQ", "one": "wˈʌn", "two": "tˈuː", "three": "θɹˈiː", "four": "fˈɔː",
        "five": "fˈIv", "nine": "nˈIn", "ten": "tˈɛn", "twenty": "twˈɛnti", "fifty": "fˈɪfti",
        "nineteen": "nˌIntˈiːn", "ninety": "nˈInti", "hundred": "hˈʌndɹəd", "first": "fˈɜːst",
        "thousand": "θˈWzᵊnd", "percent": "pəsˈɛnt", "N": "ˈɛn", "A": "ˈA", "S": "ˈɛs",
        "used": {"DEFAULT": "jˈuːzd", "VBD": "jˈuːst"},
        "won't": {"DEFAULT": "wQnt", "None": "wˈQnt"},
        "egg": "ˈɛɡ", "apple": "ˈapᵊl",
    });
    let silver = serde_json::json!({ "lattice": "lˈatɪs" });
    Lexicon::from_json(gold.to_string().as_bytes(), silver.to_string().as_bytes()).unwrap()
}

fn read(lexicon: &Lexicon, word: &str) -> Option<String> {
    lexicon.read(word, Context::default(), true, None)
}

#[test]
fn reads_dictionary_words_and_their_capitalised_twins() {
    let lexicon = sample();
    assert_eq!(read(&lexicon, "hello").as_deref(), Some("həlˈQ"));
    assert_eq!(read(&lexicon, "Hello").as_deref(), Some("həlˈQ"));
    assert_eq!(read(&lexicon, "lattice").as_deref(), Some("lˈatɪs"));
    assert_eq!(read(&lexicon, "zxqv"), None);
}

#[test]
fn inflects_known_stems() {
    let lexicon = sample();
    assert_eq!(read(&lexicon, "cats").as_deref(), Some("kˈats"));
    assert_eq!(read(&lexicon, "kisses").as_deref(), Some("kˈɪsɪz"));
    assert_eq!(read(&lexicon, "walked").as_deref(), Some("wˈɔːkt"));
    assert_eq!(read(&lexicon, "running").as_deref(), Some("ɹˈʌnɪŋ"));
}

#[test]
fn function_words_follow_the_next_sound() {
    let lexicon = sample();
    let before = |vowel| Context {
        future_vowel: Some(vowel),
        future_to: false,
    };
    assert_eq!(
        lexicon.read("the", before(true), true, None).as_deref(),
        Some("ði")
    );
    assert_eq!(
        lexicon.read("the", before(false), true, None).as_deref(),
        Some("ðə")
    );
    assert_eq!(
        lexicon.read("to", before(false), true, None).as_deref(),
        Some("tə")
    );
    assert_eq!(
        lexicon.read("a", before(false), true, None).as_deref(),
        Some("ɐ")
    );
    let at_end = Context::default();
    assert_eq!(
        lexicon.read("won't", at_end, true, None).as_deref(),
        Some("wˈQnt")
    );
}

#[test]
fn spells_unknown_capitals_as_initials() {
    let lexicon = sample();
    assert_eq!(read(&lexicon, "NSA").as_deref(), Some("ˌɛnˌɛsˈA"));
}

#[test]
fn reads_numbers() {
    let lexicon = sample();
    assert_eq!(read(&lexicon, "1990").as_deref(), Some("nˌIntˈiːn nˈInti"));
    assert_eq!(
        read(&lexicon, "1990s").as_deref(),
        Some("nˌIntˈiːn nˈIntiz")
    );
    assert_eq!(read(&lexicon, "1st").as_deref(), Some("fˈɜːst"));
    assert_eq!(read(&lexicon, "3.5").as_deref(), Some("θɹˈiː pYnt fˈIv"));
    assert_eq!(read(&lexicon, "-2").as_deref(), Some("mˈInəs tˈuː"));
    assert_eq!(
        lexicon
            .read("5", Context::default(), true, Some("$"))
            .as_deref(),
        Some("fˈIv dˈɒləz")
    );
    assert_eq!(
        lexicon
            .read("1.50", Context::default(), true, Some("£"))
            .as_deref(),
        Some("wˈʌn pˈWnd and fˈɪfti pˈɛns")
    );
}

#[test]
fn stress_moves_as_misaki_moves_it() {
    assert_eq!(apply_stress("həlˈQ", Some(-0.5)), "həlˌQ");
    assert_eq!(apply_stress("həlˈQ", Some(-2.0)), "həlQ");
    assert_eq!(apply_stress("kat", Some(2.0)), "kˈat");
    assert_eq!(apply_stress("kat", Some(0.5)), "kˌat");
    assert_eq!(apply_stress("ˌkat", Some(1.0)), "ˈkat");
}

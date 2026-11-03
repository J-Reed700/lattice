//! The boundary table: what `sentences` makes of the English Golden Rules
//! (pragmatic_segmenter, the set pySBD is measured on) and of the app's own
//! text. `golden` is set only where our answer misses the golden one.

pub(super) struct Row {
    pub name: &'static str,
    pub text: &'static str,
    pub ours: &'static [&'static str],
    pub golden: Option<&'static [&'static str]>,
}

const fn row(name: &'static str, text: &'static str, ours: &'static [&'static str]) -> Row {
    Row {
        name,
        text,
        ours,
        golden: None,
    }
}

const fn miss(
    name: &'static str,
    text: &'static str,
    ours: &'static [&'static str],
    golden: &'static [&'static str],
) -> Row {
    Row {
        name,
        text,
        ours,
        golden: Some(golden),
    }
}

pub(super) const GOLDEN_RULES: &[Row] = &[
    row("simple period", "Hello World. My name is Jonas.", &["Hello World.", "My name is Jonas."]),
    row("question mark", "What is your name? My name is Jonas.", &["What is your name?", "My name is Jonas."]),
    row("exclamation point", "There it is! I found it.", &["There it is!", "I found it."]),
    row("one-letter uppercase abbreviation", "My name is Jonas E. Smith.", &["My name is Jonas E. Smith."]),
    miss("one-letter lowercase abbreviation", "Please turn to p. 55.", &["Please turn to p.", "55."], &["Please turn to p. 55."]),
    row("two-letter lowercase abbreviation mid-sentence", "Were Jane and co. at the party?", &["Were Jane and co. at the party?"]),
    row("two-letter uppercase abbreviation mid-sentence", "They closed the deal with Pitt, Briggs & Co. at noon.", &["They closed the deal with Pitt, Briggs & Co. at noon."]),
    row("two-letter lowercase abbreviation ending a sentence", "Let's ask Jane and co. They should know.", &["Let's ask Jane and co.", "They should know."]),
    row("two-letter uppercase abbreviation ending a sentence", "They closed the deal with Pitt, Briggs & Co. It closed yesterday.", &["They closed the deal with Pitt, Briggs & Co.", "It closed yesterday."]),
    row("prepositive abbreviation", "I can see Mt. Fuji from here.", &["I can see Mt. Fuji from here."]),
    row("prepositive and postpositive abbreviations", "St. Michael's Church is on 5th st. near the light.", &["St. Michael's Church is on 5th st. near the light."]),
    row("possessive abbreviation", "That is JFK Jr.'s book.", &["That is JFK Jr.'s book."]),
    row("multi-period abbreviation mid-sentence", "I visited the U.S.A. last year.", &["I visited the U.S.A. last year."]),
    row("multi-period abbreviation ending a sentence", "I live in the E.U. How about you?", &["I live in the E.U.", "How about you?"]),
    miss("U.S. ending a sentence", "I live in the U.S. How about you?", &["I live in the U.S. How about you?"], &["I live in the U.S.", "How about you?"]),
    row("U.S. before a capitalised word", "I work for the U.S. Government in Virginia.", &["I work for the U.S. Government in Virginia."]),
    row("U.S. mid-sentence", "I have lived in the U.S. for 20 years.", &["I have lived in the U.S. for 20 years."]),
    miss("a.m. and P.M.", "At 5 a.m. Mr. Smith went to the bank. He left the bank at 6 P.M. Mr. Smith then went to the store.", &["At 5 a.m. Mr. Smith went to the bank.", "He left the bank at 6 P.M. Mr. Smith then went to the store."], &["At 5 a.m. Mr. Smith went to the bank.", "He left the bank at 6 P.M.", "Mr. Smith then went to the store."]),
    row("decimal mid-sentence", "She has $100.00 in her bag.", &["She has $100.00 in her bag."]),
    row("decimal ending a sentence", "She has $100.00. It is in her bag.", &["She has $100.00.", "It is in her bag."]),
    row("parenthetical sentence", "He teaches science (He previously worked for 5 years as an engineer.) at the local University.", &["He teaches science (He previously worked for 5 years as an engineer.) at the local University."]),
    row("email address", "Her email is Jane.Doe@example.com. I sent her an email.", &["Her email is Jane.Doe@example.com.", "I sent her an email."]),
    row("web address", "The site is: https://www.example.50.com/new-site/awesome_content.html. Please check it out.", &["The site is: https://www.example.50.com/new-site/awesome_content.html.", "Please check it out."]),
    row("single quotation mid-sentence", "She turned to him, 'This is great.' she said.", &["She turned to him, 'This is great.' she said."]),
    row("double quotation mid-sentence", "She turned to him, \"This is great.\" she said.", &["She turned to him, \"This is great.\" she said."]),
    row("double quotation ending a sentence", "She turned to him, \"This is great.\" She held the book out to show him.", &["She turned to him, \"This is great.\"", "She held the book out to show him."]),
    row("double exclamation", "Hello!! Long time no see.", &["Hello!!", "Long time no see."]),
    row("double question", "Hello?? Who is there?", &["Hello??", "Who is there?"]),
    row("exclamation and question", "Hello!? Is that you?", &["Hello!?", "Is that you?"]),
    row("question and exclamation", "Hello?! Is that you?", &["Hello?!", "Is that you?"]),
    miss("inline numbered list", "1.) The first item 2.) The second item", &["1.) The first item 2.)", "The second item"], &["1.) The first item", "2.) The second item"]),
    miss("wrapped line (a line break ends a sentence here, by design)", "This is a sentence\ncut off in the middle because pdf.", &["This is a sentence", "cut off in the middle because pdf."], &["This is a sentence cut off in the middle because pdf."]),
    miss("wrapped line after a space (by design)", "It was a cold \nnight in the city.", &["It was a cold", "night in the city."], &["It was a cold night in the city."]),
    row("lowercase lines", "features\ncontact manager\nevents, activities\n", &["features", "contact manager", "events, activities"]),
    miss("number after an abbreviation", "You can find it at N°. 1026.253.553. That is where the treasure is.", &["You can find it at N°.", "1026.253.553.", "That is where the treasure is."], &["You can find it at N°. 1026.253.553.", "That is where the treasure is."]),
    row("exclamation in a name", "She works at Yahoo! in the accounting department.", &["She works at Yahoo! in the accounting department."]),
    miss("I ending a sentence and as an initial", "We make a good team, you and I. Did you see Albert I. Jones yesterday?", &["We make a good team, you and I. Did you see Albert I. Jones yesterday?"], &["We make a good team, you and I.", "Did you see Albert I. Jones yesterday?"]),
    row("ellipsis closing a quotation", "Thoreau argues that by simplifying one’s life, “the laws of the universe will appear less complex. . . .”", &["Thoreau argues that by simplifying one’s life, “the laws of the universe will appear less complex. . . .”"]),
    miss("bracketed ellipses", "\"Bohr [...] used the analogy of parallel stairways [...]\" (Smith 55).", &["\"Bohr [...] used the analogy of parallel stairways [...]\"", "(Smith 55)."], &["\"Bohr [...] used the analogy of parallel stairways [...]\" (Smith 55)."]),
    row("spaced ellipsis ending a sentence", "If words are left off at the end of a sentence, and that is all that is omitted, indicate the omission with ellipsis marks (preceded and followed by a space) and then indicate the end of the sentence with a period . . . . Next sentence.", &["If words are left off at the end of a sentence, and that is all that is omitted, indicate the omission with ellipsis marks (preceded and followed by a space) and then indicate the end of the sentence with a period . . . .", "Next sentence."]),
    row("four dots", "I never meant that.... She left the store.", &["I never meant that....", "She left the store."]),
    miss("ellipses inside a sentence", "I wasn’t really ... well, what I mean...see . . . what I'm saying, the thing is . . . I didn’t mean it.", &["I wasn’t really ... well, what I mean...see . . . what I'm saying, the thing is . . .", "I didn’t mean it."], &["I wasn’t really ... well, what I mean...see . . . what I'm saying, the thing is . . . I didn’t mean it."]),
];

pub(super) const APP_TEXT: &[Row] = &[
    row(
        "emphasis closes its sentence",
        "**Bold.** Next one.",
        &["**Bold.**", "Next one."],
    ),
    row(
        "underscore and bracket close their sentence",
        "It works (_mostly._) Next one.",
        &["It works (_mostly._)", "Next one."],
    ),
    row(
        "inline code holds its stops",
        "Use `a. b` here. Then go.",
        &["Use `a. b` here.", "Then go."],
    ),
    row(
        "inline code closes its sentence",
        "Run `cargo test.` Then read.",
        &["Run `cargo test.`", "Then read."],
    ),
    row(
        "inline math holds its stops",
        "We know $n! = n(n-1)!$ holds. Next.",
        &["We know $n! = n(n-1)!$ holds.", "Next."],
    ),
    row(
        "a fence is cut only between its lines",
        "Intro.\n\n```rust\nlet x = 1. Y\nfoo(); bar();\n```\nAfter.",
        &[
            "Intro.",
            "```rust",
            "let x = 1. Y",
            "foo(); bar();",
            "```",
            "After.",
        ],
    ),
    row(
        "a table row is one unit",
        "| A. B | C. D |\n|---|---|\n| e. F | g |",
        &["| A. B | C. D |", "|---|---|", "| e. F | g |"],
    ),
    row(
        "ordered list markers end nothing",
        "1. First item\n2. Second item\n10) Tenth.",
        &["1. First item", "2. Second item", "10) Tenth."],
    ),
    row(
        "full-width stops",
        "这是第一句话。这是第二句话！还有吗？",
        &["这是第一句话。", "这是第二句话！", "还有吗？"],
    ),
    row(
        "a closing quote stays with its sentence",
        "He said \"Stop.\" Then he left.",
        &["He said \"Stop.\"", "Then he left."],
    ),
    row(
        "dialogue continues after ! and ?",
        "\"Stop!\" she said. \"Why?\" he asked.",
        &["\"Stop!\" she said.", "\"Why?\" he asked."],
    ),
    row(
        "decimals and file names",
        "Version 2.5 ships in file.rs today. Done.",
        &["Version 2.5 ships in file.rs today.", "Done."],
    ),
    row(
        "headings, CRLF and paragraphs",
        "## Heading\r\nBody text. More text.\r\n\r\nNew paragraph.",
        &["## Heading", "Body text.", "More text.", "New paragraph."],
    ),
    row(
        "a link keeps its address",
        "See [the guide](https://example.com/v2.0/guide). Then stop.",
        &[
            "See [the guide](https://example.com/v2.0/guide).",
            "Then stop.",
        ],
    ),
    row(
        "honorifics",
        "Dr. Smith met Mrs. Robinson and Mr. Brown. They talked.",
        &["Dr. Smith met Mrs. Robinson and Mr. Brown.", "They talked."],
    ),
    row(
        "lowercase abbreviations",
        "Use e.g. the first or i.e. the second. Next.",
        &["Use e.g. the first or i.e. the second.", "Next."],
    ),
    row(
        "surrounding whitespace",
        "  One.  Two.  ",
        &["One.", "Two."],
    ),
    row(
        "bullets",
        "# Title\n\n- Item one.\n- Item two",
        &["# Title", "- Item one.", "- Item two"],
    ),
    row(
        "the old rule's own case",
        "Cells divide. Version 2.5 ships!  Why?\nA list item",
        &["Cells divide.", "Version 2.5 ships!", "Why?", "A list item"],
    ),
    row(
        "a unicode ellipsis ends nothing",
        "Wait… What happened? Nothing.",
        &["Wait… What happened?", "Nothing."],
    ),
    row("blank text", "   \n  ", &[]),
    row("empty text", "", &[]),
];

/// Golden Rules our answer misses; it may only fall.
pub(super) const KNOWN_MISSES: usize = 10;

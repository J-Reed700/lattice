//! Attaching pictures to the newest user message and stripping them again.

use super::*;

fn picture(alt: &str) -> ImagePart {
    ImagePart {
        mime: "image/jpeg".into(),
        data_base64: "QUJDRA==".repeat(1000),
        width: 448,
        height: 336,
        tokens: 194,
        alt: alt.into(),
    }
}

fn message(role: &str, content: &str) -> CompletionInput {
    CompletionInput::Message {
        role: role.into(),
        content: content.into(),
    }
}

/// Each part as a short label: the image's alt or the text.
fn parts(item: &CompletionInput) -> Vec<String> {
    let CompletionInput::Multimodal { role, parts } = item else {
        panic!("expected a multimodal message, got {item:?}");
    };
    assert_eq!(role, "user");
    parts
        .iter()
        .map(|part| match part {
            ContentPart::Text { text } => format!("text:{text}"),
            ContentPart::Image(image) => format!("image:{}", image.alt),
        })
        .collect()
}

#[test]
fn pictures_go_to_the_newest_user_message_before_its_text() {
    let mut input = vec![
        message("system", "Be brief"),
        message("user", "Earlier"),
        message("assistant", "Answer"),
        message("user", "What is in these?"),
    ];
    assert!(attach_images_to_last_user(
        &mut input,
        vec![picture("a.png"), picture("b.png")]
    ));
    assert_eq!(
        parts(&input[3]),
        ["image:a.png", "image:b.png", "text:What is in these?"]
    );
    assert!(matches!(&input[1], CompletionInput::Message { content, .. } if content == "Earlier"));
    assert!(attach_images_to_last_user(
        &mut input,
        vec![picture("c.png")]
    ));
    assert_eq!(
        parts(&input[3]),
        [
            "image:a.png",
            "image:b.png",
            "image:c.png",
            "text:What is in these?"
        ]
    );
}

#[test]
fn nothing_is_attached_without_a_user_message() {
    let mut input = vec![message("system", "Be brief")];
    assert!(!attach_images_to_last_user(
        &mut input,
        vec![picture("a.png")]
    ));
    assert!(matches!(&input[0], CompletionInput::Message { .. }));
}

#[test]
fn stripping_turns_pictures_into_caption_lines() {
    let mut input = vec![message("user", "Describe it")];
    attach_images_to_last_user(&mut input, vec![picture("cat.jpg")]);
    let stripped = strip_images(&input);
    assert!(matches!(
        &stripped[0],
        CompletionInput::Message { role, content }
            if role == "user" && content == "Describe it\n[Picture: cat.jpg (448×336)]"
    ));
    assert!(!CompletionRequest {
        input: stripped,
        ..Default::default()
    }
    .has_images());
}

#[test]
fn the_text_a_tokenizer_reads_leaves_the_bytes_out() {
    let mut input = vec![message("user", "Describe it")];
    attach_images_to_last_user(&mut input, vec![picture("cat.jpg")]);
    assert_eq!(input[0].text(), "cat.jpg\nDescribe it");
    assert_eq!(input[0].image_tokens(), 194);
    assert!(input[0].is_user());
    let request = CompletionRequest {
        input,
        ..Default::default()
    };
    assert!(request.has_images());
    assert_eq!(request.image_tokens(), 194);
    assert!(!format!("{request:?}").contains("QUJDRA=="));
}

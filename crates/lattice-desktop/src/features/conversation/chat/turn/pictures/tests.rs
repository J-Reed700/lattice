use super::*;
use crate::features::conversation::chat::test_runtime::{
    test_attachment, FakeChatImages, FakeRuntime,
};
use crate::features::llm::engine::factory::MockLLMPort;

const GRID_28: VisionGeometry = VisionGeometry::CONSERVATIVE_DEFAULT;
const FIXED: VisionGeometry = VisionGeometry::Fixed {
    side: 896,
    tokens: 256,
};

fn three() -> Vec<ChatImageDto> {
    (1..=3).map(|n| test_attachment(n, 1600, 1200)).collect()
}

fn shown(planned: &[Planned<'_>]) -> Vec<bool> {
    planned.iter().map(|plan| plan.shown).collect()
}

#[test]
fn pictures_that_fit_are_sent_at_their_full_fit() {
    let pictures = three();
    let full = GRID_28.plan(1600, 1200).tokens as usize;
    let planned = fit_to_room(GRID_28, &pictures, 3 * full);
    assert_eq!(shown(&planned), [true, true, true]);
    assert!(planned.iter().all(|plan| plan.max_tokens.is_none()));
}

#[test]
fn the_last_attached_shrinks_first() {
    let pictures = three();
    let full = GRID_28.plan(1600, 1200).tokens as usize;
    let planned = fit_to_room(GRID_28, &pictures, 3 * full - full / 2);
    assert_eq!(shown(&planned), [true, true, true]);
    assert!(planned[0].max_tokens.is_none() && planned[1].max_tokens.is_none());
    assert!(planned[2].max_tokens.is_some());
    assert!(planned.iter().map(|plan| plan.tokens).sum::<usize>() <= 3 * full - full / 2);
}

#[test]
fn when_shrinking_is_not_enough_the_last_attached_is_dropped_first() {
    let pictures = three();
    let min = GRID_28.min_tokens() as usize;
    // Room for two pictures at their smallest, not three.
    let planned = fit_to_room(GRID_28, &pictures, 2 * min + 1);
    assert_eq!(shown(&planned), [true, true, false]);
    let kept: usize = planned
        .iter()
        .filter(|plan| plan.shown)
        .map(|plan| plan.tokens)
        .sum();
    assert!(kept <= 2 * min + 1);
}

#[test]
fn a_fixed_encoder_drops_rather_than_shrinks() {
    let pictures = three();
    let planned = fit_to_room(FIXED, &pictures, 2 * 256 + 100);
    assert_eq!(shown(&planned), [true, true, false]);
    assert!(planned.iter().all(|plan| plan.max_tokens.is_none()));
}

#[test]
fn the_final_user_message_gets_images_first_and_caption_lines_last() {
    let mut input = vec![
        CompletionInput::Message {
            role: "system".into(),
            content: "Be helpful.".into(),
        },
        CompletionInput::Message {
            role: "user".into(),
            content: "What is this?".into(),
        },
    ];
    let carried = CarriedPictures {
        parts: vec![
            lattice_core::application::contracts::images::PreparedImageDto {
                mime: "image/jpeg".into(),
                data_base64: "QUJD".into(),
                width: 224,
                height: 224,
                tokens: 66,
            }
            .into_part("leaf.jpg"),
        ],
        captions: vec!["[Picture attached: bark.jpg (10×10). Not shown.]".into()],
        tokens: 70,
        seen: true,
        shrunk: 0,
        dropped: 1,
    };
    carried.apply(&mut input);
    let CompletionInput::Multimodal { role, parts } = &input[1] else {
        panic!("the question carries its picture: {:?}", input[1]);
    };
    assert_eq!(role, "user");
    assert!(matches!(&parts[0], ContentPart::Image(image) if image.alt == "leaf.jpg"));
    assert!(matches!(
        &parts[1],
        ContentPart::Text { text } if text == "What is this?\n[Picture attached: bark.jpg (10×10). Not shown.]"
    ));
}

fn turn_with(
    llm: Arc<dyn LLMPort>,
    runtime: &FakeRuntime,
    pictures: Vec<ChatImageDto>,
) -> PreparedTurn {
    let mut turn = PreparedTurn::for_test(
        "pictures-conversation",
        "pictures-turn",
        llm,
        runtime.conversations.clone(),
        runtime.turn_cancellation.clone(),
    )
    .unwrap();
    turn.budget.picture_room = 1_000_000;
    turn.pictures = pictures;
    turn
}

#[tokio::test]
async fn a_model_that_sees_gets_each_picture_prepared_at_its_fit() {
    let pictures = vec![test_attachment(1, 1600, 1200), test_attachment(2, 400, 300)];
    let images = FakeChatImages::staged(pictures.clone());
    let runtime = FakeRuntime {
        images: images.clone(),
        ..FakeRuntime::default()
    };
    let llm: Arc<dyn LLMPort> = Arc::new(MockLLMPort::new().with_vision(GRID_28));
    let turn = turn_with(llm, &runtime, pictures.clone());

    let carried = carry_pictures(&runtime, &turn).await;

    assert!(carried.seen);
    assert_eq!(carried.parts.len(), 2);
    assert!(carried.captions.is_empty());
    assert_eq!(carried.parts[0].alt, "shot-1.png");
    assert_eq!(
        carried.tokens,
        planned_tokens(turn.llm.as_ref(), &pictures),
        "planning from the rows charges what preparing sends"
    );
    assert_eq!(
        images.calls(),
        vec![
            format!("prepared {} -", pictures[0].id),
            format!("prepared {} -", pictures[1].id)
        ]
    );
}

#[tokio::test]
async fn a_model_that_cannot_see_is_told_the_pictures_are_there() {
    let pictures = vec![test_attachment(1, 800, 600)];
    let images = FakeChatImages::staged(pictures.clone());
    let runtime = FakeRuntime {
        images: images.clone(),
        ..FakeRuntime::default()
    };
    let llm: Arc<dyn LLMPort> = Arc::new(MockLLMPort::new());
    let turn = turn_with(llm, &runtime, pictures);

    let carried = carry_pictures(&runtime, &turn).await;

    assert!(!carried.seen);
    assert!(carried.parts.is_empty());
    assert_eq!(
        carried.captions,
        [
            "[Picture attached: shot-1.png (800×600). This model cannot see pictures; \
          say so if the answer depends on it.]"
        ]
    );
    assert!(
        images.calls().is_empty(),
        "nothing is prepared for a model that cannot see"
    );
}

#[tokio::test]
async fn a_picture_with_no_room_becomes_its_caption_line() {
    let pictures = vec![
        test_attachment(1, 1600, 1200),
        test_attachment(2, 1600, 1200),
    ];
    let images = FakeChatImages::staged(pictures.clone());
    let runtime = FakeRuntime {
        images: images.clone(),
        ..FakeRuntime::default()
    };
    let llm: Arc<dyn LLMPort> = Arc::new(MockLLMPort::new().with_vision(FIXED));
    let mut turn = turn_with(llm, &runtime, pictures.clone());
    turn.budget.picture_room = 300;

    let carried = carry_pictures(&runtime, &turn).await;

    assert!(carried.seen);
    assert_eq!(carried.parts.len(), 1);
    assert_eq!(carried.dropped, 1);
    assert_eq!(
        carried.captions,
        ["[Picture attached: shot-2.png (1600×1200). Not shown: the request has no room for it.]"]
    );
    assert_eq!(
        images.calls(),
        vec![format!("prepared {} -", pictures[0].id)]
    );
}

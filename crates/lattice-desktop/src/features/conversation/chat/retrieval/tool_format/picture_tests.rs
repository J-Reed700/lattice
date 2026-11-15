use super::*;
use lattice_core::application::contracts::images::FoundImage;
use lattice_core::application::services::tool_handles::ToolHandles;

fn found(number: u32, caption: &str, license: Option<&str>) -> FoundImage {
    FoundImage {
        r#ref: format!("img-{number}"),
        caption: caption.to_owned(),
        source_title: Some("File:Kiln.jpg".to_owned()),
        source_url: "https://example.test/kiln".to_owned(),
        license: license.map(str::to_owned),
        width: 640,
        height: 480,
        markdown: format!("![{caption}](img-{number})"),
    }
}

fn render(output: &FindImageOutput) -> String {
    let handles = ToolHandles::new().in_request("request-1");
    let arguments = json!({});
    let result = FunctionResult::success(serde_json::to_value(output).unwrap_or_default());
    format_tool_result(
        "find_image",
        &result,
        &ToolOutputSettingsDto::default(),
        50_000,
        0,
        false,
        &HashMap::new(),
        &CallView::new("find_image", &arguments, &handles),
    )
    .text
}

#[test]
fn a_result_names_each_picture_and_shows_no_image_address() {
    let output = FindImageOutput {
        images: vec![
            found(1, "A kiln", Some("Public domain")),
            found(2, "A lamp", None),
        ],
        note: Some("2 candidates were shown to you and 2 kept.".to_owned()),
    };
    let text = render(&output);
    assert_eq!(
        text,
        "[i1] A kiln · File:Kiln.jpg · Public domain · 640×480\n[i2] A lamp · File:Kiln.jpg · 640×480\n2 candidates were shown to you and 2 kept."
    );
    assert!(!text.contains("img-"), "{text}");
}

#[test]
fn a_result_without_pictures_says_why() {
    let noted = FindImageOutput {
        images: Vec::new(),
        note: Some("Nothing fit.".to_owned()),
    };
    assert_eq!(render(&noted), "Nothing fit.");
    assert_eq!(render(&FindImageOutput::default()), NO_PICTURE_NOTE);
}

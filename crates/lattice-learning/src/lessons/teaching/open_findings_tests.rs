//! A lesson that changed since the reviewer saw it has every open finding
//! judged again, wherever in the lesson it is.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::loop_tests::{parts_lesson, retitled, run, ByRequest};
use super::standing_tests::{lesson, report, reviewed, FIRST, SECOND};
use super::*;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};

#[test]
fn a_change_to_one_section_asks_about_a_finding_at_another() {
    let candidate = lesson(&[FIRST, SECOND]);
    let (standing, fresh) = reviewed(
        &candidate,
        vec![report(
            Some(FindingSite::Section(1)),
            "Alpha holds the line.",
            Some(SECOND),
        )],
    );
    let changed = lesson(&[
        "The first part states one plain fact. It adds a detail.",
        SECOND,
    ]);
    let round = standing.round(&changed).unwrap();
    assert_eq!(round.sections, vec![0]);
    assert_eq!(round.asked.len(), 1);
    assert_eq!(round.asked[0].id, fresh[0].id());
    assert_eq!(round.asked[0].site, FindingSite::Section(1));
}

/// A reviewer that finds nothing it is asked about still present.
struct Mended<'a>(&'a ByRequest);

#[async_trait::async_trait]
impl LLMPort for Mended<'_> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut response = self.0.complete(request).await?;
        let judges = request
            .json_schema
            .as_ref()
            .is_some_and(|schema| schema.pointer("/properties/openFindings").is_some());
        let Ok(Value::Object(mut answer)) = serde_json::from_str::<Value>(&response.text) else {
            return Ok(response);
        };
        if !judges {
            return Ok(response);
        }
        if let Some(Value::Array(verdicts)) = answer.get_mut("openFindings") {
            for verdict in verdicts {
                verdict["verdict"] = json!("resolved");
            }
        }
        let mended = CompletionResponse::from_text(Value::Object(answer).to_string());
        response.text = mended.text;
        response.replay = mended.replay;
        Ok(response)
    }
    fn model_name(&self) -> &str {
        self.0.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.0.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.0.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// Two sections each hold a defect; the one repair changes only the first.
async fn mended_in_the_first_section() -> Result<(ByRequest, Reviewed)> {
    let model = ByRequest::new(
        |text| text.contains("first idea"),
        |sent, _| vec![retitled(sent, 0)],
    );
    let reviewed = run(&Mended(&model), parts_lesson(2)).await?;
    Ok((model, reviewed))
}

#[tokio::test]
async fn the_recheck_lists_the_finding_in_the_section_the_repair_did_not_change() -> Result<()> {
    let (model, _) = mended_in_the_first_section().await?;
    let reviews = model.reviews();
    assert_eq!(reviews[1]["sectionIndicesToReview"], json!([0]));
    let parts: Vec<&str> = reviews[1]["openFindings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["part"].as_str())
        .collect();
    assert_eq!(parts, ["Section 1", "Section 2"]);
    Ok(())
}

#[tokio::test]
async fn a_defect_mended_by_a_change_elsewhere_is_not_set_aside() -> Result<()> {
    let (model, reviewed) = mended_in_the_first_section().await?;
    assert!(reviewed.set_aside.is_empty(), "{:?}", reviewed.set_aside);
    assert_eq!(model.repairs().len(), 1);
    Ok(())
}

#[tokio::test]
async fn the_recheck_carries_no_known_findings() -> Result<()> {
    let (model, _) = mended_in_the_first_section().await?;
    assert!(model.reviews()[1].get("knownFindings").is_none());
    Ok(())
}

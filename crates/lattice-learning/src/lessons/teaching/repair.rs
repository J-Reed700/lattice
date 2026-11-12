//! The repair call: the model's answer to a review's findings.
use super::answers;
use crate::lessons::copied_wording::Wording;
use crate::lessons::text_edits;
use crate::lessons::writer_excerpts::{self, WriterExcerpts};
use crate::outline_progress::OutlineProgress;
use crate::review_findings::{FindingSite, ReviewFinding};
use lattice_core::shared::hash::sha256_hex;
use lattice_core::{
    application::ports::LLMPort,
    application::services::model_contract::{Contract, ContractCall, ContractId, PromptInput},
    protocol,
    shared::error::{AppError, Result},
    shared::protocol::Protocol,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// One finding sent to a repair.
pub(super) struct Issue<'a> {
    pub finding: &'a ReviewFinding,
    /// What became of the last repair's attempt at it, when there was one.
    pub last_attempt: Option<String>,
}

impl Issue<'_> {
    /// The finding as `issuesToFix` lists it for the model.
    fn entry(&self) -> Value {
        let finding = self.finding;
        let id = finding.id();
        let part = if finding.mends_anywhere() {
            None
        } else {
            finding.site.and_then(FindingSite::pointer)
        };
        json!({
            "id": id.as_str(),
            "part": part,
            "also": finding.also.and_then(FindingSite::pointer),
            "passage": finding.passage.as_deref().or(finding.sentence.as_deref()),
            "note": finding.note,
            "lastAttempt": self.last_attempt,
        })
    }
}

pub(super) struct Request<'a> {
    pub system: &'a str,
    pub schema: &'a Value,
    pub authoring: &'a Value,
    pub candidate: &'a Value,
    pub quote_checks: &'a [Value],
    /// The findings the repair is asked to correct.
    pub issues: Vec<Issue<'a>>,
    /// The repair may give edits; otherwise it rewrites the material whole.
    pub edits: bool,
    pub wording: &'a Wording,
    pub output_tokens: usize,
}

pub(super) enum Repaired {
    /// The flat patch, not yet applied, and each finding's plan (edits form).
    Patch {
        patch: String,
        plans: BTreeMap<String, String>,
    },
    /// An answer of plans and no change (edits form).
    Declined { plans: BTreeMap<String, String> },
    /// The whole material (rewrite form).
    Whole(String),
}

impl Request<'_> {
    /// `issuesToFix`: one entry for each finding sent.
    fn issues_to_fix(&self) -> Value {
        Value::Array(self.issues.iter().map(Issue::entry).collect())
    }

    /// The ids a change may name: each distinct id sent, in the order they
    /// first appear.
    fn finding_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for issue in &self.issues {
            let id = issue.finding.id();
            if !ids.iter().any(|known| known == id.as_str()) {
                ids.push(id.as_str().to_owned());
            }
        }
        ids
    }
}

/// Which parts a change naming one of `findings` may touch: its own part or
/// its second part; any part when it is about the whole material or
/// something the lesson lacks.
pub(super) fn reach<'a>(
    findings: impl IntoIterator<Item = &'a ReviewFinding>,
) -> impl Fn(Option<&str>, &str) -> std::result::Result<(), String> + 'a {
    let findings: Vec<&'a ReviewFinding> = findings.into_iter().collect();
    move |named, part| {
        let Some(id) = named else {
            return Err("names no finding.".to_owned());
        };
        let mut refusal = None;
        for finding in findings
            .iter()
            .filter(|finding| finding.id().as_str() == id)
        {
            if finding.mends_anywhere() {
                return Ok(());
            }
            let Some(own) = finding.site.and_then(FindingSite::pointer) else {
                return Ok(());
            };
            let also = finding.also.and_then(FindingSite::pointer);
            if part == own || also.as_deref() == Some(part) {
                return Ok(());
            }
            refusal.get_or_insert_with(|| {
                let what = if part.is_empty() {
                    "a field outside every block and question"
                } else {
                    part
                };
                let allowed = match &also {
                    Some(also) => format!("{own} and {also}"),
                    None => own.clone(),
                };
                format!("is for {id} and changes {what}, which is outside that finding's part ({allowed}).")
            });
        }
        Err(refusal
            .unwrap_or_else(|| format!("is for {id}, which is not a finding of this repair.")))
    }
}

/// The candidate a repair's answer makes, from the `sent` findings it
/// answered.
pub(super) fn apply(
    previous: &Value,
    repaired: Repaired,
    sent: &[ReviewFinding],
    wording: &Wording,
) -> Result<String> {
    match repaired {
        Repaired::Patch { patch, .. } => {
            text_edits::apply_for(previous, &patch, &Value::Null, wording, &reach(sent))
                .map(|applied| applied.value.to_string())
        }
        Repaired::Declined { .. } => Ok(previous.to_string()),
        Repaired::Whole(text) => Ok(text),
    }
}

const REPAIR_POLICY: Protocol = protocol!("teaching-repair", 3);

/// The repair of a lesson's findings.
pub(crate) struct TeachingRepair;

impl Contract for TeachingRepair {
    const ID: ContractId = ContractId::new("teaching-repair");
    type Input = PromptInput;
}

/// What text a repair adds or changes is held to, beyond the finding it is for.
const KEPT_TO_THE_CONTRACT: &str =
    "Added or changed text uses a term, or asks the learner to do or use something, only where the lesson has taught it by that point; otherwise it says there what the term means or how to do it. It never says again what the sentence before or after it already says.";

/// One way of asking for a repair: what it sends, and what it makes of the answer.
trait Form: Sync {
    fn system(&self, request: &Request<'_>) -> String;
    fn schema(&self, request: &Request<'_>) -> Value;
    /// The whole prompt, `requirements` standing for `originalRequirements`.
    fn prompt(&self, request: &Request<'_>, requirements: Value) -> Map<String, Value>;
    /// Whether the form can use this raw answer.
    fn accept(&self, request: &Request<'_>, raw: &str) -> Result<()>;
    fn repaired(&self, raw: String) -> Result<Repaired>;
}

struct Edits;
struct Rewrite;

fn parts(request: &Request<'_>, requirements: Value) -> [(&'static str, Value); 4] {
    [
        ("originalRequirements", requirements),
        ("candidate", request.candidate.clone()),
        ("sourceQuoteChecks", json!(request.quote_checks)),
        ("issuesToFix", request.issues_to_fix()),
    ]
}

impl Form for Edits {
    fn system(&self, request: &Request<'_>) -> String {
        format!("You are editing a rejected draft. Correct only the established teaching defects. issuesToFix lists them, one entry each: its id, the part it is about (the path of a block or question; null when it is about the material as a whole), a second part when it is about two (also), the words it is about when it names any (passage), what to correct (note), and what became of the last repair's attempt at it (lastAttempt; null on a first attempt). Answer every entry once in answers: its id (finding), what you will change for it and where (plan), then the edits, citations and uncite that make that change. These change only that entry's part or its also part; those for an entry whose part is null may change any part. A defect that something is not taught is corrected by adding it: before is an existing sentence beside the place, after is that sentence followed by the added text. Add it after a sentence whose thought is complete, so that no sentence is parted from the one it continues. The learner sees only the lesson: added or changed text teaches in the lesson's own words and never mentions the course plan, the review, the sources' excerpts or these instructions. {KEPT_TO_THE_CONTRACT} {}", text_edits::instructions(request.schema))
    }

    fn schema(&self, request: &Request<'_>) -> Value {
        text_edits::schema_for_findings(request.candidate, request.schema, &request.finding_ids())
    }

    fn prompt(&self, request: &Request<'_>, requirements: Value) -> Map<String, Value> {
        parts(request, requirements)
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn accept(&self, request: &Request<'_>, raw: &str) -> Result<()> {
        let answered = answers::read(raw, &request.finding_ids())?;
        if !answered.changed {
            return Ok(());
        }
        let may_change = reach(request.issues.iter().map(|issue| issue.finding));
        text_edits::apply_for(
            request.candidate,
            &answered.patch,
            &Value::Null,
            request.wording,
            &may_change,
        )
        .map(|_| ())
    }

    fn repaired(&self, raw: String) -> Result<Repaired> {
        answers::reread(&raw).map(|answered| {
            if answered.changed {
                Repaired::Patch {
                    patch: answered.patch,
                    plans: answered.plans,
                }
            } else {
                Repaired::Declined {
                    plans: answered.plans,
                }
            }
        })
    }
}

impl Form for Rewrite {
    fn system(&self, request: &Request<'_>) -> String {
        format!("{}\nYou are editing a rejected draft. Apply every listed correction to the actual content. Do not reproduce the draft unchanged.", request.system)
    }

    fn schema(&self, request: &Request<'_>) -> Value {
        request.schema.clone()
    }

    fn prompt(&self, request: &Request<'_>, requirements: Value) -> Map<String, Value> {
        [("task", json!("Repair the teaching material"))]
            .into_iter()
            .chain(parts(request, requirements))
            .chain([("instruction", json!(format!("Give the complete corrected material. Change every defective passage, exercise, key or explanation identified above. An unchanged draft is not a repair. Preserve valid content. Do not mention this internal review, the course plan or the sources' excerpts in the learner-facing material. {KEPT_TO_THE_CONTRACT}")))])
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn accept(&self, _request: &Request<'_>, _raw: &str) -> Result<()> {
        Ok(())
    }

    fn repaired(&self, raw: String) -> Result<Repaired> {
        Ok(Repaired::Whole(raw))
    }
}

/// Asks the model for the repair, unless the job already holds its answer
/// (or its refusal) for these inputs.
pub(super) async fn ask(
    llm: &dyn LLMPort,
    request: &Request<'_>,
    progress: Option<&OutlineProgress>,
) -> Result<Repaired> {
    let form: &dyn Form = if request.edits { &Edits } else { &Rewrite };
    let system = form.system(request);
    let schema = form.schema(request);
    let key = format!(
        "teaching-repair-answer:{}",
        sha256_hex(
            json!({"policy":REPAIR_POLICY,"model":llm.model_name(),"context":llm.max_context_tokens(),"system":system,"prompt":form.prompt(request, request.authoring.clone()),"schema":schema}).to_string()
        )
    );
    if let Some(saved) = crate::lesson_drafts::checkpoint(&key).await? {
        if let Some(raw) = saved.get("answer").and_then(Value::as_str) {
            crate::lesson_progress::stage("Reusing the completed teaching repair");
            return form.repaired(raw.to_owned());
        }
        if let Some(reason) = saved.get("refused").and_then(Value::as_str) {
            return Err(AppError::ModelAnswerUnusable(reason.to_owned()));
        }
    }
    // Fitted to the window as the review is.
    let mut requirements = request.authoring.clone();
    let excerpts = WriterExcerpts::take(llm, &mut requirements, request.candidate);
    let prompt = form.prompt(request, requirements);
    let wr = writer_excerpts::Request {
        subject: "the teaching repair",
        system: &system,
        prompt: &prompt,
        context: "originalRequirements",
        schema,
        output_tokens: request.output_tokens,
    };
    let call = ContractCall::of::<TeachingRepair>(&PromptInput {
        system: system.clone(),
        prompt: Value::Object(prompt.clone()).to_string(),
    })?;
    let result =
        writer_excerpts::complete(llm, &call, &wr, excerpts.as_ref(), progress, None, |raw| {
            form.accept(request, raw).map(|()| raw.to_owned())
        })
        .await;
    match &result {
        Ok(raw) => crate::lesson_drafts::record_checkpoint(&key, json!({"answer": raw})).await?,
        Err(AppError::ModelAnswerUnusable(reason)) => {
            crate::lesson_drafts::record_checkpoint(&key, json!({"refused": reason})).await?;
        }
        Err(_) => {}
    }
    result.and_then(|raw| form.repaired(raw))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    /// One finding for each site: findings at one site are one finding.
    fn at(site: FindingSite, note: &str) -> ReviewFinding {
        let number = match site {
            FindingSite::Lesson => 0,
            FindingSite::Section(index) => 2 * index + 1,
            FindingSite::Question(index) => 2 * index + 2,
        };
        ReviewFinding::reviewer(u32::try_from(number).unwrap(), Some(site), note)
    }

    fn request<'a>(issues: Vec<Issue<'a>>, schema: &'a Value, wording: &'a Wording) -> Request<'a> {
        Request {
            system: "",
            schema,
            authoring: schema,
            candidate: schema,
            quote_checks: &[],
            issues,
            edits: true,
            wording,
            output_tokens: 0,
        }
    }

    fn plain(finding: &ReviewFinding) -> Issue<'_> {
        Issue {
            finding,
            last_attempt: None,
        }
    }

    #[test]
    fn a_change_may_touch_its_findings_part_and_its_second_part_only() {
        let finding = at(FindingSite::Section(0), "n").with_also(FindingSite::Section(2));
        let may = reach([&finding]);
        assert_eq!(may(Some("reviewer/1"), "/blocks/0"), Ok(()));
        assert_eq!(may(Some("reviewer/1"), "/blocks/2"), Ok(()));
        let refused = may(Some("reviewer/1"), "/blocks/1").unwrap_err();
        assert!(
            refused.contains("/blocks/1") && refused.contains("(/blocks/0 and /blocks/2)"),
            "{refused}"
        );
        let outside = may(Some("reviewer/1"), "").unwrap_err();
        assert!(
            outside.contains("a field outside every block and question"),
            "{outside}"
        );
    }

    #[test]
    fn a_finding_with_one_part_names_only_that_part_when_it_refuses() {
        let finding = at(FindingSite::Section(1), "n");
        let may = reach([&finding]);
        let refused = may(Some("reviewer/3"), "/blocks/0").unwrap_err();
        assert!(refused.contains("(/blocks/1)"), "{refused}");
    }

    #[test]
    fn a_finding_about_the_whole_material_may_change_any_part() {
        let finding = ReviewFinding::reviewer(0, None, "n");
        let may = reach([&finding]);
        for part in ["/blocks/0", "/questions/1", ""] {
            assert_eq!(may(Some("reviewer/0"), part), Ok(()), "{part}");
        }
    }

    #[test]
    fn a_change_naming_no_finding_or_one_not_sent_is_refused() {
        let finding = at(FindingSite::Section(0), "n");
        let may = reach([&finding]);
        assert_eq!(may(None, "/blocks/0"), Err("names no finding.".to_owned()));
        let unknown = may(Some("reviewer/11"), "/blocks/0").unwrap_err();
        assert!(
            unknown.contains("reviewer/11") && unknown.contains("not a finding"),
            "{unknown}"
        );
    }

    #[test]
    fn findings_that_share_an_id_each_open_their_own_part() {
        let first = at(FindingSite::Section(0), "one");
        let second = ReviewFinding::reviewer(0, None, "two");
        let may = reach([&first, &second]);
        assert_eq!(may(Some("reviewer/1"), "/blocks/0"), Ok(()));
        assert!(may(Some("reviewer/1"), "/blocks/1").is_err());
        let sibling = at(FindingSite::Section(0), "three").with_also(FindingSite::Section(1));
        let may = reach([&first, &sibling]);
        assert_eq!(may(Some("reviewer/1"), "/blocks/1"), Ok(()));
    }

    #[test]
    fn issues_to_fix_lists_one_entry_for_each_finding_sent() {
        let named = at(FindingSite::Section(0), "note one").with_passage("Passage one.");
        let mut sentenced = at(FindingSite::Question(1), "note two");
        sentenced.sentence = Some("Whole sentence.".to_owned());
        let both = at(FindingSite::Section(1), "note three").with_also(FindingSite::Section(3));
        let whole = ReviewFinding::reviewer(0, None, "note four");
        let schema = Value::Null;
        let wording = Wording::default();
        let sent = request(
            vec![
                Issue {
                    finding: &named,
                    last_attempt: Some("it named no finding".to_owned()),
                },
                plain(&sentenced),
                plain(&both),
                plain(&whole),
            ],
            &schema,
            &wording,
        );
        assert_eq!(
            sent.issues_to_fix(),
            json!([
                {"id":"reviewer/1","part":"/blocks/0","also":null,"passage":"Passage one.","note":"note one","lastAttempt":"it named no finding"},
                {"id":"reviewer/4","part":"/questions/1","also":null,"passage":"Whole sentence.","note":"note two","lastAttempt":null},
                {"id":"reviewer/3","part":"/blocks/1","also":"/blocks/3","passage":null,"note":"note three","lastAttempt":null},
                {"id":"reviewer/0","part":null,"also":null,"passage":null,"note":"note four","lastAttempt":null}
            ])
        );
    }

    #[test]
    fn a_shared_id_is_offered_to_the_repair_once() {
        let first = at(FindingSite::Section(0), "one");
        let again = at(FindingSite::Section(0), "two");
        let other = at(FindingSite::Section(2), "three");
        let schema = Value::Null;
        let wording = Wording::default();
        let sent = request(
            vec![plain(&first), plain(&again), plain(&other)],
            &schema,
            &wording,
        );
        assert_eq!(sent.finding_ids(), ["reviewer/1", "reviewer/5"]);
    }

    #[test]
    fn an_answer_of_plans_and_no_change_is_accepted_and_changes_nothing() {
        let finding = at(FindingSite::Section(0), "one");
        let schema = json!({"blocks":[{"body":"One."}],"questions":[]});
        let wording = Wording::default();
        let sent = request(vec![plain(&finding)], &schema, &wording);
        let raw = json!({"answers":[{"finding":"reviewer/1","plan":"It is right.","edits":[]}]})
            .to_string();
        assert!(Edits.accept(&sent, &raw).is_ok());
        let declined = Edits.repaired(raw).unwrap();
        assert!(matches!(declined, Repaired::Declined { .. }));
        if let Repaired::Declined { plans } = &declined {
            assert_eq!(
                plans.get("reviewer/1").map(String::as_str),
                Some("It is right.")
            );
        }
        let sent_findings = [finding];
        assert_eq!(
            apply(&schema, declined, &sent_findings, &wording).unwrap(),
            schema.to_string()
        );
    }

    #[test]
    fn the_edits_form_asks_for_one_answer_for_each_distinct_id_sent() {
        let first = at(FindingSite::Section(0), "one");
        let again = at(FindingSite::Section(0), "two");
        let other = at(FindingSite::Section(2), "three");
        let schema = json!({"blocks":[{"body":"One."}],"questions":[]});
        let wording = Wording::default();
        let sent = request(
            vec![plain(&first), plain(&again), plain(&other)],
            &schema,
            &wording,
        );
        let asked = Edits.schema(&sent);
        let answers = &asked["properties"]["answers"];
        assert_eq!(answers["minItems"], 2);
        assert_eq!(answers["maxItems"], 2);
        assert_eq!(
            answers["items"]["properties"]["finding"]["enum"],
            json!(["reviewer/1", "reviewer/5"])
        );
    }
}

#[cfg(test)]
#[path = "repair_reach_tests.rs"]
mod reach_tests;

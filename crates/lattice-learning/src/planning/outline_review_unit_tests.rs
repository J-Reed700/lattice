mod placement_tests {
    use super::super::*;

    /// Module 2 of the accept4 run, where a finding about "index seeks" was
    /// filed against the B-tree outcome.
    fn candidate() -> Value {
        json!({"modules":[{},{},{
            "title":"PostgreSQL performance",
            "summary":{"text":"Tune Postgres queries for transactional and analytical workloads."},
            "outcomes":[
                {"text":"Write complex queries using CTEs, window functions, and recursive patterns."},
                {"text":"Analyze query execution plans to identify bottlenecks (full table scans vs. index seeks)."},
                {"text":"Design B-tree and partial indexes with covering columns for efficient Postgres queries."}
            ],
            "lessons":[{"title":"Reading plans","objective":"Read EXPLAIN output for a slow query."}]
        }]})
    }

    const REASON: &str = "The text at /modules/2/outcomes/2 still uses the term 'index seeks'. This is Microsoft SQL Server terminology. PostgreSQL execution plans use terms like 'Index Scan' or 'Bitmap Index Scan', not 'Index Seek'.";

    fn issue(path: &str, message: &str) -> LearningOutlineIssueDto {
        LearningOutlineIssueDto {
            id: "finding-2".into(),
            path: path.into(),
            kind: LearningOutlineIssueKind::Plan,
            claim: String::new(),
            message: message.into(),
            attempts: 1,
            exhausted: false,
            quote: String::new(),
            source_id: None,
            searched: vec![],
        }
    }

    #[test]
    fn a_finding_moves_to_the_one_text_that_holds_the_phrase_it_quotes() {
        let mut finding = issue("/modules/2/outcomes/2", REASON);
        place(&candidate(), &mut finding);
        assert_eq!(finding.path, "/modules/2/outcomes/1");
        assert!(finding.claim.contains("index seeks"));
    }

    #[test]
    fn every_text_that_holds_the_phrase_is_named_and_none_leaves_it_in_place() {
        let mut several = candidate();
        several["modules"][2]["lessons"][0]["objective"] =
            json!("Compare index seeks with sequential scans in EXPLAIN output.");
        let mut finding = issue("/modules/2/outcomes/2", REASON);
        place(&several, &mut finding);
        assert_eq!(finding.path, "/modules/2/outcomes/2");
        assert!(finding
            .message
            .ends_with("appears at /modules/2/outcomes/1, /modules/2/lessons/0; correct it wherever it appears."));

        let mut nowhere = issue(
            "/modules/2/outcomes/2",
            "The term 'clustered index' is SQL Server terminology.",
        );
        place(&candidate(), &mut nowhere);
        assert_eq!(nowhere.path, "/modules/2/outcomes/2");
        assert_eq!(
            nowhere.message,
            "The term 'clustered index' is SQL Server terminology."
        );
    }

    #[test]
    fn apostrophes_are_not_quotes() {
        assert_eq!(
            quoted_phrases("The module's outcome uses `index seeks` and Postgres' plans."),
            vec!["index seeks".to_owned()]
        );
    }
}

mod apply_tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::super::super::outline_reference_tests::candidate;
    use super::super::*;
    use crate::lessons::course_generation_tests::request;
    use lattice_core::application::services::model_contract::shape::{
        Ended, Found, Listed, New, Why, Written,
    };

    fn open_issue(id: &str, path: &str) -> LearningOutlineIssueDto {
        LearningOutlineIssueDto {
            id: id.into(),
            path: path.into(),
            kind: LearningOutlineIssueKind::Plan,
            claim: String::new(),
            message: "The earlier reason.".into(),
            attempts: 1,
            exhausted: false,
            quote: String::new(),
            source_id: None,
            searched: vec![],
        }
    }

    fn draft_with(paths: &[&str]) -> OutlineDraft {
        let mut draft = OutlineDraft::new(request(LearningCourseDepth::Course), candidate(), &[]);
        draft.review.issues = paths
            .iter()
            .enumerate()
            .map(|(at, path)| open_issue(&format!("finding-{}", at + 1), path))
            .collect();
        draft.findings_raised = paths.len() as u32;
        draft
    }

    fn places_of(draft: &OutlineDraft) -> Vec<Place> {
        crate::outline_review_scope::module_paths(&draft.candidate, 0)
            .into_iter()
            .chain(crate::outline_review_scope::module_paths(
                &draft.candidate,
                1,
            ))
            .map(|path| Place { path: Told(path) })
            .collect()
    }

    fn place_at(draft: &OutlineDraft, path: &str) -> String {
        let at = places_of(draft)
            .iter()
            .position(|place| place.path.0 == path)
            .unwrap();
        format!("p{}", at + 1)
    }

    fn verdict(at: &str, status: &str, reason: &str) -> Value {
        json!({"entry":{"kind":"verdict","at":at,"status":status,"reason":reason}})
    }

    fn found(place: &str, category: &str, reason: &str) -> Value {
        json!({"entry":{"kind":"finding","place":place,"category":category,"reason":reason}})
    }

    fn entries(draft: &OutlineDraft, items: Vec<Value>) -> Vec<Entry> {
        let open = open_of(draft);
        let places = places_of(draft);
        let mut scope = Scope::for_question();
        let mut showing = scope.request();
        showing.show(&open).unwrap();
        showing.show(&places).unwrap();
        let text = json!({ "items": items }).to_string();
        let read = <New<Entry> as Listed>::read(
            &Written {
                text: &text,
                ended: Ended::Complete,
            },
            &showing,
        );
        assert_eq!(read.rest, None);
        read.items
    }

    #[test]
    fn a_resolved_verdict_closes_a_finding_and_an_unresolved_one_refreshes_it() {
        let mut draft = draft_with(&["/modules/0/lessons/0", "/modules/0/lessons/1"]);
        let answer = entries(
            &draft,
            vec![
                verdict("f1", "resolved", "The objective now names the tide."),
                verdict(
                    "f2",
                    "unresolved",
                    "The objective still names no measurement.",
                ),
            ],
        );
        apply(&mut draft, answer, true, &[]);
        assert_eq!(draft.review.issues.len(), 1);
        let kept = &draft.review.issues[0];
        assert_eq!(kept.id, "finding-2");
        assert_eq!(kept.message, "The objective still names no measurement.");
        assert_eq!(
            Some(kept.claim.clone()),
            item_text(&draft.candidate, "/modules/0/lessons/1")
        );
    }

    #[test]
    fn an_open_finding_without_a_verdict_stays_open_unchanged() {
        let mut draft = draft_with(&["/modules/0/lessons/0"]);
        let answer = entries(&draft, vec![]);
        apply(&mut draft, answer, true, &[]);
        assert_eq!(draft.review.issues.len(), 1);
        assert_eq!(draft.review.issues[0].message, "The earlier reason.");
    }

    #[test]
    fn the_first_verdict_for_a_finding_counts() {
        let mut draft = draft_with(&["/modules/0/lessons/0"]);
        let answer = entries(
            &draft,
            vec![
                verdict("f1", "unresolved", "The first judgement."),
                verdict("f1", "resolved", "The second judgement."),
            ],
        );
        apply(&mut draft, answer, true, &[]);
        assert_eq!(draft.review.issues.len(), 1);
        assert_eq!(draft.review.issues[0].message, "The first judgement.");
    }

    #[test]
    fn a_new_finding_is_filed_with_the_text_of_its_place_without_the_model_writing_it() {
        let mut draft = draft_with(&[]);
        let lesson = place_at(&draft, "/modules/0/lessons/0");
        let module = place_at(&draft, "/modules/1");
        let answer = entries(
            &draft,
            vec![
                found(&lesson, "plan", "The objective names no measurement."),
                found(&module, "plan", "The project repeats the last one."),
            ],
        );
        apply(&mut draft, answer, true, &[]);
        assert_eq!(draft.review.issues.len(), 2);
        let first = &draft.review.issues[0];
        assert_eq!(first.id, "finding-1");
        assert_eq!(first.path, "/modules/0/lessons/0");
        assert_eq!(
            Some(first.claim.clone()),
            item_text(&draft.candidate, "/modules/0/lessons/0")
        );
        let second = &draft.review.issues[1];
        assert_eq!(second.path, "/modules/1");
        assert!(second
            .claim
            .contains(draft.candidate["modules"][1]["title"].as_str().unwrap()));
    }

    #[test]
    fn a_factual_finding_on_a_plan_item_is_recorded_as_a_plan_finding() {
        let mut draft = draft_with(&[]);
        let lesson = place_at(&draft, "/modules/0/lessons/0");
        let answer = entries(
            &draft,
            vec![found(
                &lesson,
                "factual",
                "No single design suits every question.",
            )],
        );
        apply(&mut draft, answer, true, &[]);
        assert_eq!(draft.review.issues.len(), 1);
        assert_eq!(draft.review.issues[0].kind, LearningOutlineIssueKind::Plan);
    }

    #[test]
    fn an_answer_not_read_to_its_end_closes_nothing_but_keeps_what_it_found() {
        let mut draft = draft_with(&["/modules/0/lessons/0"]);
        let summary = place_at(&draft, "/modules/0/summary");
        let cut = Found {
            items: entries(
                &draft,
                vec![
                    verdict("f1", "resolved", "The objective now names the tide."),
                    found(&summary, "plan", "The summary repeats the lesson."),
                ],
            ),
            rest: Some(Why::CutOff),
            left_out: 0,
            unread_why: None,
        };
        apply(&mut draft, cut.items, cut.rest.is_none(), &[]);
        let paths: Vec<&str> = draft
            .review
            .issues
            .iter()
            .map(|issue| issue.path.as_str())
            .collect();
        assert_eq!(paths, vec!["/modules/0/lessons/0", "/modules/0/summary"]);
    }
}

mod module_claim_tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::super::*;
    use crate::lessons::course_generation_tests::request;

    fn candidate() -> Value {
        json!({"modules":[{
            "title":"Connector foundations",
            "summary":{"text":"Why connectors share one contract."},
            "outcomes":[{"text":"Explain the connector contract."}],
            "project":{
                "title":"Connector contract draft",
                "brief":"Write the first version of the shared connector interface.",
                "deliverables":["An interface definition.","A list of failure modes."],
                "successCriteria":["Every connector operation has a typed result."]
            },
            "lessons":[{"title":"Contracts","objective":"Describe a connector contract."}]
        }]})
    }

    #[test]
    fn a_module_claim_reads_as_text_not_json() {
        let claim = claim_of(&candidate(), "/modules/0").unwrap();
        for part in [
            "Connector foundations",
            "Connector contract draft",
            "Write the first version of the shared connector interface.",
            "An interface definition.",
            "A list of failure modes.",
            "Every connector operation has a typed result.",
        ] {
            assert!(claim.contains(part), "{part} missing from {claim}");
        }
        for json_mark in ["{", "}", "\"title\"", "successCriteria"] {
            assert!(!claim.contains(json_mark), "{json_mark} in {claim}");
        }
    }

    #[test]
    fn a_supported_module_claim_stays_verified_until_the_module_changes() {
        let mut draft = OutlineDraft::new(request(LearningCourseDepth::Course), candidate(), &[]);
        let claim = claim_of(&draft.candidate, "/modules/0").unwrap();
        draft.claim_receipts.insert(
            "module".into(),
            OutlineClaimReceipt {
                verdict: ClaimVerdict::Supported,
                reason: "The page says so.".into(),
                supporting_quote: None,
                checked_at: 1,
                claim: claim.clone(),
                path: "/modules/0".into(),
                source_id: None,
                shown: None,
            },
        );
        assert_eq!(verified_assertions(&draft, &[]), vec![claim]);
        draft.candidate["modules"][0]["project"]["brief"] = json!("Write a different interface.");
        assert!(verified_assertions(&draft, &[]).is_empty());
    }
}

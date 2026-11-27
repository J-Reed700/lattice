use super::*;

fn source(title: &str, url: Option<&str>) -> LearningSourceDto {
    LearningSourceDto {
        id: "src-1".into(),
        title: title.into(),
        url: url.map(str::to_owned),
        excerpt: "Standard spec text.".into(),
        acquired_at: 0,
    }
}

#[test]
fn extracts_ungrounded_employer_without_flagging_documented_tech_or_roles() {
    let goal = "Prepare for a healthcare integrations engineering role at Documo building SMART Backend Services connectors.";
    let public_sources = vec![
        source(
            "SMART App Launch: Backend Services",
            Some("https://hl7.org/fhir/smart-app-launch/backend-services.html"),
        ),
        source("RFC 7523", Some("https://www.rfc-editor.org/rfc/rfc7523")),
    ];
    assert_eq!(
        ungrounded_companies(
            &[goal, "Prior experience in TypeScript and PostgreSQL"],
            &public_sources
        ),
        vec!["Documo"]
    );

    let stripe_goal = "Prepare for an interview with Stripe building Stripe webhook receivers";
    let stripe_sources = vec![source(
        "Stripe API Webhooks Guide",
        Some("https://docs.stripe.com/webhooks"),
    )];
    assert!(ungrounded_companies(&[stripe_goal], &stripe_sources).is_empty());

    let generic_goal = "Prepare for a Senior Backend Engineer role building REST APIs with Python";
    assert!(ungrounded_companies(&[generic_goal], &public_sources).is_empty());
}

#[test]
fn catches_proprietary_assertions_and_allows_neutral_or_goal_framing() {
    let companies = vec!["Documo".to_owned()];

    assert!(!company_framing_faults(
        "Documo\u{2019}s delivery service runs in the background on Kubernetes.",
        &companies
    )
    .is_empty());
    assert!(!company_framing_faults(
        "Connect the worker to Documo's PostgreSQL database before requesting a token.",
        &companies
    )
    .is_empty());
    assert!(!company_framing_faults(
        "Which capability would system/Binary.w permit Documo to do?",
        &companies
    )
    .is_empty());
    assert!(!company_framing_faults(
        "Documo is a confidential client connected to a clinic EHR.",
        &companies
    )
    .is_empty());

    assert!(company_framing_faults(
        "To prepare for a role at Documo, consider a proposed delivery service running in the background.",
        &companies
    )
    .is_empty());
    assert!(company_framing_faults(
        "Suppose a candidate delivery service uses a backend PostgreSQL store to track tenant configurations.",
        &companies
    )
    .is_empty());
    assert!(company_framing_faults(
        "```typescript\nconst clientId = \"documo-connector-prod\";\n```",
        &companies
    )
    .is_empty());
}

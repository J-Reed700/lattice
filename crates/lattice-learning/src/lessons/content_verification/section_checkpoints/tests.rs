use super::*;

#[test]
fn a_saved_section_with_a_long_quote_is_reused() {
    let long = format!("Alpha {}omega.", "beta gamma delta ".repeat(100));
    let content = json!({"body": long});
    let checkpoint = SectionCheckpoint {
        inventory: UnitClaims {
            index: 0,
            claims: vec![Claim {
                quote: long.clone(),
                statement: "A scoped factual statement.".into(),
            }],
            non_factual_reason: String::new(),
        },
        coverage: CoverageUnit {
            index: 0,
            complete: true,
            reason: "All assertions are represented.".into(),
            missing_assertions: vec![],
        },
    };
    assert!(checkpoint.valid(&content, 0));
}

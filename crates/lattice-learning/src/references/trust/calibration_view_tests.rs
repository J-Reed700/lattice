use super::*;

fn derivation(name: &str) -> String {
    parameters(&POLICY)
        .into_iter()
        .find(|parameter| parameter.name == name)
        .map(|parameter| parameter.derivation)
        .unwrap_or_default()
}

#[test]
fn the_accept_reliability_explanation_says_statement() {
    assert_eq!(
        derivation("accept_reliability"),
        "A level is accepted as a statement's only support when its statements hold this often. A wrong published statement costs a misconception to unlearn; a right statement held for review costs one confirmation. At 0.9 an error is weighed as nine confirmations."
    );
}

#[test]
fn the_prior_strength_explanation_says_statements() {
    assert!(derivation("prior_strength")
        .ends_with("takes 21 straight held statements, tightening it two straight failures."));
}

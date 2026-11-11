use super::*;
use crate::practice::coding::harness::fake::{
    sample_problem, ScriptedRunner, ALWAYS_ZERO, BROKEN_SNIPPET, FIRST_ONLY, REFERENCE, SUBTRACTS,
};
use crate::practice::coding::harness::PythonHarness;

#[test]
fn code_blocks_reads_fenced_blocks_in_order() {
    let markdown = "Intro\n\n```python\nx = 1\nprint(x)\n```\n\ntext\n\n~~~\nplain\n~~~\n\n   ```js\nindented\n   ```\n";
    assert_eq!(
        code_blocks(markdown),
        ["x = 1\nprint(x)", "plain", "indented"]
    );
}

#[test]
fn code_blocks_closes_only_on_a_fence_as_long_as_the_opening_one() {
    let markdown = "````\n```\ninner\n```\n````\nafter\n";
    assert_eq!(code_blocks(markdown), ["```\ninner\n```"]);
}

#[test]
fn code_blocks_keeps_a_block_left_open_and_ignores_text_without_fences() {
    assert_eq!(code_blocks("```python\nx = 1\n"), ["x = 1"]);
    assert!(code_blocks("no code here\n`inline` code only").is_empty());
    assert!(code_blocks("``` has `ticks` in its info\nnot a fence\n").is_empty());
}

#[tokio::test]
async fn a_solution_passes_when_every_case_passes_and_not_when_one_fails() {
    let problem = sample_problem();
    let runner = ScriptedRunner::new();
    let harness = PythonHarness::new();
    for (code, passes) in [
        (REFERENCE, true),
        (SUBTRACTS, false),
        (FIRST_ONLY, false),
        (ALWAYS_ZERO, false),
    ] {
        let answer = passes_every_case(&runner, &harness, &problem, code, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(answer, passes, "{code}");
    }
}

#[tokio::test]
async fn code_that_parses_is_told_from_code_that_does_not() {
    let runner = ScriptedRunner::new();
    assert!(
        parses_as_python(&runner, "x = 1\n", CancellationToken::new())
            .await
            .unwrap()
    );
    assert!(
        !parses_as_python(&runner, BROKEN_SNIPPET, CancellationToken::new())
            .await
            .unwrap()
    );
}

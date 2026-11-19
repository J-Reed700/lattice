use super::*;

#[test]
fn lines_inside_a_longer_fence_have_no_keys() {
    assert!(keys("````md\n```\nRepeat this line of text.\n````").is_empty());
}

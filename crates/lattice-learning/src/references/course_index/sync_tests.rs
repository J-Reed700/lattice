use super::write_version;
use lattice_index::passage_index::PassageStore;

#[tokio::test]
async fn a_run_on_sentence_longer_than_a_passage_is_cut_between_words() {
    let store = PassageStore::memory().await.unwrap();
    let text = "alpha beta gamma delta ".repeat(80) + "End.";
    write_version(&store, "v", "h", &text).await.unwrap();
    let passages = store.passages_of("v").await.unwrap();
    let first = passages.first().unwrap();
    assert_eq!(text.as_bytes().get(first.end), Some(&b' '));
}

#[tokio::test]
async fn a_stored_version_records_the_rule_that_cut_it() {
    let store = PassageStore::memory().await.unwrap();
    write_version(&store, "v", "h", "One sentence here.")
        .await
        .unwrap();
    let documents = store.documents("").await.unwrap();
    assert!(!documents["v"].stamp.is_empty());
}

use super::*;
use crate::features::indexing::content_extraction_adapter::ContentExtractionAdapter;
use lattice_core::application::ports::ContentExtractionPort;

#[tokio::test]
#[ignore = "requires LATTICE_EMBED_GGUF, a runnable llama-server and LATTICE_STRUCTURE_PDF (read-only)"]
async fn real_pdf_context_and_vectors_keep_source_pages() -> anyhow::Result<()> {
    let model = crate::interfaces::di::modules::embedding::standalone::open_gguf(
        std::path::Path::new(&std::env::var("LATTICE_EMBED_GGUF")?),
    )
    .await?;
    let path = std::path::PathBuf::from(std::env::var("LATTICE_STRUCTURE_PDF")?);
    let extracted = ContentExtractionAdapter::new()
        .extract_content(&path)
        .await?;
    let metadata = lattice_core::application::factories::FileMetadataFactory::from_path(&path)?;
    let checksum = lattice_core::application::factories::ChecksumFactory::from_path(&path)?;
    let mut doc = Document::from_file(
        lattice_core::shared::types::ValidatedFilePath::new(path)?,
        metadata,
        checksum,
        extracted.text.clone(),
        lattice_core::domain::value_objects::ChunkingStrategy::default(),
    )?;
    doc.set_source_context(Some(
        lattice_core::domain::value_objects::source_context::SourceContext {
            group: lattice_core::domain::value_objects::source_context::SourceGroup {
                id: uuid::Uuid::new_v4().to_string(),
                title: "Manual of Patent Examining Procedure".into(),
                edition: Some("Rev. 01.2024".into()),
                description: None,
                ordered: true,
                structure: StructureMode::Sections,
            },
            position: 0,
        },
    ));
    let doc = prepare_structured(doc, &model, &extracted.page_ranges)?;
    assert!(!doc.chunks().is_empty());
    let mut covered = vec![false; extracted.text.len()];
    for chunk in doc.chunks() {
        let (start, end) = (chunk.start_char().unwrap(), chunk.end_char().unwrap());
        assert_eq!(&extracted.text[start..end], chunk.content());
        assert!(extracted
            .page_ranges
            .iter()
            .any(|(page, a, b)| Some(*page as u32) == chunk.page_number()
                && start >= *a
                && end <= *b));
        assert!(
            chunk.token_count() > 0
                && chunk.token_count() as usize
                    <= crate::features::embedding::input_policy::RETRIEVAL_CHUNK_TARGET_TOKENS
        );
        assert!(chunk.embedding_text().contains("Manual of Patent"));
        covered[start..end].fill(true);
    }
    assert!(extracted
        .text
        .char_indices()
        .all(|(offset, ch)| ch.is_whitespace() || covered[offset]));
    let inputs: Vec<_> = doc
        .chunks()
        .iter()
        .map(|chunk| chunk.embedding_text())
        .collect();
    let started = std::time::Instant::now();
    let vectors = model.embed_batch(&inputs).await?;
    assert_eq!(vectors.len(), inputs.len());
    assert!(vectors
        .iter()
        .all(|v| v.len() == model.dimension() && v.iter().all(|n| n.is_finite())));
    println!(
        "Structured PDF smoke: {} pages, {} contextual passages, {} vectors; embedding {:?}",
        extracted.page_ranges.len(),
        inputs.len(),
        vectors.len(),
        started.elapsed()
    );
    Ok(())
}

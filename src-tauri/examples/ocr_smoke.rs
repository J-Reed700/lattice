//! Live OCR check: the bundled `llama-server` reading scanned pages through
//! the same `ContentExtractor` → `VisionOcr` path the indexer uses.
//!
//! Run:
//! ```text
//! cargo run --example ocr_smoke -- LLAMA_SERVER MODEL_DIR OUT_DIR FILE...
//! ```
//!
//! `MODEL_DIR` holds `model.gguf` and `mmproj.gguf` (the catalog's local
//! names). Each FILE is a PDF (indexed with OCR, as an import would) or a
//! PNG/JPEG (one page). The text of each is written to `OUT_DIR/<name>.txt`,
//! and the first PDF's first page as rendered for the model to
//! `OUT_DIR/<name>.page1.png`. The server listens on port 8096 and is stopped
//! before exit.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use lattice::application::ports::OcrPort;
use lattice::features::indexing::engine::ContentExtractor;
use lattice::features::ocr::{render, FixedOcrModel, LlamaServerLauncher, OcrModel, VisionOcr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("lattice=info")
        .with_target(false)
        .init();

    let mut args = std::env::args().skip(1);
    let usage = "usage: ocr_smoke LLAMA_SERVER MODEL_DIR OUT_DIR FILE...";
    let binary = PathBuf::from(args.next().ok_or(usage)?);
    let model_dir = PathBuf::from(args.next().ok_or(usage)?);
    let out_dir = PathBuf::from(args.next().ok_or(usage)?);
    let files: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if files.is_empty() {
        return Err(usage.into());
    }

    let model = OcrModel::in_dir("paddleocr-vl-1.6", &model_dir, "OCR:");
    let ocr = Arc::new(VisionOcr::new(
        Arc::new(FixedOcrModel(Some(model))),
        Arc::new(LlamaServerLauncher::with_binary(binary).on_port(8096)),
        Arc::default(),
    )?);
    let extractor = ContentExtractor::new().with_ocr(Arc::clone(&ocr) as Arc<dyn OcrPort>);

    let mut dumped_render = false;
    for file in &files {
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file");
        let started = Instant::now();
        let is_pdf = render::image_mime(file).is_none();
        let text = if is_pdf {
            if !dumped_render {
                let image = render::page_image(file, 1)?;
                std::fs::write(out_dir.join(format!("{name}.page1.png")), &image.bytes)?;
                dumped_render = true;
            }
            let extracted = extractor.extract_from_file(file).await?;
            println!(
                "{name}: pages={:?} needs_ocr={:?} chars={}",
                extracted.page_ranges,
                extracted.needs_ocr,
                extracted.text.chars().count()
            );
            extracted.text
        } else {
            ocr.recognize_page(file, 1).await?
        };
        println!(
            "{name}: {} ms, {} chars",
            started.elapsed().as_millis(),
            text.chars().count()
        );
        write_text(&out_dir, name, &text)?;
    }

    ocr.stop().await;
    Ok(())
}

fn write_text(dir: &Path, name: &str, text: &str) -> std::io::Result<()> {
    std::fs::write(dir.join(format!("{name}.txt")), text)
}

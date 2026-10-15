//! # OCR
//!
//! Reads scanned pages with a small document vision-language model
//! (PaddleOCR-VL, about 0.9B parameters) run by a dedicated `llama-server`.
//!
//! - [`model`] finds the downloaded OCR model: its weights and its vision
//!   projector, two files from one catalog entry.
//! - [`render`] turns a PDF page (or a PNG/JPEG file) into the image the model
//!   reads.
//! - [`server`] owns the OCR server's lifecycle: started when a page needs it,
//!   stopped after a quiet minute and at app shutdown, one page at a time.
//! - [`launcher`] spawns the bundled `llama-server` with the model and
//!   projector and waits for `/health`.
//! - [`client`] is the one HTTP request a page costs.
//! - [`recognizer`] composes them into the [`OcrPort`] the indexer calls.
//!
//! Nothing here runs for a document with a text layer: the PDF extractor only
//! asks for pages it found textless (`ExtractedContent::needs_ocr`), so a
//! library without scans never starts the process.
//!
//! [`OcrPort`]: lattice_core::application::ports::OcrPort

pub mod client;
pub mod di;
pub mod launcher;
pub mod model;
pub mod recognizer;
pub mod render;
pub mod server;

pub use launcher::LlamaServerLauncher;
pub use model::{DownloadedOcrModel, FixedOcrModel, OcrModel, OcrModelSource};
pub use recognizer::VisionOcr;
pub use server::OcrServer;

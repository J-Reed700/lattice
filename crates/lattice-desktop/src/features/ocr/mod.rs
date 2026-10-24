//! Scanned page OCR via bundled document vision model and llama-server.

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

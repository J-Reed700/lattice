//! Speech slice — reads chat answers and lessons aloud, on-device.
//!
//! Kokoro-82M runs on Candle with a British English G2P (misaki's lexicons
//! and its small BART fallback), delivered through the catalog and download
//! pipeline like every other local model. The renderer cleans the text,
//! splits it into sentences and plays each clip while the next is made.

pub mod commands;
pub mod di;
pub mod dto;
pub mod engine;
pub mod plugin;

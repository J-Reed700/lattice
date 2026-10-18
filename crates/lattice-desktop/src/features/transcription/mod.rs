//! Transcription slice — voice into the corpus, on-device.
//!
//! An audio file dropped into the vault becomes a normal, searchable document
//! whose text is its transcript and whose chunks carry `section = "m:ss–m:ss"`,
//! so a citation points at a moment in a recording and the viewer plays from
//! there. Whisper runs on-device via candle, delivered through the existing
//! catalog + download pipeline.

pub mod di;
pub mod dto;
pub mod engine;
pub mod plugin;
pub mod use_cases;

pub mod commands;

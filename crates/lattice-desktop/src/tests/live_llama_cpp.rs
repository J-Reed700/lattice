//! How the live tests reach their llama.cpp server. The settings file names
//! the connection and the header; the header value comes from the environment.

use lattice_core::application::contracts::settings::LLMSettingsDto;
use lattice_core::application::ports::llm_port::RemoteHeader;
use lattice_core::shared::error::{AppError, Result};

/// `settings.llm` of the file `LATTICE_LLAMACPP_SETTINGS` names.
pub(crate) fn settings() -> Result<LLMSettingsDto> {
    let path = std::env::var("LATTICE_LLAMACPP_SETTINGS").map_err(|_| {
        AppError::InvalidConfig(
            "Set LATTICE_LLAMACPP_SETTINGS to the settings file the live tests read.".into(),
        )
    })?;
    let file: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    Ok(serde_json::from_value(file["settings"]["llm"].clone())?)
}

/// The header a live run sends to its llama.cpp server. The settings file
/// holds the header name; the value comes from the environment.
pub(crate) fn header(name: &str) -> Result<Option<RemoteHeader>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    match std::env::var("LATTICE_LLAMACPP_AUTH_HEADER_VALUE") {
        Ok(value) if !value.trim().is_empty() => Ok(Some(RemoteHeader {
            name: name.to_owned(),
            value: value.trim().to_owned(),
        })),
        _ => Err(AppError::InvalidConfig(format!(
            "The settings file names the header `{name}`; set LATTICE_LLAMACPP_AUTH_HEADER_VALUE to its value"
        ))),
    }
}

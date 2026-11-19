//! The request a lesson-preparation job is saved with.
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationRequest {
    pub lesson_ids: Vec<String>,
}

impl PreparationRequest {
    pub fn parse(requested: &str) -> Result<Self> {
        serde_json::from_str(requested).map_err(|_| {
            AppError::InvalidData("The lesson preparation request could not be read".into())
        })
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_reads_back_what_it_wrote() {
        let request = PreparationRequest {
            lesson_ids: vec!["a".into(), "b".into()],
        };
        let json = request.to_json().unwrap();
        assert_eq!(json, r#"{"lessonIds":["a","b"]}"#);
        assert_eq!(PreparationRequest::parse(&json).unwrap(), request);
    }

    #[test]
    fn an_unreadable_request_is_invalid_data() {
        assert!(matches!(
            PreparationRequest::parse("{}"),
            Err(AppError::InvalidData(_))
        ));
    }
}

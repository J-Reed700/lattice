//! Interview standards remain private even inside exported attempt snapshots.
use super::{invalid, json_string, parse};
use crate::features::learning::practice::interview_answers::INTERVIEW_ANSWERS_FILE_PATH;
use lattice_core::shared::error::Result;
use serde_json::Value;

fn private_file(value: &Value) -> bool {
    value.get("path").and_then(Value::as_str) == Some(INTERVIEW_ANSWERS_FILE_PATH)
}

pub(super) fn public_snapshot(raw: &str) -> Result<String> {
    let mut value: Value = parse(raw.as_bytes())?;
    let fields = value
        .as_object_mut()
        .ok_or_else(|| invalid("A practical activity snapshot must be an object."))?;
    let mut changed = fields.remove("interviewAnswers").is_some();
    if let Some(files) = fields.get_mut("files").and_then(Value::as_array_mut) {
        let count = files.len();
        files.retain(|file| !private_file(file));
        changed |= count != files.len();
    }
    if changed {
        json_string(&value)
    } else {
        Ok(raw.to_owned())
    }
}

pub(super) fn check_public_snapshot(raw: &str) -> Result<()> {
    let value: Value = parse(raw.as_bytes())?;
    if value
        .get("interviewAnswers")
        .is_some_and(|answers| !answers.is_null())
        || value
            .get("files")
            .and_then(Value::as_array)
            .is_some_and(|files| files.iter().any(private_file))
    {
        return Err(invalid(
            "A pack cannot import private interview answer guides inside an attempt snapshot.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn private_interview_standards_are_omitted_and_cannot_be_imported_in_snapshots() {
        let raw = json!({"title":"Frozen interview","interviewPlan":{"companyName":"Acme"},
            "interviewAnswers":{"questions":[{"referenceAnswer":"PRIVATE_STANDARD"}]},
            "files":[{"path":INTERVIEW_ANSWERS_FILE_PATH,"content":"PRIVATE_STANDARD"},{"path":"notes.md","content":"Learner notes"}]
        }).to_string();
        assert!(check_public_snapshot(&raw).is_err());
        let clean = public_snapshot(&raw).expect("sanitized snapshot");
        assert!(!clean.contains("PRIVATE_STANDARD"));
        assert!(!clean.contains(INTERVIEW_ANSWERS_FILE_PATH));
        assert!(
            clean.contains("Frozen interview")
                && clean.contains("Learner notes")
                && clean.contains("interviewPlan")
        );
        assert!(check_public_snapshot(&clean).is_ok());
        let unaffected = "{ \"title\": \"Existing simulation\", \"files\": [] }";
        assert_eq!(
            public_snapshot(unaffected).expect("public snapshot"),
            unaffected
        );
    }
}

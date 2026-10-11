//! Import preserves a historical receipt, not a new verification decision.
use super::{invalid, LessonVerificationReport};
use crate::shared::error::Result;
use std::collections::HashMap;

impl LessonVerificationReport {
    pub(in crate::features::learning) fn archived_metadata(
        &self,
        lesson_id: &str,
    ) -> Result<(&str, &str, i64)> {
        if self.lesson_id != lesson_id
            || self.policy.is_empty()
            || self.checker_model.is_empty()
            || self.content_sha256.len() != 64
            || !self
                .content_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid(
                "Saved verification metadata does not match its lesson.",
            ));
        }
        Ok((&self.policy, &self.content_sha256, self.checked_at))
    }

    pub(in crate::features::learning) fn remap_archived_ids(
        &mut self,
        ids: &HashMap<String, String>,
    ) {
        let remap = |id: &mut String| {
            if let Some(new) = ids.get(id) {
                *id = new.clone();
            }
        };
        remap(&mut self.lesson_id);
        for source in &mut self.sources {
            remap(&mut source.id);
        }
        for finding in &mut self.findings {
            for passage in &mut finding.evidence {
                // Execution identities are not source-library identifiers.
                if passage.retrieval_kind != "execution" {
                    remap(&mut passage.source_id);
                }
            }
        }
        // Keep the original fingerprint, policy, timestamps and judgments.
        // In particular, never call bind(): importing must not mint approval
        // for a differently identified lesson or a redacted source snapshot.
    }
}

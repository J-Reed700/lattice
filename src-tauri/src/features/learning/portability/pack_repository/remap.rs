//! Identifier rewriting for a pack imported as a copy: every imported ID is
//! given a fresh one, and every reference follows it.
use super::*;

pub(super) fn map_evidence_ids(
    snapshot: &LearningEvidenceSnapshot,
    map: &mut HashMap<String, String>,
) -> Result<()> {
    let mut namespaces = HashMap::<String, String>::new();
    for table in &snapshot.tables {
        for row in &table.rows {
            let object = row
                .as_object()
                .ok_or_else(|| invalid("Pack aggregate contains a malformed row."))?;
            for key in ["id", "operation_id"] {
                if let Some(old) = object.get(key).and_then(serde_json::Value::as_str) {
                    // An operation ID names one request, so the ledger row and
                    // the entity that request created share it.
                    let namespace = if key == "operation_id" {
                        "operation".to_owned()
                    } else {
                        format!("{}:{key}", table.table)
                    };
                    if let Some(previous) = namespaces.get(old) {
                        if previous != &namespace {
                            return Err(invalid(
                                "Pack aggregate reuses an identifier across entity types.",
                            ));
                        }
                        continue;
                    }
                    namespaces.insert(old.to_owned(), namespace);
                    // Cross-snapshot aliases are checked before this mapper
                    // runs; preserve their already assigned canonical mapping.
                    if map.contains_key(old) {
                        continue;
                    }
                    map_id(map, old, uuid::Uuid::new_v4().to_string());
                }
            }
        }
    }
    Ok(())
}

pub(super) fn map_id(map: &mut HashMap<String, String>, from: &str, to: String) {
    // Do not let a later entity silently change the destination of an earlier
    // mapping. Duplicate namespaces are rejected by the aggregate/core
    // validators; deliberate repeated references reuse the first mapping.
    map.entry(from.to_owned()).or_insert(to);
}
pub(super) fn remap(map: &HashMap<String, String>, value: &str) -> String {
    map.get(value).cloned().unwrap_or_else(|| value.to_owned())
}
pub(super) fn remap_refs(map: &HashMap<String, String>, values: &mut Vec<String>) {
    for value in values {
        *value = remap(map, value);
    }
}
pub(super) fn remap_json_field(
    value: &mut serde_json::Value,
    key: &str,
    map: &HashMap<String, String>,
) -> Result<()> {
    if let Some(old) = value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
    {
        value[key] = serde_json::Value::String(remap(map, &old));
    }
    Ok(())
}
pub(super) fn remap_json_string_field(
    value: &mut serde_json::Value,
    key: &str,
    map: &HashMap<String, String>,
) -> Result<()> {
    let Some(serialized) = value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
    else {
        return Ok(());
    };
    let mut nested: serde_json::Value = parse(serialized.as_bytes())?;
    remap_nested_value(&mut nested, map);
    value[key] = serde_json::Value::String(json_string(&nested)?);
    Ok(())
}
pub(super) fn remap_nested_value(value: &mut serde_json::Value, map: &HashMap<String, String>) {
    match value {
        serde_json::Value::String(_) => {}
        serde_json::Value::Array(values) => {
            values
                .iter_mut()
                .for_each(|child| remap_nested_value(child, map));
        }
        serde_json::Value::Object(values) => {
            for (key, child) in values.iter_mut() {
                if matches!(
                    key.as_str(),
                    "id" | "programId"
                        | "moduleId"
                        | "lessonId"
                        | "questionId"
                        | "sourceId"
                        | "versionId"
                        | "sourceVersionId"
                        | "outcomeId"
                        | "activityId"
                        | "runId"
                        | "sessionId"
                        | "revisesSessionId"
                        | "cardId"
                        | "operationId"
                        | "resultId"
                        | "snapshotId"
                        | "canvasId"
                        | "deckId"
                        | "reviewId"
                        | "blueprintId"
                        | "candidateId"
                        | "formId"
                        | "revisionId"
                        | "jobId"
                        | "diagnosticId"
                        | "practiceSessionId"
                        | "tutorTurnId"
                        | "proposalId"
                        | "evidenceEventId"
                        | "predecessorId"
                        | "replacementLessonId"
                        | "retryOfJobId"
                        | "acceptedCardId"
                        | "possibleDuplicateCardId"
                        | "sourceIds"
                        | "sourceVersionIds"
                        | "outcomeIds"
                        | "prerequisiteModuleIds"
                        | "evidenceEventIds"
                ) {
                    remap_scalar_or_id_array(child, map);
                } else {
                    remap_nested_value(child, map);
                }
            }
        }
        _ => {}
    }
}
fn remap_scalar_or_id_array(value: &mut serde_json::Value, map: &HashMap<String, String>) {
    match value {
        serde_json::Value::String(old) => {
            if let Some(new) = map.get(old) {
                *old = new.clone()
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                if let serde_json::Value::String(old) = child {
                    if let Some(new) = map.get(old) {
                        *old = new.clone();
                    }
                }
            }
        }
        _ => {}
    }
}
pub(super) fn remap_id_array(
    value: &mut serde_json::Value,
    map: &HashMap<String, String>,
) -> Result<()> {
    let values = value
        .as_array_mut()
        .ok_or_else(|| invalid("Pack ID list must be a JSON array."))?;
    for child in values {
        let old = child
            .as_str()
            .ok_or_else(|| invalid("Pack ID list contains a non-text value."))?;
        *child = serde_json::Value::String(remap(map, old));
    }
    Ok(())
}
pub(super) fn rewrite_program(program: &mut LearningProgramDto, map: &HashMap<String, String>) {
    program.summary.id = remap(map, &program.summary.id);
    program.summary.current_lesson_id = program
        .summary
        .current_lesson_id
        .as_deref()
        .map(|id| remap(map, id));
    for module in &mut program.modules {
        module.id = remap(map, &module.id);
        remap_refs(map, &mut module.prerequisite_module_ids);
        for lesson in &mut module.lessons {
            lesson.id = remap(map, &lesson.id);
            for block in &mut lesson.blocks {
                remap_refs(map, &mut block.source_ids);
            }
            for question in &mut lesson.questions {
                question.id = remap(map, &question.id);
                remap_refs(map, &mut question.source_ids);
            }
        }
    }
    for source in &mut program.sources {
        source.id = remap(map, &source.id);
    }
    for attempt in &mut program.attempts {
        attempt.id = remap(map, &attempt.id);
        attempt.module_id = remap(map, &attempt.module_id);
        attempt.lesson_id = attempt.lesson_id.as_deref().map(|id| remap(map, id));
        for result in &mut attempt.results {
            result.question_id = remap(map, &result.question_id);
            remap_refs(map, &mut result.source_ids);
        }
    }
}

//! Pure checks on a decoded pack, run before anything is written.
use super::evidence_tables::LearningEvidenceSnapshot;
use super::*;
use std::collections::HashSet;

pub(super) fn validate_aggregate_id_namespaces(
    snapshot: &LearningEvidenceSnapshot,
    program: &LearningProgramDto,
    outcome_rows: &[serde_json::Value],
) -> Result<()> {
    let mut core = HashMap::<String, &'static str>::new();
    let mut register = |id: &str, namespace: &'static str| -> Result<()> {
        if core.insert(id.to_owned(), namespace).is_some() {
            return Err(invalid("Pack reuses an ID across core entity types."));
        }
        Ok(())
    };
    register(&program.summary.id, "program")?;
    for module in &program.modules {
        register(&module.id, "module")?;
        for lesson in &module.lessons {
            register(&lesson.id, "lesson")?;
        }
    }
    for source in &program.sources {
        register(&source.id, "source_version")?;
    }
    for row in outcome_rows {
        register(value_str(row, "id")?, "outcome")?;
    }
    let mut seen = HashMap::<String, String>::new();
    for table in &snapshot.tables {
        for row in &table.rows {
            for (key, namespace) in [("id", table.table.as_str()), ("operation_id", "operation")] {
                let Some(id) = row.get(key).and_then(serde_json::Value::as_str) else {
                    continue;
                };
                if let Some(previous) = seen.insert(id.to_owned(), namespace.to_owned()) {
                    if previous != namespace {
                        return Err(invalid(
                            "Pack aggregate reuses an identifier across entity types.",
                        ));
                    }
                }
                if let Some(core_namespace) = core.get(id) {
                    let expected_alias = table.table == "learning_outcome_definitions"
                        && key == "id"
                        && *core_namespace == "outcome";
                    if !expected_alias {
                        return Err(invalid(
                            "Pack aggregate identifier collides with another imported entity type.",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
pub(super) fn validate_program(p: &LearningProgramDto) -> Result<()> {
    validate_uuid(&p.summary.id, "program")?;
    let mut ids = HashSet::new();
    ids.insert(p.summary.id.as_str());
    let mut sources = HashSet::new();
    for source in &p.sources {
        validate_source_id(&source.id)?;
        if !ids.insert(source.id.as_str()) {
            return Err(invalid("Pack has duplicate entity IDs."));
        }
        sources.insert(source.id.as_str());
    }
    let mut modules = HashSet::new();
    let mut lessons = HashSet::new();
    for m in &p.modules {
        validate_uuid(&m.id, "module")?;
        if !ids.insert(m.id.as_str()) {
            return Err(invalid("Pack has duplicate module IDs."));
        }
        if m.prerequisite_module_ids
            .iter()
            .any(|id| !modules.contains(id.as_str()))
            || m.prerequisite_module_ids
                .iter()
                .collect::<HashSet<_>>()
                .len()
                != m.prerequisite_module_ids.len()
        {
            return Err(invalid(
                "Pack prerequisites must reference distinct earlier modules.",
            ));
        }
        if let Some(project) = &m.project {
            crate::features::learning::teaching::validate_project(project)?;
        }
        modules.insert(m.id.as_str());
        for l in &m.lessons {
            validate_uuid(&l.id, "lesson")?;
            if !ids.insert(l.id.as_str()) {
                return Err(invalid("Pack has duplicate entity IDs."));
            }
            lessons.insert(l.id.as_str());
            for block in &l.blocks {
                crate::features::learning::teaching::validate_saved_rubric(&block.rubric)?;
                if block
                    .source_ids
                    .iter()
                    .any(|id| !sources.contains(id.as_str()))
                {
                    return Err(invalid(
                        "Pack lesson block refers to an unknown source version.",
                    ));
                }
            }
        }
    }
    if p.summary
        .current_lesson_id
        .as_deref()
        .is_some_and(|id| !lessons.contains(id))
    {
        return Err(invalid(
            "Pack current lesson is missing from its curriculum.",
        ));
    }
    if p.modules.is_empty()
        || p.modules.len() > 100
        || p.modules.iter().map(|m| m.lessons.len()).sum::<usize>() > 1000
    {
        return Err(invalid("Pack curriculum exceeds import limits."));
    }
    Ok(())
}
pub(super) fn validate_source_id(id: &str) -> Result<()> {
    if id.trim().is_empty() || id.len() > 300 || id.chars().any(char::is_control) {
        return Err(invalid("Pack source version ID is invalid."));
    }
    Ok(())
}

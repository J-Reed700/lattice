//! The input of a section repair, and the corrections taken from its answer.

use super::*;

/// `failure` without what the entry and the sentence labels say instead.
fn row_of(failure: &Value) -> Value {
    let mut row = failure.clone();
    if let Some(fields) = row.as_object_mut() {
        for key in ["unit", "claimId", "sentence", "evidence"] {
            fields.remove(key);
        }
    }
    row
}

/// The entry for the failed claims `ids` of section `unit`.
fn failed_in(
    ledger: &ClaimLedger,
    unit: usize,
    ids: Vec<String>,
    claims: Vec<Claim>,
    execution: Option<Told<Value>>,
) -> Failed {
    let named: Vec<ClaimId> = ids.iter().cloned().map(ClaimId).collect();
    let sent = ledger.with_cotenants(&named);
    let of_unit = |set: Vec<Protected>| -> Vec<Protected> {
        set.into_iter().filter(|held| held.unit == unit).collect()
    };
    Failed {
        id: ids.first().cloned().unwrap_or_default(),
        claims,
        targets: Internal::of(of_unit(Protected::of_claims(ledger, &sent))),
        protected: Internal::of(of_unit(Protected::of_ledger(ledger, &sent))),
        ids: Internal::of(ids),
        shown: Internal::of(None),
        execution,
    }
}

/// What a section's repair is asked, from the work that remains for it.
pub(super) fn section_input(
    llm: &dyn LLMPort,
    ledger: &ClaimLedger,
    (context, candidate): (&Value, &Value),
    (unit, part): (usize, &str),
    (wording, cites, output_tokens): (&Wording, bool, usize),
) -> Result<Input> {
    let (field, _) = section_of(candidate, unit)?;
    let failures = section_failures(context, unit);
    let mut ids: Vec<&str> = Vec::new();
    for id in failures.iter().flat_map(evidence_ids) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let table = context.get("evidence").and_then(Value::as_array);
    let held = ids
        .iter()
        .map(|id| -> Result<Passage> {
            let entry = table
                .into_iter()
                .flatten()
                .find(|passage| passage.get("id").and_then(Value::as_str) == Some(id))
                .ok_or_else(|| invalid("A section repair is missing its recorded evidence."))?;
            Ok(Passage {
                id: (*id).to_owned(),
                title: entry
                    .get("sourceTitle")
                    .and_then(Value::as_str)
                    .map(|title| Told(title.to_owned())),
                text: Told(
                    entry
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                ),
                source_index: Internal::of(entry.get("sourceIndex").and_then(Value::as_u64)),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let label = llm.count_tokens("[q0000] ");
    let passages = Packed::new(held, |passage| {
        let excerpt = passage.excerpt();
        let sentences = Quotes::of(excerpt.iter()).all().len();
        let shown = json!({"sourceTitle": passage.title, "text": passage.text.0});
        llm.count_tokens(&shown.to_string()) + 1 + sentences * label
    });
    let mut items: Vec<Failed> = Vec::new();
    let mut groups: Vec<(&str, Vec<&Value>)> = Vec::new();
    for failure in &failures {
        let sentence = failure
            .get("sentence")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match groups.iter_mut().find(|(known, _)| *known == sentence) {
            Some((_, group)) => group.push(failure),
            None => groups.push((sentence, vec![failure])),
        }
    }
    for (_, group) in groups {
        let named: Vec<String> = group
            .iter()
            .filter_map(|failure| failure.get("claimId").and_then(Value::as_str))
            .map(str::to_owned)
            .collect();
        let claims: Vec<Claim> = group
            .iter()
            .map(|failure| Claim {
                row: Told(row_of(failure)),
                evidence: Internal::of(
                    evidence_ids(failure)
                        .iter()
                        .filter_map(|id| ids.iter().position(|known| known == id))
                        .collect(),
                ),
            })
            .collect();
        items.push(failed_in(ledger, unit, named, claims, None));
    }
    let observed = observations(context)?;
    for example in section_examples(context, unit) {
        let Some(id) = example.get("claimId").and_then(Value::as_str) else {
            continue;
        };
        let execution = observed
            .iter()
            .find(|observation| {
                ledger
                    .example_claim(&observation.id)
                    .is_some_and(|claim| claim.0 == id)
            })
            .and_then(|observation| serde_json::to_value(observation).ok())
            .map(Told);
        items.push(failed_in(
            ledger,
            unit,
            vec![id.to_owned()],
            Vec::new(),
            execution,
        ));
    }
    let sections: Vec<Value> = candidate
        .get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|block| json!({"kind": block.get("kind"), "title": block.get("title")}))
        .collect();
    let history: Vec<Value> = context
        .get("repairHistory")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|finding| finding.get("unit").and_then(Value::as_u64) == Some(unit as u64))
        .cloned()
        .collect();
    Ok(Input {
        requirements: Told(context.get("requirements").cloned().unwrap_or(Value::Null)),
        sections: Told(sections),
        section: Told(only_section(candidate, part)),
        lesson: Internal::of(candidate.clone()),
        part: Internal::of(part.to_owned()),
        question: field == "questions",
        cites,
        items,
        passages,
        history: Told(history),
        wording: Internal::of(wording.clone()),
        output_tokens,
    })
}

/// The entries taken so far: one section's rows with each correction made.
pub(super) struct Taking<'a> {
    pub(super) input: &'a Input,
    pub(super) schema: &'a Value,
    pub(super) field: &'a str,
    pub(super) quotes: Quotes,
    pub(super) rows: Rows,
}

impl Taking<'_> {
    /// Takes the entry for `item`, or says why not.
    pub(super) fn settle(
        &mut self,
        item: &Failed,
        settled: Settled<Correction>,
    ) -> std::result::Result<(), String> {
        let correction = match settled {
            Settled::Answered(correction) => correction,
            Settled::Unanswered(Why::NoRoom) => return Err(NO_ROOM.to_owned()),
            Settled::Unanswered(why) => return Err(refusal(&ask::failed(&why))),
        };
        let part = self.input.part.value();
        let narrowed = narrowed(&self.rows, part, item, &correction.edits);
        if let Some(reason) = narrowed.put_back {
            return Err(reason);
        }
        let Some(remaining) = narrowed.remaining() else {
            return Err(NO_CHANGE.to_owned());
        };
        let mut trial = self.rows.clone();
        let held = Held {
            wording: self.input.wording.value(),
        };
        if let Err(left) = trial.take(remaining, &self.quotes, &held) {
            return Err(match left {
                Left::Met(words) => words,
                Left::Refused(reasons) => reasons.join(" "),
            });
        }
        let made = trial.value();
        let before = self.rows.value();
        if made == before {
            return Err(NO_CHANGE.to_owned());
        }
        if let Some(error) = self.unfit(&before, &made) {
            return Err(invalid(error).to_string());
        }
        self.rows = trial;
        Ok(())
    }

    /// Why the section of `made` may not stand in place of the one of `before`, when it may not.
    fn unfit(&self, before: &Value, made: &Value) -> Option<String> {
        let part = self.input.part.value();
        let section = made.pointer(part)?;
        let held = vec![section.clone()];
        let shaped = if self.field == "questions" {
            json!({"blocks": [], "questions": held})
        } else {
            json!({"blocks": held, "questions": []})
        };
        let validator =
            match jsonschema::JSONSchema::compile(&section_schema(self.schema, self.field)) {
                Ok(validator) => validator,
                Err(error) => return Some(format!("Invalid repair schema: {error}")),
            };
        if !validator.is_valid(&shaped) {
            return Some(
                "The edited section does not match the lesson schema. Its prior draft is saved."
                    .to_owned(),
            );
        }
        let kind = |lesson: &Value| lesson.pointer(part)?.get("kind").cloned();
        (kind(made) != kind(before))
            .then(|| "A section repair changed its teaching or assessment role.".to_owned())
    }
}

fn section_schema(schema: &Value, field: &str) -> Value {
    let array = |name: &str| json!({"type":"array","minItems":usize::from(name==field),"maxItems":usize::from(name==field),"items":schema.pointer(&format!("/properties/{name}/items")).cloned().unwrap_or_else(||json!({"type":"object"}))});
    json!({"type":"object","additionalProperties":false,"required":["blocks","questions"],"properties":{"blocks":array("blocks"),"questions":array("questions")}})
}

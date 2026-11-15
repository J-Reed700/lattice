//! The material as it stands after the entries taken.

use super::*;

impl Rows {
    /// The material as it stands after the entries taken.
    pub(crate) fn value(&self) -> Value {
        let mut out = self.base.clone();
        for field in self.fields.iter().filter(|field| field.units.changed()) {
            if let Some(slot) = out.pointer_mut(&field.pointer) {
                *slot = Value::String(field.units.text());
            }
        }
        for (part, to) in &self.keyed {
            if let Some(slot) = out.pointer_mut(&format!("{part}/correctIndex")) {
                *slot = json!(to);
            }
        }
        for (part_pointer, question) in part_pointers(&self.base) {
            let (own, listed) = self.citations_of(&out, &part_pointer, question);
            if let Some(Value::Object(map)) = out.pointer_mut(&part_pointer) {
                if let Some((index, quote)) = own {
                    map.insert("sourceIndex".to_owned(), index);
                    map.insert("quote".to_owned(), quote);
                }
                if let Some(list) = listed {
                    map.insert("citations".to_owned(), Value::Array(list));
                }
            }
        }
        out
    }

    /// A part's own pair and its listed citations as they now stand; `None` for what is as it was.
    fn citations_of(
        &self,
        out: &Value,
        part_pointer: &str,
        question: bool,
    ) -> (Option<(Value, Value)>, Option<Vec<Value>>) {
        let own_at = CitationRef {
            part: part_pointer.to_owned(),
            index: None,
        };
        let own =
            self.recited.iter().find(|(done, _)| *done == own_at).map(
                |(_, recited)| match recited {
                    Some((source, quote)) => (json!(source), json!(quote)),
                    None => (Value::Null, json!("")),
                },
            );
        let entries = self
            .base
            .pointer(&format!("{part_pointer}/citations"))
            .and_then(Value::as_array);
        let mut list: Vec<Value> = Vec::new();
        for (index, entry) in entries.into_iter().flatten().enumerate() {
            let at = CitationRef {
                part: part_pointer.to_owned(),
                index: Some(index),
            };
            let recited = self.recited.iter().find(|(done, _)| *done == at);
            if let Some((_, None)) = recited {
                continue;
            }
            let mut entry = entry.clone();
            if let (Some((_, Some((source, quote)))), Some(map)) = (recited, entry.as_object_mut())
            {
                map.insert("sourceIndex".to_owned(), json!(source));
                map.insert("quote".to_owned(), json!(quote));
            }
            let covers = self
                .citations
                .iter()
                .find(|held| held.at == at)
                .map(|held| held.covers.as_slice())
                .unwrap_or_default();
            let field = covers.first().and_then(|first| self.field_of(first));
            if let Some(field) = field.filter(|field| field.units.touched(covers)) {
                match field.units.stands(covers) {
                    Some(sentence) => {
                        if let Some(map) = entry.as_object_mut() {
                            map.insert("sentence".to_owned(), Value::String(sentence));
                        }
                    }
                    None => continue,
                }
            }
            list.push(entry);
        }
        for added in &self.added {
            let (part, sentence, source, quote) = match added {
                Added::Cite {
                    unit,
                    part,
                    source,
                    quote,
                } => (
                    part,
                    self.field_of(unit)
                        .and_then(|field| field.units.stands(&[*unit])),
                    *source,
                    quote,
                ),
                Added::Text {
                    part,
                    sentence,
                    source,
                    quote,
                } => (part, Some(sentence.clone()), *source, quote),
            };
            let (Some(sentence), true) = (sentence, part == part_pointer) else {
                continue;
            };
            let Some(target) = out.pointer(part_pointer) else {
                continue;
            };
            if let Some(entry) = citation_entry(target, question, &sentence, json!(source), quote) {
                if !list.contains(&entry) {
                    list.push(entry);
                }
            }
        }
        let listed = (entries.is_some() || !list.is_empty()).then_some(list);
        (own, listed)
    }
}

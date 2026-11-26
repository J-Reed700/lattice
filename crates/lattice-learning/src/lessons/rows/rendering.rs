//! Showing the rows to the model and rendering them back, with the units as shown.

use super::*;

impl ShownRows<'_> {
    /// The handle a unit was shown under.
    pub(crate) fn handle_of(&self, unit: UnitRef) -> Option<Handle<Unit>> {
        self.units
            .iter()
            .flat_map(|units| units.iter())
            .find(|(_, held)| held.reference() == unit)
            .map(|(handle, _)| handle.clone())
    }
}

impl Rows {
    /// Shows every unit, field by field in document order, then every citation.
    pub(crate) fn shown<'a>(&'a self, showing: &mut Showing<'_>) -> Result<ShownRows<'a>> {
        let mut units = Vec::with_capacity(self.fields.len());
        for field in &self.fields {
            units.push(field.units.shown(showing)?);
        }
        let citations = showing.show(&self.citations)?;
        Ok(ShownRows { units, citations })
    }

    /// The material as a request shows it.
    pub(crate) fn rendered(&self, shown: &ShownRows<'_>) -> Value {
        let mut out = self.base.clone();
        for (at, field) in self.fields.iter().enumerate() {
            let text = match shown.units.get(at) {
                Some(units) => field.units.marked(units),
                None => field.units.text(),
            };
            if let Some(slot) = out.pointer_mut(&field.pointer) {
                *slot = Value::String(text);
            }
        }
        for (part_pointer, _) in part_pointers(&self.base) {
            if self
                .only
                .as_deref()
                .is_some_and(|only| part_pointer != only)
            {
                continue;
            }
            if let Some(Value::Object(map)) = out.pointer_mut(&part_pointer) {
                *map = self.part_rendered(&part_pointer, std::mem::take(map), shown);
            }
        }
        out
    }

    /// A part with its pointer first and its citations as handles.
    fn part_rendered(
        &self,
        part_pointer: &str,
        map: Map<String, Value>,
        shown: &ShownRows<'_>,
    ) -> Map<String, Value> {
        let has_index = map.contains_key("sourceIndex");
        let mut rebuilt = Map::new();
        rebuilt.insert("part".to_owned(), Value::String(part_pointer.to_owned()));
        let mut placed = false;
        for (key, value) in map {
            match key.clone().as_str() {
                "sourceIndex" | "quote" => {
                    let here = key == "sourceIndex" || !has_index;
                    if here && !placed {
                        placed = true;
                        rebuilt.insert(
                            "citation".to_owned(),
                            self.own_rendered(part_pointer, shown),
                        );
                    }
                }
                "citations" => {
                    let value = match value {
                        Value::Array(entries) => Value::Array(
                            entries
                                .into_iter()
                                .enumerate()
                                .map(|(index, entry)| {
                                    self.listed_rendered(part_pointer, index, entry, shown)
                                })
                                .collect(),
                        ),
                        other => other,
                    };
                    rebuilt.insert(key, value);
                }
                _ => {
                    rebuilt.insert(key, value);
                }
            }
        }
        rebuilt
    }

    fn own_rendered(&self, part_pointer: &str, shown: &ShownRows<'_>) -> Value {
        let at = CitationRef {
            part: part_pointer.to_owned(),
            index: None,
        };
        match shown.citations.iter().find(|(_, held)| held.at == at) {
            Some((handle, held)) => json!({
                "at": handle.written(),
                "sourceIndex": held.source_index,
                "quote": held.quote,
            }),
            None => Value::Null,
        }
    }

    fn listed_rendered(
        &self,
        part_pointer: &str,
        index: usize,
        entry: Value,
        shown: &ShownRows<'_>,
    ) -> Value {
        let at = CitationRef {
            part: part_pointer.to_owned(),
            index: Some(index),
        };
        let Some((handle, held)) = shown.citations.iter().find(|(_, held)| held.at == at) else {
            return entry;
        };
        let sentences: Vec<String> = held
            .covers
            .iter()
            .filter_map(|unit| {
                shown
                    .units
                    .iter()
                    .flat_map(|units| units.iter())
                    .find(|(_, shown_unit)| shown_unit.reference() == *unit)
                    .map(|(handle, _)| handle.written())
            })
            .collect();
        json!({
            "at": handle.written(),
            "sentences": sentences,
            "sourceIndex": held.source_index,
            "quote": held.quote,
        })
    }
}

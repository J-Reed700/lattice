//! The notes as the writing round replays them.

use std::collections::BTreeMap;

use super::Passages;

/// The lines as kept, one per passage in label order, as `label note` or
/// `label -`, each passage under the label its message shows it by.
pub(super) fn as_kept(lines: &BTreeMap<u32, Option<String>>, passages: &Passages) -> String {
    lines
        .iter()
        .filter_map(|(number, note)| {
            let passage = passages.0.get(number)?;
            Some(format!(
                "{} {}",
                passage.label,
                note.as_deref().unwrap_or("-")
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

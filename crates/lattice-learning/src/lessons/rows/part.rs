//! The rows of one part of the material.

use super::*;

impl Rows {
    /// The rows of the one part at JSON pointer `part` of `material` (`/blocks/2`, `/questions/0`): only that
    /// part's texts and citations are fields, each under its real pointer in `material`. `value()` is the
    /// whole material with the changes made. A pointer that names nothing gives rows with no field.
    pub(crate) fn of_part(material: &Value, part: &str) -> Self {
        Self::build(material, Some(part))
    }
}

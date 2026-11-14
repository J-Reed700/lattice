//! An error shown with the causes beneath it.

use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// Shows an error's message followed by each cause beneath it, joined by
/// ": ". A cause whose message is already shown is left out.
pub struct WithCauses<'a>(pub &'a (dyn Error + 'static));

impl Display for WithCauses<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut shown = self.0.to_string();
        let mut cause = self.0.source();
        while let Some(error) = cause {
            let message = error.to_string();
            if !message.is_empty() && !shown.contains(&message) {
                shown.push_str(": ");
                shown.push_str(&message);
            }
            cause = error.source();
        }
        f.write_str(&shown)
    }
}

#[cfg(test)]
mod tests;

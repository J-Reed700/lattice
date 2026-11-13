//! An error shown with the causes beneath it.

use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// Shows an error's message followed by each cause beneath it, joined by
/// ": ". A cause whose message is already shown is left out.
pub struct WithCauses<'a>(pub &'a (dyn Error + 'static));

impl Display for WithCauses<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self.0, f)
    }
}

#[cfg(test)]
mod tests;

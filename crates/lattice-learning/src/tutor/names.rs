//! What a tutor chat shows its model, under the names it gives each thing. A
//! tool takes a name back and the app resolves it; no id, no list position and
//! no offset is ever written by the model. The names are kept with the chat,
//! so a name a turn showed still means the same thing in a later turn.

use super::course_turn::CourseTurn;
use lattice_core::application::services::model_contract::shown::{Drawn, Kind};
use lattice_core::application::services::model_contract::tools::{
    ArgumentError, Arguments, Parameters,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

/// A lesson of the course the outline lists.
pub(super) struct Lesson(pub String);

impl Kind for Lesson {
    const PREFIX: &'static str = "l";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = String;

    fn reference(&self) -> String {
        self.0.clone()
    }
}

/// A source version of the course.
pub(super) struct Source(pub String);

impl Kind for Source {
    const PREFIX: &'static str = "r";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = String;

    fn reference(&self) -> String {
        self.0.clone()
    }
}

/// A card of the course's deck.
pub(super) struct Card(pub String);

impl Kind for Card {
    const PREFIX: &'static str = "c";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = String;

    fn reference(&self) -> String {
        self.0.clone()
    }
}

/// One option of a multiple-choice card: the card and the option's place in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Chosen {
    pub card: String,
    pub index: usize,
}

/// An option of a multiple-choice card.
pub(super) struct Choice(pub Chosen);

impl Kind for Choice {
    const PREFIX: &'static str = "o";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = Chosen;

    fn reference(&self) -> Chosen {
        self.0.clone()
    }
}

impl CourseTurn {
    /// The bracketed name `thing` is shown under, given the first time it is shown.
    pub(super) fn name<K: Kind>(&self, thing: K) -> Result<String> {
        let things = [thing];
        self.names.with(|scope| {
            let shown = scope.request().show(&things)?;
            shown
                .handles()
                .first()
                .map(|handle| handle.bracketed())
                .ok_or_else(|| AppError::InternalError("A thing was shown under no name.".into()))
        })
    }

    /// The arguments of one call read against the chat's names; a name that
    /// resolves to nothing is tool-error text the model can act on.
    pub(super) fn read_arguments<T>(
        &self,
        parameters: &Parameters,
        arguments: &Value,
        read: impl FnOnce(&Arguments<'_>) -> std::result::Result<T, ArgumentError>,
    ) -> Result<T> {
        self.names
            .with(|scope| read(&parameters.read(arguments, scope)))
            .map_err(|error| AppError::InvalidInput(error.0))
    }

    /// Keeps the names given since they were last kept.
    pub(super) async fn keep_names(&self) {
        if let Err(error) = self.names.save().await {
            warn!(%error, "The tutor chat's names could not be kept");
        }
    }
}

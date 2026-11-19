//! What the debrief's model may point at: each a kind of thing, with the app's own identity of it.

use lattice_core::application::services::model_contract::shown::{Drawn, Kind, Told};
use lattice_core::domain::learning::practice::work::PartRef;
use serde::{Deserialize, Serialize};

/// One thing the session is scored on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Scored {
    pub id: String,
    pub title: Told<String>,
    pub earns: Told<String>,
}

impl Kind for Scored {
    const PREFIX: &'static str = "c";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.clone()
    }
}

/// One sentence of a learner turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Said {
    pub turn: String,
    pub index: usize,
    pub words: Told<String>,
}

impl Kind for Said {
    const PREFIX: &'static str = "q";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = (String, usize);

    fn reference(&self) -> (String, usize) {
        (self.turn.clone(), self.index)
    }
}

/// One part of the piece a check was made on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Line {
    pub check: String,
    pub part: PartRef,
}

impl Kind for Line {
    const PREFIX: &'static str = "l";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = (String, PartRef);

    fn reference(&self) -> (String, PartRef) {
        (self.check.clone(), self.part.clone())
    }
}

/// One check of the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Run {
    pub id: String,
}

impl Kind for Run {
    const PREFIX: &'static str = "r";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.clone()
    }
}

/// One case the learner wrote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Example {
    pub ordinal: u32,
}

impl Kind for Example {
    const PREFIX: &'static str = "e";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = u32;

    fn reference(&self) -> u32 {
        self.ordinal
    }
}

/// One hint the learner was given.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Hinted {
    pub ordinal: u32,
}

impl Kind for Hinted {
    const PREFIX: &'static str = "h";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = u32;

    fn reference(&self) -> u32 {
        self.ordinal
    }
}

/// One thing the debrief states beside the scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Stated {
    pub key: Told<String>,
}

impl Kind for Stated {
    const PREFIX: &'static str = "k";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.key.0.clone()
    }
}

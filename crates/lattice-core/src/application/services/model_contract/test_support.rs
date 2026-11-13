use super::Contract;
use crate::shared::error::Result;
use std::fmt::Debug;

mod scripted;

pub use scripted::{Reply, ScriptedModel, Sent, SCRIPTED_WINDOW};

/// A contract whose answer can be written as the model's reply text and read back by its production reader.
pub trait RenderAnswer: Contract {
    type Answer;

    fn render(answer: &Self::Answer) -> String;

    fn read(text: &str) -> Result<Self::Answer>;
}

pub fn assert_round_trip<C: RenderAnswer>(answer: &C::Answer)
where
    C::Answer: PartialEq + Debug,
{
    let read = C::read(&C::render(answer));
    assert_eq!(
        read.as_ref().ok(),
        Some(answer),
        "{} could not read its own rendering: {:?}",
        C::ID.as_str(),
        read.as_ref().err()
    );
}

//! The practice-session engine: it knows kinds of work only through the roles in `kind`.

pub mod counterpart;
pub mod debrief;
pub mod hints;
pub mod kind;
pub mod observation;
pub mod policy;
pub mod proof;
pub mod record;

#[cfg(test)]
pub(crate) mod fake_kind;

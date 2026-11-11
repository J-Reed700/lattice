//! The practice-session engine: it knows kinds of work only through the roles in `kind`.

pub mod kind;
pub mod observation;
pub mod proof;

#[cfg(test)]
pub(crate) mod fake_kind;

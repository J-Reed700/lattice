//! The practice-session engine: it knows kinds of work only through the roles in `kind`.

pub mod budget;
pub mod counterpart;
pub mod debrief;
pub mod hints;
pub mod kind;
pub mod live;
pub mod observation;
pub mod observe;
pub mod policy;
pub mod ports;
pub mod proof;
pub mod record;
pub mod service;

#[cfg(test)]
pub(crate) mod fake_kind;

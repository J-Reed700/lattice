//! Background task lifecycle, supervision, and runtime helpers.

pub mod autorelease;
pub mod background;
pub mod jobs;
pub mod observer;
pub mod supervised_task;
pub mod user_activity;

pub use autorelease::with_autorelease_pool;
pub use supervised_task::supervise;

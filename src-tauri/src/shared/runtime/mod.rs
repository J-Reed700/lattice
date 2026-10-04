//! Background task lifecycle, supervision, and runtime helpers.

pub mod autorelease;
pub mod background;
pub mod observer;
pub mod supervised_task;

pub use autorelease::with_autorelease_pool;
pub use supervised_task::supervise;

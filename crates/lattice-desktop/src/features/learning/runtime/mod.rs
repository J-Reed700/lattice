//! Runtime capabilities owned by Learning Studio.

pub mod embedded_runtime;
#[cfg(feature = "learning-labs")]
mod guest;
pub mod lab_runner;
pub mod lab_runtime;
pub mod python_runtime;
pub mod runtime_catalog;

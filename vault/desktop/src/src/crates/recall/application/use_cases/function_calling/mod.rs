//! Function calling use cases
//!
//! Use cases for LLM function calling capabilities.

pub mod execute_function;
pub mod list_available_functions;

pub use execute_function::*;
pub use list_available_functions::*;

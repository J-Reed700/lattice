//! Function-calling feature dependency injection.
//!
//! The registry and executor are built in `Container::new` (they need the
//! search, library and storage modules); the accessors live here.

use std::sync::Arc;

use crate::features::function_calling::{FunctionExecutorTrait, FunctionRegistryTrait};
use crate::interfaces::di::Container;

/// Function calling's registrar surface on `Container`.
impl Container {
    pub fn function_registry(&self) -> &Arc<dyn FunctionRegistryTrait> {
        &self.function_registry
    }

    pub fn function_executor(&self) -> &Arc<dyn FunctionExecutorTrait> {
        &self.function_executor
    }
}

//! Model-management feature dependency injection.

use std::sync::Arc;

use crate::features::model_management::cache_adapter::ModelCacheAdapter;
use crate::interfaces::di::Container;
use lattice_core::application::ports::{ModelCatalogPort, SystemInfoPort};
use lattice_core::shared::error::Result;

/// Model management's registrar surface on `Container`.
impl Container {
    /// Get the model catalog service.
    pub fn model_catalog(&self) -> Arc<dyn ModelCatalogPort> {
        Arc::clone(self.ai.model_catalog())
    }

    /// Get model catalog cache adapter (Hugging Face cache)
    pub fn model_catalog_cache(&self) -> Arc<ModelCacheAdapter> {
        Arc::clone(self.ai.model_catalog_cache())
    }

    /// Get system info port (for model management - from SystemModule)
    pub fn system_info(&self) -> Arc<dyn SystemInfoPort> {
        Arc::clone(self.system.system_info())
    }

    pub fn downloaded_model_repository(
        &self,
    ) -> Arc<crate::features::download::downloaded_model_repository::DownloadedModelRepository>
    {
        Arc::clone(self.ai.downloaded_model_repo())
    }

    /// Get download manager service
    ///
    /// Returns the shared download manager instance from AIModule.
    pub fn download_manager(
        &self,
    ) -> Result<Arc<dyn crate::features::download::manager::DownloadManager>> {
        Ok(Arc::clone(self.ai.download_manager()))
    }
}

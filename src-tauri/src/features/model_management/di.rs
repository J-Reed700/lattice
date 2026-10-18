//! Model-management feature dependency injection.

use std::sync::Arc;

use crate::features::model_management::cache_adapter::ModelCacheAdapter;
use crate::interfaces::di::Container;
use lattice_core::application::ports::active_models::ActiveModels;
use lattice_core::application::ports::{ModelCatalogPort, SystemInfoPort};
use lattice_core::domain::repositories::DownloadedModelRepository;
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

    /// The downloaded-model records.
    pub fn downloaded_models(&self) -> Arc<dyn DownloadedModelRepository> {
        Arc::clone(self.ai.downloaded_model_repo()) as Arc<dyn DownloadedModelRepository>
    }

    /// Which downloaded models are active for chat, embedding and utility.
    pub fn active_models(&self) -> Arc<dyn ActiveModels> {
        Arc::clone(self.ai.downloaded_model_repo()) as Arc<dyn ActiveModels>
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

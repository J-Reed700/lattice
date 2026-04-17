// Vertical-slice migration (download): repository trait lives in features/download/domain/.
#[path = "../../features/download/domain/downloaded_model_repository.rs"]
pub mod downloaded_model_repository;
pub mod model_repository;
pub mod search_repository;
pub mod system_repository;
pub mod unit_of_work;

pub use downloaded_model_repository::DownloadedModelRepository;
pub use model_repository::ModelRepository;
pub use search_repository::SearchRepository;
pub use system_repository::SystemRepository;
pub use unit_of_work::{UnitOfWork, UnitOfWorkFactory};

// Mock implementations for testing
#[cfg(test)]
pub mod mocks;

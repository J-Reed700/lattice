//! Embedding feature — use cases.

pub mod generate_batch;
pub mod generate_single;
pub mod get_model_info;

pub use generate_batch::GenerateBatchEmbeddingsUseCase;
pub use generate_single::GenerateSingleEmbeddingUseCase;
pub use get_model_info::GetEmbeddingModelInfoUseCase;

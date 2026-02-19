//! # Tag Use Cases
//!
//! Use cases for tag management operations.
//!
//! Orchestrates tag creation, assignment, generation, and search operations
//! using the TagServiceTrait.

mod apply_tags;
mod auto_tag_all;
mod create_tag;
mod delete_tag;
mod generate_tags;
mod get_tags;
mod remove_tag;
mod search_by_tag;
mod update_tag;

pub use apply_tags::ApplyTagsUseCase;
pub use auto_tag_all::AutoTagAllDocumentsUseCase;
pub use create_tag::CreateTagUseCase;
pub use delete_tag::DeleteTagUseCase;
pub use generate_tags::GenerateTagsUseCase;
pub use get_tags::GetTagsUseCase;
pub use remove_tag::RemoveTagFromDocumentUseCase;
pub use search_by_tag::SearchByTagUseCase;
pub use update_tag::UpdateTagUseCase;

//! Tags feature — use cases.

mod apply;
mod auto_tag_all;
mod create;
mod delete;
mod generate;
mod get;
mod remove;
mod search_by;
mod update;

pub use apply::ApplyTagsUseCase;
pub use auto_tag_all::AutoTagAllDocumentsUseCase;
pub use create::CreateTagUseCase;
pub use delete::DeleteTagUseCase;
pub use generate::GenerateTagsUseCase;
pub use get::GetTagsUseCase;
pub use remove::RemoveTagFromDocumentUseCase;
pub use search_by::SearchByTagUseCase;
pub use update::UpdateTagUseCase;

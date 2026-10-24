//! Creating, updating and deleting tags.

use tauri::State;

use crate::features::tags::dto::*;
use crate::interfaces::di::Container;
use lattice_core::shared::error::{AppError, Result};

pub async fn create_tag_ddd(
    container: State<'_, Container>,
    request: CreateTagRequestDto,
) -> Result<CreateTagResponseDto> {
    if request.name.is_empty() || request.name.len() > 100 {
        return Err(AppError::InvalidInput(
            "Tag name must be 1-100 characters".into(),
        ));
    }

    let use_case = container.create_tag_use_case();
    let response = use_case.execute(request).await?;

    Ok(response)
}

pub async fn update_tag_ddd(
    container: State<'_, Container>,
    request: UpdateTagRequestDto,
) -> Result<TagDto> {
    let use_case = container.update_tag_use_case();
    let response = use_case.execute(request.clone()).await?;

    Ok(response)
}

pub async fn delete_tag_ddd(
    container: State<'_, Container>,
    request: DeleteTagRequestDto,
) -> Result<()> {
    let use_case = container.delete_tag_use_case();
    use_case.execute(request.clone()).await?;

    Ok(())
}

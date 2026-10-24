//! Application update commands.

use crate::features::updates::dto::{UpdateInfoDto, VersionInfoDto};
use crate::interfaces::di::Container;
use lattice_core::shared::error::Result;
use tauri::State;

pub async fn check_for_updates(container: State<'_, Container>) -> Result<UpdateInfoDto> {
    let use_case = container.check_for_updates_use_case();
    let update_info = use_case.execute().await?;

    Ok(update_info)
}

pub async fn get_version_info(container: State<'_, Container>) -> Result<VersionInfoDto> {
    let use_case = container.get_version_info_use_case();
    let version_info = use_case.execute().await?;

    Ok(version_info)
}

pub async fn check_for_updates_impl(container: &Container) -> Result<UpdateInfoDto> {
    let use_case = container.check_for_updates_use_case();
    let update_info = use_case.execute().await?;

    Ok(update_info)
}

pub async fn get_version_info_impl(container: &Container) -> Result<VersionInfoDto> {
    let use_case = container.get_version_info_use_case();
    let version_info = use_case.execute().await?;

    Ok(version_info)
}

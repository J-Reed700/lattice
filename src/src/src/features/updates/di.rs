//! Updates feature dependency injection.

use std::sync::Arc;

use crate::application::ports::UpdateCheckerPort;
use crate::features::updates::adapter::UpdateCheckerAdapter;
use crate::features::updates::use_cases::{CheckForUpdatesUseCase, GetCurrentVersionUseCase};
use crate::interfaces::di::Container;

#[derive(Clone)]
pub struct UpdatesDi {
    pub update_checker: Arc<dyn UpdateCheckerPort>,
    pub check_for_updates_use_case: Arc<CheckForUpdatesUseCase>,
    pub get_current_version_use_case: Arc<GetCurrentVersionUseCase>,
}

pub fn build() -> UpdatesDi {
    let update_checker = Arc::new(UpdateCheckerAdapter::new()) as Arc<dyn UpdateCheckerPort>;

    UpdatesDi {
        check_for_updates_use_case: Arc::new(CheckForUpdatesUseCase::new(update_checker.clone())),
        get_current_version_use_case: Arc::new(GetCurrentVersionUseCase::new(
            update_checker.clone(),
        )),
        update_checker,
    }
}

/// Updates' registrar surface on `Container`.
impl Container {
    // Updates (from SystemModule)
    pub fn check_for_updates_use_case(&self) -> Arc<CheckForUpdatesUseCase> {
        Arc::clone(self.system.check_for_updates_use_case())
    }

    /// Get version info use case (alias for the current-version use case)
    pub fn get_version_info_use_case(&self) -> Arc<GetCurrentVersionUseCase> {
        Arc::clone(self.system.get_current_version_use_case())
    }
}

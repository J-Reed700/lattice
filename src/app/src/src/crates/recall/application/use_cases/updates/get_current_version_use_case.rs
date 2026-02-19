use crate::application::dtos::update_dto::VersionInfoDto;
use crate::application::ports::UpdateCheckerPort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetCurrentVersionUseCase {
    update_checker: Arc<dyn UpdateCheckerPort>,
}

impl GetCurrentVersionUseCase {
    pub fn new(update_checker: Arc<dyn UpdateCheckerPort>) -> Self {
        Self { update_checker }
    }

    pub async fn execute(&self) -> Result<VersionInfoDto, AppError> {
        let version = self.update_checker.get_current_version();

        Ok(VersionInfoDto {
            version,
            build_date: None,
            commit_hash: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::UpdateInfoData;
    use async_trait::async_trait;

    struct MockUpdateCheckerPort {
        current_version: String,
    }

    impl MockUpdateCheckerPort {
        fn new(version: &str) -> Self {
            Self {
                current_version: version.to_string(),
            }
        }
    }

    #[async_trait]
    impl UpdateCheckerPort for MockUpdateCheckerPort {
        async fn check_for_updates(&self) -> Result<UpdateInfoData, AppError> {
            Ok(UpdateInfoData {
                available: false,
                current_version: self.current_version.clone(),
                latest_version: Some(self.current_version.clone()),
                download_url: None,
                release_notes: None,
            })
        }

        fn get_current_version(&self) -> String {
            self.current_version.clone()
        }
    }

    #[tokio::test]
    async fn test_get_current_version_basic() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new("1.0.0"));
        let use_case = GetCurrentVersionUseCase::new(mock_checker);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let version_info = result.unwrap();
        assert_eq!(version_info.version, "1.0.0");
        assert!(version_info.build_date.is_none());
        assert!(version_info.commit_hash.is_none());
    }

    #[tokio::test]
    async fn test_get_current_version_semver() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new("2.5.3"));
        let use_case = GetCurrentVersionUseCase::new(mock_checker);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let version_info = result.unwrap();
        assert_eq!(version_info.version, "2.5.3");
    }

    #[tokio::test]
    async fn test_get_current_version_with_prerelease() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new("1.0.0-beta.1"));
        let use_case = GetCurrentVersionUseCase::new(mock_checker);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let version_info = result.unwrap();
        assert_eq!(version_info.version, "1.0.0-beta.1");
    }

    #[tokio::test]
    async fn test_get_current_version_with_build_metadata() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new("1.0.0+20240101"));
        let use_case = GetCurrentVersionUseCase::new(mock_checker);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let version_info = result.unwrap();
        assert_eq!(version_info.version, "1.0.0+20240101");
    }

    #[tokio::test]
    async fn test_get_current_version_development() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new("0.0.1-dev"));
        let use_case = GetCurrentVersionUseCase::new(mock_checker);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let version_info = result.unwrap();
        assert_eq!(version_info.version, "0.0.1-dev");
    }

    #[tokio::test]
    async fn test_get_current_version_multiple_calls() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new("3.2.1"));
        let use_case = GetCurrentVersionUseCase::new(mock_checker);

        let result1 = use_case.execute().await.unwrap();
        let result2 = use_case.execute().await.unwrap();

        assert_eq!(result1.version, result2.version);
        assert_eq!(result1.version, "3.2.1");
    }

    #[tokio::test]
    async fn test_get_current_version_empty_metadata() {
        let mock_checker = Arc::new(MockUpdateCheckerPort::new("1.0.0"));
        let use_case = GetCurrentVersionUseCase::new(mock_checker);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let version_info = result.unwrap();
        assert!(version_info.build_date.is_none());
        assert!(version_info.commit_hash.is_none());
    }
}

use crate::shared::error::Result;
use async_trait::async_trait;
use std::time::Duration;

#[async_trait]
pub trait BackupSchedulerPort: Send + Sync {
    async fn start(&self, interval: Duration) -> Result<()>;
    async fn stop(&self) -> Result<()>;
}

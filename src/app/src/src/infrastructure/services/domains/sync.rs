pub struct SyncService {}

impl Default for SyncService {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncService {
    pub fn new() -> Self {
        SyncService {}
    }

    pub async fn sync(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}

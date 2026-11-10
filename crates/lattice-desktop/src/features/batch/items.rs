//! The items of a batch import: one file or URL each, keyed by its job. The job
//! runtime owns the job's status; an item's status says how far that one file
//! or URL got.
use lattice_core::shared::error::{AppError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    Pending,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

impl ItemState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub(super) fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "pending" => Self::Pending,
            "processing" => Self::Processing,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            other => {
                return Err(AppError::Database(format!(
                    "Invalid batch item status: {other}"
                )))
            }
        })
    }
}

/// One file or URL of a batch import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchItem {
    pub id: String,
    /// Where the user put it in the batch; files of an ordered source keep
    /// this as their reading position.
    pub position: u32,
    /// A file path or a URL.
    pub target: String,
    /// The document it imported. A failed item keeps the document it
    /// committed before a later step failed, so its retry can finish that step.
    pub document_id: Option<String>,
    pub state: ItemState,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemCounts {
    pub total: u32,
    pub completed: u32,
    pub failed: u32,
}

impl ItemCounts {
    pub fn of(items: &[BatchItem]) -> Self {
        let tally = |state: ItemState| {
            u32::try_from(items.iter().filter(|item| item.state == state).count())
                .unwrap_or(u32::MAX)
        };
        Self {
            total: u32::try_from(items.len()).unwrap_or(u32::MAX),
            completed: tally(ItemState::Completed),
            failed: tally(ItemState::Failed),
        }
    }

    pub fn processed(&self) -> u32 {
        self.completed + self.failed
    }
}

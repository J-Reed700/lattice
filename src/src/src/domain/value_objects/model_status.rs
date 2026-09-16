use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelStatus {
    Pending,
    Downloading,
    Completed,
    Failed,
}

impl fmt::Display for ModelStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelStatus::Pending => write!(f, "pending"),
            ModelStatus::Downloading => write!(f, "downloading"),
            ModelStatus::Completed => write!(f, "completed"),
            ModelStatus::Failed => write!(f, "failed"),
        }
    }
}

impl FromStr for ModelStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(ModelStatus::Pending),
            "downloading" => Ok(ModelStatus::Downloading),
            "completed" => Ok(ModelStatus::Completed),
            "failed" => Ok(ModelStatus::Failed),
            _ => Err(format!("Invalid ModelStatus: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStatus {
    Pending,
    Downloading,
    Completed,
    Failed,
}

impl fmt::Display for FileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileStatus::Pending => write!(f, "pending"),
            FileStatus::Downloading => write!(f, "downloading"),
            FileStatus::Completed => write!(f, "completed"),
            FileStatus::Failed => write!(f, "failed"),
        }
    }
}

impl FromStr for FileStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(FileStatus::Pending),
            "downloading" => Ok(FileStatus::Downloading),
            "completed" => Ok(FileStatus::Completed),
            "failed" => Ok(FileStatus::Failed),
            _ => Err(format!("Invalid FileStatus: {}", s)),
        }
    }
}

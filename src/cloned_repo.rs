use std::path::PathBuf;
use serde::{Serialize, Deserialize};

/// Represents a repository that has been successfully cloned locally
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClonedRepository {
    pub repo_name: String,
    pub full_name: String,
    pub local_path: PathBuf,
    pub rust_files: Vec<PathBuf>,
}


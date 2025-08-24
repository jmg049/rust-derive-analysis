use leabharlann_processing::Processor;
use std::path::PathBuf;
use tracing::{info, warn};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{RepositoryInfo, DeriveStatement, parser::RustParser, repo_cache::RepositoryCache};

#[derive(Clone)]
pub struct RustFileProcessor {
    parser: RustParser,
    cache: Arc<Mutex<RepositoryCache>>,
}

impl RustFileProcessor {

    async fn process_repository(&self, repo: &RepositoryInfo) -> Result<Vec<DeriveStatement>, ProcessingError> {
        info!("Processing repository: {}", repo.full_name);
        
        // Clone or get cached repository
        let repo_path = {
            let mut cache = self.cache.lock().await;
            cache.ensure_repository(repo).await
                .map_err(|e| ProcessingError::CacheError(e.to_string()))?
        };
        
        // Find all Rust files in the repository
        let rust_files = {
            let cache = self.cache.lock().await;
            cache.find_rust_files(&repo_path)
                .map_err(|e| ProcessingError::CacheError(e.to_string()))?
        };
        
        info!("Found {} Rust files in {}", rust_files.len(), repo.full_name);
        
        let mut all_derives = Vec::new();
        
        // Process each Rust file
        for file_path in rust_files {
            match self.process_rust_file(&repo.full_name, &file_path, &repo_path).await {
                Ok(mut derives) => {
                    all_derives.append(&mut derives);
                }
                Err(e) => {
                    warn!("Failed to process file {:?}: {}", file_path, e);
                }
            }
        }
        
        info!("Found {} derive statements in {}", all_derives.len(), repo.full_name);
        Ok(all_derives)
    }

    async fn process_rust_file(&self, repo_name: &str, file_path: &PathBuf, repo_root: &PathBuf) -> Result<Vec<DeriveStatement>, ProcessingError> {
        // Read file content from local filesystem
        let content = tokio::fs::read_to_string(file_path).await
            .map_err(|e| ProcessingError::FileReadError(format!("Failed to read {}: {}", file_path.display(), e)))?;
        
        // Convert absolute path to relative path for reporting
        let relative_path = file_path.strip_prefix(repo_root)
            .unwrap_or(file_path)
            .to_string_lossy();
        
        // Extract derive statements using the parser
        let derives = self.parser.extract_derives(&content, repo_name, &relative_path);
        
        if !derives.is_empty() {
            info!("Found {} derive statements in {}", derives.len(), file_path.display());
        }
        
        Ok(derives)
    }
}

impl Processor<RepositoryInfo, Vec<DeriveStatement>, ProcessingError> for RustFileProcessor {
    fn process(&self, repo: RepositoryInfo) -> Result<Vec<DeriveStatement>, ProcessingError> {
        // Use the current runtime handle instead of creating a new one
        let handle = tokio::runtime::Handle::current();
        
        handle.block_on(self.process_repository(&repo))
    }

    fn name(&self) -> &'static str {
        "RustFileProcessor"
    }

    fn can_process(&self, repo: &RepositoryInfo) -> bool {
        // Only process Rust repositories
        repo.language.as_ref().map_or(false, |lang| lang == "Rust")
    }

    fn config_info(&self) -> String {
        "RustFileProcessor: git clone based processing".to_string()
    }
}

#[derive(Debug)]
pub enum ProcessingError {
    CacheError(String),
    FileReadError(String),
}

impl std::fmt::Display for ProcessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessingError::CacheError(msg) => write!(f, "Cache error: {}", msg),
            ProcessingError::FileReadError(msg) => write!(f, "File read error: {}", msg),
        }
    }
}

impl std::error::Error for ProcessingError {}


use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use leabharlann_processing::*;
use tracing::{info, warn};

use crate::{RepositoryInfo, DeriveStatement, parser::RustParser, repo_cache::{RepositoryCache, CacheConfig}, persistence::ResultsPersistence};

#[derive(Debug, Clone)]
pub struct RepositoryTask {
    pub repo_info: RepositoryInfo,
}

#[derive(Debug, Clone)]
pub struct RepositoryResult {
    pub repo_name: String,
    pub derive_statements: Vec<DeriveStatement>,
    pub rust_files_processed: usize,
}

#[derive(Debug)]
pub enum ProcessingError {
    CloneError(String),
    FileAccessError(String),
}

impl std::fmt::Display for ProcessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessingError::CloneError(msg) => write!(f, "Clone error: {}", msg),
            ProcessingError::FileAccessError(msg) => write!(f, "File access error: {}", msg),
        }
    }
}

impl std::error::Error for ProcessingError {}

#[derive(Clone)]
pub struct RepositoryProcessor {
    cache_config: CacheConfig,
    parser: RustParser,
    results_storage: Arc<Mutex<Vec<RepositoryResult>>>,
    output_dir: PathBuf,
}

impl RepositoryProcessor {
    pub fn new(cache_config: CacheConfig, results_storage: Arc<Mutex<Vec<RepositoryResult>>>, output_dir: PathBuf) -> Self {
        Self {
            cache_config,
            parser: RustParser::new(),
            results_storage,
            output_dir,
        }
    }

    fn process_file_safely(&self, content: &str, repository: &str, file_path: &str) -> Result<Vec<DeriveStatement>, ProcessingError> {
        // For files that are likely to cause issues, use text-based parsing only
        if self.should_use_text_only_parsing(content, file_path) {
            return Ok(self.parser.extract_derives_text_only(content, repository, file_path));
        }

        // For normal files, use the standard parser with fallback
        Ok(self.parser.extract_derives(content, repository, file_path))
    }

    fn should_use_text_only_parsing(&self, content: &str, file_path: &str) -> bool {
        // Use text-only parsing for known problematic patterns
        
        // Files in rust-lang/rust tests are particularly problematic
        if file_path.contains("rust-lang/rust/tests/") {
            return true;
        }
        
        // Very large files
        if content.len() > 200_000 {
            return true;
        }
        
        // Files with excessive macro usage
        let macro_count = content.matches("macro_rules!").count() + 
                         content.matches("#[").count();
        if macro_count > 100 {
            return true;
        }
        
        // Files with very deep nesting
        let open_braces = content.chars().filter(|&c| c == '{').count();
        if open_braces > 500 {
            return true;
        }
        
        // Check for specific problematic patterns
        if content.contains("expected `!`") || 
           content.contains("issue-105330") ||
           file_path.contains("associated-consts") {
            return true;
        }
        
        false
    }
}

impl Processor<RepositoryTask, RepositoryResult, ProcessingError> for RepositoryProcessor {
    fn process(&self, task: RepositoryTask) -> Result<RepositoryResult, ProcessingError> {
        let repo = &task.repo_info;
        info!("Processing repository: {}", repo.full_name);

        // Create a thread-local cache for this repository
        let mut cache = RepositoryCache::new(self.cache_config.clone());
        
        // Clone the repository
        let repo_path = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt.block_on(cache.ensure_repository(repo))
                .map_err(|e| ProcessingError::CloneError(format!("Failed to clone {}: {}", repo.full_name, e)))?,
            Err(e) => return Err(ProcessingError::CloneError(format!("Failed to create tokio runtime: {}", e))),
        };

        // Find all Rust files
        let rust_files = cache.find_rust_files(&repo_path)
            .map_err(|e| ProcessingError::FileAccessError(format!("Failed to find Rust files in {}: {}", repo.full_name, e)))?;

        if rust_files.is_empty() {
            info!("No Rust files found in {}, skipping", repo.full_name);
            return Ok(RepositoryResult {
                repo_name: repo.full_name.clone(),
                derive_statements: Vec::new(),
                rust_files_processed: 0,
            });
        }

        info!("Found {} Rust files in {}", rust_files.len(), repo.full_name);

        // Process all files in this repository
        let mut all_derives = Vec::new();
        let mut files_processed = 0;

        for rust_file in &rust_files {
            match std::fs::read_to_string(rust_file) {
                Ok(content) => {
                    // Convert absolute path to relative path for reporting
                    let relative_path = rust_file.strip_prefix(&repo_path)
                        .unwrap_or(rust_file)
                        .to_string_lossy();

                    // Process each file safely with timeout and error isolation
                    match self.process_file_safely(&content, &repo.full_name, &relative_path) {
                        Ok(derives) => {
                            if !derives.is_empty() {
                                info!("Found {} derive statements in {}/{}", 
                                      derives.len(), repo.full_name, relative_path);
                                all_derives.extend(derives);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to process file {}/{}: {}", repo.full_name, relative_path, e);
                        }
                    }
                    files_processed += 1;
                }
                Err(e) => {
                    warn!("Failed to read {}: {}", rust_file.display(), e);
                }
            }
        }

        info!("Finished processing {} ({} files, {} derive statements)", 
              repo.full_name, files_processed, all_derives.len());

        let result = RepositoryResult {
            repo_name: repo.full_name.clone(),
            derive_statements: all_derives,
            rust_files_processed: files_processed,
        };

        // Store the result in shared storage
        if let Ok(mut storage) = self.results_storage.lock() {
            storage.push(result.clone());
            
            // Persist all current results to disk after each repository completion
            let all_derives: Vec<DeriveStatement> = storage.iter()
                .flat_map(|repo_result| repo_result.derive_statements.iter())
                .cloned()
                .collect();
            
            if !all_derives.is_empty() {
                let json_output = self.output_dir.join("derive_statements_incremental.json");
                
                // Use blocking runtime to save the results
                if let Err(e) = tokio::runtime::Runtime::new()
                    .map_err(|e| ProcessingError::FileAccessError(format!("Failed to create runtime: {}", e)))
                    .and_then(|rt| rt.block_on(ResultsPersistence::save_to_json(&all_derives, &json_output))
                        .map_err(|e| ProcessingError::FileAccessError(format!("Failed to save incremental results: {}", e))))
                {
                    warn!("Failed to save incremental results after {}: {}", repo.full_name, e);
                } else {
                    info!("Saved incremental results after processing {} ({} total derives)", 
                          repo.full_name, all_derives.len());
                }
            }
        }

        Ok(result)
    }

    fn name(&self) -> &'static str {
        "RepositoryProcessor"
    }

    fn can_process(&self, task: &RepositoryTask) -> bool {
        // Basic validation - could add more sophisticated filtering here
        !task.repo_info.full_name.is_empty()
    }

    fn config_info(&self) -> String {
        format!("RepositoryProcessor: cache_limit={}, cache_size={}GB", 
                self.cache_config.max_repositories, self.cache_config.max_size_gb)
    }
}


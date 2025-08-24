use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use tokio::process::Command;
use tracing::{info, warn, error};
use serde::{Serialize, Deserialize};

use crate::RepositoryInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub max_repositories: usize,
    pub cache_root: PathBuf,
    pub max_size_gb: f64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_repositories: 5,
            cache_root: PathBuf::from("./repo_cache"),
            max_size_gb: 2.0,
        }
    }
}

#[derive(Debug)]
pub struct RepositoryCache {
    config: CacheConfig,
    active_repos: HashMap<String, PathBuf>,
}

impl RepositoryCache {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            active_repos: HashMap::new(),
        }
    }

    pub async fn ensure_repository(&mut self, repo: &RepositoryInfo) -> Result<PathBuf, CacheError> {
        let repo_key = repo.full_name.clone();
        
        // Check if already cached
        if let Some(path) = self.active_repos.get(&repo_key) {
            if path.exists() {
                info!("Repository {} already cached at {:?}", repo_key, path);
                return Ok(path.clone());
            } else {
                // Path no longer exists, remove from cache
                self.active_repos.remove(&repo_key);
            }
        }

        // Ensure we have space for a new repository
        self.make_space_if_needed().await?;

        // Clone the repository
        let repo_path = self.clone_repository(repo).await?;
        self.active_repos.insert(repo_key, repo_path.clone());

        Ok(repo_path)
    }

    async fn clone_repository(&self, repo: &RepositoryInfo) -> Result<PathBuf, CacheError> {
        let repo_dir = self.config.cache_root.join(self.sanitize_repo_name(&repo.full_name));
        
        // Create cache directory if it doesn't exist
        if let Err(e) = fs::create_dir_all(&self.config.cache_root) {
            return Err(CacheError::IoError(format!("Failed to create cache directory: {}", e)));
        }

        // Check if repository already exists locally
        if repo_dir.exists() && self.is_valid_git_repo(&repo_dir).await {
            info!("Repository {} already exists locally at {:?}, using existing copy", repo.full_name, repo_dir);
            return Ok(repo_dir);
        }

        // Remove existing directory if it exists but is invalid
        if repo_dir.exists() {
            info!("Removing invalid repository directory for fresh clone");
            if let Err(e) = fs::remove_dir_all(&repo_dir) {
                warn!("Failed to remove existing repository directory: {}", e);
            }
        }

        info!("Cloning repository {} to {:?}", repo.full_name, repo_dir);

        // Clone with shallow depth to save space and time
        let output = Command::new("git")
            .args([
                "clone",
                "--depth", "1",
                "--single-branch",
                &repo.clone_url,
                repo_dir.to_str().unwrap()
            ])
            .output()
            .await
            .map_err(|e| CacheError::GitError(format!("Failed to execute git clone: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            error!("Git clone failed for {}: status={:?}, stderr={}, stdout={}", 
                   repo.full_name, output.status.code(), stderr, stdout);
            return Err(CacheError::GitError(format!("Git clone failed: {}", stderr)));
        }

        info!("Successfully cloned {} to {:?}", repo.full_name, repo_dir);
        Ok(repo_dir)
    }

    async fn make_space_if_needed(&mut self) -> Result<(), CacheError> {
        // Check repository count limit
        while self.active_repos.len() >= self.config.max_repositories {
            info!("Cache at repository limit ({}/{}), removing oldest repository", 
                  self.active_repos.len(), self.config.max_repositories);
            self.remove_oldest_repository().await?;
        }

        // Check disk space limit
        let mut cache_size = self.get_cache_size_gb().await?;
        while cache_size > self.config.max_size_gb && !self.active_repos.is_empty() {
            info!("Cache size {:.2}GB exceeds limit {:.2}GB, removing oldest repository", 
                  cache_size, self.config.max_size_gb);
            self.remove_oldest_repository().await?;
            cache_size = self.get_cache_size_gb().await?;
        }

        Ok(())
    }

    async fn remove_oldest_repository(&mut self) -> Result<(), CacheError> {
        // For simplicity, remove the first repository in the HashMap
        // In a more sophisticated implementation, we'd track access times
        if let Some((repo_name, repo_path)) = self.active_repos.iter().next() {
            let repo_name = repo_name.clone();
            let repo_path = repo_path.clone();
            
            info!("Removing repository {} from cache to make space", repo_name);
            
            if let Err(e) = fs::remove_dir_all(&repo_path) {
                warn!("Failed to remove repository directory {:?}: {}", repo_path, e);
            }
            
            self.active_repos.remove(&repo_name);
        }

        Ok(())
    }

    async fn get_cache_size_gb(&self) -> Result<f64, CacheError> {
        if !self.config.cache_root.exists() {
            return Ok(0.0);
        }

        let output = Command::new("du")
            .args(["-sb", self.config.cache_root.to_str().unwrap()])
            .output()
            .await
            .map_err(|e| CacheError::IoError(format!("Failed to get cache size: {}", e)))?;

        if !output.status.success() {
            return Ok(0.0);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let size_bytes: u64 = stdout
            .split_whitespace()
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        Ok(size_bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }

    fn sanitize_repo_name(&self, repo_name: &str) -> String {
        repo_name.replace('/', "_").replace('\\', "_")
    }

    async fn is_valid_git_repo(&self, repo_path: &Path) -> bool {
        // Check if .git directory exists
        let git_dir = repo_path.join(".git");
        if !git_dir.exists() {
            return false;
        }

        // Check if we can run git status in the directory
        match Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo_path)
            .output()
            .await
        {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    pub fn find_rust_files(&self, repo_path: &Path) -> Result<Vec<PathBuf>, CacheError> {
        let mut rust_files = Vec::new();
        self.find_rust_files_recursive(repo_path, &mut rust_files)?;
        Ok(rust_files)
    }

    fn find_rust_files_recursive(&self, dir: &Path, rust_files: &mut Vec<PathBuf>) -> Result<(), CacheError> {
        let entries = fs::read_dir(dir)
            .map_err(|e| CacheError::IoError(format!("Failed to read directory {:?}: {}", dir, e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| CacheError::IoError(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();

            if path.is_dir() {
                // Skip common directories that don't contain source code
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if matches!(dir_name, "target" | "node_modules" | ".git" | ".github" | "vendor" | "third_party" | "deps" | "build") {
                        continue;
                    }
                }
                
                // Recursively search subdirectories
                self.find_rust_files_recursive(&path, rust_files)?;
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                rust_files.push(path);
            }
        }

        Ok(())
    }

    pub async fn cleanup(&mut self) -> Result<(), CacheError> {
        info!("Cleaning up repository cache");
        
        for (repo_name, repo_path) in &self.active_repos {
            info!("Removing cached repository: {}", repo_name);
            if let Err(e) = fs::remove_dir_all(repo_path) {
                warn!("Failed to remove repository directory {:?}: {}", repo_path, e);
            }
        }
        
        self.active_repos.clear();

        // Remove the entire cache directory
        if self.config.cache_root.exists() {
            if let Err(e) = fs::remove_dir_all(&self.config.cache_root) {
                warn!("Failed to remove cache root directory: {}", e);
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum CacheError {
    IoError(String),
    GitError(String),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::IoError(msg) => write!(f, "I/O error: {}", msg),
            CacheError::GitError(msg) => write!(f, "Git error: {}", msg),
        }
    }
}

impl std::error::Error for CacheError {}
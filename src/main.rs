mod github;
mod parser;
mod processor;
mod persistence;
mod error_handling;
mod repo_cache;
mod cloned_repo;
mod parallel_processor;

use leabharlann_logging::{LogConfig, LogLevel, LogFormat, init_logging};
use leabharlann_string::ColoredString;
use leabharlann_processing::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::info;
use error_handling::ErrorReporter;
use repo_cache::CacheConfig;
use parallel_processor::{RepositoryTask, RepositoryProcessor};
use clap::Parser;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeriveStatement {
    repository: String,
    file_path: String,
    line_number: usize,
    derives: Vec<String>,
    full_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RepositoryInfo {
    name: String,
    full_name: String,
    clone_url: String,
    language: Option<String>,
    stars: u32,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Rust Derive Analysis Tool - Analyzes derive macro usage across Rust repositories", long_about = None)]
struct Args {
    /// Output directory for analysis results
    #[arg(short, long, default_value = "data")]
    output: PathBuf,
    
    /// Maximum number of repositories to analyze
    #[arg(short = 'r', long, default_value_t = 5)]
    repo_limit: usize,
    
    /// Maximum number of repositories to keep cached on disk
    #[arg(short = 'c', long, default_value_t = 10)]
    cache_limit: usize,
    
    /// Maximum cache size in GB
    #[arg(short = 's', long, default_value_t = 5.0)]
    cache_size: f64,
    
    /// Number of worker threads for processing
    #[arg(short = 't', long, default_value_t = 4)]
    threads: usize,
    
    /// Minimum stars required for repository selection
    #[arg(long, default_value_t = 100)]
    min_stars: u32,
    
    /// Verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args = Args::parse();
    
    // Initialize logging with console output and file logging
    let log_level = if args.verbose { LogLevel::Debug } else { LogLevel::Info };
    let config = LogConfig::new()
        .level(log_level)
        .console(true)
        .file("rust_derive_analysis.log")
        .format(LogFormat::Pretty)
        .colored(true);
    
    init_logging(&config)?;
    
    info!("Starting Rust Derive Analysis Tool - Phase 1: Data Acquisition");
    info!("Configuration: repo_limit={}, cache_limit={}, cache_size={}GB, threads={}, min_stars={}, output_dir={:?}", 
          args.repo_limit, args.cache_limit, args.cache_size, args.threads, args.min_stars, args.output);
    info!("Threading: Configured for {} worker threads (parallel repository processing)", args.threads);
    
    // Create output directory for collected data
    tokio::fs::create_dir_all(&args.output).await?;
    
    // Phase 1: Data Acquisition Pipeline
    let github_token = std::env::var("GITHUB_TOKEN").ok();
    if github_token.is_none() {
        ErrorReporter::report_warning("GITHUB_TOKEN not set - API rate limits will be more restrictive");
    }
    
    // Step 1: Discover Rust repositories
    let github_client = github::GitHubClient::new(github_token.clone());
    let repositories = github_client.search_rust_repositories(args.repo_limit, args.min_stars).await?;
    info!("Discovered {} repositories for analysis", repositories.len());
    ErrorReporter::report_info(&format!("Successfully discovered {} Rust repositories", repositories.len()));
    
    // Step 2: Process repositories in parallel using leabharlann-processing
    let cache_dir = args.output.join("cache");
    tokio::fs::create_dir_all(&cache_dir).await?;
    
    // Create repository tasks
    let repository_tasks: Vec<RepositoryTask> = repositories
        .into_iter()
        .map(|repo| RepositoryTask { repo_info: repo })
        .collect();
    
    if repository_tasks.is_empty() {
        ErrorReporter::report_warning("No repositories found matching criteria");
        return Ok(());
    }
    
    info!("Processing {} repositories using {} worker threads in parallel...", repository_tasks.len(), args.threads);
    
    // Set up parallel processing system
    let cache_config = CacheConfig {
        max_repositories: args.cache_limit,
        cache_root: cache_dir,
        max_size_gb: args.cache_size,
    };
    
    let num_threads = args.threads;
    let system_config = SystemConfig::default();
    let system_metrics = Arc::new(SystemMetrics::new());
    
    // Create shared storage for results
    let results_storage = Arc::new(Mutex::new(Vec::new()));
    
    // Create channel hub
    let (hub, work_receivers) = ChannelHub::new(num_threads, system_config);
    
    // Create processor with shared storage
    let processor = RepositoryProcessor::new(cache_config, results_storage.clone(), args.output.clone());
    info!("Processor configuration: {}", processor.config_info());
    
    // Spawn workers
    let mut worker_handles = Vec::new();
    for (thread_id, work_receiver) in work_receivers.into_iter().enumerate() {
        let worker = Worker::new(thread_id, processor.clone(), WorkerConfig::default());
        let channels = hub.get_thread_channels();
        system_metrics.register_thread(worker.metrics.clone());
        let handle = worker.spawn(work_receiver, channels);
        worker_handles.push(handle);
    }
    
    // Spawn standard collector without progress bar
    let collector_config = CollectorConfig {
        show_progress: false,
        ..Default::default()
    };
    let collector = Collector::new(system_metrics.clone(), Some(collector_config));
    let collector_handle = collector.spawn(hub.get_collector_channels());
    
    // Spawn thread manager
    let manager = ThreadManager::new(system_metrics.clone(), None);
    let manager_handle = manager.spawn(hub.get_manager_channels(), repository_tasks);
    
    // Wait for completion
    info!("Starting parallel repository processing...");
    let _ = manager_handle.join();
    info!("Thread manager completed");
    
    // Give a small delay to ensure proper signal propagation
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    
    // Drop the hub to ensure all channels are properly closed
    drop(hub);
    info!("All channels closed");
    
    // Wait for workers to complete with timeout
    info!("Waiting for {} workers to complete...", worker_handles.len());
    for (i, handle) in worker_handles.into_iter().enumerate() {
        info!("Waiting for worker {} to join...", i);
        
        // Try to join the thread with a reasonable timeout approach
        // Since JoinHandle doesn't have a timeout, we'll just join normally
        // but add logging to help diagnose if it hangs
        match handle.join() {
            Ok(_) => info!("Worker {} completed successfully", i),
            Err(_) => info!("Worker {} panicked", i),
        }
    }
    
    // Wait for collector
    info!("Waiting for collector to complete...");
    let collector_stats = collector_handle.join().unwrap();
    info!("Collector completed");
    
    let mut all_derives = Vec::new();
    let mut total_files_processed = 0;
    
    // Extract results from shared storage
    let repository_results = if let Ok(results) = results_storage.lock() {
        results.clone()
    } else {
        Vec::new()
    };
    
    info!("Parallel processing completed. Processed: {} successful, {} failed", 
          collector_stats.successful, collector_stats.failed);
    
    // Aggregate all derive statements from all repository results
    for (idx, repo_result) in repository_results.iter().enumerate() {
        info!("Repository {}: {} files processed, {} derive statements found",
              repo_result.repo_name, repo_result.rust_files_processed, repo_result.derive_statements.len());
        
        // Report progress through repositories
        ErrorReporter::report_progress(idx + 1, repository_results.len(), &repo_result.repo_name);
        
        all_derives.extend(repo_result.derive_statements.clone());
        total_files_processed += repo_result.rust_files_processed;
    }
    
    info!("Total: {} files processed, {} derive statements found across {} repositories",
          total_files_processed, all_derives.len(), collector_stats.successful);
    
    info!("Found {} total derive statements across all repositories", all_derives.len());
    
    let json_output = args.output.join("derive_statements.json");
    let csv_output = args.output.join("derive_statements.csv");
    let summary_output = args.output.join("analysis_summary.json");
    
    // Save results in multiple formats
    if !all_derives.is_empty() {
        match persistence::ResultsPersistence::save_to_json(&all_derives, &json_output).await {
            Ok(_) => ErrorReporter::report_info("JSON output saved successfully"),
            Err(e) => {
                let error = error_handling::AnalysisError::Persistence(format!("Failed to save JSON: {}", e));
                ErrorReporter::report_error(&error);
                return Err(e);
            }
        }
        
        match persistence::ResultsPersistence::save_to_csv(&all_derives, &csv_output).await {
            Ok(_) => ErrorReporter::report_info("CSV output saved successfully"),
            Err(e) => {
                let error = error_handling::AnalysisError::Persistence(format!("Failed to save CSV: {}", e));
                ErrorReporter::report_error(&error);
                return Err(e);
            }
        }
        
        match persistence::ResultsPersistence::save_summary(&all_derives, &summary_output).await {
            Ok(_) => ErrorReporter::report_info("Summary output saved successfully"),
            Err(e) => {
                let error = error_handling::AnalysisError::Persistence(format!("Failed to save summary: {}", e));
                ErrorReporter::report_error(&error);
                return Err(e);
            }
        }
        
        ErrorReporter::report_success("Analysis results saved to JSON, CSV, and summary files");
    } else {
        ErrorReporter::report_warning("No derive statements found in any repositories");
    }
    
    let completion_msg = ColoredString::new(&format!(
        "✅ Analysis Complete! Processed {} repositories and found {} derive statements",
        collector_stats.successful, all_derives.len()
    )).green().bold();
    
    println!("{}", completion_msg);
    info!("Output files: {}, {}, {}", 
          json_output.display(), csv_output.display(), summary_output.display());
    
    Ok(())
}


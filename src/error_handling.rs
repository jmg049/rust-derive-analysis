use leabharlann_string::{ColoredString, TextFormatter};
use std::fmt;
use tracing::error;

#[derive(Debug)]
pub enum AnalysisError {
    GitHub(String),
    Parser(String),
    Processing(String),
    Persistence(String),
    Network(String),
    Configuration(String),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnalysisError::GitHub(msg) => write!(f, "GitHub API error: {}", msg),
            AnalysisError::Parser(msg) => write!(f, "Parsing error: {}", msg),
            AnalysisError::Processing(msg) => write!(f, "Processing error: {}", msg),
            AnalysisError::Persistence(msg) => write!(f, "Persistence error: {}", msg),
            AnalysisError::Network(msg) => write!(f, "Network error: {}", msg),
            AnalysisError::Configuration(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

impl std::error::Error for AnalysisError {}

pub struct ErrorReporter;

impl ErrorReporter {
    pub fn report_error(error: &AnalysisError) {
        let error_msg = match error {
            AnalysisError::GitHub(msg) => {
                TextFormatter::error(&format!("🐙 GitHub API Error: {}", msg))
            }
            AnalysisError::Parser(msg) => {
                TextFormatter::error(&format!("📝 Parser Error: {}", msg))
            }
            AnalysisError::Processing(msg) => {
                TextFormatter::error(&format!("⚙️ Processing Error: {}", msg))
            }
            AnalysisError::Persistence(msg) => {
                TextFormatter::error(&format!("💾 Persistence Error: {}", msg))
            }
            AnalysisError::Network(msg) => {
                TextFormatter::error(&format!("🌐 Network Error: {}", msg))
            }
            AnalysisError::Configuration(msg) => {
                TextFormatter::error(&format!("⚙️ Configuration Error: {}", msg))
            }
        };
        
        println!("{}", error_msg);
        error!("{}", error);
    }
    
    pub fn report_warning(message: &str) {
        let warning_msg = TextFormatter::warning(&format!("⚠️ {}", message));
        println!("{}", warning_msg);
    }
    
    pub fn report_info(message: &str) {
        let info_msg = TextFormatter::info(&format!("ℹ️ {}", message));
        println!("{}", info_msg);
    }
    
    pub fn report_success(message: &str) {
        let success_msg = TextFormatter::success(&format!("✅ {}", message));
        println!("{}", success_msg);
    }
    
    pub fn report_progress(current: usize, total: usize, item: &str) {
        let percentage = (current as f64 / total as f64) * 100.0;
        let progress_bar = TextFormatter::progress_bar(current, total, 30);
        
        let progress_msg = ColoredString::new(&format!(
            "Progress: [{}] {:.1}% ({}/{}) - {}",
            progress_bar, percentage, current, total, item
        )).blue();
        
        println!("{}", progress_msg);
    }
}
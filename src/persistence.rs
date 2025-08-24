use csv::Writer;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::info;

use crate::DeriveStatement;

pub struct ResultsPersistence;

impl ResultsPersistence {
    pub async fn save_to_json<P: AsRef<Path>>(
        derives: &[DeriveStatement], 
        path: P
    ) -> Result<(), Box<dyn std::error::Error>> {
        let json_data = serde_json::to_string_pretty(derives)?;
        let mut file = File::create(path.as_ref()).await?;
        file.write_all(json_data.as_bytes()).await?;
        
        info!("Saved {} derive statements to {}", derives.len(), path.as_ref().display());
        Ok(())
    }
    
    pub async fn save_to_csv<P: AsRef<Path>>(
        derives: &[DeriveStatement], 
        path: P
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut wtr = Writer::from_path(path.as_ref())?;
        
        // Write header
        wtr.write_record(&["repository", "file_path", "line_number", "derives", "full_line"])?;
        
        // Write data rows
        for derive in derives {
            let derives_str = derive.derives.join(", ");
            wtr.write_record(&[
                &derive.repository,
                &derive.file_path,
                &derive.line_number.to_string(),
                &derives_str,
                &derive.full_line,
            ])?;
        }
        
        wtr.flush()?;
        info!("Saved {} derive statements to {}", derives.len(), path.as_ref().display());
        Ok(())
    }
    
    pub async fn save_summary<P: AsRef<Path>>(
        derives: &[DeriveStatement], 
        path: P
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::collections::HashMap;
        
        // Analyze derive patterns
        let mut derive_counts: HashMap<String, usize> = HashMap::new();
        let mut repo_counts: HashMap<String, usize> = HashMap::new();
        let mut total_statements = 0;
        
        for derive_stmt in derives {
            total_statements += 1;
            repo_counts.entry(derive_stmt.repository.clone())
                .and_modify(|e| *e += 1)
                .or_insert(1);
                
            for derive in &derive_stmt.derives {
                derive_counts.entry(derive.clone())
                    .and_modify(|e| *e += 1)
                    .or_insert(1);
            }
        }
        
        // Sort by frequency
        let mut sorted_derives: Vec<_> = derive_counts.into_iter().collect();
        sorted_derives.sort_by(|a, b| b.1.cmp(&a.1));
        
        let mut sorted_repos: Vec<_> = repo_counts.into_iter().collect();
        sorted_repos.sort_by(|a, b| b.1.cmp(&a.1));
        
        // Create summary
        let summary = serde_json::json!({
            "total_derive_statements": total_statements,
            "total_repositories": sorted_repos.len(),
            "total_unique_derives": sorted_derives.len(),
            "most_common_derives": sorted_derives.into_iter().take(20).collect::<Vec<_>>(),
            "repositories_by_derive_count": sorted_repos.into_iter().take(20).collect::<Vec<_>>(),
            "analysis_timestamp": chrono::Utc::now().to_rfc3339()
        });
        
        let summary_json = serde_json::to_string_pretty(&summary)?;
        let mut file = File::create(path.as_ref()).await?;
        file.write_all(summary_json.as_bytes()).await?;
        
        info!("Saved analysis summary to {}", path.as_ref().display());
        Ok(())
    }
}
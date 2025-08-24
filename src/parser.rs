use syn::{parse_file, Item, Attribute, Meta, MetaList};
use quote::ToTokens;
use tracing::{debug, warn};
use std::panic;
use crate::DeriveStatement;

#[derive(Clone)]
pub struct RustParser;

impl RustParser {
    pub fn new() -> Self {
        Self
    }

    pub fn extract_derives(&self, content: &str, repository: &str, file_path: &str) -> Vec<DeriveStatement> {
        let mut derives = Vec::new();
        
        // Skip files that are likely to cause stack overflow
        if self.is_likely_problematic_file(content, file_path) {
            debug!("Skipping potentially problematic file {}/{} (too complex for syn parser)", repository, file_path);
            self.extract_derives_text_based(content, repository, file_path, &mut derives);
            return derives;
        }
        
        // Try to parse the file as Rust syntax with error handling
        let content_clone = content.to_string();
        match panic::catch_unwind(move || parse_file(&content_clone)) {
            Ok(Ok(file)) => {
                for item in file.items {
                    self.extract_derives_from_item(&item, &mut derives, repository, file_path, content);
                }
            }
            Ok(Err(e)) => {
                warn!("Failed to parse Rust file {}/{}: {}", repository, file_path, e);
                // Fallback to text-based extraction
                self.extract_derives_text_based(content, repository, file_path, &mut derives);
            }
            Err(_) => {
                warn!("Parser panicked on file {}/{}, using text-based fallback", repository, file_path);
                // Fallback to text-based extraction
                self.extract_derives_text_based(content, repository, file_path, &mut derives);
            }
        }
        
        derives
    }

    fn extract_derives_from_item(
        &self,
        item: &Item,
        derives: &mut Vec<DeriveStatement>,
        repository: &str,
        file_path: &str,
        content: &str,
    ) {
        match item {
            Item::Struct(item_struct) => {
                self.extract_derives_from_attrs(&item_struct.attrs, derives, repository, file_path, content);
            }
            Item::Enum(item_enum) => {
                self.extract_derives_from_attrs(&item_enum.attrs, derives, repository, file_path, content);
            }
            Item::Union(item_union) => {
                self.extract_derives_from_attrs(&item_union.attrs, derives, repository, file_path, content);
            }
            _ => {}
        }
    }

    fn extract_derives_from_attrs(
        &self,
        attrs: &[Attribute],
        derives: &mut Vec<DeriveStatement>,
        repository: &str,
        file_path: &str,
        content: &str,
    ) {
        for attr in attrs {
            if attr.path().is_ident("derive") {
                let meta = attr.meta.clone();
                match meta {
                    Meta::List(MetaList { tokens, .. }) => {
                        let derive_list = self.parse_derive_tokens(&tokens.to_string());
                        if !derive_list.is_empty() {
                            let line_number = self.find_line_number(content, &attr.to_token_stream().to_string());
                            let full_line = self.get_line_at_number(content, line_number);
                            
                            derives.push(DeriveStatement {
                                repository: repository.to_string(),
                                file_path: file_path.to_string(),
                                line_number,
                                derives: derive_list.clone(),
                                full_line,
                            });
                            
                            debug!("Found derive in {}/{} at line {}: {:?}", 
                                  repository, file_path, line_number, derive_list);
                        }
                    }
                    _ => {
                        warn!("Unexpected derive format in {}/{}", repository, file_path);
                    }
                }
            }
        }
    }

    fn parse_derive_tokens(&self, tokens: &str) -> Vec<String> {
        // Parse the derive list from tokens like "Clone , Copy , Debug"
        tokens
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == ':'))
            .collect()
    }

    fn extract_derives_text_based(
        &self,
        content: &str,
        repository: &str,
        file_path: &str,
        derives: &mut Vec<DeriveStatement>,
    ) {
        // Fallback text-based parsing for files that can't be parsed syntactically
        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[derive(") && trimmed.ends_with(")]") {
                let derive_content = &trimmed[9..trimmed.len()-2]; // Remove "#[derive(" and ")]"
                let derive_list: Vec<String> = derive_content
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                
                if !derive_list.is_empty() {
                    derives.push(DeriveStatement {
                        repository: repository.to_string(),
                        file_path: file_path.to_string(),
                        line_number: line_num + 1,
                        derives: derive_list,
                        full_line: line.to_string(),
                    });
                    
                    debug!("Found derive (text-based) in {}/{} at line {}", 
                          repository, file_path, line_num + 1);
                }
            }
        }
    }

    fn find_line_number(&self, content: &str, _target: &str) -> usize {
        // Simple heuristic to find line number - could be improved
        for (line_num, line) in content.lines().enumerate() {
            if line.contains("derive") {
                return line_num + 1;
            }
        }
        1
    }

    fn get_line_at_number(&self, content: &str, line_number: usize) -> String {
        content
            .lines()
            .nth(line_number.saturating_sub(1))
            .unwrap_or("")
            .to_string()
    }

    pub fn extract_derives_text_only(&self, content: &str, repository: &str, file_path: &str) -> Vec<DeriveStatement> {
        let mut derives = Vec::new();
        debug!("Using text-only parsing for potentially problematic file {}/{}", repository, file_path);
        self.extract_derives_text_based(content, repository, file_path, &mut derives);
        derives
    }

    fn is_likely_problematic_file(&self, content: &str, file_path: &str) -> bool {
        // Skip files that are likely to cause parser issues
        
        // Skip test files in rust-lang/rust that are known to be problematic
        if file_path.contains("rust-lang/rust/tests/") {
            return true;
        }
        
        // Skip files that are too large (> 500KB)
        if content.len() > 500_000 {
            return true;
        }
        
        // Skip files with very deep nesting (heuristic: count braces)
        let open_braces = content.chars().filter(|&c| c == '{').count();
        let close_braces = content.chars().filter(|&c| c == '}').count();
        if open_braces > 1000 || close_braces > 1000 {
            return true;
        }
        
        // Skip files with excessive macro usage (heuristic)
        let macro_count = content.matches("macro_rules!").count() + 
                         content.matches("#[").count();
        if macro_count > 200 {
            return true;
        }
        
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_derive() {
        let parser = RustParser::new();
        let content = r#"
#[derive(Clone, Copy)]
struct MyStruct;
"#;
        let derives = parser.extract_derives(content, "test/repo", "src/lib.rs");
        assert_eq!(derives.len(), 1);
        assert_eq!(derives[0].derives, vec!["Clone", "Copy"]);
    }

    #[test]
    fn test_parse_complex_derive() {
        let parser = RustParser::new();
        let content = r#"
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComplexStruct {
    field: String,
}
"#;
        let derives = parser.extract_derives(content, "test/repo", "src/lib.rs");
        assert_eq!(derives.len(), 1);
        assert!(derives[0].derives.contains(&"Debug".to_string()));
        assert!(derives[0].derives.contains(&"Clone".to_string()));
    }
}
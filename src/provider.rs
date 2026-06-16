use crate::models::{ParsedProviderCall, Session};
use std::path::PathBuf;

pub trait Provider {
    /// Discover local session files for the provider.
    fn discover_sessions(&self) -> Vec<PathBuf>;
    
    /// Parse a single session file into standard ParsedProviderCalls.
    fn parse_session(&self, file_path: &PathBuf) -> Result<Session, Box<dyn std::error::Error>>;
}

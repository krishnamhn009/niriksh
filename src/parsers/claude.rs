use crate::models::{CarbonFootprint, Cost, ParsedProviderCall, Session, TokenUsage};
use crate::pricing::PricingEngine;
use crate::provider::Provider;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ClaudeParser {
    pub pricing_engine: PricingEngine,
}

impl ClaudeParser {
    fn get_claude_dirs(&self) -> Vec<PathBuf> {
        // Expand home directory logic
        let home = directories::UserDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("~"));

        let mut dirs = Vec::new();
        
        // Environment variable takes precedence
        if let Ok(config_dirs) = std::env::var("CLAUDE_CONFIG_DIRS") {
            for part in config_dirs.split(';') {
                let p = PathBuf::from(part.trim());
                if p.exists() {
                    dirs.push(p);
                }
            }
        }
        
        if dirs.is_empty() {
            let default_dir = home.join(".claude");
            if default_dir.exists() {
                dirs.push(default_dir);
            }
        }
        
        dirs
    }
}

impl Provider for ClaudeParser {
    fn discover_sessions(&self) -> Vec<PathBuf> {
        let mut sources = Vec::new();
        let dirs = self.get_claude_dirs();
        
        for dir in dirs {
            let projects_dir = dir.join("projects");
            if projects_dir.exists() && projects_dir.is_dir() {
                if let Ok(entries) = fs::read_dir(projects_dir) {
                    for entry in entries.flatten() {
                        if entry.path().is_dir() {
                            // Find JSONL files in the project dir
                            if let Ok(project_entries) = fs::read_dir(entry.path()) {
                                for proj_entry in project_entries.flatten() {
                                    if proj_entry.path().extension().and_then(|e| e.to_str()) == Some("jsonl") {
                                        sources.push(proj_entry.path());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        sources
    }

    fn parse_session(&self, file_path: &PathBuf) -> Result<Session, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(file_path)?;
        
        let mut parsed_calls = Vec::new();
        let mut total_input = 0;
        let mut total_output = 0;
        let mut session_cost = 0.0;
        
        // Example parsing line by line for Claude JSONL
        for line in content.lines() {
            if line.trim().is_empty() { continue; }
            if let Ok(json) = serde_json::from_str::<Value>(line) {
                if let Some(role) = json.get("role").and_then(|r| r.as_str()) {
                    if role == "assistant" {
                        let model = json.get("model").and_then(|m| m.as_str()).unwrap_or("claude-3-5-sonnet-20241022").to_string();
                        
                        let mut call_input = 0;
                        let mut call_output = 0;
                        
                        if let Some(usage) = json.get("usage") {
                            call_input = usage.get("input_tokens").and_then(|i| i.as_u64()).unwrap_or(0);
                            call_output = usage.get("output_tokens").and_then(|o| o.as_u64()).unwrap_or(0);
                        }
                        
                        let cost = self.pricing_engine.calculate_cost(&model, call_input, call_output, 0, 0);
                        
                        total_input += call_input;
                        total_output += call_output;
                        session_cost += cost;
                        
                        parsed_calls.push(ParsedProviderCall {
                            timestamp: "2026-06-15T00:00:00Z".to_string(), // Stub
                            model: model.clone(),
                            token_usage: TokenUsage {
                                input: call_input,
                                output: call_output,
                                cache_read: 0,
                                cache_write: 0,
                            },
                            cost: Cost {
                                amount_usd: cost,
                                currency: "USD".to_string(),
                            },
                        });
                    }
                }
            }
        }

        let project_name = file_path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(Session {
            id: file_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string(),
            provider_name: "claude".to_string(),
            project_path: project_name,
            total_cost: Cost {
                amount_usd: session_cost,
                currency: "USD".to_string(),
            },
            carbon_footprint: CarbonFootprint {
                estimated_gco2eq: 0.0,
            },
            total_token_usage: TokenUsage {
                input: total_input,
                output: total_output,
                cache_read: 0,
                cache_write: 0,
            },
            calls: parsed_calls,
        })
    }
}

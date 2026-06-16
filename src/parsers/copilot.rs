use crate::models::{CarbonFootprint, Cost, ParsedProviderCall, Session, TokenUsage};
use crate::pricing::PricingEngine;
use crate::provider::Provider;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub struct CopilotParser {
    pub pricing_engine: PricingEngine,
}

impl CopilotParser {
    fn get_vscode_workspace_storage_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(user_dirs) = directories::UserDirs::new() {
            let app_data = user_dirs.home_dir().join("AppData").join("Roaming");
            dirs.push(app_data.join("Code").join("User").join("workspaceStorage"));
            dirs.push(app_data.join("Code - Insiders").join("User").join("workspaceStorage"));
            dirs.push(app_data.join("VSCodium").join("User").join("workspaceStorage"));
        }
        dirs
    }

    fn get_copilot_session_state_dir() -> PathBuf {
        let home = directories::UserDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("~"));
        home.join(".copilot").join("session-state")
    }

    fn get_jetbrains_session_dir() -> PathBuf {
        let home = directories::UserDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("~"));
        home.join(".copilot").join("jb")
    }

    fn infer_model_from_events(events: &[Value]) -> String {
        for e in events {
            if let Some(data) = e.get("data") {
                if let Some(model) = data.get("model").and_then(|m| m.as_str()) {
                    return model.to_string();
                }
            }
        }
        "copilot-auto".to_string()
    }
}

impl Provider for CopilotParser {
    fn discover_sessions(&self) -> Vec<PathBuf> {
        let mut sources = Vec::new();

        // VS Code Transcripts
        for ws_dir in Self::get_vscode_workspace_storage_dirs() {
            if let Ok(entries) = fs::read_dir(ws_dir) {
                for entry in entries.flatten() {
                    let transcripts_dir = entry.path().join("GitHub.copilot-chat").join("transcripts");
                    if let Ok(files) = fs::read_dir(transcripts_dir) {
                        for file in files.flatten() {
                            if file.path().extension().and_then(|e| e.to_str()) == Some("jsonl") {
                                sources.push(file.path());
                            }
                        }
                    }
                }
            }
        }

        // Legacy standalone events
        if let Ok(entries) = fs::read_dir(Self::get_copilot_session_state_dir()) {
            for entry in entries.flatten() {
                let events_path = entry.path().join("events.jsonl");
                if events_path.is_file() {
                    sources.push(events_path);
                }
            }
        }

        // JetBrains events
        if let Ok(entries) = fs::read_dir(Self::get_jetbrains_session_dir()) {
            for entry in entries.flatten() {
                if let Ok(files) = fs::read_dir(entry.path()) {
                    for file in files.flatten() {
                        if file.path().extension().and_then(|e| e.to_str()) == Some("jsonl") {
                            sources.push(file.path());
                        }
                    }
                }
            }
        }

        sources
    }

    fn parse_session(&self, file_path: &PathBuf) -> Result<Session, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(file_path)?;
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.is_empty() {
            return Err("Empty session".into());
        }

        let mut events: Vec<Value> = Vec::new();
        for line in lines {
            if let Ok(event) = serde_json::from_str::<Value>(line) {
                events.push(event);
            }
        }

        let mut parsed_calls = Vec::new();
        let mut total_input = 0;
        let mut total_output = 0;
        let mut session_cost = 0.0;
        let mut session_id = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();

        let model = Self::infer_model_from_events(&events);
        let chars_per_token = 4;
        let mut pending_user_message = String::new();

        for event in events {
            let e_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let timestamp = event.get("timestamp").and_then(|t| t.as_str()).unwrap_or("");

            if e_type == "user.message" || e_type == "user.message_rendered" {
                if let Some(data) = event.get("data") {
                    let text = data.get("content").or(data.get("renderedMessage")).and_then(|c| c.as_str()).unwrap_or("");
                    pending_user_message = text.chars().take(500).collect();
                }
                continue;
            }

            if e_type == "assistant.message" {
                if let Some(data) = event.get("data") {
                    let content_text = data.get("text").or(data.get("content")).and_then(|c| c.as_str()).unwrap_or("");
                    let reasoning_text = data.get("reasoningText").and_then(|c| c.as_str()).unwrap_or("");
                    
                    let mut output_tokens = data.get("outputTokens").and_then(|t| t.as_u64()).unwrap_or(0);
                    let mut reasoning_tokens = 0;
                    
                    if output_tokens == 0 {
                        output_tokens = (content_text.len() as u64) / chars_per_token;
                        reasoning_tokens = (reasoning_text.len() as u64) / chars_per_token;
                    }
                    
                    if output_tokens == 0 && reasoning_tokens == 0 && data.get("toolRequests").is_none() {
                        continue;
                    }
                    
                    let input_tokens = (pending_user_message.len() as u64) / chars_per_token;
                    let cost = self.pricing_engine.calculate_cost(&model, input_tokens, output_tokens + reasoning_tokens, 0, 0);

                    total_input += input_tokens;
                    total_output += output_tokens + reasoning_tokens;
                    session_cost += cost;

                    parsed_calls.push(ParsedProviderCall {
                        timestamp: timestamp.to_string(),
                        model: model.clone(),
                        token_usage: TokenUsage {
                            input: input_tokens,
                            output: output_tokens + reasoning_tokens,
                            cache_read: 0,
                            cache_write: 0,
                        },
                        cost: Cost {
                            amount_usd: cost,
                            currency: "USD".to_string(),
                        },
                    });

                    pending_user_message = String::new();
                }
            }
        }

        let project_name = session_id.clone();

        Ok(Session {
            id: session_id,
            provider_name: "copilot".to_string(),
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

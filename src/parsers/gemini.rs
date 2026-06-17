use crate::models::{CarbonFootprint, Cost, ParsedProviderCall, Session, TokenUsage};
use crate::pricing::PricingEngine;
use crate::provider::Provider;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

pub struct GeminiParser {
    pub pricing_engine: PricingEngine,
}

impl GeminiParser {
    fn get_gemini_tmp_dir(&self) -> PathBuf {
        let home = directories::UserDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("~"));
        home.join(".gemini").join("tmp")
    }
}

impl Provider for GeminiParser {
    fn discover_sessions(&self) -> Vec<PathBuf> {
        let mut sources = Vec::new();
        let tmp_dir = self.get_gemini_tmp_dir();

        if let Ok(entries) = fs::read_dir(&tmp_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let chats_dir = entry.path().join("chats");
                    if let Ok(chat_entries) = fs::read_dir(chats_dir) {
                        for chat_entry in chat_entries.flatten() {
                            if let Ok(file_type) = chat_entry.file_type() {
                                if file_type.is_file() {
                                    let file_name = chat_entry.file_name().into_string().unwrap_or_default();
                                    if file_name.starts_with("session-") && (file_name.ends_with(".json") || file_name.ends_with(".jsonl")) {
                                        sources.push(chat_entry.path());
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
        
        // Parse either JSON or JSONL (as seen in `gemini.ts`)
        let mut messages = Vec::new();
        let mut session_id = String::new();
        let mut start_time = String::new();
        
        if let Ok(parsed) = serde_json::from_str::<Value>(&content) {
            if let Some(sess_id) = parsed.get("sessionId").and_then(|s| s.as_str()) {
                session_id = sess_id.to_string();
            }
            if let Some(st) = parsed.get("startTime").and_then(|s| s.as_str()) {
                start_time = st.to_string();
            }
            if let Some(msg_array) = parsed.get("messages").and_then(|m| m.as_array()) {
                messages = msg_array.clone();
            }
        } else {
            // JSONL parse
            for line in content.lines() {
                if line.trim().is_empty() { continue; }
                if let Ok(obj) = serde_json::from_str::<Value>(line) {
                    if obj.get("$set").is_some() { continue; }
                    if let Some(sess_id) = obj.get("sessionId").and_then(|s| s.as_str()) {
                        if session_id.is_empty() {
                            session_id = sess_id.to_string();
                            if let Some(st) = obj.get("startTime").and_then(|s| s.as_str()) {
                                start_time = st.to_string();
                            }
                        }
                    } else if obj.get("id").is_some() && obj.get("type").is_some() {
                        messages.push(obj);
                    }
                }
            }
        }

        if session_id.is_empty() || messages.is_empty() {
            return Err("Invalid Gemini session format".into());
        }

        let mut parsed_calls = Vec::new();
        let mut total_input = 0;
        let mut total_output = 0;
        let mut total_cached = 0;
        let mut session_cost = 0.0;
        let mut last_user_message = String::new();

        let mut seen_keys = std::collections::HashSet::new();
        let mut gemini_ordinal = 0;

        for msg in messages {
            if let Some(msg_type) = msg.get("type").and_then(|t| t.as_str()) {
                if msg_type == "user" {
                    if let Some(content) = msg.get("content") {
                        if let Some(text) = content.as_str() {
                            last_user_message = text.chars().take(500).collect();
                        } else if let Some(arr) = content.as_array() {
                            let mut text = String::new();
                            for item in arr {
                                if let Some(t) = item.get("text").and_then(|txt| txt.as_str()) {
                                    text.push_str(t);
                                    text.push(' ');
                                }
                            }
                            last_user_message = text.chars().take(500).collect();
                        }
                    }
                    continue;
                }

                if msg_type != "gemini" { continue; }

                let tokens = match msg.get("tokens") {
                    Some(t) => t,
                    None => continue,
                };
                
                let model = match msg.get("model").and_then(|m| m.as_str()) {
                    Some(m) if !m.is_empty() => m,
                    _ => continue,
                };
                
                let input = tokens.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
                let output = tokens.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
                let cached = tokens.get("cached").and_then(|v| v.as_u64()).unwrap_or(0);
                let thoughts = tokens.get("thoughts").and_then(|v| v.as_u64()).unwrap_or(0);
                
                if input == 0 && output == 0 && cached == 0 && thoughts == 0 { continue; }

                // Deduplication check using the same format as gemini.ts
                let msg_id = msg.get("id").and_then(|i| i.as_str()).map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        let key = format!("idx-{}", gemini_ordinal);
                        gemini_ordinal += 1;
                        key
                    });
                
                let dedup_key = format!("gemini:{}:{}", session_id, msg_id);
                if seen_keys.contains(&dedup_key) {
                    continue;
                }
                seen_keys.insert(dedup_key.clone());

                // Timestamp validation and fallback to start_time
                let ts_str = msg.get("timestamp")
                    .and_then(|t| t.as_str())
                    .filter(|t| !t.is_empty())
                    .unwrap_or(&start_time);

                let parsed_dt = if !ts_str.is_empty() {
                    if let Ok(dt) = ts_str.parse::<DateTime<Utc>>() {
                        Some(dt)
                    } else if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts_str) {
                        Some(dt.with_timezone(&Utc))
                    } else {
                        None
                    }
                } else {
                    None
                };

                let dt = match parsed_dt {
                    Some(dt) if dt.timestamp_millis() >= 1_000_000_000_000 => dt,
                    _ => continue,
                };

                // Gemini's `input` includes `cached`
                let fresh_input = input.saturating_sub(cached);
                
                let cost = self.pricing_engine.calculate_cost(model, fresh_input, output + thoughts, cached, 0);

                total_input += fresh_input;
                total_output += output;
                total_cached += cached;
                session_cost += cost;

                parsed_calls.push(ParsedProviderCall {
                    timestamp: dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                    model: model.to_string(),
                    token_usage: TokenUsage {
                        input: fresh_input,
                        output,
                        cache_read: cached,
                        cache_write: 0,
                    },
                    cost: Cost {
                        amount_usd: cost,
                        currency: "USD".to_string(),
                    },
                });
            }
        }

        let project_name = file_path.parent()
            .and_then(|p| p.parent()) // Move up from /chats
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(Session {
            id: session_id,
            provider_name: "gemini".to_string(),
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
                cache_read: total_cached,
                cache_write: 0,
            },
            calls: parsed_calls,
        })
    }
}

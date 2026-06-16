use crate::models::{CarbonFootprint, Cost, ParsedProviderCall, Session, TokenUsage};
use crate::pricing::PricingEngine;
use crate::provider::Provider;
use rusqlite::Connection;
use std::path::PathBuf;

pub struct CursorParser {
    pub pricing_engine: PricingEngine,
}

impl CursorParser {
    fn get_cursor_db_path(&self) -> PathBuf {
        let home = directories::UserDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("~"));

        // Approximate for Windows as an example
        home.join("AppData")
            .join("Roaming")
            .join("Cursor")
            .join("User")
            .join("globalStorage")
            .join("state.vscdb")
    }
}

impl Provider for CursorParser {
    fn discover_sessions(&self) -> Vec<PathBuf> {
        let db_path = self.get_cursor_db_path();
        if db_path.exists() {
            vec![db_path]
        } else {
            vec![]
        }
    }

    fn parse_session(&self, file_path: &PathBuf) -> Result<Session, Box<dyn std::error::Error>> {
        let conn = Connection::open(file_path)?;
        
        let mut parsed_calls = Vec::new();
        let mut total_input = 0;
        let mut total_output = 0;
        let mut session_cost = 0.0;

        // Stubbed query logic since rusqlite requires matching schema
        // Cursor's `cursorDiskKV` table schema:
        let mut stmt = conn.prepare("SELECT key, value FROM cursorDiskKV WHERE key LIKE 'bubbleId:%'")?;
        let rows = stmt.query_map([], |row| {
            let key: String = row.get(0)?;
            let value: String = row.get(1)?;
            Ok((key, value))
        })?;

        for row in rows {
            if let Ok((_key, value_str)) = row {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&value_str) {
                    if let Some(token_count) = json.get("tokenCount") {
                        let input = token_count.get("inputTokens").and_then(|i| i.as_u64()).unwrap_or(0);
                        let output = token_count.get("outputTokens").and_then(|o| o.as_u64()).unwrap_or(0);
                        
                        let model_name = json.get("modelInfo")
                            .and_then(|m| m.get("modelName"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("claude-3-5-sonnet-20241022");
                        
                        let cost = self.pricing_engine.calculate_cost(model_name, input, output, 0, 0);
                        
                        total_input += input;
                        total_output += output;
                        session_cost += cost;
                        
                        parsed_calls.push(ParsedProviderCall {
                            timestamp: "2026-06-15T00:00:00Z".to_string(), // Stub
                            model: model_name.to_string(),
                            token_usage: TokenUsage {
                                input,
                                output,
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

        Ok(Session {
            id: "cursor-global-session".to_string(),
            provider_name: "cursor".to_string(),
            project_path: "globalStorage".to_string(),
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

mod cli;
mod filters;
mod models;
mod parsers;
mod pricing;
mod provider;

mod tui;

use clap::Parser;
use cli::{Cli, Commands};
use parsers::gemini::GeminiParser;
use parsers::claude::ClaudeParser;
use parsers::cursor::CursorParser;
use parsers::copilot::CopilotParser;
use parsers::antigravity::AntigravityParser;
use pricing::PricingEngine;
use provider::Provider;
use std::path::PathBuf;
use std::fs::File;
use std::io::Write;
use tui::{DashboardData, ProviderSummary, run_app};

fn main() {
    let args = Cli::parse();

    match &args.command {
        Commands::Status { provider } => {
            println!("Calculating status...");
            if let Some(p) = provider {
                println!("Filtering by provider: {}", p);
            }

            let pricing_engine: PricingEngine = PricingEngine::new();
            
            // 1. Gemini Stub
            let stub_path = PathBuf::from("stub_session.json");
            let stub_json = r#"{
                "history": [
                    { "role": "user", "parts": ["hello"] },
                    { "role": "model", "token_usage": { "input": 100, "output": 50 } }
                ]
            }"#;
            
            if let Ok(mut file) = File::create(&stub_path) {
                let _ = file.write_all(stub_json.as_bytes());
            }

            let gemini_parser = GeminiParser { pricing_engine: PricingEngine::new() };
            if let Ok(session) = gemini_parser.parse_session(&stub_path) {
                println!("--- [Gemini] ---");
                println!("Parsed session: {}", session.id);
                println!("Total Tokens (Input): {}", session.total_token_usage.input);
                println!("Total Cost: ${:.6} {}", session.total_cost.amount_usd, session.total_cost.currency);
            }
            let _ = std::fs::remove_file(&stub_path);

            // 2. Claude Discovery
            println!("\n--- [Claude] ---");
            let claude_parser = ClaudeParser { pricing_engine: PricingEngine::new() };
            let claude_sessions = claude_parser.discover_sessions();
            println!("Discovered {} Claude session files.", claude_sessions.len());
            for s in claude_sessions.iter().take(2) { 
                if let Ok(sess) = claude_parser.parse_session(s) {
                    println!("Parsed Claude Session: {}, Cost: ${:.6}", sess.project_path, sess.total_cost.amount_usd);
                }
            }

            // 3. Cursor Discovery
            println!("\n--- [Cursor] ---");
            let cursor_parser = CursorParser { pricing_engine: PricingEngine::new() };
            let cursor_sessions = cursor_parser.discover_sessions();
            println!("Discovered {} Cursor DBs.", cursor_sessions.len());
            for s in cursor_sessions.iter().take(1) {
                if let Ok(sess) = cursor_parser.parse_session(s) {
                    println!("Parsed Cursor Session DB, total known cost: ${:.6}", sess.total_cost.amount_usd);
                } else {
                    println!("Could not parse Cursor DB (schema mismatch or locked).");
                }
            }
            // 4. Antigravity Discovery
            println!("\n--- [Antigravity] ---");
            let antigravity_parser = AntigravityParser { pricing_engine: PricingEngine::new() };
            let antigravity_sessions = antigravity_parser.discover_sessions();
            println!("Discovered {} Antigravity sessions/status files.", antigravity_sessions.len());
            for s in antigravity_sessions.iter().take(2) {
                if let Ok(sess) = antigravity_parser.parse_session(s) {
                    println!("Parsed Antigravity Session: {}, Cost: ${:.6}", sess.id, sess.total_cost.amount_usd);
                } else {
                    println!("Could not parse Antigravity session/status.");
                }
            }
        }
        Commands::Report { period } => {
            use crate::filters::Filters;
            use crate::models::CarbonFootprint;
            
            let filters = Filters::new(period.clone(), None);

            // Aggregate all data
            let mut providers = Vec::new();
            let mut total_cost = 0.0;
            let mut total_input = 0;
            let mut total_output = 0;

            let mut process_session = |provider_name: &str, session: &mut crate::models::Session, 
                                       p_sessions: &mut usize, p_in: &mut u64, p_out: &mut u64, p_cost: &mut f64| {
                let mut valid_calls = 0;
                for call in &session.calls {
                    if filters.is_allowed(provider_name, call) {
                        *p_in += call.token_usage.input;
                        *p_out += call.token_usage.output;
                        *p_cost += call.cost.amount_usd;
                        valid_calls += 1;
                    }
                }
                if valid_calls > 0 {
                    *p_sessions += 1;
                }
            };

            // Gemini
            let gemini_parser = GeminiParser { pricing_engine: PricingEngine::new() };
            let gemini_sessions = gemini_parser.discover_sessions();
            let mut ge_sessions = 0; let mut ge_in = 0; let mut ge_out = 0; let mut ge_cost = 0.0;
            for s in &gemini_sessions {
                if let Ok(mut sess) = gemini_parser.parse_session(s) {
                    process_session("Gemini", &mut sess, &mut ge_sessions, &mut ge_in, &mut ge_out, &mut ge_cost);
                }
            }
            if ge_sessions > 0 {
                providers.push(ProviderSummary {
                    name: "Gemini CLI".to_string(),
                    sessions_count: ge_sessions,
                    total_input_tokens: ge_in,
                    total_output_tokens: ge_out,
                    total_cost: ge_cost,
                });
                total_input += ge_in; total_output += ge_out; total_cost += ge_cost;
            }

            // Claude
            let claude_parser = ClaudeParser { pricing_engine: PricingEngine::new() };
            let claude_sessions = claude_parser.discover_sessions();
            let mut cl_sessions = 0; let mut cl_in = 0; let mut cl_out = 0; let mut cl_cost = 0.0;
            for s in &claude_sessions {
                if let Ok(mut sess) = claude_parser.parse_session(s) {
                    process_session("Claude", &mut sess, &mut cl_sessions, &mut cl_in, &mut cl_out, &mut cl_cost);
                }
            }
            if cl_sessions > 0 {
                providers.push(ProviderSummary {
                    name: "Claude".to_string(),
                    sessions_count: cl_sessions,
                    total_input_tokens: cl_in,
                    total_output_tokens: cl_out,
                    total_cost: cl_cost,
                });
                total_input += cl_in; total_output += cl_out; total_cost += cl_cost;
            }

            // Cursor
            let cursor_parser = CursorParser { pricing_engine: PricingEngine::new() };
            let cursor_sessions = cursor_parser.discover_sessions();
            let mut cu_sessions = 0; let mut cu_in = 0; let mut cu_out = 0; let mut cu_cost = 0.0;
            for s in &cursor_sessions {
                if let Ok(mut sess) = cursor_parser.parse_session(s) {
                    process_session("Cursor", &mut sess, &mut cu_sessions, &mut cu_in, &mut cu_out, &mut cu_cost);
                }
            }
            if cu_sessions > 0 {
                providers.push(ProviderSummary {
                    name: "Cursor".to_string(),
                    sessions_count: cu_sessions,
                    total_input_tokens: cu_in,
                    total_output_tokens: cu_out,
                    total_cost: cu_cost,
                });
                total_input += cu_in; total_output += cu_out; total_cost += cu_cost;
            }

            // Copilot
            let copilot_parser = CopilotParser { pricing_engine: PricingEngine::new() };
            let copilot_sessions = copilot_parser.discover_sessions();
            let mut cp_sessions = 0; let mut cp_in = 0; let mut cp_out = 0; let mut cp_cost = 0.0;
            for s in &copilot_sessions {
                if let Ok(mut sess) = copilot_parser.parse_session(s) {
                    process_session("Copilot", &mut sess, &mut cp_sessions, &mut cp_in, &mut cp_out, &mut cp_cost);
                }
            }
            if cp_sessions > 0 {
                providers.push(ProviderSummary {
                    name: "Copilot".to_string(),
                    sessions_count: cp_sessions,
                    total_input_tokens: cp_in,
                    total_output_tokens: cp_out,
                    total_cost: cp_cost,
                });
                total_input += cp_in; total_output += cp_out; total_cost += cp_cost;
            }

            // Antigravity
            let antigravity_parser = AntigravityParser { pricing_engine: PricingEngine::new() };
            let antigravity_sessions = antigravity_parser.discover_sessions();
            let mut ag_sessions = 0; let mut ag_in = 0; let mut ag_out = 0; let mut ag_cost = 0.0;
            for s in &antigravity_sessions {
                if let Ok(mut sess) = antigravity_parser.parse_session(s) {
                    process_session("Antigravity", &mut sess, &mut ag_sessions, &mut ag_in, &mut ag_out, &mut ag_cost);
                }
            }
            if ag_sessions > 0 {
                providers.push(ProviderSummary {
                    name: "Antigravity".to_string(),
                    sessions_count: ag_sessions,
                    total_input_tokens: ag_in,
                    total_output_tokens: ag_out,
                    total_cost: ag_cost,
                });
                total_input += ag_in; total_output += ag_out; total_cost += ag_cost;
            }

            let data = DashboardData {
                period: period.clone(),
                total_cost,
                total_input_tokens: total_input,
                total_output_tokens: total_output,
                carbon_footprint_gco2eq: CarbonFootprint::estimate_gco2eq(total_input + total_output),
                providers,
            };

            if let Err(e) = run_app(data) {
                eprintln!("Error running TUI: {}", e);
            }
        }
    }
}

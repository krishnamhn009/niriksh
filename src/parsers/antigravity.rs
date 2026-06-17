use crate::models::{CarbonFootprint, Cost, ParsedProviderCall, Session, TokenUsage};
use crate::pricing::PricingEngine;
use crate::provider::Provider;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::runtime::Runtime;

#[derive(Serialize, Deserialize, Clone)]
struct CachedCascade {
    mtime_ms: u64,
    size_bytes: u64,
    calls: Vec<ParsedProviderCall>,
}

#[derive(Serialize, Deserialize, Clone)]
struct AntigravityCache {
    version: u32,
    cascades: HashMap<String, CachedCascade>,
}

#[derive(Clone, Debug)]
struct ServerInfo {
    port: u16,
    csrf_token: String,
    app_data_dir: Option<String>,
}

pub struct AntigravityParser {
    pub pricing_engine: PricingEngine,
}

impl AntigravityParser {
    fn cache_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("NIRIKSH_CACHE_DIR") {
            PathBuf::from(dir)
        } else {
            let home = directories::UserDirs::new()
                .map(|dirs| dirs.home_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("~"));
            home.join(".cache").join("niriksh")
        }
    }

    fn cache_path() -> PathBuf {
        Self::cache_dir().join("antigravity-results.json")
    }

    fn statusline_path() -> PathBuf {
        Self::cache_dir().join("antigravity-statusline.jsonl")
    }

    fn get_flag_value(line: &str, flags: &[&str]) -> Option<String> {
        for flag in flags {
            let prefix = format!("--{}", flag);
            if let Some(idx) = line.find(&prefix) {
                let rest = &line[idx + prefix.len()..];
                let val_start = if rest.starts_with('=') {
                    rest[1..].trim_start()
                } else {
                    rest.trim_start()
                };
                
                let mut val = String::new();
                let mut in_quotes = false;
                let mut quote_char = ' ';
                
                for c in val_start.chars() {
                    if !in_quotes && (c == '"' || c == '\'') {
                        in_quotes = true;
                        quote_char = c;
                    } else if in_quotes && c == quote_char {
                        break;
                    } else if !in_quotes && c.is_whitespace() {
                        break;
                    } else {
                        val.push(c);
                    }
                }
                
                if !val.is_empty() && !val.starts_with("--") {
                    return Some(val);
                }
            }
        }
        None
    }

    fn test_port(port: u16, csrf_token: &str) -> bool {
        let rt = match Runtime::new() {
            Ok(r) => r,
            Err(_) => return false,
        };
        let client = match reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_millis(1000))
            .build() {
                Ok(c) => c,
                Err(_) => return false,
            };
        let url = format!("https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/GetAvailableModels", port);
        
        let res = rt.block_on(async {
            client.post(&url)
                .header("Content-Type", "application/json")
                .header("Connect-Protocol-Version", "1")
                .header("X-Codeium-Csrf-Token", csrf_token)
                .body("{}")
                .send()
                .await
        });
        
        if let Ok(response) = res {
            response.status() == reqwest::StatusCode::OK
        } else {
            false
        }
    }

    fn get_process_ports_windows(pid: u32) -> Vec<u16> {
        let script = format!("Get-NetTCPConnection -State Listen -OwningProcess {} | Select-Object -ExpandProperty LocalPort", pid);
        if let Ok(output) = Command::new("powershell.exe")
            .args(&["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
            .output()
        {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                return stdout.lines()
                    .filter_map(|line| line.trim().parse::<u16>().ok())
                    .collect();
            }
        }
        Vec::new()
    }

    fn get_process_ports_unix(pid: u32) -> Vec<u16> {
        if let Ok(output) = Command::new("lsof")
            .args(&["-a", "-i", "-P", "-n", "-p", &pid.to_string()])
            .output()
        {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                let mut ports = Vec::new();
                for line in stdout.lines() {
                    if line.contains("LISTEN") {
                        if let Some(idx) = line.rfind(':') {
                            let after_colon = &line[idx + 1..];
                            let port_str: String = after_colon.chars().take_while(|c| c.is_ascii_digit()).collect();
                            if let Ok(port) = port_str.parse::<u16>() {
                                ports.push(port);
                            }
                        }
                    }
                }
                return ports;
            }
        }
        Vec::new()
    }

    fn detect_servers() -> Vec<ServerInfo> {
        let mut servers = Vec::new();
        let mut processes = Vec::new();

        if cfg!(windows) {
            let script = "$ErrorActionPreference = 'SilentlyContinue'; [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine -like '*language_server*' -and $_.CommandLine -like '*antigravity*' } | ForEach-Object { @{ PID = $_.ProcessId; Cmd = $_.CommandLine } | ConvertTo-Json -Compress }";
            if let Ok(output) = Command::new("powershell.exe")
                .args(&["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
                .output()
            {
                if let Ok(stdout) = String::from_utf8(output.stdout) {
                    for line in stdout.lines() {
                        if line.trim().is_empty() { continue; }
                        if let Ok(v) = serde_json::from_str::<Value>(line) {
                            if let (Some(pid), Some(cmd)) = (v.get("PID").and_then(|p| p.as_u64()), v.get("Cmd").and_then(|c| c.as_str())) {
                                processes.push((pid as u32, cmd.to_string()));
                            }
                        }
                    }
                }
            }
        } else {
            if let Ok(output) = Command::new("ps").args(&["-ww", "-eo", "pid=,args="]).output() {
                if let Ok(stdout) = String::from_utf8(output.stdout) {
                    for line in stdout.lines() {
                        let line = line.trim();
                        if line.is_empty() { continue; }
                        let lower = line.to_lowercase();
                        if lower.contains("language_server") && lower.contains("antigravity") {
                            if let Some(space_idx) = line.find(' ') {
                                if let Ok(pid) = line[..space_idx].parse::<u32>() {
                                    processes.push((pid, line[space_idx + 1..].to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }

        let port_flags = ["https_server_port", "extension_server_port", "https-server-port", "extension-server-port"];
        let csrf_flags = ["csrf_token", "extension_server_csrf_token", "csrf-token", "extension-server-csrf-token"];
        let app_data_dir_flags = ["app_data_dir", "app-data-dir"];

        for (pid, cmd) in processes {
            let port_str = Self::get_flag_value(&cmd, &port_flags);
            let csrf = Self::get_flag_value(&cmd, &csrf_flags);
            let app_data_dir = Self::get_flag_value(&cmd, &app_data_dir_flags).map(|s| {
                let lower = s.replace("\\", "/").to_lowercase();
                if lower.contains("antigravity-ide") { "antigravity-ide".to_string() }
                else if lower.contains("antigravity-cli") { "antigravity-cli".to_string() }
                else { "antigravity".to_string() }
            });

            if let Some(csrf_token) = csrf {
                let mut resolved_port = 0;

                if app_data_dir.as_deref() != Some("antigravity-ide") {
                    if let Some(p_str) = port_str {
                        if let Ok(p) = p_str.parse::<u16>() {
                            resolved_port = p;
                        }
                    }
                }

                if resolved_port == 0 {
                    let ports = if cfg!(windows) {
                        Self::get_process_ports_windows(pid)
                    } else {
                        Self::get_process_ports_unix(pid)
                    };
                    for p in ports {
                        if Self::test_port(p, &csrf_token) {
                            resolved_port = p;
                            break;
                        }
                    }
                }

                if resolved_port > 0 {
                    servers.push(ServerInfo {
                        port: resolved_port,
                        csrf_token,
                        app_data_dir,
                    });
                }
            }
        }
        servers
    }

    fn normalize_pricing_model(model: &str) -> String {
        let mut stripped = model.replace("-high", "").replace("-medium", "").replace("-low", "").replace("-agent", "");
        if stripped == "gemini-pro" {
            stripped = "gemini-3.1-pro".to_string();
        }
        stripped
    }

    fn load_cache() -> AntigravityCache {
        if let Ok(content) = fs::read_to_string(Self::cache_path()) {
            if let Ok(cache) = serde_json::from_str::<AntigravityCache>(&content) {
                if cache.version == 2 {
                    return cache;
                }
            }
        }
        AntigravityCache { version: 2, cascades: HashMap::new() }
    }

    fn save_cache(cache: &AntigravityCache) {
        let _ = fs::create_dir_all(Self::cache_dir());
        if let Ok(content) = serde_json::to_string(cache) {
            let _ = fs::write(Self::cache_path(), content);
        }
    }

    fn fetch_cascade(server: &ServerInfo, cascade_id: &str) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let rt = Runtime::new()?;
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()?;
        
        let url = format!("https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/GetCascadeTrajectoryGeneratorMetadata", server.port);
        let body = json!({ "cascadeId": cascade_id });

        let res = rt.block_on(async {
            client.post(&url)
                .header("Content-Type", "application/json")
                .header("Connect-Protocol-Version", "1")
                .header("X-Codeium-Csrf-Token", &server.csrf_token)
                .json(&body)
                .send()
                .await
        })?;

        let json: Value = rt.block_on(async { res.json().await })?;
        
        let metadata = json.get("response")
            .and_then(|r| r.get("generatorMetadata"))
            .or_else(|| json.get("generatorMetadata"))
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(metadata)
    }

    fn parse_statusline(&self, path: &PathBuf) -> Vec<ParsedProviderCall> {
        let mut calls = Vec::new();
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                if line.trim().is_empty() { continue; }
                if let Ok(v) = serde_json::from_str::<Value>(line) {
                    if let (Some(model), Some(usage)) = (v.get("model").and_then(|m| m.as_str()), v.get("usage")) {
                        let input = usage.get("inputTokens").and_then(|i| i.as_u64()).unwrap_or(0);
                        let output = usage.get("outputTokens").and_then(|o| o.as_u64()).unwrap_or(0);
                        let cache_read = usage.get("cacheReadInputTokens").and_then(|c| c.as_u64()).unwrap_or(0);
                        
                        if input > 0 || output > 0 {
                            let pricing_model = Self::normalize_pricing_model(model);
                            let cost = self.pricing_engine.calculate_cost(&pricing_model, input, output, cache_read, 0);
                            calls.push(ParsedProviderCall {
                                timestamp: v.get("at").and_then(|a| a.as_str()).unwrap_or("").to_string(),
                                model: pricing_model,
                                token_usage: TokenUsage { input, output, cache_read, cache_write: 0 },
                                cost: Cost { amount_usd: cost, currency: "USD".to_string() },
                            });
                        }
                    }
                }
            }
        }
        calls
    }
}

impl Provider for AntigravityParser {
    fn discover_sessions(&self) -> Vec<PathBuf> {
        let mut sources = Vec::new();
        let home = directories::UserDirs::new()
            .map(|dirs| dirs.home_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("~"));

        let dirs = vec![
            home.join(".gemini").join("antigravity").join("conversations"),
            home.join(".gemini").join("antigravity-cli").join("conversations"),
            home.join(".gemini").join("antigravity-cli").join("implicit"),
            home.join(".gemini").join("antigravity-ide").join("conversations"),
            home.join(".gemini").join("antigravity-ide").join("implicit"),
        ];

        for dir in dirs {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if ext == "pb" || ext == "db" {
                                sources.push(path);
                            }
                        }
                    }
                }
            }
        }

        let statusline = Self::statusline_path();
        if statusline.exists() {
            sources.push(statusline);
        }

        sources
    }

    fn parse_session(&self, file_path: &PathBuf) -> Result<Session, Box<dyn std::error::Error>> {
        let is_statusline = file_path.file_name().and_then(|n| n.to_str()) == Some("antigravity-statusline.jsonl");
        let mut session_cost = 0.0;
        let mut total_input = 0;
        let mut total_output = 0;
        let mut total_cache_read = 0;
        let mut calls = Vec::new();

        let cascade_id = if is_statusline {
            "antigravity-statusline".to_string()
        } else {
            file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string()
        };

        if is_statusline {
            calls = self.parse_statusline(file_path);
        } else {
            let mut cache = Self::load_cache();
            let metadata = fs::metadata(file_path)?;
            let mtime = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64;
            let size = metadata.len();

            let mut fetched_from_rpc = false;

            if let Some(cached) = cache.cascades.get(&cascade_id) {
                if cached.mtime_ms == mtime && cached.size_bytes == size {
                    calls = cached.calls.clone();
                    fetched_from_rpc = true; // Use cache as truth
                }
            }

            if !fetched_from_rpc {
                let servers = Self::detect_servers();
                let lower_path = file_path.to_string_lossy().to_lowercase();
                let app_dir = if lower_path.contains("antigravity-ide") { Some("antigravity-ide".to_string()) }
                              else if lower_path.contains("antigravity-cli") { Some("antigravity-cli".to_string()) }
                              else { Some("antigravity".to_string()) };

                let target_server = servers.iter().find(|s| s.app_data_dir == app_dir).or_else(|| servers.first());

                if let Some(server) = target_server {
                    if let Ok(metadata_array) = Self::fetch_cascade(server, &cascade_id) {
                        for item in metadata_array {
                            if let Some(chat_model) = item.get("chatModel") {
                                if let Some(usage) = chat_model.get("usage") {
                                    let model = usage.get("model").and_then(|m| m.as_str()).unwrap_or("unknown");
                                    let input_str = usage.get("inputTokens").and_then(|i| i.as_str()).unwrap_or("0");
                                    let output_str = usage.get("outputTokens").and_then(|o| o.as_str()).unwrap_or("0");
                                    let think_str = usage.get("thinkingOutputTokens").and_then(|o| o.as_str()).unwrap_or("0");

                                    let input: u64 = input_str.parse().unwrap_or(0);
                                    let output: u64 = output_str.parse().unwrap_or(0);
                                    let thinking: u64 = think_str.parse().unwrap_or(0);
                                    let total_out = output + thinking;

                                    if input > 0 || total_out > 0 {
                                        let pricing_model = Self::normalize_pricing_model(model);
                                        let cost = self.pricing_engine.calculate_cost(&pricing_model, input, total_out, 0, 0);
                                        
                                        let timestamp = chat_model.get("chatStartMetadata")
                                            .and_then(|m| m.get("createdAt"))
                                            .and_then(|t| t.as_str())
                                            .unwrap_or("")
                                            .to_string();

                                        calls.push(ParsedProviderCall {
                                            timestamp,
                                            model: pricing_model,
                                            token_usage: TokenUsage { input, output: total_out, cache_read: 0, cache_write: 0 },
                                            cost: Cost { amount_usd: cost, currency: "USD".to_string() },
                                        });
                                    }
                                }
                            }
                        }

                        // Save cache
                        cache.cascades.insert(cascade_id.clone(), CachedCascade {
                            mtime_ms: mtime,
                            size_bytes: size,
                            calls: calls.clone(),
                        });
                        Self::save_cache(&cache);
                        fetched_from_rpc = true;
                    }
                }
            }

            if !fetched_from_rpc && cache.cascades.contains_key(&cascade_id) {
                // Best effort fallback
                calls = cache.cascades.get(&cascade_id).unwrap().calls.clone();
            }
        }

        for call in &calls {
            session_cost += call.cost.amount_usd;
            total_input += call.token_usage.input;
            total_output += call.token_usage.output;
            total_cache_read += call.token_usage.cache_read;
        }

        Ok(Session {
            id: cascade_id,
            provider_name: "antigravity".to_string(),
            project_path: "unknown".to_string(), // can be parsed from sqlite if needed
            total_cost: Cost { amount_usd: session_cost, currency: "USD".to_string() },
            carbon_footprint: CarbonFootprint { estimated_gco2eq: 0.0 },
            total_token_usage: TokenUsage {
                input: total_input,
                output: total_output,
                cache_read: total_cache_read,
                cache_write: 0,
            },
            calls,
        })
    }
}

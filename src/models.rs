use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cost {
    pub amount_usd: f64,
    pub currency: String,
}

impl Default for Cost {
    fn default() -> Self {
        Self {
            amount_usd: 0.0,
            currency: "USD".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CarbonFootprint {
    pub estimated_gco2eq: f64,
}

impl CarbonFootprint {
    pub fn estimate_gco2eq(total_tokens: u64) -> f64 {
        // Simple rough industry estimate: 0.0002 gCO2eq per token
        (total_tokens as f64) * 0.0002
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedProviderCall {
    pub timestamp: String,
    pub model: String,
    pub token_usage: TokenUsage,
    pub cost: Cost,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub provider_name: String,
    pub project_path: String,
    pub total_cost: Cost,
    pub carbon_footprint: CarbonFootprint,
    pub total_token_usage: TokenUsage,
    pub calls: Vec<ParsedProviderCall>,
}

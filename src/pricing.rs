use serde_json::Value;
use std::collections::HashMap;

pub struct PricingEngine {
    prices: HashMap<String, ModelPrice>,
}

pub struct ModelPrice {
    pub input_cost_per_token: f64,
    pub output_cost_per_token: f64,
    pub cache_read_cost_per_token: f64,
    pub cache_write_cost_per_token: f64,
}

impl PricingEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            prices: HashMap::new(),
        };
        engine.load_embedded_pricing();
        engine
    }

    fn load_embedded_pricing(&mut self) {
        // Load the JSON fetched at compile time by build.rs
        let json_str = include_str!(concat!(env!("OUT_DIR"), "/litellm_pricing.json"));
        
        if let Ok(parsed) = serde_json::from_str::<Value>(json_str) {
            if let Some(obj) = parsed.as_object() {
                for (model_name, details) in obj {
                    if let Some(details_obj) = details.as_object() {
                        let input_cost = details_obj.get("input_cost_per_token").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let output_cost = details_obj.get("output_cost_per_token").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        // Some models might not define cache prices, default to standard input/output if missing, 
                        // or 0 depending on strategy. We'll use 0 or standard rates.
                        let cache_read = details_obj.get("cache_read_input_token_cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let cache_write = details_obj.get("cache_creation_input_token_cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        
                        self.prices.insert(model_name.clone(), ModelPrice {
                            input_cost_per_token: input_cost,
                            output_cost_per_token: output_cost,
                            cache_read_cost_per_token: cache_read,
                            cache_write_cost_per_token: cache_write,
                        });
                    }
                }
            }
        }
        
        // Add fallbacks if empty (e.g. build script failed)
        if self.prices.is_empty() {
            self.prices.insert(
                "gemini-1.5-pro".to_string(),
                ModelPrice {
                    input_cost_per_token: 0.0000035,
                    output_cost_per_token: 0.0000105,
                    cache_read_cost_per_token: 0.000001,
                    cache_write_cost_per_token: 0.0000035,
                },
            );
        }
    }

    pub fn calculate_cost(
        &self,
        model: &str,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
    ) -> f64 {
        if let Some(price) = self.prices.get(model) {
            (input as f64 * price.input_cost_per_token)
                + (output as f64 * price.output_cost_per_token)
                + (cache_read as f64 * price.cache_read_cost_per_token)
                + (cache_write as f64 * price.cache_write_cost_per_token)
        } else {
            0.0
        }
    }
}

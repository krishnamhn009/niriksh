use crate::models::Session;
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct ModelStats {
    pub provider: String,
    pub model: String,
    pub calls: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost_usd: f64,
}

pub fn run_models_report(sessions: &[Session]) {
    let mut stats_map: HashMap<String, ModelStats> = HashMap::new();

    for session in sessions {
        for call in &session.calls {
            let key = format!("{}:{}", session.provider_name, call.model);
            let stats = stats_map.entry(key).or_insert_with(|| ModelStats {
                provider: session.provider_name.clone(),
                model: call.model.clone(),
                ..Default::default()
            });

            stats.calls += 1;
            stats.input_tokens += call.token_usage.input;
            stats.output_tokens += call.token_usage.output;
            stats.cache_read_tokens += call.token_usage.cache_read;
            stats.cost_usd += call.cost.amount_usd;
        }
    }

    if stats_map.is_empty() {
        println!("\n  No model usage data found.\n");
        return;
    }

    let mut stats_list: Vec<ModelStats> = stats_map.into_values().collect();
    stats_list.sort_by(|a, b| b.cost_usd.partial_cmp(&a.cost_usd).unwrap_or(std::cmp::Ordering::Equal));

    println!("\n┌──────────────────────────────┬─────────┬──────────────┬──────────────┬──────────────┬──────────────┐");
    println!("│ Model                        │ Calls   │ Input Tokens │ Output Tokens│ Cache Read   │ Cost (USD)   │");
    println!("├──────────────────────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┤");

    let mut total_calls = 0;
    let mut total_input = 0;
    let mut total_output = 0;
    let mut total_cache = 0;
    let mut total_cost = 0.0;

    for stats in &stats_list {
        let display_name = if stats.model.len() > 28 {
            format!("{}...", &stats.model[..25])
        } else {
            stats.model.clone()
        };

        println!(
            "│ {:<28} │ {:>7} │ {:>12} │ {:>12} │ {:>12} │ ${:>11.6} │",
            display_name,
            stats.calls,
            stats.input_tokens,
            stats.output_tokens,
            stats.cache_read_tokens,
            stats.cost_usd
        );

        total_calls += stats.calls;
        total_input += stats.input_tokens;
        total_output += stats.output_tokens;
        total_cache += stats.cache_read_tokens;
        total_cost += stats.cost_usd;
    }

    println!("├──────────────────────────────┼─────────┼──────────────┼──────────────┼──────────────┼──────────────┤");
    println!(
        "│ {:<28} │ {:>7} │ {:>12} │ {:>12} │ {:>12} │ ${:>11.6} │",
        "Total", total_calls, total_input, total_output, total_cache, total_cost
    );
    println!("└──────────────────────────────┴─────────┴──────────────┴──────────────┴──────────────┴──────────────┘\n");
}

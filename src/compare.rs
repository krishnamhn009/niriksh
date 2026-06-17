use crate::models::Session;
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
struct CompareStats {
    pub model: String,
    pub calls: usize,
    pub cost: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
}

pub fn run_compare(sessions: &[Session]) {
    let mut stats_map: HashMap<String, CompareStats> = HashMap::new();

    for session in sessions {
        for call in &session.calls {
            let stats = stats_map.entry(call.model.clone()).or_insert_with(|| CompareStats {
                model: call.model.clone(),
                ..Default::default()
            });

            stats.calls += 1;
            stats.input_tokens += call.token_usage.input;
            stats.output_tokens += call.token_usage.output;
            stats.cache_read_tokens += call.token_usage.cache_read;
            stats.cost += call.cost.amount_usd;
        }
    }

    let mut stats_list: Vec<CompareStats> = stats_map.into_values().collect();
    stats_list.sort_by(|a, b| b.calls.cmp(&a.calls).then_with(|| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal)));

    if stats_list.len() < 2 {
        println!("\n  Need at least 2 models to compare. Found {}.\n", stats_list.len());
        return;
    }

    let a = &stats_list[0];
    let b = &stats_list[1];

    let avg_cost_a = if a.calls > 0 { a.cost / (a.calls as f64) } else { 0.0 };
    let avg_cost_b = if b.calls > 0 { b.cost / (b.calls as f64) } else { 0.0 };

    let avg_in_a = if a.calls > 0 { (a.input_tokens as f64) / (a.calls as f64) } else { 0.0 };
    let avg_in_b = if b.calls > 0 { (b.input_tokens as f64) / (b.calls as f64) } else { 0.0 };

    let avg_out_a = if a.calls > 0 { (a.output_tokens as f64) / (a.calls as f64) } else { 0.0 };
    let avg_out_b = if b.calls > 0 { (b.output_tokens as f64) / (b.calls as f64) } else { 0.0 };

    let cache_ratio_a = if a.input_tokens + a.cache_read_tokens > 0 {
        (a.cache_read_tokens as f64) / ((a.input_tokens + a.cache_read_tokens) as f64) * 100.0
    } else {
        0.0
    };
    let cache_ratio_b = if b.input_tokens + b.cache_read_tokens > 0 {
        (b.cache_read_tokens as f64) / ((b.input_tokens + b.cache_read_tokens) as f64) * 100.0
    } else {
        0.0
    };

    let name_a = if a.model.len() > 22 { format!("{}...", &a.model[..19]) } else { a.model.clone() };
    let name_b = if b.model.len() > 22 { format!("{}...", &b.model[..19]) } else { b.model.clone() };

    println!("\n┌──────────────────────────────┬────────────────────────┬────────────────────────┐");
    println!("│ Metric                       │ {:<22} │ {:<22} │", name_a, name_b);
    println!("├──────────────────────────────┼────────────────────────┼────────────────────────┤");
    println!("│ Total Calls                  │ {:>22} │ {:>22} │", a.calls, b.calls);
    println!("│ Total Cost                   │ ${:>21.4} │ ${:>21.4} │", a.cost, b.cost);
    println!("│ Avg Cost per Call            │ ${:>21.6} │ ${:>21.6} │", avg_cost_a, avg_cost_b);
    println!("│ Avg Input Tokens / Call      │ {:>22.1} │ {:>22.1} │", avg_in_a, avg_in_b);
    println!("│ Avg Output Tokens / Call     │ {:>22.1} │ {:>22.1} │", avg_out_a, avg_out_b);
    println!("│ Cache Read Ratio             │ {:>21.1}% │ {:>21.1}% │", cache_ratio_a, cache_ratio_b);
    println!("└──────────────────────────────┴────────────────────────┴────────────────────────┘\n");
}

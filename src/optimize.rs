use crate::models::Session;

pub struct Finding {
    pub title: String,
    pub explanation: String,
    pub impact: String,
    pub savings_hint: String,
}

pub fn run_optimize(sessions: &[Session]) {
    if sessions.is_empty() {
        println!("\n  No sessions found to optimize.\n");
        return;
    }

    let mut findings = Vec::new();

    // 1. High-Cost Outliers
    let costs: Vec<f64> = sessions.iter().map(|s| s.total_cost.amount_usd).collect();
    let count = costs.len() as f64;
    let sum: f64 = costs.iter().sum();
    
    if count > 2.0 {
        let mean = sum / count;
        let variance: f64 = costs.iter().map(|c| (c - mean).powi(2)).sum::<f64>() / count;
        let std_dev = variance.sqrt();
        let threshold = mean + 2.0 * std_dev;

        let mut outliers = Vec::new();
        for session in sessions {
            if session.total_cost.amount_usd > threshold && session.total_cost.amount_usd > 0.50 {
                outliers.push(session);
            }
        }

        if !outliers.is_empty() {
            outliers.sort_by(|a, b| b.total_cost.amount_usd.partial_cmp(&a.total_cost.amount_usd).unwrap_or(std::cmp::Ordering::Equal));
            let mut list_str = String::new();
            for s in outliers.iter().take(3) {
                let id_len = std::cmp::min(12, s.id.len());
                list_str.push_str(&format!("\n    - Session {} in project {} (Cost: ${:.2})", &s.id[..id_len], s.project_path, s.total_cost.amount_usd));
            }

            findings.push(Finding {
                title: "High-Cost Session Outliers Detected".to_string(),
                explanation: format!(
                    "Some sessions consume significantly more tokens and cost than the project average (Mean: ${:.4}, StdDev: ${:.4}). These are typically long-running chat histories where the full context is sent repeatedly.{}",
                    mean, std_dev, list_str
                ),
                impact: "High".to_string(),
                savings_hint: "Try resetting or starting a new session periodically to clear context bloat.".to_string(),
            });
        }
    }

    // 2. Low Cache Hit Rate
    let mut total_cache_read = 0;
    let mut total_input = 0;
    let mut cache_capable_calls = 0;

    for s in sessions {
        for call in &s.calls {
            let model_lower = call.model.to_lowercase();
            if model_lower.contains("claude") || model_lower.contains("gemini") {
                cache_capable_calls += 1;
                total_cache_read += call.token_usage.cache_read;
                total_input += call.token_usage.input;
            }
        }
    }

    if cache_capable_calls > 0 && total_input > 50_000 {
        let hit_rate = (total_cache_read as f64) / ((total_input + total_cache_read) as f64);
        if hit_rate < 0.20 {
            findings.push(Finding {
                title: "Low Prompt Cache Utilization".to_string(),
                explanation: format!(
                    "For cache-capable models, your cache read hit rate is only {:.1}%. This means you are paying full price for context repeatedly instead of utilizing cached tokens.",
                    hit_rate * 100.0
                ),
                impact: "Medium".to_string(),
                savings_hint: "Verify if prompt caching is enabled on your API keys or use sessions that run sequentially to keep the cache alive (expires in ~5 mins).".to_string(),
            });
        }
    }

    // 3. Model Substitution
    let mut expensive_calls = 0;
    let mut total_expensive_cost = 0.0;
    for s in sessions {
        for call in &s.calls {
            let m = call.model.to_lowercase();
            if m.contains("opus") || m.contains("gpt-4") || m.contains("pro") {
                if call.token_usage.input < 2000 {
                    expensive_calls += 1;
                    total_expensive_cost += call.cost.amount_usd;
                }
            }
        }
    }

    if expensive_calls > 5 {
        findings.push(Finding {
            title: "Sub-optimal Model Choices for Small Tasks".to_string(),
            explanation: format!(
                "You made {} calls using premium/expensive models for queries containing less than 2,000 input tokens, costing ${:.4}.",
                expensive_calls, total_expensive_cost
            ),
            impact: "Low to Medium".to_string(),
            savings_hint: "Consider using faster, cheaper models like 'gemini-2.0-flash' or 'claude-3-5-haiku' for simple lookups or minor edits.".to_string(),
        });
    }

    // Print optimization findings
    println!("\n┌────────────────────────────────────────────────────────────────────────┐");
    println!("│                       Niriksh Optimization Findings                    │");
    println!("└────────────────────────────────────────────────────────────────────────┘");

    if findings.is_empty() {
        println!("  🎉 Great job! No major token waste or inefficiencies detected.");
        println!("  All models and caches are performing optimally.");
        println!("──────────────────────────────────────────────────────────────────────────\n");
        return;
    }

    for (i, f) in findings.iter().enumerate() {
        println!("\n  [{}] {} (Impact: {})", i + 1, f.title, f.impact);
        println!("  ────────────────────────────────────────────────────────────────────────");
        println!("  Explanation: {}", f.explanation);
        println!("  Recommendation: {}", f.savings_hint);
    }
    println!("\n──────────────────────────────────────────────────────────────────────────\n");
}

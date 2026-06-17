use crate::models::Session;
use chrono::{DateTime, Duration, FixedOffset};
use std::collections::HashSet;
use std::process::Command;

pub fn run_yield_report(sessions: &[Session], cwd: &str) {
    let mut productive_cost = 0.0;
    let mut productive_sessions = 0;
    let mut reverted_cost = 0.0;
    let mut reverted_sessions = 0;
    let mut abandoned_cost = 0.0;
    let mut abandoned_sessions = 0;

    let is_git = is_git_repo(cwd);
    let main_branch = if is_git { get_main_branch(cwd) } else { "main".to_string() };
    let reverted_shas = if is_git { get_reverted_shas(cwd) } else { HashSet::new() };

    for session in sessions {
        if session.calls.is_empty() {
            abandoned_cost += session.total_cost.amount_usd;
            abandoned_sessions += 1;
            continue;
        }

        let mut min_dt: Option<DateTime<FixedOffset>> = None;
        let mut max_dt: Option<DateTime<FixedOffset>> = None;

        for call in &session.calls {
            if let Ok(dt) = DateTime::parse_from_rfc3339(&call.timestamp) {
                min_dt = Some(min_dt.map_or(dt, |m| std::cmp::min(m, dt)));
                max_dt = Some(max_dt.map_or(dt, |m| std::cmp::max(m, dt)));
            }
        }

        let (session_start, session_end) = match (min_dt, max_dt) {
            (Some(start), Some(end)) => (start, end + Duration::hours(1)),
            _ => {
                abandoned_cost += session.total_cost.amount_usd;
                abandoned_sessions += 1;
                continue;
            }
        };

        if !is_git {
            abandoned_cost += session.total_cost.amount_usd;
            abandoned_sessions += 1;
            continue;
        }

        let commits = get_commits_in_window(cwd, session_start, session_end);

        if commits.is_empty() {
            abandoned_cost += session.total_cost.amount_usd;
            abandoned_sessions += 1;
            continue;
        }

        let mut in_main_commits = Vec::new();
        for sha in &commits {
            if is_commit_in_branch(cwd, sha, &main_branch) {
                in_main_commits.push(sha);
            }
        }

        if in_main_commits.is_empty() {
            abandoned_cost += session.total_cost.amount_usd;
            abandoned_sessions += 1;
            continue;
        }

        let mut reverted_count = 0;
        for sha in &in_main_commits {
            let short_sha = if sha.len() >= 7 { &sha[..7] } else { sha };
            if reverted_shas.contains(*sha) || reverted_shas.contains(short_sha) {
                reverted_count += 1;
            }
        }

        if reverted_count > 0 && reverted_count >= in_main_commits.len() / 2 {
            reverted_cost += session.total_cost.amount_usd;
            reverted_sessions += 1;
        } else {
            productive_cost += session.total_cost.amount_usd;
            productive_sessions += 1;
        }
    }

    let total_cost = productive_cost + reverted_cost + abandoned_cost;
    let total_sessions = productive_sessions + reverted_sessions + abandoned_sessions;

    let pct = |cost: f64| {
        if total_cost > 0.0 {
            ((cost / total_cost) * 100.0).round() as usize
        } else {
            0
        }
    };

    println!("\n  Yield Analysis (Productive vs Abandoned Spend):");
    println!("  ────────────────────────────────────────────────────────────────────────");
    println!(
        "  Productive:  ${:>7.2} ({:>2}%) - {} sessions shipped to {}",
        productive_cost,
        pct(productive_cost),
        productive_sessions,
        main_branch
    );
    println!(
        "  Reverted:    ${:>7.2} ({:>2}%) - {} sessions were reverted",
        reverted_cost,
        pct(reverted_cost),
        reverted_sessions
    );
    println!(
        "  Abandoned:   ${:>7.2} ({:>2}%) - {} sessions never committed",
        abandoned_cost,
        pct(abandoned_cost),
        abandoned_sessions
    );
    println!("  ────────────────────────────────────────────────────────────────────────");
    println!(
        "  Total:       ${:>7.2}      - {} sessions\n",
        total_cost, total_sessions
    );
}

fn is_git_repo(cwd: &str) -> bool {
    Command::new("git")
        .args(&["rev-parse", "--is-inside-work-tree"])
        .current_dir(cwd)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false)
}

fn get_main_branch(cwd: &str) -> String {
    if let Ok(output) = Command::new("git")
        .args(&["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(cwd)
        .output()
    {
        let res = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !res.is_empty() {
            return res.replace("refs/remotes/origin/", "");
        }
    }
    if let Ok(output) = Command::new("git")
        .args(&["branch", "-a"])
        .current_dir(cwd)
        .output()
    {
        let branches = String::from_utf8_lossy(&output.stdout);
        if branches.contains("main") {
            return "main".to_string();
        }
        if branches.contains("master") {
            return "master".to_string();
        }
    }
    "main".to_string()
}

fn get_reverted_shas(cwd: &str) -> HashSet<String> {
    let mut shas = HashSet::new();
    if let Ok(output) = Command::new("git")
        .args(&["log", "--all", "--grep=^This reverts commit", "--format=%B"])
        .current_dir(cwd)
        .output()
    {
        let bodies = String::from_utf8_lossy(&output.stdout);
        for line in bodies.lines() {
            if line.starts_with("This reverts commit ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    shas.insert(parts[3].trim_matches('.').to_lowercase());
                }
            }
        }
    }
    shas
}

fn get_commits_in_window(cwd: &str, start: DateTime<FixedOffset>, end: DateTime<FixedOffset>) -> Vec<String> {
    let mut commits = Vec::new();
    let since = start.to_rfc3339();
    let until = end.to_rfc3339();

    if let Ok(output) = Command::new("git")
        .args(&["log", "--all", &format!("--since={}", since), &format!("--until={}", until), "--format=%H"])
        .current_dir(cwd)
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let sha = line.trim().to_string();
            if !sha.is_empty() {
                commits.push(sha);
            }
        }
    }
    commits
}

fn is_commit_in_branch(cwd: &str, sha: &str, branch: &str) -> bool {
    Command::new("git")
        .args(&["merge-base", "--is-ancestor", sha, branch])
        .current_dir(cwd)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

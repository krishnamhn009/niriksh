use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "niriksh")]
#[command(about = "AI Coding Token Usage Tracker (Rust Replica)", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show a compact status summary
    Status {
        /// Optional provider filter
        #[arg(short, long)]
        provider: Option<String>,
    },
    /// Generate a detailed report
    Report {
        /// Period to report (e.g., today, week, month)
        #[arg(short, long, default_value = "today")]
        period: String,
    },
}

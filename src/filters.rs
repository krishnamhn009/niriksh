use crate::models::ParsedProviderCall;
use chrono::{DateTime, Utc, Duration};

pub struct Filters {
    pub period: String,
    pub provider: Option<String>,
}

impl Filters {
    pub fn new(period: String, provider: Option<String>) -> Self {
        Self { period, provider }
    }

    pub fn is_allowed(&self, provider_name: &str, call: &ParsedProviderCall) -> bool {
        // Provider filter
        if let Some(ref p) = self.provider {
            if !provider_name.eq_ignore_ascii_case(p) {
                return false;
            }
        }

        // Date filter
        // If timestamp is empty or invalid, we default to keep it unless the period explicitly rules it out.
        // Or we assume it's from the past.
        if self.period == "all" {
            return true;
        }

        let call_time = match call.timestamp.parse::<DateTime<Utc>>() {
            Ok(dt) => dt,
            Err(_) => return true, // If we can't parse it, keep it (like stub data)
        };

        let now = Utc::now();
        let cutoff = match self.period.as_str() {
            "today" => now - Duration::days(1),
            "week" => now - Duration::days(7),
            "month" => now - Duration::days(30),
            _ => return true, // Default "all" or unrecognized
        };

        call_time >= cutoff
    }
}

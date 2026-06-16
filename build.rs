use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("litellm_pricing.json");

    // Fetch litellm pricing json at build time
    let url = "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
    
    // In a real robust build script, you'd handle network errors gracefully.
    // If it fails, you might want to write a tiny stub JSON to prevent build failure,
    // or panic to enforce that it works. We'll do a basic fetch and panic on failure for now.
    match ureq::get(url).call() {
        Ok(mut response) => {
            if let Ok(json_string) = response.body_mut().read_to_string() {
                fs::write(&dest_path, json_string).unwrap();
            } else {
                fs::write(&dest_path, "{}").unwrap();
            }
        }
        Err(_) => {
            // Write empty stub if network fails
            fs::write(&dest_path, "{}").unwrap();
        }
    }

    // Tell Cargo to re-run this script only if build.rs changes
    println!("cargo:rerun-if-changed=build.rs");
}

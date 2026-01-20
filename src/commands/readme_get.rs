use crate::cli::ReadmeGetArgs;
use crate::config::ConfigManager;
use crate::failure::log_failure;
use crate::util::{get_provider_factory, ReverseBufferReader};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn readme_get(_args: ReadmeGetArgs) -> Result<()> {
    let archlist_path = "archlist";
    let fail_file = ".fail";

    let config_manager = ConfigManager::new()?;
    let config = config_manager.load()?;
    let config = Arc::new(Mutex::new(config));
    let lines_to_skip = config.lock().await.lines_from_bottom;

    let mut reader = ReverseBufferReader::new(archlist_path)?;

    let lines_read = Arc::new(Mutex::new(0usize));
    let should_stop = Arc::new(Mutex::new(false));

    let config_clone = Arc::clone(&config);
    let lines_read_clone = Arc::clone(&lines_read);
    let should_stop_clone = Arc::clone(&should_stop);

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            if *should_stop_clone.lock().await {
                break;
            }

            let current_lines = *lines_read_clone.lock().await;
            let mut cfg = config_clone.lock().await;
            cfg.lines_from_bottom = current_lines;

            if let Err(e) = config_manager.save(&cfg) {
                eprintln!("Failed to save config: {}", e);
            }
        }
    });

    for _ in 0..lines_to_skip {
        if reader.read_line()?.is_none() {
            break;
        }
    }

    loop {
        let line = match reader.read_line()? {
            Some(line) => line,
            None => break,
        };

        let url = line.trim();
        if url.is_empty() || url.starts_with('#') {
            continue;
        }

        let factory = get_provider_factory().await;
        let provider = match factory.get_provider(url).await {
            Ok(provider) => provider,
            Err(e) => {
                log_failure(url, "INVALID-PROVIDER", fail_file)?;
                eprintln!("Failed to get provider for {}: {}", url, e);
                continue;
            }
        };

        match provider.get_readme(url).await {
            Ok(readme) => {
                let output_path = url_to_path(url)?;
                if let Some(parent) = Path::new(&output_path).parent() {
                    fs::create_dir_all(parent)?;
                }

                fs::write(&output_path, readme)
                    .context(format!("Failed to write README to {}", output_path))?;

                println!("Downloaded README from {}", url);
            }
            Err(e) => {
                let error_code = if e.to_string().contains("404") {
                    "NO-README"
                } else if e.to_string().contains("Not Found") {
                    "NO-REPO"
                } else if e.to_string().contains("rate limit") || e.to_string().contains("403") || e.to_string().contains("429") {
                    "RATE-LIMIT"
                } else if e.to_string().contains("No valid tokens available") {
                    "NO-TOKENS"
                } else {
                    "UNKNOWN"
                };

                log_failure(url, error_code, fail_file)?;
                eprintln!("Failed to fetch README from {}: {}", url, e);
            }
        }

        *lines_read.lock().await += 1;
    }

    *should_stop.lock().await = true;

    Ok(())
}

fn url_to_path(url: &str) -> Result<String> {
    let url = url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    Ok(url.to_string())
}

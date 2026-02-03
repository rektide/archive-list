use crate::cli::ReadmeGetArgs;
use crate::config::ConfigManager;
use crate::failure::log_failure;
use crate::provider::ProviderTrait;
use crate::util::{get_provider_factory, ReverseBufferReader};
use anyhow::Result;
use futures::stream::{self, StreamExt};
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

    let mut urls = Vec::new();
    while let Some(line) = reader.read_line()? {
        urls.push(line);
    }

    stream::iter(urls)
        .filter_map(|url| async move {
            let url = url.trim();
            if url.is_empty() || url.starts_with('#') {
                None
            } else {
                Some(url.to_string())
            }
        })
        .map(|url| {
            let lines_read_clone = Arc::clone(&lines_read);
            async move {
                let url = url.trim();
                let fail_file_owned = fail_file.to_string();
                let url_owned = url.to_string();

                let factory = get_provider_factory().await;

            let provider = match factory.get_provider(&url_owned).await {
                Ok(provider) => provider,
                Err(e) => {
                    let _ = log_failure(&url_owned, "INVALID-PROVIDER", &fail_file_owned);
                    eprintln!("Failed to get provider for {}: {}", url_owned, e);
                    *lines_read_clone.lock().await += 1;
                    return;
                }
            };

            match provider.get_readme(&url_owned).await {
                Ok(readme) => {
                    let output_path = match url_to_path(&url_owned) {
                        Ok(path) => path,
                        Err(e) => {
                            eprintln!("Failed to create path for {}: {}", url_owned, e);
                            *lines_read_clone.lock().await += 1;
                            return;
                        }
                    };

                    if let Some(parent) = Path::new(&output_path).parent() {
                        if let Err(e) = fs::create_dir_all(parent) {
                            eprintln!("Failed to create directory {}: {}", parent.display(), e);
                        }
                    }

                    if let Err(e) = fs::write(&output_path, readme) {
                        eprintln!("Failed to write README to {}: {}", output_path, e);
                    } else {
                        println!("Downloaded README from {}", url_owned);
                    }
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

                    let _ = log_failure(&url_owned, error_code, &fail_file_owned);
                    eprintln!("Failed to fetch README from {}: {}", url_owned, e);
                }
            }

            *lines_read_clone.lock().await += 1;
        }
        })
        .buffer_unordered(10)
        .collect::<Vec<_>>()
        .await;

    *should_stop.lock().await = true;

    Ok(())
}

fn url_to_path(url: &str) -> Result<String> {
    let url = url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    Ok(url.to_string())
}

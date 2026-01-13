use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::header;
use super::Provider;

pub struct HuggingFaceProvider;

#[async_trait]
impl Provider for HuggingFaceProvider {
    async fn detect(url: &str) -> Option<Box<dyn Provider>> {
        if url.contains("huggingface.co") {
            Some(Box::new(HuggingFaceProvider))
        } else {
            None
        }
    }

    async fn get_readme(&self, url: &str) -> Result<String> {
        let model_id = parse_huggingface_url(url)?;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("https://huggingface.co/api/models/{}/raw/README.md", model_id))
            .header(header::USER_AGENT, "archive-list")
            .send()
            .await
            .context("Failed to fetch README from HuggingFace API")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("HuggingFace API returned status: {}", response.status()));
        }

        let content = response
            .text()
            .await
            .context("Failed to read README content")?;

        Ok(content)
    }

    fn rate_limit(&self) -> u32 {
        60
    }
}

fn parse_huggingface_url(url: &str) -> Result<String> {
    let url = url.trim_end_matches('/');
    let parts: Vec<&str> = url.split('/').collect();

    if parts.len() < 5 {
        return Err(anyhow::anyhow!("Invalid HuggingFace URL format"));
    }

    let model_id = parts[4..].join("/");
    Ok(model_id)
}


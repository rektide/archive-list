use super::Provider;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::header;

pub struct CodebergProvider;

#[async_trait]
impl Provider for CodebergProvider {
    async fn detect(url: &str) -> Option<Box<dyn Provider>> {
        if url.contains("codeberg.org") {
            Some(Box::new(CodebergProvider))
        } else {
            None
        }
    }

    async fn get_readme(&self, url: &str) -> Result<String> {
        let (owner, repo) = parse_codeberg_url(url)?;

        let client = reqwest::Client::new();
        let response = client
            .get(format!(
                "https://codeberg.org/api/v1/repos/{}/{}/readme",
                owner, repo
            ))
            .header(header::ACCEPT, "application/json")
            .header(header::USER_AGENT, "archive-list")
            .send()
            .await
            .context("Failed to fetch README from Codeberg API")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Codeberg API returned status: {}",
                response.status()
            ));
        }

        let content = response
            .text()
            .await
            .context("Failed to read README content")?;

        Ok(content)
    }

    fn rate_limit(&self) -> u32 {
        200
    }
}

fn parse_codeberg_url(url: &str) -> Result<(String, String)> {
    let url = url.trim_end_matches('/');
    let parts: Vec<&str> = url.split('/').collect();

    if parts.len() < 5 {
        return Err(anyhow::anyhow!("Invalid Codeberg URL format"));
    }

    let owner = parts[3].to_string();
    let repo = parts[4].to_string();

    Ok((owner, repo))
}

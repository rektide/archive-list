use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::header;
use super::Provider;

pub struct GitHubProvider;

#[async_trait]
impl Provider for GitHubProvider {
    async fn detect(url: &str) -> Option<Box<dyn Provider>> {
        if url.starts_with("https://github.com/") || url.starts_with("http://github.com/") {
            Some(Box::new(GitHubProvider))
        } else {
            None
        }
    }

    async fn get_readme(&self, url: &str) -> Result<String> {
        let repo = parse_github_url(url)?;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("https://api.github.com/repos/{}/readme", repo))
            .header(header::ACCEPT, "application/vnd.github.raw")
            .header(header::USER_AGENT, "archive-list")
            .send()
            .await
            .context("Failed to fetch README from GitHub API")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("GitHub API returned status: {}", response.status()));
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

fn parse_github_url(url: &str) -> Result<String> {
    let url = url.trim_end_matches('/');
    let parts: Vec<&str> = url.split('/').collect();

    if parts.len() < 5 {
        return Err(anyhow::anyhow!("Invalid GitHub URL format"));
    }

    let owner = parts[3];
    let repo = parts[4];

    Ok(format!("{}/{}", owner, repo))
}

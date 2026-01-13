use super::Provider;
use anyhow::{Context, Result};
use async_trait::async_trait;
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use once_cell::sync::Lazy;
use reqwest::header;

pub struct GitHubProvider;

static RATE_LIMITER: Lazy<RateLimiter<NotKeyed, InMemoryState, DefaultClock>> = Lazy::new(|| {
    let velocity = (60_f64 * 1.5) as u32;
    let requests = u32::max(1, velocity);
    let quota = Quota::per_second(requests.try_into().unwrap());
    RateLimiter::direct(quota)
});

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
        while RATE_LIMITER.check().is_err() {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }

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
            return Err(anyhow::anyhow!(
                "GitHub API returned status: {}",
                response.status()
            ));
        }

        let content = response
            .text()
            .await
            .context("Failed to read README content")?;

        Ok(content)
    }

    fn name(&self) -> &'static str {
        "github"
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

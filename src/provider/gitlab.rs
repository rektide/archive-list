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

pub struct GitLabProvider;

static RATE_LIMITER: Lazy<RateLimiter<NotKeyed, InMemoryState, DefaultClock>> = Lazy::new(|| {
    let velocity = (2000_f64 * 1.5) as u32;
    let requests = u32::max(1, velocity);
    let quota = Quota::per_second(requests.try_into().unwrap());
    RateLimiter::direct(quota)
});

#[async_trait]
impl Provider for GitLabProvider {
    async fn detect(url: &str) -> Option<Box<dyn Provider>> {
        if url.contains("gitlab") {
            Some(Box::new(GitLabProvider))
        } else {
            None
        }
    }

    async fn get_readme(&self, url: &str) -> Result<String> {
        while RATE_LIMITER.check().is_err() {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }

        let (base_url, project_id) = parse_gitlab_url(url)?;

        let client = reqwest::Client::new();
        let response = client
            .get(format!(
                "{}/api/v4/projects/{}/repository/files/README.md/raw",
                base_url, project_id
            ))
            .header(header::USER_AGENT, "archive-list")
            .send()
            .await
            .context("Failed to fetch README from GitLab API")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "GitLab API returned status: {}",
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
        "gitlab"
    }
}

fn parse_gitlab_url(url: &str) -> Result<(String, String)> {
    let url = url.trim_end_matches('/');
    let parts: Vec<&str> = url.split('/').collect();

    if parts.len() < 5 {
        return Err(anyhow::anyhow!("Invalid GitLab URL format"));
    }

    let base = format!("{}//{}", parts[0], parts[2]);
    let project_path = parts[3..].join("/");

    let encoded_project = urlencoding::encode(&project_path);
    Ok((base, encoded_project.to_string()))
}

use anyhow::{Context, Result};
use async_trait::async_trait;

#[async_trait]
pub trait Strategy: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &'static str;

    async fn get_readme_url(&self, domain: &str, url: &str) -> Option<String>;

    async fn get_url(
        &self,
        url: &str,
        token: Option<&str>,
    ) -> Result<reqwest::Response>;

    fn clone_box(&self) -> Box<dyn Strategy>;
}

#[derive(Debug)]
pub struct ApiStrategy {
    api_pattern: String,
}

impl ApiStrategy {
    pub fn new(api_pattern: String) -> Self {
        Self { api_pattern }
    }
}

#[async_trait]
impl Strategy for ApiStrategy {
    fn name(&self) -> &'static str {
        "api"
    }

    async fn get_readme_url(&self, domain: &str, url: &str) -> Option<String> {
        let repo_path = extract_repo_path(url)?;
        let api_url = self.api_pattern.replace("{repo}", &repo_path);
        Some(format!("https://{}{}", domain, api_url))
    }

    async fn get_url(
        &self,
        url: &str,
        token: Option<&str>,
    ) -> Result<reqwest::Response> {
        let client = reqwest::Client::new();
        let mut request = client.get(url);

        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        request
            .send()
            .await
            .context("Failed to fetch URL via API strategy")
    }

    fn clone_box(&self) -> Box<dyn Strategy> {
        Box::new(ApiStrategy {
            api_pattern: self.api_pattern.clone(),
        })
    }
}

#[derive(Debug)]
pub struct RawGitStrategy;

#[async_trait]
impl Strategy for RawGitStrategy {
    fn name(&self) -> &'static str {
        "raw-git"
    }

    async fn get_readme_url(&self, domain: &str, url: &str) -> Option<String> {
        let repo_path = extract_repo_path(url)?;
        let parts: Vec<&str> = repo_path.split('/').collect();
        if parts.len() != 2 {
            return None;
        }

        Some(format!(
            "https://raw.githubusercontent.com/{}/master/README.md",
            repo_path
        ))
    }

    async fn get_url(&self, url: &str, _token: Option<&str>) -> Result<reqwest::Response> {
        let client = reqwest::Client::new();
        client
            .get(url)
            .send()
            .await
            .context("Failed to fetch URL via raw Git strategy")
    }

    fn clone_box(&self) -> Box<dyn Strategy> {
        Box::new(RawGitStrategy)
    }
}

#[derive(Debug)]
pub struct HtmlScrapeStrategy;

#[async_trait]
impl Strategy for HtmlScrapeStrategy {
    fn name(&self) -> &'static str {
        "html-scrape"
    }

    async fn get_readme_url(&self, _domain: &str, url: &str) -> Option<String> {
        Some(url.to_string())
    }

    async fn get_url(&self, url: &str, _token: Option<&str>) -> Result<reqwest::Response> {
        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .send()
            .await
            .context("Failed to fetch URL via HTML scrape strategy")?;

        let html = response.text().await.ok().ok_or_else(|| anyhow::anyhow!("Failed to read response"))?;

        if html.contains("README") {
            Ok(client.get(url).send().await?)
        } else {
            Err(anyhow::anyhow!("README not found in HTML"))
        }
    }

    fn clone_box(&self) -> Box<dyn Strategy> {
        Box::new(HtmlScrapeStrategy)
    }
}

fn extract_repo_path(url: &str) -> Option<String> {
    let url = url.trim_end_matches('/');
    let parts: Vec<&str> = url.split('/').collect();

    if parts.len() < 5 {
        return None;
    }

    let owner = parts[3];
    let repo = parts[4];

    Some(format!("{}/{}", owner, repo))
}

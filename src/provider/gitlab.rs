use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::header;
use super::Provider;

pub struct GitLabProvider;

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
        let (base_url, project_id) = parse_gitlab_url(url)?;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/api/v4/projects/{}/repository/files/README.md/raw", base_url, project_id))
            .header(header::USER_AGENT, "archive-list")
            .send()
            .await
            .context("Failed to fetch README from GitLab API")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("GitLab API returned status: {}", response.status()));
        }

        let content = response
            .text()
            .await
            .context("Failed to read README content")?;

        Ok(content)
    }

    fn rate_limit(&self) -> u32 {
        2000
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


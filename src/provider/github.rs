use anyhow::Result;
use async_trait::async_trait;
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
        todo!("Implement GitHub get_readme");
    }

    fn rate_limit(&self) -> u32 {
        60
    }
}

use anyhow::Result;
use async_trait::async_trait;
use super::Provider;

pub struct GitHubProvider;

#[async_trait]
impl Provider for GitHubProvider {
    async fn get_readme(&self, url: &str) -> Result<String> {
        todo!("Implement GitHub get_readme");
    }

    fn rate_limit(&self) -> u32 {
        60
    }
}

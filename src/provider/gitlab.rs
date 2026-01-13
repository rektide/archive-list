use anyhow::Result;
use async_trait::async_trait;
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
        todo!("Implement GitLab get_readme");
    }

    fn rate_limit(&self) -> u32 {
        2000
    }
}

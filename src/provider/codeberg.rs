use anyhow::Result;
use async_trait::async_trait;
use super::Provider;

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
        todo!("Implement Codeberg get_readme");
    }

    fn rate_limit(&self) -> u32 {
        200
    }
}

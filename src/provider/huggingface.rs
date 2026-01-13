use anyhow::Result;
use async_trait::async_trait;
use super::Provider;

pub struct HuggingFaceProvider;

#[async_trait]
impl Provider for HuggingFaceProvider {
    async fn detect(url: &str) -> Option<Box<dyn Provider>> {
        if url.contains("huggingface.co") {
            Some(Box::new(HuggingFaceProvider))
        } else {
            None
        }
    }

    async fn get_readme(&self, url: &str) -> Result<String> {
        todo!("Implement HuggingFace get_readme");
    }

    fn rate_limit(&self) -> u32 {
        60
    }
}

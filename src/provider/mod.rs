pub mod github;
pub mod gitlab;
pub mod huggingface;
pub mod codeberg;

use async_trait::async_trait;

#[async_trait]
pub trait Provider: Send + Sync {
    async fn detect(url: &str) -> Option<Box<dyn Provider>>
    where
        Self: Sized;

    async fn get_readme(&self, url: &str) -> anyhow::Result<String>;
    fn rate_limit(&self) -> u32;
}

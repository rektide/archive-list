pub mod codeberg;
pub mod github;
pub mod gitlab;
pub mod huggingface;

use async_trait::async_trait;

#[async_trait]
pub trait Provider: Send + Sync {
    async fn detect(url: &str) -> Option<Box<dyn Provider>>
    where
        Self: Sized;

    async fn get_readme(&self, url: &str) -> anyhow::Result<String>;

    #[allow(dead_code)]
    fn name(&self) -> &'static str;
}

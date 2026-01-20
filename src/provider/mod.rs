pub mod strategy;
pub mod domain;
pub mod generic;
pub mod factory;

pub use factory::ProviderFactory;

use async_trait::async_trait;

#[async_trait]
pub trait ProviderTrait: Send + Sync {
    async fn get_readme_url(&self, url: &str) -> anyhow::Result<String>;
    async fn fetch_url(&self, url: &str) -> anyhow::Result<reqwest::Response>;
}

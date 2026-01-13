use crate::provider::{ProviderFactory, ProviderTrait};
use std::sync::Arc;
use tokio::sync::OnceCell;
use async_trait::async_trait;

static FACTORY: OnceCell<ProviderFactory> = OnceCell::const_new();

async fn get_factory() -> &'static ProviderFactory {
    FACTORY
        .get_or_init(|| async { ProviderFactory::new() })
        .await
}

pub async fn detect_provider(url: &str) -> Option<Box<dyn ProviderTrait>> {
    let factory = get_factory().await;

    match factory.get_provider(url).await {
        Ok(provider) => Some(Box::new(ProviderWrapper(Arc::clone(&provider)))),
        Err(_) => None,
    }
}

struct ProviderWrapper(Arc<crate::provider::Provider>);

#[async_trait]
impl ProviderTrait for ProviderWrapper {
    async fn get_readme_url(&self, url: &str) -> anyhow::Result<String> {
        self.0.get_readme_url(url).await
    }

    async fn fetch_url(&self, url: &str) -> anyhow::Result<reqwest::Response> {
        self.0.fetch_url(url).await
    }
}


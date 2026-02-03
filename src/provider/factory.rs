use crate::provider::domain::{get_domain_configs, get_default_config, DomainConfig};
use crate::provider::generic::Provider;
use anyhow::{Context, Result};
use reqwest_middleware::ClientWithMiddleware;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ProviderFactory {
    domains: HashMap<String, DomainConfig>,
    providers: Arc<RwLock<HashMap<String, Arc<Provider>>>>,
    client: Arc<ClientWithMiddleware>,
}

impl ProviderFactory {
    pub fn new() -> Self {
        Self {
            domains: get_domain_configs(),
            providers: Arc::new(RwLock::new(HashMap::new())),
            client: crate::util::create_shared_client(),
        }
    }

    pub async fn get_provider(&self, url: &str) -> Result<Arc<Provider>> {
        let domain = self.extract_domain(url)?;

        let providers = self.providers.read().await;
        if let Some(provider) = providers.get(&domain) {
            return Ok(Arc::clone(provider));
        }
        drop(providers);

        // Use known config or generate default for unknown domains
        let config = self
            .domains
            .get(&domain)
            .cloned()
            .unwrap_or_else(|| {
                log::info!("Using default config for unknown domain: {}", domain);
                get_default_config(&domain)
            });

        let provider = Arc::new(Provider::new(
            domain.clone(),
            config,
            Arc::clone(&self.client),
        ));

        let mut providers = self.providers.write().await;
        providers.insert(domain.clone(), Arc::clone(&provider));

        log::debug!("Created provider for domain: {}", domain);

        Ok(provider)
    }

    fn extract_domain(&self, url: &str) -> Result<String> {
        let parsed = url::Url::parse(url)
            .context("Failed to parse URL")?;

        let domain = parsed
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("URL has no host"))?
            .to_string();

        Ok(domain)
    }
}

impl Default for ProviderFactory {
    fn default() -> Self {
        Self::new()
    }
}

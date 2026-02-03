use crate::provider::strategy::Strategy;
use crate::provider::domain::DomainConfig;
use crate::provider::ProviderTrait;
use crate::util::TokenRateLimiter;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest_middleware::ClientWithMiddleware;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct Provider {
    pub domain: String,
    strategies: Vec<Box<dyn Strategy>>,
    working_strategy: Arc<RwLock<Option<Box<dyn Strategy>>>>,
    token_limiter: Arc<TokenRateLimiter>,
    client: Arc<ClientWithMiddleware>,
}

#[async_trait]
impl ProviderTrait for Provider {
    async fn get_readme_url(&self, url: &str) -> anyhow::Result<String> {
        let working = self.working_strategy.read().await;

        if let Some(strategy) = working.as_ref() {
            log::debug!("{}: Trying cached strategy: {}", self.domain, strategy.name());
            if let Some(readme_url) = strategy.get_readme_url(&self.domain, url).await {
                log::debug!("{}: Cached strategy '{}' succeeded", self.domain, strategy.name());
                return Ok(readme_url);
            }
            log::debug!("{}: Cached strategy '{}' failed, falling back to all strategies", self.domain, strategy.name());
        }
        drop(working);

        log::debug!("{}: Trying all strategies in order", self.domain);
        for strategy in &self.strategies {
            log::debug!("{}: Trying strategy: {}", self.domain, strategy.name());
            if let Some(readme_url) = strategy.get_readme_url(&self.domain, url).await {
                log::debug!("{}: Strategy '{}' succeeded, caching for future use", self.domain, strategy.name());
                let mut working = self.working_strategy.write().await;
                *working = Some(strategy.clone_box());
                return Ok(readme_url);
            }
        }

        Err(anyhow::anyhow!("No strategy worked for domain: {}", self.domain))
    }

    async fn fetch_url(&self, url: &str) -> anyhow::Result<reqwest::Response> {
        if self.token_limiter.token_count().await == 0 {
            self.token_limiter.load_tokens().await;
        }

        let token = if self.token_limiter.all_tokens_exhausted().await {
            self.validate_tokens().await;
            self.token_limiter.get_next_token().await
        } else {
            self.token_limiter.get_next_token().await
        };

        let working = self.working_strategy.read().await;
        let strategy = working.as_ref()
            .or_else(|| self.strategies.first())
            .ok_or_else(|| anyhow::anyhow!("No strategy available"))?
            .clone_box();
        drop(working);

        let response = strategy.get_url(url, token.as_deref(), &self.client).await?;

        if let Some(token_value) = token {
            self.update_token_state(&response, &token_value).await?;
        }

        Ok(response)
    }

    async fn get_readme(&self, url: &str) -> Result<String> {
        let readme_url = self.get_readme_url(url).await?;
        self.fetch_content(&readme_url).await
    }
}

impl Provider {
    pub fn new(
        domain: String,
        config: DomainConfig,
        client: Arc<ClientWithMiddleware>,
    ) -> Self {
        let strategies = create_strategies(&config);
        let token_limiter = Arc::new(TokenRateLimiter::new(config.env_var));

        Self {
            domain,
            strategies,
            working_strategy: Arc::new(RwLock::new(None)),
            token_limiter,
            client,
        }
    }

    async fn fetch_content(&self, url: &str) -> Result<String> {
        let response = self.fetch_url(url).await?;
        let content = response
            .text()
            .await
            .context("Failed to read response content")?;
        Ok(content)
    }

    async fn validate_tokens(&self) {
        let validation_url = self.get_validation_url().unwrap_or_default();
        self.token_limiter.validate_all_tokens(&validation_url).await;
    }

    fn get_validation_url(&self) -> Option<String> {
        match self.domain.as_str() {
            "github.com" => Some("https://api.github.com/user".to_string()),
            "gitlab.com" => Some("https://gitlab.com/api/v4/user".to_string()),
            "codeberg.org" => Some("https://codeberg.org/api/v1/user".to_string()),
            "huggingface.co" => Some("https://huggingface.co/api/whoami".to_string()),
            _ => None,
        }
    }

    async fn update_token_state(&self, response: &reqwest::Response, token_value: &str) -> Result<()> {
        let status = response.status().as_u16();

        // Rate limiting is now handled by reqgov middleware
        // Only track token validity
        if status == 401 {
            self.token_limiter.mark_invalid(token_value).await;
        }

        Ok(())
    }
}

fn create_strategies(config: &DomainConfig) -> Vec<Box<dyn Strategy>> {
    let mut strategies: Vec<Box<dyn Strategy>> = vec![
        Box::new(crate::provider::strategy::RawGitStrategy),
        Box::new(crate::provider::strategy::HtmlScrapeStrategy),
    ];

    if let Some(api_pattern) = &config.api_pattern {
        strategies.insert(0, Box::new(crate::provider::strategy::ApiStrategy::new(api_pattern.clone())));
    }

    strategies
}

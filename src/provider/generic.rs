use crate::provider::strategy::Strategy;
use crate::provider::domain::{DomainConfig, TimestampFormat};
use crate::provider::ProviderTrait;
use crate::util::ratelimit::create_rate_limiter;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct Provider {
    pub domain: String,
    rate_limiter: Arc<governor::RateLimiter<governor::state::NotKeyed, governor::state::InMemoryState, governor::clock::DefaultClock>>,
    strategies: Vec<Box<dyn Strategy>>,
    working_strategy: Arc<RwLock<Option<Box<dyn Strategy>>>>,
    config: DomainConfig,
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
        while !crate::util::ratelimit::is_ok(&self.rate_limiter) {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }

        let tokens = self.load_tokens();
        let token = self.get_best_token(&tokens);

        let working = self.working_strategy.read().await;
        let strategy = working.as_ref()
            .or_else(|| self.strategies.first())
            .ok_or_else(|| anyhow::anyhow!("No strategy available"))?
            .clone_box();
        drop(working);

        let response = strategy.get_url(url, token.as_deref()).await?;

        self.parse_rate_limit_headers(&response)?;

        Ok(response)
    }
}

impl Provider {
    pub async fn get_readme(&self, url: &str) -> Result<String> {
        let readme_url = self.get_readme_url(url).await?;
        self.fetch_content(&readme_url).await
    }

    pub fn new(
        domain: String,
        config: DomainConfig,
        _tokens: Vec<String>,
    ) -> Self {
        let strategies = create_strategies(&config);

        let requests_per_second = match config.api_pattern.as_deref() {
            Some(_) => 60,
            None => 10,
        };

        let rate_limiter = Arc::new(create_rate_limiter(requests_per_second));

        Self {
            domain,
            rate_limiter,
            strategies,
            working_strategy: Arc::new(RwLock::new(None)),
            config,
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

    pub fn load_tokens(&self) -> Vec<String> {
        std::env::var(self.config.env_var)
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn get_best_token(&self, tokens: &[String]) -> Option<String> {
        if tokens.is_empty() {
            return None;
        }
        Some(tokens[0].clone())
    }

    fn parse_rate_limit_headers(&self, response: &reqwest::Response) -> Result<()> {
        let headers = response.headers();

        let remaining = self.parse_header(headers, &self.config.rate_limit_headers.remaining);
        let limit = self.parse_header(headers, &self.config.rate_limit_headers.limit);
        let reset = self.parse_header(headers, &self.config.rate_limit_headers.reset);

        if let (Some(rem), Some(lim), Some(rst)) = (remaining, limit, reset) {
            let reset_at = self.parse_reset_timestamp(&rst)?;

            log::debug!(
                "Rate limit for {}: remaining={}, limit={}, reset_at={}",
                self.domain,
                rem,
                lim,
                reset_at
            );
        }

        Ok(())
    }

    fn parse_header(&self, headers: &reqwest::header::HeaderMap, header_name: &str) -> Option<String> {
        let header_name = normalize_header_name(header_name);

        headers
            .iter()
            .find(|(name, _)| normalize_header_name(name.as_str()) == header_name)
            .and_then(|(_, value)| value.to_str().ok().map(|s| s.to_string()))
    }

    fn parse_reset_timestamp(&self, value: &str) -> Result<DateTime<Utc>> {
        match self.config.rate_limit_headers.format {
            TimestampFormat::UnixEpoch => {
                let timestamp = value.parse::<i64>()
                    .context("Failed to parse Unix epoch timestamp")?;
                Ok(DateTime::from_timestamp(timestamp, 0).unwrap())
            }
            TimestampFormat::Iso8601 => {
                DateTime::parse_from_rfc3339(value)
                    .context("Failed to parse ISO 8601 timestamp")
                    .map(|dt| dt.with_timezone(&Utc))
            }
        }
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

fn normalize_header_name(name: &str) -> String {
    name.to_lowercase()
        .trim_start_matches("x-")
        .replace('-', "_")
}

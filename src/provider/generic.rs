use crate::provider::strategy::Strategy;
use crate::provider::domain::{DomainConfig, TimestampFormat};
use crate::provider::ProviderTrait;
use crate::util::ratelimit::create_rate_limiter;
use crate::util::TokenRateLimiter;
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
    token_limiter: Arc<TokenRateLimiter>,
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

        if self.token_limiter.all_tokens_exhausted().await {
            self.validate_tokens().await;
        }

        while !crate::util::ratelimit::is_ok(&self.rate_limiter) {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }

        let token = self.token_limiter.get_next_token().await;

        if token.is_none() {
            return Err(anyhow::anyhow!("No valid tokens available"));
        }

        let working = self.working_strategy.read().await;
        let strategy = working.as_ref()
            .or_else(|| self.strategies.first())
            .ok_or_else(|| anyhow::anyhow!("No strategy available"))?
            .clone_box();
        drop(working);

        let response = strategy.get_url(url, token.as_deref()).await?;

        self.update_token_state(&response, token.as_deref()).await?;

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
    ) -> Self {
        let strategies = create_strategies(&config);

        let requests_per_second = match config.api_pattern.as_deref() {
            Some(_) => 60,
            None => 10,
        };

        let rate_limiter = Arc::new(create_rate_limiter(requests_per_second));
        let token_limiter = Arc::new(TokenRateLimiter::new(config.env_var));

        Self {
            domain,
            rate_limiter,
            strategies,
            working_strategy: Arc::new(RwLock::new(None)),
            config,
            token_limiter,
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
        let validation_url = self.get_validation_url();
        if let Some(url) = validation_url {
            let headers = (
                self.config.rate_limit_headers.remaining.to_string(),
                self.config.rate_limit_headers.limit.to_string(),
                self.config.rate_limit_headers.reset.to_string(),
            );
            self.token_limiter.validate_all_tokens(&url, Some(&headers)).await;
        } else {
            self.token_limiter.validate_all_tokens("", None).await;
        }
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

    async fn update_token_state(&self, response: &reqwest::Response, token: Option<&str>) -> Result<()> {
        let Some(token_value) = token else {
            return Ok(());
        };

        let headers = response.headers();

        let remaining = parse_header(headers, self.config.rate_limit_headers.remaining);
        let limit = parse_header(headers, self.config.rate_limit_headers.limit);
        let reset = parse_header(headers, self.config.rate_limit_headers.reset);

        if let (Some(rem_str), Some(lim_str)) = (remaining, limit) {
            let rem = rem_str.parse::<u32>()
                .context("Failed to parse remaining requests")?;
            let lim = lim_str.parse::<u32>()
                .context("Failed to parse limit")?;

            let reset_at = if let Some(ref rst_str) = reset {
                Some(parse_reset_timestamp(rst_str, &self.config.rate_limit_headers.format)?)
            } else {
                None
            };

            self.token_limiter.update_token(token_value, rem, lim, reset_at).await;

            log::debug!(
                "Rate limit for {}: remaining={}, limit={}, reset_at={}",
                self.domain,
                rem,
                lim,
                reset_at.map(|dt| dt.to_string()).unwrap_or_else(|| "none".to_string())
            );
        }

        let status = response.status().as_u16();
        
        // 401 = auth failure, token is invalid
        if status == 401 {
            self.token_limiter.mark_invalid(token_value).await;
        }
        // 429 = rate limited, token is temporarily exhausted
        // 403 with rate limit headers = also rate limited (GitHub does this)
        else if status == 429 || (status == 403 && reset.is_some()) {
            let reset_at = if let Some(rst_str) = reset {
                parse_reset_timestamp(&rst_str, &self.config.rate_limit_headers.format).ok()
            } else {
                None
            };
            self.token_limiter.mark_rate_limited(token_value, reset_at).await;
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

fn parse_header(headers: &reqwest::header::HeaderMap, header_name: &str) -> Option<String> {
    let header_name = normalize_header_name(header_name);

    headers
        .iter()
        .find(|(name, _)| normalize_header_name(name.as_str()) == header_name)
        .and_then(|(_, value)| value.to_str().ok().map(|s| s.to_string()))
}

fn parse_reset_timestamp(value: &str, format: &TimestampFormat) -> Result<DateTime<Utc>> {
    match format {
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

fn normalize_header_name(name: &str) -> String {
    name.to_lowercase()
        .trim_start_matches("x-")
        .replace('-', "_")
}

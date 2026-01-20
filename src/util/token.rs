use chrono::{DateTime, Utc};
use reqwest::Client;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct Token {
    pub value: String,
    pub valid: Option<bool>,
    pub remaining: Option<u32>,
    pub limit: u32,
    pub reset_at: Option<DateTime<Utc>>,
}

impl Token {
    pub fn new(value: String) -> Self {
        Self {
            value,
            valid: None,
            remaining: None,
            limit: 0,
            reset_at: None,
        }
    }

    pub fn is_available(&self) -> bool {
        match (self.valid, self.remaining) {
            (Some(true), Some(rem)) => rem > 0,
            (Some(true), None) => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct TokenRateLimiter {
    tokens: Arc<RwLock<Vec<Token>>>,
    current_index: Arc<AtomicUsize>,
    env_var_name: &'static str,
}

impl TokenRateLimiter {
    pub fn new(env_var_name: &'static str) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(Vec::new())),
            current_index: Arc::new(AtomicUsize::new(0)),
            env_var_name,
        }
    }

    pub async fn load_tokens(&self) {
        let token_values: Vec<String> = std::env::var(self.env_var_name)
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let tokens: Vec<Token> = token_values.into_iter().map(Token::new).collect();

        let mut tokens_lock = self.tokens.write().await;
        *tokens_lock = tokens;
    }

    pub async fn get_next_token(&self) -> Option<String> {
        self.check_reset().await;

        let tokens = self.tokens.read().await;
        let token_count = tokens.len();

        if token_count == 0 {
            return None;
        }

        let all_uninitialized = tokens.iter().all(|t| t.valid.is_none());
        drop(tokens);

        if all_uninitialized {
            return None;
        }

        let tokens = self.tokens.read().await;
        let mut attempts = 0;
        while attempts < token_count {
            let index = self.current_index.fetch_add(1, Ordering::SeqCst) % token_count;
            if let Some(token) = tokens.get(index) {
                if token.is_available() {
                    return Some(token.value.clone());
                }
            }
            attempts += 1;
        }

        None
    }

    pub async fn update_token(
        &self,
        token_value: &str,
        remaining: u32,
        limit: u32,
        reset_at: Option<DateTime<Utc>>,
    ) {
        let mut tokens = self.tokens.write().await;
        if let Some(token) = tokens.iter_mut().find(|t| t.value == token_value) {
            token.remaining = Some(remaining);
            token.limit = limit;
            token.reset_at = reset_at;
            token.valid = Some(true);
        }
    }

    pub async fn mark_invalid(&self, token_value: &str) {
        let mut tokens = self.tokens.write().await;
        if let Some(token) = tokens.iter_mut().find(|t| t.value == token_value) {
            token.valid = Some(false);
            token.remaining = Some(0);
        }
    }

    async fn check_reset(&self) {
        let now = Utc::now();
        let mut tokens = self.tokens.write().await;

        for token in tokens.iter_mut() {
            if let Some(reset_at) = token.reset_at {
                if now > reset_at {
                    token.remaining = Some(token.limit);
                    token.reset_at = None;
                }
            }
        }
    }

    pub async fn all_tokens_exhausted(&self) -> bool {
        let tokens = self.tokens.read().await;
        tokens.iter().all(|t| !t.is_available())
    }

    pub async fn token_count(&self) -> usize {
        self.tokens.read().await.len()
    }

    pub async fn validate_all_tokens(&self, validation_url: &str, rate_limit_headers: Option<&(String, String, String)>) {
        let tokens = self.tokens.read().await;
        if tokens.is_empty() {
            return;
        }

        let client = Client::new();
        let handles: Vec<_> = tokens.iter().map(|token| {
            let client = client.clone();
            let token_value = token.value.clone();
            let validation_url = validation_url.to_string();
            let rate_limit_headers = rate_limit_headers.map(|h| (h.0.clone(), h.1.clone(), h.2.clone()));

            tokio::spawn(async move {
                Self::validate_token(&client, &token_value, &validation_url, rate_limit_headers).await
            })
        }).collect();

        let results = futures::future::join_all(handles).await;

        let mut tokens_write = self.tokens.write().await;
        for (i, result) in results.into_iter().enumerate() {
            if let Ok(Some((remaining, limit, reset_at))) = result {
                if let Some(token) = tokens_write.get_mut(i) {
                    token.remaining = Some(remaining);
                    token.limit = limit;
                    token.reset_at = reset_at;
                    token.valid = Some(true);
                }
            } else if let Some(token) = tokens_write.get_mut(i) {
                token.valid = Some(false);
                token.remaining = Some(0);
            }
        }
    }

    async fn validate_token(
        client: &Client,
        token_value: &str,
        validation_url: &str,
        rate_limit_headers: Option<(String, String, String)>,
    ) -> Option<(u32, u32, Option<DateTime<Utc>>)> {
        let mut request = client.get(validation_url);
        request = request.header("Authorization", format!("Bearer {}", token_value));

        let response = request.send().await.ok()?;

        if !response.status().is_success() {
            return None;
        }

        if let Some((remaining_header, limit_header, reset_header)) = rate_limit_headers {
            let remaining = response
                .headers()
                .get(&remaining_header)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u32>().ok())?;

            let limit = response
                .headers()
                .get(&limit_header)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u32>().ok())?;

            let reset_at = response
                .headers()
                .get(&reset_header)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<i64>().ok())
                .map(|ts| DateTime::from_timestamp(ts, 0).unwrap());

            Some((remaining, limit, reset_at))
        } else {
            Some((0, 0, None))
        }
    }
}

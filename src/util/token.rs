use chrono::{DateTime, Utc};
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use reqwest::Client;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

type Governor = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

#[derive(Debug)]
pub struct Token {
    pub value: String,
    pub valid: Option<bool>,
    pub remaining: Option<u32>,
    pub limit: u32,
    pub reset_at: Option<DateTime<Utc>>,
    governor: Option<Arc<Governor>>,
    governor_computed_from: Option<(u32, DateTime<Utc>)>,
}

impl Clone for Token {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            valid: self.valid,
            remaining: self.remaining,
            limit: self.limit,
            reset_at: self.reset_at,
            governor: self.governor.clone(),
            governor_computed_from: self.governor_computed_from,
        }
    }
}

impl Token {
    pub fn new(value: String) -> Self {
        Self {
            value,
            valid: None,
            remaining: None,
            limit: 0,
            reset_at: None,
            governor: None,
            governor_computed_from: None,
        }
    }

    pub fn is_available(&self) -> bool {
        match (self.valid, self.remaining) {
            (Some(true), Some(rem)) => rem > 0,
            (Some(true), None) => true,
            _ => false,
        }
    }

    pub fn get_or_rebuild_governor(&mut self, velocity: f64) -> Option<Arc<Governor>> {
        let (remaining, reset_at) = match (self.remaining, self.reset_at) {
            (Some(r), Some(t)) => (r, t),
            _ => return self.governor.clone(),
        };

        let should_rebuild = match &self.governor_computed_from {
            None => true,
            Some((old_rem, old_reset)) => {
                *old_reset != reset_at || remaining < old_rem.saturating_sub(100)
            }
        };

        if should_rebuild {
            let now = Utc::now();
            let seconds_until_reset = (reset_at - now).num_seconds().max(1) as f64;
            let rps = ((remaining as f64 / seconds_until_reset) * velocity).max(1.0) as u32;
            let rps = NonZeroU32::new(rps.max(1)).unwrap();
            
            let quota = Quota::per_second(rps);
            let new_gov = Arc::new(RateLimiter::direct(quota));
            
            self.governor = Some(new_gov);
            self.governor_computed_from = Some((remaining, reset_at));
            
            log::debug!(
                "Rebuilt governor for token: rps={}, remaining={}, seconds_until_reset={}",
                rps, remaining, seconds_until_reset
            );
        }

        self.governor.clone()
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

    pub async fn get_next_token_with_governor(&self, velocity: f64) -> Option<(String, Option<Arc<Governor>>)> {
        self.check_reset().await;

        let mut tokens = self.tokens.write().await;
        let token_count = tokens.len();

        if token_count == 0 {
            return None;
        }

        let all_uninitialized = tokens.iter().all(|t| t.valid.is_none());
        if all_uninitialized {
            return None;
        }

        let mut attempts = 0;
        while attempts < token_count {
            let index = self.current_index.fetch_add(1, Ordering::SeqCst) % token_count;
            if let Some(token) = tokens.get_mut(index) {
                if token.is_available() {
                    let gov = token.get_or_rebuild_governor(velocity);
                    return Some((token.value.clone(), gov));
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

    pub async fn mark_rate_limited(&self, token_value: &str, reset_at: Option<DateTime<Utc>>) {
        let mut tokens = self.tokens.write().await;
        if let Some(token) = tokens.iter_mut().find(|t| t.value == token_value) {
            token.remaining = Some(0);
            token.valid = Some(true);  // Still valid, just exhausted
            if let Some(reset) = reset_at {
                token.reset_at = Some(reset);
            } else {
                // Conservative backoff: 60 seconds if no reset time provided
                token.reset_at = Some(Utc::now() + chrono::Duration::seconds(60));
            }
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

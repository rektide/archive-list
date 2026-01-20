use chrono::{DateTime, Utc};
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
}

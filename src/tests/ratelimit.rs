use crate::util::TokenRateLimiter;
use chrono::{Duration, Utc};

#[tokio::test]
async fn test_token_creation() {
    let token = crate::util::Token::new("test_token".to_string());

    assert_eq!(token.value, "test_token");
    assert_eq!(token.valid, None);
    assert_eq!(token.remaining, None);
    assert_eq!(token.limit, 0);
    assert_eq!(token.reset_at, None);
}

#[tokio::test]
async fn test_token_is_available() {
    let mut token = crate::util::Token::new("test_token".to_string());

    // Uninitialized token is not available
    assert!(!token.is_available());

    // Valid token with no remaining info is available
    token.valid = Some(true);
    assert!(token.is_available());

    // Valid token with remaining > 0 is available
    token.remaining = Some(10);
    assert!(token.is_available());

    // Valid token with remaining = 0 is not available
    token.remaining = Some(0);
    assert!(!token.is_available());

    // Invalid token is not available
    token.valid = Some(false);
    token.remaining = Some(10);
    assert!(!token.is_available());
}

#[tokio::test]
async fn test_rate_limiter_loads_tokens() {
    std::env::set_var("TEST_TOKEN", "token1,token2,token3");

    let limiter = TokenRateLimiter::new("TEST_TOKEN");
    limiter.load_tokens().await;

    assert_eq!(limiter.token_count().await, 3);

    std::env::remove_var("TEST_TOKEN");
}

#[tokio::test]
async fn test_rate_limiter_round_robin() {
    std::env::set_var("TEST_TOKEN", "token1,token2,token3");

    let limiter = TokenRateLimiter::new("TEST_TOKEN");
    limiter.load_tokens().await;

    // Initialize tokens via update
    limiter.update_token("token1", 100, 5000, None).await;
    limiter.update_token("token2", 100, 5000, None).await;
    limiter.update_token("token3", 100, 5000, None).await;

    // Round-robin should cycle through tokens
    let token1 = limiter.get_next_token().await;
    let token2 = limiter.get_next_token().await;
    let token3 = limiter.get_next_token().await;
    let token4 = limiter.get_next_token().await;

    // Check order (may wrap around)
    let tokens = vec![token1, token2, token3, token4];
    assert_eq!(tokens.len(), 4);
    assert!(tokens.contains(&Some("token1".to_string())));
    assert!(tokens.contains(&Some("token2".to_string())));
    assert!(tokens.contains(&Some("token3".to_string())));

    std::env::remove_var("TEST_TOKEN");
}

#[tokio::test]
async fn test_rate_limiter_skips_invalid_tokens() {
    std::env::set_var("TEST_TOKEN", "token1,token2,token3");

    let limiter = TokenRateLimiter::new("TEST_TOKEN");
    limiter.load_tokens().await;

    // Mark token2 as invalid by setting remaining to 0
    limiter.update_token("token1", 10, 5000, None).await;
    limiter.update_token("token2", 0, 5000, None).await;
    limiter.mark_invalid("token2").await;
    limiter.update_token("token3", 10, 5000, None).await;

    let token1 = limiter.get_next_token().await;
    let token2 = limiter.get_next_token().await;
    let token3 = limiter.get_next_token().await;

    // Should get 3 tokens, none should be token2 (invalid)
    let tokens = vec![token1, token2, token3];
    assert_eq!(tokens.len(), 3);
    assert!(tokens.iter().all(|t| t.is_some()));
    assert!(!tokens.contains(&Some("token2".to_string())));

    std::env::remove_var("TEST_TOKEN");
}

#[tokio::test]
async fn test_rate_limiter_skips_exhausted_tokens() {
    std::env::set_var("TEST_TOKEN", "token1,token2,token3");

    let limiter = TokenRateLimiter::new("TEST_TOKEN");
    limiter.load_tokens().await;

    // Mark token2 as exhausted
    limiter.update_token("token1", 10, 5000, None).await;
    limiter.update_token("token2", 0, 5000, None).await;
    limiter.update_token("token3", 10, 5000, None).await;

    let token1 = limiter.get_next_token().await;
    let token2 = limiter.get_next_token().await;
    let token3 = limiter.get_next_token().await;

    // Should skip token2 (exhausted)
    assert_eq!(token1, Some("token1".to_string()));
    assert_eq!(token2, Some("token3".to_string()));
    assert_eq!(token3, Some("token1".to_string()));

    std::env::remove_var("TEST_TOKEN");
}

#[tokio::test]
async fn test_rate_limiter_update_token() {
    std::env::set_var("TEST_TOKEN", "token1");

    let limiter = TokenRateLimiter::new("TEST_TOKEN");
    limiter.load_tokens().await;

    // Update token state
    let now = Utc::now();
    limiter.update_token("token1", 100, 5000, Some(now + Duration::seconds(3600))).await;

    // Token should not be exhausted (has remaining > 0)
    assert!(!limiter.all_tokens_exhausted().await);

    std::env::remove_var("TEST_TOKEN");
}

#[tokio::test]
async fn test_rate_limiter_reset_logic() {
    std::env::set_var("TEST_TOKEN", "token1");

    let limiter = TokenRateLimiter::new("TEST_TOKEN");
    limiter.load_tokens().await;

    // Set reset time in the past
    let past = Utc::now() - Duration::seconds(3600);
    limiter.update_token("token1", 0, 5000, Some(past)).await;

    // Check that token is exhausted
    assert!(limiter.all_tokens_exhausted().await);

    // Trigger reset check (happens automatically in get_next_token)
    limiter.get_next_token().await;

    // Token should now be available
    assert!(!limiter.all_tokens_exhausted().await);

    std::env::remove_var("TEST_TOKEN");
}

#[tokio::test]
async fn test_rate_limiter_mark_invalid() {
    std::env::set_var("TEST_TOKEN", "token1");

    let limiter = TokenRateLimiter::new("TEST_TOKEN");
    limiter.load_tokens().await;

    limiter.update_token("token1", 100, 5000, None).await;

    // Should not be exhausted
    assert!(!limiter.all_tokens_exhausted().await);

    limiter.mark_invalid("token1").await;

    // Should now be exhausted (token invalid)
    assert!(limiter.all_tokens_exhausted().await);

    std::env::remove_var("TEST_TOKEN");
}

#[tokio::test]
async fn test_rate_limiter_all_tokens_exhausted() {
    std::env::set_var("TEST_TOKEN", "token1,token2");

    let limiter = TokenRateLimiter::new("TEST_TOKEN");
    limiter.load_tokens().await;

    // Exhausted initially (tokens are uninitialized)
    assert!(limiter.all_tokens_exhausted().await);

    // Initialize tokens
    limiter.update_token("token1", 10, 5000, None).await;
    limiter.update_token("token2", 20, 5000, None).await;

    // Now not exhausted
    assert!(!limiter.all_tokens_exhausted().await);

    // Exhaust both tokens
    limiter.update_token("token1", 0, 5000, None).await;
    limiter.update_token("token2", 0, 5000, None).await;

    // Now exhausted again
    assert!(limiter.all_tokens_exhausted().await);

    std::env::remove_var("TEST_TOKEN");
}

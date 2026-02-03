use reqgov::{ConcurrencyRateLimiter, ResponseAdapter};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use std::sync::Arc;

pub fn create_client() -> ClientWithMiddleware {
    let rate_limiter = ConcurrencyRateLimiter::builder()
        .max_concurrent_global(10)
        .max_concurrent_per_domain(2)
        .build();

    ClientBuilder::new(reqwest::Client::new())
        .with(rate_limiter)
        .with(ResponseAdapter)
        .build()
}

pub fn create_shared_client() -> Arc<ClientWithMiddleware> {
    Arc::new(create_client())
}

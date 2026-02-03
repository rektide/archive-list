use reqgov::{ConcurrencyRateLimiter, OriginRegistry, ResponseAdapter, SmootherConfig};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use std::sync::Arc;

pub fn create_client() -> ClientWithMiddleware {
    let smoother_config = SmootherConfig {
        micro_interval_secs: 2,
        velocity: 1.5,
    };

    let origin_registry = OriginRegistry::builder().smoother(smoother_config).build();

    let concurrency_limiter = ConcurrencyRateLimiter::builder()
        .max_concurrent_global(10)
        .max_concurrent_per_domain(2)
        .build();

    ClientBuilder::new(reqwest::Client::new())
        .with(origin_registry)
        .with(ResponseAdapter)
        .with(concurrency_limiter)
        .build()
}

pub fn create_shared_client() -> Arc<ClientWithMiddleware> {
    Arc::new(create_client())
}

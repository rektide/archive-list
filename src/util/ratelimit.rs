use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};

pub fn create_rate_limiter(requests_per_second: u32) -> RateLimiter<NotKeyed, InMemoryState, DefaultClock> {
    let quota = Quota::per_second(u32::max(1, requests_per_second).try_into().unwrap());
    RateLimiter::direct(quota)
}

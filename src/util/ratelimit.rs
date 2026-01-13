use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};

pub fn create_rate_limiter(requests_per_second: u32) -> RateLimiter<NotKeyed, InMemoryState, DefaultClock> {
    let velocity = (requests_per_second as f64 * 1.5) as u32;
    let requests = u32::max(1, velocity);
    let quota = Quota::per_second(requests.try_into().unwrap());
    RateLimiter::direct(quota)
}

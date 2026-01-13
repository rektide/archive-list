# ADR 0001: Provider Architecture and Rate Limiting

## Status
Accepted

## Context
The original implementation used fixed rate limits with the `governor` crate, initialized with arbitrary values (90 req/s for GitHub, 3000 req/s for HuggingFace, etc.). This approach had several limitations:

1. **No awareness of actual API limits**: The system didn't know the real rate limits enforced by each provider
2. **No support for authentication**: API tokens were not supported, meaning we were limited to unauthenticated request rates
3. **No quota tracking**: We couldn't track how many requests we had remaining
4. **No token rotation**: Multiple API tokens could not be used to increase total quota
5. **No reset awareness**: The system didn't know when rate limits would reset
6. **Fixed limits**: Different API tiers and authentication levels have different limits that we couldn't adapt to

## Decision

### Core Architecture

We will implement a **token-aware rate limiting system** with the following characteristics:

1. **Multiple token support** per provider, loaded from environment variables
2. **Actual rate limit tracking** by parsing response headers
3. **Round-robin token rotation** among tokens with remaining quota
4. **Reset awareness** with timezone-aware timestamp parsing
5. **Uninitialized state** that populates on first API call
6. **Fast rate limiting** using governor to spread load across reset interval

### Provider Interface

```rust
pub trait Provider: Send + Sync {
    async fn detect(url: &str) -> Option<Box<dyn Provider>>
    where
        Self: Sized;

    async fn get_readme_url(&self, url: &str) -> anyhow::Result<String>;

    async fn fetch_url(&self, url: &str) -> anyhow::Result<reqwest::Response> {
        // Get next token via round-robin, make request, update rate limits from headers
    }

    fn name(&self) -> &'static str;
}
```

### Token Management

Each provider will have a `RateLimiter` that manages multiple tokens:

```rust
struct Token {
    value: String,
    valid: bool,
    remaining: Option<u32>,
    limit: u32,
    reset_at: Option<DateTime<Utc>>,
}

struct RateLimiter {
    tokens: Vec<Token>,
    env_var_name: &'static str,
    // ... other fields
}
```

**Token Loading:**
- Tokens loaded from environment variables at initialization
- Format: `GITHUB_TOKEN=token1,token2,token3` (comma-separated)
- Uninitialized tokens start with `valid: undefined`, `remaining: None`

**Token Selection:**
- Round-robin through tokens with `remaining > 0`
- Filter out invalid tokens
- Maintain a current token index that increments on each request
- Skip exhausted tokens (remaining = 0) in rotation
- If all tokens exhausted, trigger revalidation

**Token Validation:**
- On first use, make a lightweight API call to validate token and get rate limits
- Parse rate limit headers from response
- Update token state

### Rate Limit Tracking

**GitHub Headers:**
- `x-ratelimit-remaining`: Requests remaining this hour
- `x-ratelimit-limit`: Total requests per hour limit
- `x-ratelimit-reset`: Unix timestamp when limit resets

**GitLab Headers:**
- `RateLimit-Remaining`: Requests remaining
- `RateLimit-Limit`: Total limit
- `RateLimit-Reset`: Unix timestamp when limit resets

**HuggingFace Headers:**
- `X-RateLimit-Limit`: Total requests per minute
- `X-RateLimit-Remaining`: Requests remaining
- `X-RateLimit-Reset`: Unix timestamp when limit resets

**Codeberg Headers:**
- Uses GitLab-compatible headers

**Reset Behavior:**
- Parse reset timestamp from header (Unix epoch or datetime)
- Convert to UTC timezone if needed
- When `now > reset_at`, reset `remaining` to `limit` for all tokens
- Store `reset_at` per-token if provider supports it

**Header Parsing:**
- Common header decoder converts headers to lowercase
- Ignores leading `x-` prefix for normalization
- Allows providers to parse `X-RateLimit-Limit` as `ratelimit-limit` or `RateLimit-Limit`
- Handles case-insensitive header matching

**Fast Rate Limiting with governor:**
- Used to spread load across the reset interval
- Per-second rate limit with "velocity" multiplier
- Allows requests faster than realtime initially (e.g., 1.5x or 2x velocity)
- Prevents firing all requests immediately after reset
- Governor is a safety net, actual API limits are enforced by header tracking

### Rate Limiter Initialization

**Uninitialized State:**
- Rate limiter starts with no tokens loaded and unknown limits
- First API call through `get_url()` triggers initialization
- After initialization, subsequent requests use known limits

**Lazy Initialization:**
- Tokens loaded on first request
- Validation happens concurrently for all tokens using `tokio::spawn`
- State becomes fully populated after validation completes

### Request Flow

1. **Check fast rate limiter (governor):**
    - Wait if per-second limit exceeded
    - Velocity allows faster-than-realtime rate until API limit hit

2. **Get next token via round-robin:**
    - Increment token index
    - Skip invalid or exhausted tokens
    - If no tokens available, trigger revalidation

3. **Validate tokens (if needed):**
    - Make parallel validation requests for all tokens
    - Update token state from response headers
    - Retry token selection

4. **Make authenticated request:**
    - Add Authorization header with selected token
    - Send request to target URL

5. **Update rate limits:**
    - Parse rate limit headers using common decoder
    - Update token's `remaining`, `limit`, `reset_at`
    - Handle rate limit errors (403, 429)

6. **Handle reset:**
    - If `now > reset_at`, reset `remaining = limit`
    - Automatically happens before token selection

### Environment Variables

Each provider reads its own environment variable:

- `GITHUB_TOKEN`: GitHub Personal Access Tokens (comma-separated)
- `GITLAB_TOKEN`: GitLab Personal Access Tokens
- `HUGGINGFACE_TOKEN`: HuggingFace API tokens
- `CODEBERG_TOKEN`: Codeberg access tokens

If no tokens are provided, the provider falls back to unauthenticated requests with lower limits.

### Error Handling

**No Valid Tokens:**
- Return error if all tokens are invalid or exhausted
- Suggest user check environment variables

**Rate Limit Exceeded:**
- If response is 403/429 with rate limit headers, sleep until reset
- On next request, reset logic should already have kicked in

**Validation Failures:**
- Mark token as `valid = false`
- Set `remaining = 0`
- Token won't be selected until revalidated

## Rationale

### Why This Design?

1. **Aligns with actual API behavior**: Real APIs use rate limit headers to communicate quotas
2. **Supports authentication**: Essential for higher API limits (GitHub: 5000/hr vs 60/hr)
3. **Token rotation**: Spreads load across multiple tokens, preventing single-token exhaustion
4. **Reset awareness**: Avoids unnecessary waiting when limits have reset
5. **Lazy initialization**: No upfront API calls, state builds up as needed

### Why Keep governor?

The `governor` crate is kept for **fast rate limiting** to spread load across the reset interval:

1. **Prevents burst behavior**: Without governor, we could fire all 5000 requests immediately after reset
2. **Velocity allows efficiency**: 1.5x-2x velocity lets us go faster than realtime initially
3. **Simplifies logic**: Governor handles timing, while header tracking handles actual limits
4. **Safety net**: Governor ensures we don't accidentally overwhelm the API

**Two-tier rate limiting:**
- **Fast tier** (governor): Per-second limit with velocity, spreads load
- **Slow tier** (headers): Per-hour/minute API limits, enforced by API
- Combined: Fast rate spreads work across slow interval

### Why Uninitialized State?

Starting uninitialized avoids:
- Blocking startup on API validation calls
- Making unnecessary API calls for tokens that won't be used
- Complexity of initialization order

The first request naturally triggers initialization when needed.

### Why Timezone Awareness?

APIs may return reset timestamps in different formats:
- Unix epoch (always UTC)
- ISO 8601 with timezone offset (e.g., `2024-01-13T12:00:00Z` or `2024-01-13T12:00:00-08:00`)
- Provider-specific formats

Using `chrono` with proper timezone handling ensures we:
- Parse any timestamp format correctly
- Compare timestamps in consistent UTC timezone
- Handle daylight saving time and timezones properly

## Alternatives Considered

### Alternative 1: Fixed Limits per Provider
**Pros:** Simple, no API calls needed
**Cons:** Doesn't adapt to different auth levels, no quota tracking, no reset awareness

**Rejected:** Too inflexible for production use with API tokens

### Alternative 2: Global Rate Limiter
**Pros:** Shared state across all providers
**Cons:** Doesn't account for provider-specific limits, harder to reason about

**Rejected:** Each provider has its own rate limits, should be managed separately

### Alternative 3: Eager Initialization
**Pros:** All tokens validated before first request
**Cons:** Blocks startup, unnecessary API calls if provider not used

**Rejected:** Lazy initialization is more efficient and doesn't block startup

## Implementation Phases

### Phase 1: Core Infrastructure (High Priority)
1. Create Token and RateLimiter structs
2. Implement token loading from environment variables
3. Implement token validation via API calls
4. Implement token selection algorithm

### Phase 2: Provider Integration (High Priority)
1. Rename `get_readme()` to `get_readme_url()` and update implementations
2. Add `fetch_url()` method to Provider trait
3. Create common header decoder (lowercase, strip `x-` prefix)
4. Update GitHub provider with header parsing and governor integration
5. Update GitLab provider with header parsing and governor integration
6. Update HuggingFace provider with header parsing and governor integration
7. Update Codeberg provider with header parsing and governor integration

### Phase 3: Reset Logic (Medium Priority)
1. Implement reset timestamp parsing with timezone handling
2. Implement reset logic (check before token selection)
3. Handle different timestamp formats per provider

### Phase 4: Integration (Medium Priority)
1. Update `readme_get.rs` to use `fetch_url()` instead of `get_readme()`
2. Implement round-robin token rotation
3. Test with multiple tokens to verify rotation

### Phase 5: Testing (Low Priority)
1. Test token rotation
2. Test rate limit handling across resets
3. Test error scenarios (invalid tokens, exhausted tokens)
4. Performance testing

## Future Considerations

1. **Token refresh**: Some APIs have refresh tokens that need periodic refresh
2. **Token pools**: Load tokens from config file, not just environment variables
3. **Persistent state**: Save token state to disk to avoid revalidation on restart
4. **Request coalescing**: Deduplicate concurrent requests to same URL
5. **Request queueing**: Queue requests when rate limited, retry after reset
6. **Adaptive velocity**: Adjust velocity multiplier based on actual API behavior
7. **Header caching**: Cache parsed headers to avoid repeated string operations

## References

- `.test-agent/RATELIMIT.md` - Reference implementation in TypeScript
- GitHub API rate limit docs: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api
- GitLab API rate limit docs: https://docs.gitlab.com/ee/user/gitlab_com/#gitlabcom-specific-rate-limits
- HuggingFace API rate limit docs: https://huggingface.co/docs/api-inference/index#rate-limits
- Codeberg API: Compatible with GitLab API

## History

- **2024-01-13**: Initial ADR created
- **2024-01-13**: Updated to use round-robin token rotation, common header decoder, and governor for fast rate limiting. Renamed methods: `get_readme` → `get_readme_url`, `get_url` → `fetch_url`

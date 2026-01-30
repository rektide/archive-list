# Adaptive Rate Limiting

The rate limiter automatically detects and adapts to API rate limits from response headers, eliminating the need for hardcoded per-provider configuration.

## How It Works

### Header Auto-Detection

When a response is received, the system probes for common rate limit header patterns:

**Remaining requests:**
- `x-ratelimit-remaining`
- `ratelimit-remaining`
- `x-rate-limit-remaining`
- `rate-limit-remaining`
- `x-ratelimit-requests-remaining`

**Total limit:**
- `x-ratelimit-limit`
- `ratelimit-limit`
- `x-rate-limit-limit`
- `rate-limit-limit`
- `x-ratelimit-requests-limit`

**Reset time:**
- `x-ratelimit-reset`
- `ratelimit-reset`
- `x-rate-limit-reset`
- `rate-limit-reset`
- `x-ratelimit-reset-after`
- `retry-after`

Header matching is case-insensitive and normalizes variations (e.g., `X-RateLimit-Remaining` matches `x-ratelimit-remaining`).

### Reset Time Parsing

The system handles multiple reset time formats:

| Format | Example | Interpretation |
|--------|---------|----------------|
| Unix timestamp | `1704067200` | Absolute time (values > 1 billion) |
| Seconds from now | `60` | Relative time (used with `retry-after` or `-after` headers) |
| ISO 8601 | `2024-01-13T12:00:00Z` | Parsed via RFC 3339 |
| HTTP date | `Sat, 13 Jan 2024 12:00:00 GMT` | Parsed via RFC 2822 |

### Fallback Defaults

When no rate limit headers are detected, conservative defaults are used:

- **Remaining:** 60 requests
- **Limit:** 60 requests
- **Reset:** None (no pacing applied)

This allows the system to work with APIs that don't provide rate limit headers while avoiding aggressive request rates.

## Per-Token Governors

Each API token has its own rate limiter (governor) that spreads requests across the reset interval:

```
rate = (remaining / seconds_until_reset) × velocity
```

Where `velocity` is a multiplier (default 1.5×) that allows faster-than-realtime pacing initially.

### Governor Rebuild Triggers

The per-token governor is rebuilt when:
- First request with the token (no governor exists)
- Reset time changes (new quota window)
- Remaining drops significantly (100+ requests consumed since last rebuild)

This avoids rebuilding on every request while adapting to quota changes.

## Custom Domain Support

Unknown domains are automatically supported:

1. **Default config generated:** Environment variable inferred from domain name (e.g., `git.example.com` → `GIT_TOKEN`)
2. **Tokens marked valid without validation:** No upfront API call required
3. **Rate limits populated on first request:** Auto-detection extracts limits from response headers
4. **Conservative pacing applied:** If no headers found, uses 60 req/min default

### Example: Custom GitLab Instance

```bash
export GIT_TOKEN=glpat-xxxxxxxxxxxx
archive-list https://git.company.com/team/project
```

The system will:
1. Create a provider for `git.company.com`
2. Load token from `GIT_TOKEN` environment variable
3. Make first request, detect `RateLimit-*` headers from response
4. Configure per-token governor based on detected limits

## Token State Machine

Tokens have three states:

| State | `valid` | `remaining` | Behavior |
|-------|---------|-------------|----------|
| Uninitialized | `None` | `None` | Awaiting first use |
| Available | `Some(true)` | `> 0` | Ready for requests |
| Exhausted | `Some(true)` | `0` | Wait until `reset_at` |
| Invalid | `Some(false)` | `0` | Permanently disabled |

### State Transitions

- **401 response:** Token marked invalid (auth failure)
- **429 response:** Token marked exhausted until reset
- **403 with rate headers:** Token marked exhausted (GitHub-style rate limit)
- **Successful response:** Token updated with detected limits
- **Reset time passed:** Token remaining restored to limit

## Unauthenticated Fallback

When no tokens are available, requests proceed unauthenticated:

- No `Authorization` header sent
- Provider-level conservative rate limiting applies
- Lower limits expected (e.g., GitHub: 60/hr vs 5000/hr authenticated)

This allows the tool to work without configuration, albeit with reduced throughput.

## Implementation

Key files:
- `src/util/ratelimit_headers.rs` — Header detection and parsing
- `src/util/token.rs` — Token state and per-token governors
- `src/provider/generic.rs` — Request flow and state updates
- `src/provider/domain.rs` — Domain configuration and defaults

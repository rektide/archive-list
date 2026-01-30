# Early Implementation Review

Review of codebase against ADR-0001 (Provider Architecture and Rate Limiting) and ADR-0002 (Generic Provider with Strategy Pattern).

## Critical Issues

### 1. 429/403 Permanently Burns Tokens

In `generic.rs`:
```rust
if response.status().as_u16() == 403 || response.status().as_u16() == 429 {
    self.token_limiter.mark_invalid(token_value).await;
}
```

**Problem:** Rate limiting (429) should pause until reset, not invalidate the token. This destroys the entire token pool under normal load.

**Fix:** Add `mark_rate_limited(token, reset_at)` that sets `remaining=0, valid=true, reset_at=parsed_reset`. Only `mark_invalid()` on actual auth failures (401, or 403 with clear auth error body).

### 2. Fast Limiter is Broken by Design

**ADR Intent:** "Fast rate limiting using governor to spread load across reset interval" — meaning calculate `rps = remaining / seconds_until_reset * velocity`.

**Current Implementation:** One fixed 60 rps (or 10 rps) governor per provider, never updated from headers.

**Result:** Bursts all requests immediately, hits rate limit, marks tokens invalid, system fails.

**Fix:** Store `Arc<RateLimiter>` per token. Recalculate when headers update:
```rust
let seconds_until_reset = (reset_at - now).num_seconds().max(1);
let rps = (remaining as f64 / seconds_until_reset as f64 * velocity).max(1.0);
```

### 3. No Unauthenticated Fallback

**ADR says:** "If no tokens are provided, the provider falls back to unauthenticated requests with lower limits."

**Current behavior:** Returns `Err("No valid tokens available")`.

**Fix:** If `token_count == 0`, proceed with `token=None` and conservative provider-level rate limiting.

### 4. Governor Polling is Wasteful

```rust
while !crate::util::ratelimit::is_ok(&self.rate_limiter) {
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
}
```

**Problems:**
- 2-second granularity wastes quota time
- Creates thundering herd (many tasks wake every 2s)
- Governor has async APIs for this

**Fix:** Use `rate_limiter.until_ready().await` instead.

### 5. Custom Domains Break Permanently

In `validate_token()`, when `rate_limit_headers` is `None`:
```rust
Some((0, 0, None))  // returns remaining=0, limit=0
```

Then `validate_all_tokens()` sets `token.remaining = Some(0), valid = true`.

`Token::is_available()` checks `rem > 0` → returns false forever.

**Fix:** If can't validate, leave tokens as `valid=None, remaining=None` and try them; update from headers on first real response.

## Architecture Gaps vs ADRs

| ADR Requirement | Status | Notes |
|-----------------|--------|-------|
| Fast limiter per token with velocity | ❌ | One fixed limiter per provider |
| Recalculate rate from remaining/reset | ❌ | Never done |
| Sleep until reset on 429 | ❌ | Marks token invalid instead |
| Unauthenticated fallback | ❌ | Errors instead of falling back |
| Lazy init (populate on first call) | ⚠️ | Forces validation first because `get_next_token()` returns None when all uninitialized |
| Round-robin through available tokens | ✅ | Implemented correctly |
| Token loading from env vars | ✅ | Works |
| Header parsing with normalization | ✅ | Works |
| Reset awareness (check before selection) | ✅ | `check_reset()` implemented |
| Strategy pattern with lazy discovery | ✅ | Works for `get_readme_url()` |
| Provider caching in factory | ✅ | Implemented |

## Code Quality Issues

### A. Timestamp Parsing Can Panic

```rust
Ok(DateTime::from_timestamp(timestamp, 0).unwrap())
```

Out-of-range timestamps will panic. Use `ok_or_else()` instead.

### B. "All Tokens Uninitialized => None" is Awkward

`get_next_token()` returns `None` if all tokens have `valid == None`. Forces up-front validation, contradicting ADR's "populate on first API call" intent.

### C. Strategy Fallback Only Works for URL Discovery

`get_readme_url()` tries all strategies on failure. `fetch_url()` uses cached strategy or `strategies.first()` and does not retry others if `get_url()` fails.

### D. Auth Header Format Not Configurable

Always uses `Authorization: Bearer <token>`. GitLab PATs often need `PRIVATE-TOKEN` header instead.

## Recommended Fix Priority

1. **Fix token state machine** — Distinguish rate-limited vs invalid
2. **Per-token governor** — Store in `Token`, recompute from headers  
3. **Add unauthenticated mode** — Proceed when no tokens
4. **Use governor's async API** — `until_ready()` instead of polling
5. **Fix validation for unknown domains** — Don't set remaining=0

## Effort Estimate

Minimal fixes to match ADR intent: ~1-2 days of focused work.

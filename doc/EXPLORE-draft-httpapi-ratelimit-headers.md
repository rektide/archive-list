# RateLimit Headers for HTTP - Draft Summary

**Document:** draft-ietf-httpapi-ratelimit-headers-10  
**Status:** Active Internet-Draft (httpapi WG)  
**Expires:** March 31, 2026

---

## Quick Start

HTTP APIs use rate limiting to prevent overload. This draft standardizes two headers that let servers communicate their limits to clients:

**`RateLimit-Policy`** - Static rules that tell clients *what* the limits are  
**`RateLimit`** - Dynamic state that tells clients *where they stand right now*

When a client makes a request, the server can return both headers. Clients use this information to throttle themselves before hitting limits, avoiding 429/503 errors.

**Example response:**
```http
RateLimit-Policy: "default";q=100;w=60
RateLimit: "default";r=45;t=55
```

Translation: This endpoint allows 100 requests per 60 seconds. You have 45 remaining, and the counter resets in 55 seconds.

---

## Practical Introduction

### The Two Headers

#### RateLimit-Policy (Static Configuration)

This header describes the server's quota policies. It's typically consistent across responses.

**Syntax:** List of policy items, each with:
- Policy name (string identifier)
- `q` - Quota amount (required)
- `w` - Time window in seconds (optional)
- `qu` - Quota unit: `requests`, `content-bytes`, or `concurrent-requests` (optional, defaults to `requests`)
- `pk` - Partition key for multi-tenant scenarios (optional)

**Examples:**
```http
# Simple: 100 requests per minute
RateLimit-Policy: "default";q=100;w=60

# Multiple policies: burst + daily limits
RateLimit-Policy: "burst";q=100;w=60,"daily";q=1000;w=86400

# Per-user partitioning with content-based quota
RateLimit-Policy: "peruser";q=65535;qu="content-bytes";w=10;pk=:cHsdsRa894==:
```

#### RateLimit (Dynamic State)

This header reports current service limits. Values change with every request.

**Syntax:** List of service limit items, each with:
- Policy name (references RateLimit-Policy)
- `r` - Remaining quota units (required)
- `t` - Seconds until quota resets (optional)
- `pk` - Partition key (optional)

**Examples:**
```http
# 50 requests remaining, resets in 30 seconds
RateLimit: "default";r=50;t=30

# No reset time specified
RateLimit: "default";r=999;pk=:dHJpYWwxMjEzMjM=:
```

### Key Behaviors

**Servers MAY:**
- Return these headers in any response (success or error)
- Adjust values dynamically based on load
- Skip sending headers entirely for performance
- Use partition keys to differentiate limits by user/resource

**Clients SHOULD:**
- Slow down when `r` approaches zero
- Wait until `t` seconds pass before retrying when throttled
- Not assume headers will always be present
- Not assume remaining quota guarantees future requests will succeed

**Precedence:** If both `Retry-After` and `RateLimit` appear, `Retry-After` takes priority.

---

## Deeper Details

### Partition Keys (`pk`)

Servers can divide capacity across different clients/users using partition keys. The key is a base64-encoded byte sequence that clients can use to:
- Track quota across multiple policies
- Predict if a future request will succeed
- Manage concurrent request limits

If the server documents its key generation algorithm, clients can compute keys for future requests and check remaining quota before sending.

### Problem Types

The draft defines three standardized problem types for rate limit violations:

1. **`quota-exceeded`** - Standard 429 when quota is exhausted
2. **`temporary-reduced-capacity`** - 503 when server is temporarily overloaded
3. **`abnormal-usage-detected`** - 429 when suspicious traffic patterns detected

Each includes a `violated-policies` array naming which policies were exceeded.

### Quota Units

Beyond simple request counting:

- **`requests`** - Count HTTP requests (default)
- **`content-bytes`** - Limit by payload size (e.g., 300MB/hour)
- **`concurrent-requests`** - Limit simultaneous connections

This enables sophisticated limiting strategies like "100MB per hour per user" or "5 concurrent uploads per API key."

### Window Parameter (`t`)

The reset time uses relative seconds (not timestamps) to avoid clock synchronization issues. The value indicates when the server *might* restore quota, but:
- It's a hint, not a guarantee
- Servers may change it between requests
- Clients with high latency should adjust calculations

### Throttling Doesn't Equal Blocking

Important: These headers are advisory. Clients can (and will) exceed limits. Servers must still implement proper resource protection. The headers improve efficiency by letting well-behaved clients self-regulate.

---

## Structured Reference

### Header Definitions

| Header | Type | Cardinality | Description |
|--------|------|-------------|-------------|
| `RateLimit-Policy` | Response | Multiple allowed | Static quota policy definitions |
| `RateLimit` | Response | Multiple allowed | Dynamic service limit state |

### RateLimit-Policy Parameters

| Parameter | Required | Type | Description |
|-----------|----------|------|-------------|
| `q` | Yes | Integer ≥ 0 | Quota amount |
| `w` | No | Integer > 0 | Window in seconds |
| `qu` | No | String | Quota unit (default: `requests`) |
| `pk` | No | Byte Sequence | Partition key |

### RateLimit Parameters

| Parameter | Required | Type | Description |
|-----------|----------|------|-------------|
| `r` | Yes | Integer ≥ 0 | Remaining quota |
| `t` | No | Integer ≥ 0 | Seconds until reset |
| `pk` | No | Byte Sequence | Partition key |

### Quota Units Registry

| Value | Description |
|-------|-------------|
| `requests` | Count of HTTP requests |
| `content-bytes` | Content bytes processed |
| `concurrent-requests` | Simultaneous connections |

### Response Status Codes

Headers may appear in any response. Common patterns:
- **2xx** - Request succeeded, headers show remaining quota
- **3xx** - Use caution; low remaining values may delay redirects
- **429** - Rate limited; headers explain which policy was violated
- **503** - Service unavailable; may indicate temporary reduced capacity

### Intermediary Behavior

- **Transparent proxies** - SHOULD forward headers unchanged
- **Enforcing intermediaries** - MAY reduce quota to be more restrictive
- **Caching proxies** - Cached responses' RateLimit headers SHOULD be ignored

### Security Considerations

- Headers are informational, not enforced
- Avoid exposing sensitive data in partition keys
- Cap the ratio of `r` to `t` to prevent information leakage
- Error responses may or may not count against quota (implementation-specific)

### Implementation Notes

- Headers use Structured Field syntax ([RFC8941])
- Both headers support multiple items via List syntax
- Headers MUST NOT appear in trailer sections
- Custom parameters SHOULD use vendor prefix (e.g., `acme-burst`)
- Multiple policy items can be split across multiple header fields

---

## See Also

- **Draft Source:** https://github.com/ietf-wg-httpapi/ratelimit-headers
- **Datatracker:** https://datatracker.ietf.org/doc/draft-ietf-httpapi-ratelimit-headers/
- **Structured Fields:** RFC 8941
- **HTTP Problem Details:** RFC 7807

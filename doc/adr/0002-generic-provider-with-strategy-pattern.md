# ADR 0002: Generic Provider with Strategy Pattern

## Status
Proposed

## Context
The current architecture has hardcoded provider implementations for each service:
- `GitHubProvider` - hardcoded GitHub-specific logic
- `GitLabProvider` - hardcoded GitLab-specific logic
- `HuggingFaceProvider` - hardcoded HuggingFace-specific logic
- `CodebergProvider` - hardcoded Codeberg-specific logic

This approach has several limitations:

1. **Inflexibility**: Adding a new provider requires creating a new struct and implementation
2. **Code duplication**: Similar logic (API calls, parsing) is repeated across providers
3. **Tight coupling**: Rate limiting, auth, and URL patterns are tightly coupled to provider implementations
4. **Strategy discovery**: Each provider hardcodes how to discover READMEs (API, scraping, etc.)
5. **Domain-specific knowledge**: Each provider knows about specific API endpoints and patterns

Additionally, we have services with similar APIs (GitLab and Codeberg both use GitLab-compatible APIs) that could share implementation if we had a more generic system.

## Decision

### Core Architecture

We will implement a **generic provider system** with the following characteristics:

1. **Single Provider struct**: One implementation that works for any domain (exposed via Provider trait)
2. **Strategy Pattern**: Pluggable strategies for different techniques (API, scraping, etc.)
3. **Lazy Strategy Discovery**: Try strategies on first use, cache working one
4. **ProviderFactory**: One-stop factory that holds domain configuration and creates providers

### Data Structures

```rust
// Strategy for fetching READMEs
trait Strategy: Send + Sync {
    fn name(&self) -> &'static str;

    async fn get_readme_url(&self, base_url: &str, repo_path: &str) -> Option<String>;

    async fn get_url(
        &self,
        url: &str,
        token: Option<&str>,
    ) -> anyhow::Result<reqwest::Response>;
}

// Configuration for a domain
struct DomainConfig {
    env_var: &'static str,
    api_pattern: Option<String>,
    rate_limit_headers: RateLimitHeaders,
    // ... other domain-specific config
}

// One-stop factory for provider creation and configuration
struct ProviderFactory {
    domains: HashMap<String, DomainConfig>,
    providers: RwLock<HashMap<String, Provider>>,
}

// Generic provider using strategies
struct Provider {
    domain: String,
    rate_limiter: RateLimiter,
    strategies: Vec<Box<dyn Strategy>>,
    working_strategy: Option<Box<dyn Strategy>>,
}
```

### Strategy Pattern

**Strategy Trait:**
- `get_readme_url()`: Attempts to construct a README URL using this strategy
- `get_url()`: Makes HTTP request with optional auth token
- `name()`: Strategy identifier for logging/debugging

**Strategy Types:**

1. **ApiStrategy**: Uses provider's REST API (e.g., `/api/v4/repos/{owner}/{repo}/readme`)
2. **RawGitStrategy**: Uses raw Git URL pattern (e.g., `https://raw.githubusercontent.com/...`)
3. **HtmlScrapeStrategy**: Scrapes the HTML repo page for README link
4. **GitLabApiStrategy**: GitLab-specific API calls
5. **GitHubApiStrategy**: GitHub-specific API calls

**Lazy Strategy Discovery:**
1. Provider starts with list of strategies
2. On first `get_readme_url()` call, try each strategy in order
3. First successful strategy is cached as `working_strategy`
4. Subsequent calls use cached strategy directly
5. If cached strategy fails, retry remaining strategies

### ProviderFactory

**Purpose:**
- One-stop place to get a provider for any URL
- Map domain names to configuration (auth tokens, rate limits, etc.)
- Provide auth tokens per domain
- Cache providers per domain to reuse rate limiters and strategies
- Store domain-specific settings

**Implementation:**
```rust
impl ProviderFactory {
    fn new() -> Self {
        let mut domains = HashMap::new();

        // GitHub
        domains.insert(
            "github.com".to_string(),
            DomainConfig {
                env_var: "GITHUB_TOKEN",
                api_pattern: Some("/api/repos/{owner}/{repo}/readme".to_string()),
                rate_limit_headers: RateLimitHeaders {
                    remaining: "x-ratelimit-remaining",
                    limit: "x-ratelimit-limit",
                    reset: "x-ratelimit-reset",
                    format: TimestampFormat::UnixEpoch,
                },
            },
        );

        // GitLab
        domains.insert(
            "gitlab.com".to_string(),
            DomainConfig {
                env_var: "GITLAB_TOKEN",
                api_pattern: Some("/api/v4/projects/{encoded_path}/repository/files/README.md/raw".to_string()),
                rate_limit_headers: RateLimitHeaders {
                    remaining: "RateLimit-Remaining",
                    limit: "RateLimit-Limit",
                    reset: "RateLimit-Reset",
                    format: TimestampFormat::UnixEpoch,
                },
            },
        );

        // ... more domains

        Self {
            domains,
            providers: RwLock::new(HashMap::new()),
        }
    }

    async fn get_provider(&self, url: &str) -> anyhow::Result<Provider> {
        let domain = extract_domain(url)?;

        // Check cache
        {
            let providers = self.providers.read().await;
            if let Some(provider) = providers.get(&domain) {
                return Ok(provider.clone());
            }
        }

        // Get config
        let config = self
            .domains
            .get(&domain)
            .ok_or_else(|| anyhow::anyhow!("Unknown domain: {}", domain))?;

        // Load tokens
        let tokens = std::env::var(config.env_var)
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Create provider
        let strategies = vec![
            Box::new(ApiStrategy::new(config)),
            Box::new(RawGitStrategy::new()),
            Box::new(HtmlScrapeStrategy::new()),
        ];

        let rate_limiter = RateLimiter::new(&domain, tokens, config);

        let provider = Provider {
            domain: domain.clone(),
            rate_limiter,
            strategies,
            working_strategy: None,
        };

        // Cache it
        {
            let mut providers = self.providers.write().await;
            providers.insert(domain.clone(), provider.clone());
        }

        Ok(provider)
    }
}

fn extract_domain(url: &str) -> anyhow::Result<String> {
    let parsed = url::Url::parse(url)?;
    Ok(parsed.host_str().unwrap_or("").to_string())
}
```

**Configurable vs Hardcoded:**
- Default domains (GitHub, GitLab, HuggingFace) are hardcoded
- Custom domains can be added via environment variables or config file
- Example: `ARCHIVE_LIST_DOMAINS="git.example.com:GITLAB_TOKEN|git.company.com:COMPANY_TOKEN"`

### Provider Detection Flow

1. **Parse URL**: Extract domain from input URL (e.g., `github.com` from `https://github.com/user/repo`)
2. **Get provider**: Call `ProviderFactory::get_provider(url)` which:
   - Extracts domain
   - Checks provider cache
   - Creates new provider if not cached
   - Configures with domain-specific settings
3. **Use provider**: Call `get_readme_url()` which will discover working strategy

1. **Parse URL**: Extract domain from input URL (e.g., `github.com` from `https://github.com/user/repo`)
2. **Lookup in registry**: Get domain configuration from `DomainRegistry`
3. **Create provider**: Instantiate `GenericProvider` with:
   - Domain-specific rate limit headers
   - Auth tokens from registry
   - List of strategies to try
4. **Cache provider**: Store in `DomainProviderFactory` for reuse
5. **Use provider**: Call `get_readme_url()` which will discover working strategy

### Rate Limiting Integration

The provider uses the two-tier rate limiting system from ADR 0001:

- **RateLimiter**: Per-domain rate limiter with token management
- **Header parsing**: Uses domain-configured rate limit headers
- **Strategy-aware**: Each strategy call checks rate limits before making requests

**Domain-Specific Rate Limits:**
```rust
struct DomainConfig {
    // ...
    rate_limit_headers: RateLimitHeaders,
}

struct RateLimitHeaders {
    remaining: &'static str,
    limit: &'static str,
    reset: &'static str,
    format: TimestampFormat,
}

enum TimestampFormat {
    UnixEpoch,
    Iso8601,
    // ... other formats
}
```

### Strategy Discovery Algorithm

**On first `get_readme_url()` call:**

```rust
impl Provider {
    async fn get_readme_url(&self, url: &str) -> anyhow::Result<String> {
        // Use cached strategy if available
        if let Some(strategy) = &self.working_strategy {
            return strategy.get_readme_url(&self.domain, url).await;
        }

        // Try each strategy
        for strategy in &self.strategies {
            match strategy.get_readme_url(&self.domain, url).await {
                Ok(readme_url) => {
                    // Cache working strategy
                    self.working_strategy = Some(strategy.clone());
                    return Ok(readme_url);
                }
                Err(_) => continue, // Try next strategy
            }
        }

        Err(anyhow::anyhow!("No strategy worked for {}", self.domain))
    }
}
```

**Strategy retry:**
- If cached strategy fails, clear it and retry all strategies
- This handles cases where a strategy works initially but fails later
- Example: API works first, then rate limited, fall back to raw Git

### Example Usage

```rust
// Factory creates providers for any domain
let factory = ProviderFactory::new();

// Get provider for GitHub
let github_provider = factory.get_provider("https://github.com/user/repo").await?;
let readme_url = github_provider.get_readme_url("https://github.com/user/repo").await?;

// Get provider for GitLab
let gitlab_provider = factory.get_provider("https://gitlab.com/user/repo").await?;
let readme_url = gitlab_provider.get_readme_url("https://gitlab.com/user/repo").await?;

// Get provider for custom domain (if configured)
let custom_provider = factory.get_provider("https://git.example.com/user/repo").await?;
let readme_url = custom_provider.get_readme_url("https://git.example.com/user/repo").await?;
```

## Rationale

### Why a Generic Provider?

1. **Flexibility**: Add new domains without creating new provider structs
2. **Code reuse**: Share rate limiting, auth, and strategy logic across domains
3. **Strategy composition**: Mix and match strategies per domain
4. **Extensibility**: New strategies can be added without modifying providers
5. **Simpler testing**: Test strategies in isolation

### Why ProviderFactory?

1. **One-stop interface**: Single place to get providers for any URL
2. **Domain configuration**: Centralized mapping of domains to settings
3. **Provider caching**: Reuse rate limiters and strategies across requests
4. **Token management**: Clean separation of auth token loading

### Why Strategy Pattern?

1. **Lazy discovery**: Don't hardcode which strategy works for which domain
2. **Fallback behavior**: Try multiple approaches until one succeeds
3. **Pluggable**: Easy to add new strategies (e.g., GraphQL, custom APIs)
4. **Domain independence**: Strategies don't know about specific domains
5. **Robustness**: If one strategy fails, fall back to another

### Why Lazy Strategy Discovery?

1. **No upfront cost**: Don't make API calls until needed
2. **Automatic fallback**: If API fails, try raw Git automatically
3. **Runtime adaptation**: Strategies may fail differently in different environments
4. **Simpler config**: Don't need to configure which strategy per domain
5. **Caching**: Once working strategy is found, reuse it

## Alternatives Considered

### Alternative 1: Keep Hardcoded Providers

**Pros:**
- Simple, explicit
- Easy to understand which provider handles which domain

**Cons:**
- Code duplication
- Hard to add new domains
- No strategy fallback
- Tight coupling

**Rejected:** Too inflexible, doesn't scale

### Alternative 2: Separate DomainRegistry and ProviderFactory

**Pros:**
- Clear separation of concerns
- Registry could be used elsewhere

**Cons:**
- Extra indirection
- Factory always needs registry
- Two places to look for domain config

**Rejected:** Unnecessary complexity - they're always used together, merge them into one

### Alternative 2: Configure Strategy Per Domain

**Pros:**
- Explicit which strategy to use per domain
- No runtime discovery overhead

**Cons:**
- Requires manual configuration
- No fallback behavior
- Need to update config when strategy fails

**Rejected:** Lazy discovery is more robust and requires less configuration

### Alternative 3: One Universal Strategy

**Pros:**
- Simple, no strategy selection

**Cons:**
- Not realistic - domains have different APIs
- Would be complex universal logic

**Rejected:** Domains are too different for one strategy

## Implementation Phases

### Phase 1: Core Infrastructure (High Priority)
1. Create Strategy trait
2. Implement basic strategies (Api, RawGit, HtmlScrape)
3. Create DomainConfig
4. Create Provider struct (not GenericProvider)
5. Create ProviderFactory with domain configuration

### Phase 2: Provider Caching (High Priority)
1. Implement provider caching in ProviderFactory (RwLock<HashMap>)
2. Implement domain extraction from URLs
3. Wire up rate limiting per provider
4. Load tokens from domain config

### Phase 3: Strategy Discovery (Medium Priority)
1. Implement lazy strategy discovery algorithm
2. Add strategy retry logic
3. Implement strategy caching
4. Add logging for strategy discovery

### Phase 4: Integration (Medium Priority)
1. Update Provider trait to use generic Provider internally
2. Update readme_get.rs to use ProviderFactory
3. Remove hardcoded providers (GitHub, GitLab, etc.)
4. Add domain configuration tests

### Phase 5: Extensibility (Low Priority)
1. Add custom domain configuration support
2. Add more strategies (GraphQL, etc.)
3. Add strategy prioritization
4. Add metrics for strategy success rates

## Interaction with ADR 0001

The generic provider system integrates with the two-tier rate limiting system:

1. **RateLimiter**: Used per-domain, configured via DomainRegistry
2. **Token management**: Domain registry provides tokens per domain
3. **Header parsing**: Domain config specifies rate limit headers to parse
4. **Strategies**: Each strategy call checks rate limits via provider

The generic provider becomes the user of the rate limiting system, abstracting away domain-specific details.

## Future Considerations

1. **Strategy chaining**: Combine strategies (e.g., try API, fallback to raw Git)
2. **Strategy learning**: Track which strategies work best per domain
3. **Custom strategies**: Allow users to provide their own strategies
4. **Config file support**: Load domain config from YAML/TOML files
5. **Strategy versioning**: Update strategies without changing provider code
6. **Provider mocking**: For testing, inject mock strategies

## References

- ADR 0001: Two-tier rate limiting
- `.test-agent/RATELIMIT.md` - Rate limit tracking reference
- Strategy Pattern: https://refactoring.guru/design-patterns/strategy
- Factory Pattern: https://refactoring.guru/design-patterns/factory-method

## History

- **2024-01-13**: Initial ADR proposed

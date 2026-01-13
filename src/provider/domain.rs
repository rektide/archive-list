use std::collections::HashMap;

#[derive(Clone)]
pub struct DomainConfig {
    pub env_var: &'static str,
    pub api_pattern: Option<String>,
    pub rate_limit_headers: RateLimitHeaders,
}

#[derive(Clone)]
pub struct RateLimitHeaders {
    pub remaining: &'static str,
    pub limit: &'static str,
    pub reset: &'static str,
    pub format: TimestampFormat,
}

#[derive(Clone, Copy, PartialEq)]
pub enum TimestampFormat {
    UnixEpoch,
    Iso8601,
}

pub fn get_domain_configs() -> HashMap<String, DomainConfig> {
    let mut domains = HashMap::new();

    domains.insert(
        "github.com".to_string(),
        DomainConfig {
            env_var: "GITHUB_TOKEN",
            api_pattern: Some("/repos/{repo}/readme".to_string()),
            rate_limit_headers: RateLimitHeaders {
                remaining: "x-ratelimit-remaining",
                limit: "x-ratelimit-limit",
                reset: "x-ratelimit-reset",
                format: TimestampFormat::UnixEpoch,
            },
        },
    );

    domains.insert(
        "gitlab.com".to_string(),
        DomainConfig {
            env_var: "GITLAB_TOKEN",
            api_pattern: Some("/api/v4/projects/{encoded_repo}/repository/files/README.md/raw".to_string()),
            rate_limit_headers: RateLimitHeaders {
                remaining: "RateLimit-Remaining",
                limit: "RateLimit-Limit",
                reset: "RateLimit-Reset",
                format: TimestampFormat::UnixEpoch,
            },
        },
    );

    domains.insert(
        "huggingface.co".to_string(),
        DomainConfig {
            env_var: "HUGGINGFACE_TOKEN",
            api_pattern: None,
            rate_limit_headers: RateLimitHeaders {
                remaining: "X-RateLimit-Remaining",
                limit: "X-RateLimit-Limit",
                reset: "X-RateLimit-Reset",
                format: TimestampFormat::UnixEpoch,
            },
        },
    );

    domains.insert(
        "codeberg.org".to_string(),
        DomainConfig {
            env_var: "CODEBERG_TOKEN",
            api_pattern: Some("/api/v1/repos/{repo}/readme".to_string()),
            rate_limit_headers: RateLimitHeaders {
                remaining: "RateLimit-Remaining",
                limit: "RateLimit-Limit",
                reset: "RateLimit-Reset",
                format: TimestampFormat::UnixEpoch,
            },
        },
    );

    domains
}

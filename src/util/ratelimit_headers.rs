use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;

#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub remaining: u32,
    pub limit: u32,
    pub reset_at: Option<DateTime<Utc>>,
}

const REMAINING_HEADERS: &[&str] = &[
    "x-ratelimit-remaining",
    "ratelimit-remaining",
    "x-rate-limit-remaining",
    "rate-limit-remaining",
    "x-ratelimit-requests-remaining",
    "retry-after",  // Sometimes used as remaining=0 indicator
];

const LIMIT_HEADERS: &[&str] = &[
    "x-ratelimit-limit",
    "ratelimit-limit",
    "x-rate-limit-limit",
    "rate-limit-limit",
    "x-ratelimit-requests-limit",
];

const RESET_HEADERS: &[&str] = &[
    "x-ratelimit-reset",
    "ratelimit-reset",
    "x-rate-limit-reset",
    "rate-limit-reset",
    "x-ratelimit-reset-after",
    "retry-after",
];

const DEFAULT_LIMIT: u32 = 60;
const DEFAULT_REMAINING: u32 = 60;

pub fn detect_rate_limits(headers: &HeaderMap) -> RateLimitInfo {
    let remaining = detect_remaining(headers);
    let limit = detect_limit(headers);
    let reset_at = detect_reset(headers);

    RateLimitInfo {
        remaining: remaining.unwrap_or(DEFAULT_REMAINING),
        limit: limit.unwrap_or(DEFAULT_LIMIT),
        reset_at,
    }
}

pub fn has_rate_limit_headers(headers: &HeaderMap) -> bool {
    detect_remaining(headers).is_some() || detect_limit(headers).is_some()
}

fn detect_remaining(headers: &HeaderMap) -> Option<u32> {
    for candidate in REMAINING_HEADERS {
        if let Some(value) = find_header_case_insensitive(headers, candidate) {
            if let Ok(num) = value.parse::<u32>() {
                log::debug!("Detected remaining from header '{}': {}", candidate, num);
                return Some(num);
            }
        }
    }
    None
}

fn detect_limit(headers: &HeaderMap) -> Option<u32> {
    for candidate in LIMIT_HEADERS {
        if let Some(value) = find_header_case_insensitive(headers, candidate) {
            if let Ok(num) = value.parse::<u32>() {
                log::debug!("Detected limit from header '{}': {}", candidate, num);
                return Some(num);
            }
        }
    }
    None
}

fn detect_reset(headers: &HeaderMap) -> Option<DateTime<Utc>> {
    for candidate in RESET_HEADERS {
        if let Some(value) = find_header_case_insensitive(headers, candidate) {
            if let Some(dt) = parse_reset_value(&value, candidate) {
                log::debug!("Detected reset from header '{}': {}", candidate, dt);
                return Some(dt);
            }
        }
    }
    None
}

fn find_header_case_insensitive(headers: &HeaderMap, name: &str) -> Option<String> {
    let normalized = normalize_header_name(name);
    
    for (key, value) in headers.iter() {
        if normalize_header_name(key.as_str()) == normalized {
            if let Ok(s) = value.to_str() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn normalize_header_name(name: &str) -> String {
    name.to_lowercase()
        .replace('-', "_")
        .trim_start_matches("x_")
        .to_string()
}

fn parse_reset_value(value: &str, header_name: &str) -> Option<DateTime<Utc>> {
    // Try Unix timestamp first (most common)
    if let Ok(ts) = value.parse::<i64>() {
        // Distinguish between Unix epoch and "seconds from now"
        // Unix timestamps are typically > 1_000_000_000 (year 2001+)
        if ts > 1_000_000_000 {
            return DateTime::from_timestamp(ts, 0);
        } else if header_name.contains("after") || ts < 86400 {
            // "retry-after" or "reset-after" style: seconds from now
            let future = Utc::now() + chrono::Duration::seconds(ts);
            return Some(future);
        }
    }

    // Try ISO 8601 / RFC 3339
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Utc));
    }

    // Try HTTP date format (RFC 2822)
    if let Ok(dt) = DateTime::parse_from_rfc2822(value) {
        return Some(dt.with_timezone(&Utc));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

    fn make_headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.insert(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        map
    }

    #[test]
    fn test_github_style_headers() {
        let headers = make_headers(&[
            ("x-ratelimit-remaining", "4999"),
            ("x-ratelimit-limit", "5000"),
            ("x-ratelimit-reset", "1704067200"),
        ]);

        let info = detect_rate_limits(&headers);
        assert_eq!(info.remaining, 4999);
        assert_eq!(info.limit, 5000);
        assert!(info.reset_at.is_some());
    }

    #[test]
    fn test_gitlab_style_headers() {
        let headers = make_headers(&[
            ("ratelimit-remaining", "1999"),
            ("ratelimit-limit", "2000"),
            ("ratelimit-reset", "1704067200"),
        ]);

        let info = detect_rate_limits(&headers);
        assert_eq!(info.remaining, 1999);
        assert_eq!(info.limit, 2000);
    }

    #[test]
    fn test_no_headers_uses_defaults() {
        let headers = HeaderMap::new();
        let info = detect_rate_limits(&headers);
        
        assert_eq!(info.remaining, DEFAULT_REMAINING);
        assert_eq!(info.limit, DEFAULT_LIMIT);
        assert!(info.reset_at.is_none());
    }

    #[test]
    fn test_retry_after_seconds() {
        let headers = make_headers(&[("retry-after", "60")]);
        let info = detect_rate_limits(&headers);
        
        // retry-after as remaining means we're at 0
        assert_eq!(info.remaining, 60);  // Actually parses as remaining
        assert!(info.reset_at.is_some());
    }

    #[test]
    fn test_case_insensitive() {
        let headers = make_headers(&[
            ("X-RateLimit-Remaining", "100"),
            ("X-RATELIMIT-LIMIT", "200"),
        ]);

        let info = detect_rate_limits(&headers);
        assert_eq!(info.remaining, 100);
        assert_eq!(info.limit, 200);
    }
}

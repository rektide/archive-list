use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct DomainConfig {
    pub env_var: &'static str,
    pub api_pattern: Option<String>,
}

pub fn get_domain_configs() -> HashMap<String, DomainConfig> {
    let mut domains = HashMap::new();

    domains.insert(
        "github.com".to_string(),
        DomainConfig {
            env_var: "GITHUB_TOKEN",
            api_pattern: Some("/repos/{repo}/readme".to_string()),
        },
    );

    domains.insert(
        "gitlab.com".to_string(),
        DomainConfig {
            env_var: "GITLAB_TOKEN",
            api_pattern: Some("/api/v4/projects/{encoded_repo}/repository/files/README.md/raw".to_string()),
        },
    );

    domains.insert(
        "huggingface.co".to_string(),
        DomainConfig {
            env_var: "HUGGINGFACE_TOKEN",
            api_pattern: None,
        },
    );

    domains.insert(
        "codeberg.org".to_string(),
        DomainConfig {
            env_var: "CODEBERG_TOKEN",
            api_pattern: Some("/api/v1/repos/{repo}/readme".to_string()),
        },
    );

    domains
}

pub fn get_default_config(domain: &str) -> DomainConfig {
    let env_var_name = domain
        .split('.')
        .next()
        .unwrap_or("UNKNOWN")
        .to_uppercase();
    
    DomainConfig {
        env_var: Box::leak(format!("{}_TOKEN", env_var_name).into_boxed_str()),
        api_pattern: None,
    }
}

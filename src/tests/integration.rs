use std::sync::Arc;

use crate::provider::ProviderFactory;
use crate::provider::ProviderTrait;

#[tokio::test]
async fn test_provider_factory_creates_provider_for_known_domains() {
    let factory = ProviderFactory::new();

    let github_result = factory.get_provider("https://github.com/rust-lang/rust").await;
    assert!(github_result.is_ok(), "GitHub provider creation should succeed");

    let gitlab_result = factory.get_provider("https://gitlab.com/gitlab-org/gitlab").await;
    assert!(gitlab_result.is_ok(), "GitLab provider creation should succeed");

    let hf_result = factory.get_provider("https://huggingface.co/gpt2/model").await;
    assert!(hf_result.is_ok(), "HuggingFace provider creation should succeed");

    let codeberg_result = factory.get_provider("https://codeberg.org/user/repo").await;
    assert!(codeberg_result.is_ok(), "Codeberg provider creation should succeed");
}

#[tokio::test]
async fn test_unknown_domain_returns_error() {
    let factory = ProviderFactory::new();

    let result = factory.get_provider("https://unknown-domain.com/repo").await;

    assert!(result.is_err(), "Unknown domain should return error");
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Unknown domain"),
        "Error should mention unknown domain");
}

#[tokio::test]
async fn test_provider_caches_provider() {
    let factory = ProviderFactory::new();

    let provider1 = factory.get_provider("https://github.com/rust-lang/rust").await.unwrap();
    let provider2 = factory.get_provider("https://github.com/another/repo").await.unwrap();

    assert_eq!(provider1.domain, provider2.domain, "Cached providers should have same domain");
    assert!(Arc::ptr_eq(&provider1, &provider2), "Should return same cached instance");
}

#[tokio::test]
async fn test_strategy_discovery_finds_working_strategy() {
    let factory = ProviderFactory::new();

    let provider = factory.get_provider("https://github.com/rust-lang/rust").await.unwrap();

    let result = provider.get_readme_url("https://github.com/rust-lang/rust").await;

    assert!(result.is_ok(), "Strategy discovery should find working strategy for GitHub");

    let readme_url = result.unwrap();
    assert!(readme_url.contains("api.github.com") || readme_url.contains("github.com"),
        "URL should be valid GitHub URL");
}

#[tokio::test]
async fn test_working_strategy_is_reused() {
    let factory = ProviderFactory::new();

    let provider = factory.get_provider("https://github.com/rust-lang/rust").await.unwrap();

    let _ = provider.get_readme_url("https://github.com/rust-lang/rust").await.unwrap();
    let result2 = provider.get_readme_url("https://github.com/rust-lang/cargo").await;

    assert!(result2.is_ok(), "Cached strategy should work for second call");
}

#[tokio::test]
async fn test_token_loading_from_env() {
    std::env::set_var("GITHUB_TOKEN", "token1,token2,token3");

    let factory = ProviderFactory::new();
    let provider = factory.get_provider("https://github.com/rust-lang/rust").await.unwrap();

    let tokens = provider.load_tokens();
    assert_eq!(tokens.len(), 3, "Should load 3 tokens");
    assert_eq!(tokens[0], "token1", "First token should be token1");
    assert_eq!(tokens[1], "token2", "Second token should be token2");
    assert_eq!(tokens[2], "token3", "Third token should be token3");

    std::env::remove_var("GITHUB_TOKEN");
}

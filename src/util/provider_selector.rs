use crate::provider::{
    codeberg::CodebergProvider, github::GitHubProvider, gitlab::GitLabProvider,
    huggingface::HuggingFaceProvider, Provider,
};

pub async fn detect_provider(url: &str) -> Option<Box<dyn Provider>> {
    if let Some(provider) = GitHubProvider::detect(url).await {
        return Some(provider);
    }

    if let Some(provider) = GitLabProvider::detect(url).await {
        return Some(provider);
    }

    if let Some(provider) = HuggingFaceProvider::detect(url).await {
        return Some(provider);
    }

    if let Some(provider) = CodebergProvider::detect(url).await {
        return Some(provider);
    }

    None
}

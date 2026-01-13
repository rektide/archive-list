use crate::provider::ProviderFactory;
use tokio::sync::OnceCell;

static FACTORY: OnceCell<ProviderFactory> = OnceCell::const_new();

pub async fn get_provider_factory() -> &'static ProviderFactory {
    FACTORY
        .get_or_init(|| async { ProviderFactory::new() })
        .await
}


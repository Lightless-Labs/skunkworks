//! Adapter: bridges a2_broker::ModelProvider → a2_core::traits::ModelProvider.

use a2_core::error::A2Result;
use a2_core::traits::GenerateResponse as CoreResponse;

use crate::broker;

/// Wraps a broker ModelProvider to implement the a2_core trait.
pub struct CoreAdapter<P: broker::ModelProvider> {
    inner: P,
}

impl<P: broker::ModelProvider> CoreAdapter<P> {
    pub fn new(inner: P) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl<P: broker::ModelProvider + Send + Sync> a2_core::traits::ModelProvider for CoreAdapter<P> {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> A2Result<CoreResponse> {
        let resp = self.inner.generate(prompt, system).await?;
        Ok(CoreResponse {
            text: resp.text,
            tokens_in: resp.tokens_in,
            tokens_out: resp.tokens_out,
        })
    }

    fn provider_id(&self) -> &str {
        self.inner.provider_id()
    }

    fn model_id(&self) -> &str {
        self.inner.model_id()
    }
}

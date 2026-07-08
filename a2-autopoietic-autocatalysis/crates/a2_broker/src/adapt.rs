//! Adapter: bridges a2_broker::ModelProvider → a2_core::traits::ModelProvider.

use a2_core::error::A2Result;
use a2_core::protocol::NetworkPolicy;
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

    async fn generate_with_network_policy(
        &self,
        prompt: &str,
        system: Option<&str>,
        network_policy: Option<&NetworkPolicy>,
    ) -> A2Result<CoreResponse> {
        let resp = self
            .inner
            .generate_with_network_policy(prompt, system, network_policy)
            .await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    struct ForwardingProvider;

    #[async_trait::async_trait]
    impl broker::ModelProvider for ForwardingProvider {
        async fn generate(
            &self,
            _prompt: &str,
            _system: Option<&str>,
        ) -> A2Result<broker::GenerateResponse> {
            panic!("adapter should forward through generate_with_network_policy")
        }

        async fn generate_with_network_policy(
            &self,
            prompt: &str,
            system: Option<&str>,
            network_policy: Option<&NetworkPolicy>,
        ) -> A2Result<broker::GenerateResponse> {
            assert_eq!(prompt, "prompt");
            assert_eq!(system, Some("system"));
            assert_eq!(
                network_policy,
                Some(&NetworkPolicy::AllowList(vec![
                    "https://api.openai.com".into()
                ]))
            );
            Ok(broker::GenerateResponse {
                text: "forwarded".into(),
                tokens_in: 3,
                tokens_out: 5,
            })
        }

        fn provider_id(&self) -> &str {
            "forwarding"
        }

        fn model_id(&self) -> &str {
            "test"
        }
    }

    #[tokio::test]
    async fn adapter_forwards_network_policy_to_broker_provider() {
        let adapter = CoreAdapter::new(ForwardingProvider);
        let response = a2_core::traits::ModelProvider::generate_with_network_policy(
            &adapter,
            "prompt",
            Some("system"),
            Some(&NetworkPolicy::AllowList(vec![
                "https://api.openai.com".into(),
            ])),
        )
        .await
        .unwrap();

        assert_eq!(response.text, "forwarded");
        assert_eq!(response.tokens_in, 3);
        assert_eq!(response.tokens_out, 5);
    }
}

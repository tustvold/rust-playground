use async_trait::async_trait;
use log::warn;
use rusoto_core::credential::{
    AwsCredentials, ChainProvider, CredentialsError, ProvideAwsCredentials,
};
use rusoto_core::Region;
use rusoto_sts::WebIdentityProvider;
use std::time::Duration;

// A custom chain provider incorporating web identity support
// See - https://github.com/rusoto/rusoto/issues/1781
pub struct CustomChainProvider {
    chain_provider: ChainProvider,
    web_provider: WebIdentityProvider,
}

impl CustomChainProvider {
    pub fn new() -> CustomChainProvider {
        CustomChainProvider {
            chain_provider: ChainProvider::new(),
            web_provider: WebIdentityProvider::from_k8s_env(),
        }
    }

    pub fn set_timeout(&mut self, duration: Duration) {
        self.chain_provider.set_timeout(duration);
    }
}

impl Default for CustomChainProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProvideAwsCredentials for CustomChainProvider {
    async fn credentials(&self) -> Result<AwsCredentials, CredentialsError> {
        match self.web_provider.credentials().await {
            Ok(creds) => return Ok(creds),
            Err(e) => warn!("Error getting AWS credentials from EKS, falling back to default chain provider - {}", e)
        }
        match self.chain_provider.credentials().await {
            Ok(creds) => return Ok(creds),
            Err(e) => warn!(
                "Error getting AWS credentials with default chain provider - {}",
                e
            ),
        }
        Err(CredentialsError::new(
            "Couldn't find AWS credentials in environment, credentials file, or IAM role.",
        ))
    }
}

pub fn parse_region(region: String, endpoint: Option<String>) -> Region {
    if let Some(endpoint) = endpoint {
        return Region::Custom {
            name: region,
            endpoint,
        };
    }
    region.parse().expect("invalid region")
}

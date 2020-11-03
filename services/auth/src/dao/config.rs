use rusoto_dynamodb::DynamoDbClient;
use serde::Deserialize;

use rusoto_core::credential::StaticProvider;
use rusoto_util::{parse_region, CustomChainProvider};

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct DaoConfig {
    pub region: String,
    pub endpoint: Option<String>,
    pub table: String,
    pub seed: bool,
    pub local: bool,
}

impl Default for DaoConfig {
    fn default() -> DaoConfig {
        DaoConfig {
            region: "us-east-1".to_string(),
            endpoint: None,
            table: "Auth".to_string(),
            seed: false,
            local: false,
        }
    }
}

impl DaoConfig {
    pub fn dynamo_client(&self) -> DynamoDbClient {
        let region = parse_region(self.region.clone(), self.endpoint.clone());
        let dispatcher =
            rusoto_core::request::HttpClient::new().expect("failed to create request dispatcher");

        if self.local {
            return DynamoDbClient::new_with(
                dispatcher,
                StaticProvider::new_minimal("local".to_string(), "development".to_string()),
                region,
            );
        }

        DynamoDbClient::new_with(dispatcher, CustomChainProvider::new(), region)
    }
}

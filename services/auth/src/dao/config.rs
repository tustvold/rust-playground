use rusoto_dynamodb::DynamoDbClient;
use serde::Deserialize;

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
        dynamo_util::dynamo_client(self.region.clone(), self.endpoint.clone(), self.local)
    }
}

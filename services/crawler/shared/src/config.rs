use rusoto_dynamodb::DynamoDbClient;
use serde::Deserialize;

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct DynamoConfig {
    pub region: String,
    pub endpoint: Option<String>,
    pub local: bool,
}

impl Default for DynamoConfig {
    fn default() -> DynamoConfig {
        DynamoConfig {
            region: "us-east-1".to_string(),
            endpoint: Some("http://localhost:8000".to_string()),
            local: true,
        }
    }
}

impl DynamoConfig {
    pub fn dynamo_client(&self) -> DynamoDbClient {
        dynamo_util::dynamo_client(self.region.clone(), self.endpoint.clone(), self.local)
    }
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct RabbitMQConfig {
    pub url: String,
    pub prefetch_count: u32,
}

impl Default for RabbitMQConfig {
    fn default() -> RabbitMQConfig {
        RabbitMQConfig {
            url: "amqp://rabbitmq:rabbitmq@127.0.0.1:5672/%2f".to_string(),
            prefetch_count: 20,
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct MetricsConfig {
    pub host: String,
    pub port: u16,
    pub prefix: String,
    pub tags: Vec<(String, String)>,
}

impl Default for MetricsConfig {
    fn default() -> MetricsConfig {
        MetricsConfig {
            host: "127.0.0.1".to_string(),
            port: 8125,
            prefix: "service.stdv1".to_string(),
            tags: vec![
                ("service_domain".to_string(), "greeter".to_string()),
                ("service_group".to_string(), "default".to_string()),
                ("service_name".to_string(), "rust".to_string()),
                ("service_role".to_string(), "internal".to_string()),
            ],
        }
    }
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Config {
    pub dynamo: DynamoConfig,
    pub rabbit: RabbitMQConfig,
    pub metrics: MetricsConfig,
}

impl Config {
    pub fn from_env() -> Result<Self, ::config::ConfigError> {
        let mut cfg = ::config::Config::new();
        cfg.merge(::config::Environment::new().prefix("APP").separator("_"))?;
        cfg.try_into()
    }
}

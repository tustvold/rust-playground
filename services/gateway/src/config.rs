use serde::Deserialize;

use jwt::ValidatorConfig;

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct UpstreamConfig {
    pub calculator: String,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        UpstreamConfig {
            calculator: "http://calculator".to_string(),
        }
    }
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Config {
    pub port: Option<u16>,
    pub validator: ValidatorConfig,
    pub upstream: UpstreamConfig,
}

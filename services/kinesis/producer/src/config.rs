use serde::Deserialize;

use jwt::ValidatorConfig;
use kinesis::producer::Producer;
use kinesis::{PipelineBuilder, PipelineHandler};

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Config {
    pub validator: ValidatorConfig,
    pub kinesis: KinesisConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct KinesisConfig {
    pub region: String,
    pub endpoint: Option<String>,
    pub stream_name: String,
    pub local: bool,
}

impl Default for KinesisConfig {
    fn default() -> KinesisConfig {
        KinesisConfig {
            region: "us-east-1".to_string(),
            stream_name: "kinesis".to_string(),
            endpoint: None,
            local: false,
        }
    }
}

impl KinesisConfig {
    pub fn pipeline(&self) -> (Producer, PipelineHandler) {
        let mut builder = PipelineBuilder::new(self.region.clone(), self.stream_name.clone());

        if self.local {
            builder.local();
        }

        if let Some(endpoint) = self.endpoint.as_ref() {
            builder.endpoint(endpoint.clone());
        }

        builder.build()
    }
}

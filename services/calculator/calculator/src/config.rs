use serde::Deserialize;

use jwt::ValidatorConfig;

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Config {
    pub validator: ValidatorConfig,
}

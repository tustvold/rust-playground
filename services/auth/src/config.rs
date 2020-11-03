use serde::Deserialize;

use credential::CredentialConfig;
use jwt::IssuerConfig;

use crate::api::ApiConfig;
use crate::dao::DaoConfig;

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Config {
    pub api: ApiConfig,
    pub issuer: IssuerConfig,
    pub dao: DaoConfig,
    pub credential: CredentialConfig,
}

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct ApiConfig {
    pub access_token_ttl: i64,
    pub refresh_token_ttl: i64,
}

impl Default for ApiConfig {
    fn default() -> ApiConfig {
        ApiConfig {
            access_token_ttl: 15 * 60,            // 15 minutes
            refresh_token_ttl: 2 * 7 * 24 * 3600, // 2 weeks
        }
    }
}

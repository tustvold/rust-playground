use std::sync::Arc;

use derive_more::Display;
use ring::rand::SecureRandom;

#[derive(Debug, Display)]
pub enum TokenError {
    #[display(fmt = "Internal Error")]
    InternalError,
}
impl std::error::Error for TokenError {}

pub struct TokenService {
    random: Arc<dyn SecureRandom + Sync + Send>,
}

impl TokenService {
    pub fn new(random: Arc<dyn SecureRandom + Sync + Send>) -> TokenService {
        TokenService { random }
    }

    // Generates a 32 character secure random string
    pub fn token(&self) -> Result<String, TokenError> {
        let mut buf = [0; 24];
        self.random
            .fill(&mut buf)
            .map_err(|_| TokenError::InternalError)?;
        Ok(base64::encode_config(buf, base64::URL_SAFE_NO_PAD))
    }
}

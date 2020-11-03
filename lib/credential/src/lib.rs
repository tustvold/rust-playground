use std::num::{NonZeroU32, NonZeroUsize};

use derive_more::Display;
use ring::{digest, pbkdf2};
use serde::Deserialize;
use tokio::sync::Semaphore;

static PBKDF2_ALG: pbkdf2::Algorithm = pbkdf2::PBKDF2_HMAC_SHA256;
const CREDENTIAL_LEN: usize = digest::SHA256_OUTPUT_LEN;

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct CredentialConfig {
    pub secret: Option<String>,
    pub iterations: NonZeroU32,
    pub max_parallel: NonZeroUsize,
}

impl Default for CredentialConfig {
    fn default() -> CredentialConfig {
        CredentialConfig {
            secret: None,
            iterations: NonZeroU32::new(100_000).unwrap(),
            max_parallel: NonZeroUsize::new(10).unwrap(),
        }
    }
}

pub struct CredentialService {
    secret: Vec<u8>,
    iterations: NonZeroU32,
    semapahore: Semaphore,
}

#[derive(Debug, Display)]
pub enum CredentialError {
    #[display(fmt = "No Salt Secret")]
    NoSecret,

    #[display(fmt = "Invalid Credential")]
    InvalidCredential,
}

impl std::error::Error for CredentialError {}

impl CredentialService {
    pub fn new(config: &CredentialConfig) -> Result<CredentialService, CredentialError> {
        let secret = config.secret.clone().ok_or(CredentialError::NoSecret)?;
        Ok(CredentialService {
            iterations: config.iterations,
            secret: secret.into_bytes(),
            semapahore: Semaphore::new(config.max_parallel.into()),
        })
    }

    pub fn test() -> Result<CredentialService, CredentialError> {
        CredentialService::new(&CredentialConfig {
            secret: Some("much secret".to_string()),
            iterations: NonZeroU32::new(10).unwrap(),
            max_parallel: NonZeroUsize::new(10).unwrap(),
        })
    }

    fn salt(&self, salt_prefix: &str) -> Vec<u8> {
        let mut salt = Vec::with_capacity(self.secret.len() + salt_prefix.as_bytes().len());
        salt.extend(self.secret.as_slice());
        salt.extend(salt_prefix.as_bytes());
        salt
    }

    pub async fn derive(
        &self,
        salt_prefix: &str,
        credential: &str,
    ) -> Result<Vec<u8>, CredentialError> {
        let salt = self.salt(salt_prefix);
        let mut hashed = [0u8; CREDENTIAL_LEN];
        let _ = self.semapahore.acquire();
        pbkdf2::derive(
            PBKDF2_ALG,
            self.iterations,
            &salt,
            credential.as_bytes(),
            &mut hashed,
        );
        Ok(hashed.to_vec())
    }

    pub async fn verify(
        &self,
        salt_prefix: &str,
        credential: &str,
        hashed: &[u8],
    ) -> Result<(), CredentialError> {
        let salt = self.salt(salt_prefix);
        let _ = self.semapahore.acquire();
        pbkdf2::verify(
            PBKDF2_ALG,
            self.iterations,
            &salt,
            credential.as_bytes(),
            hashed,
        )
        .map_err(|_| CredentialError::InvalidCredential)
    }
}

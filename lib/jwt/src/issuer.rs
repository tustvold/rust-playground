use std::fs::File;
use std::io::prelude::*;
use std::sync::Arc;

use chrono::{Duration, Utc};
use ring::rand::SecureRandom;
use ring::signature::{self, KeyPair};
use serde::{Deserialize, Serialize};

use crate::error::{IssuerError, ValidatorError};
use crate::model::*;
use crate::tag;
use crate::{Validator, ValidatorConfig};

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct IssuerConfig {
    pub secret: Option<String>,

    #[serde(rename = "secretpath")]
    pub secret_path: Option<String>,

    // JSON Key ID
    pub kid: String,

    // JSON key URL
    pub jku: Option<String>,
}

impl Default for IssuerConfig {
    fn default() -> IssuerConfig {
        IssuerConfig {
            secret: None,
            secret_path: None,
            kid: "1".to_string(),
            jku: None,
        }
    }
}

#[derive(Clone)]
pub struct Issuer {
    key_pair: Arc<signature::RsaKeyPair>,
    random: Arc<dyn SecureRandom + Sync + Send>,

    jku: String,
    jwks: String,
    header: String,
}

fn b64_encode_obj<T: Serialize>(obj: &T) -> Result<String, serde_json::Error> {
    let string = serde_json::to_string(obj)?;
    Ok(base64::encode_config(string, base64::URL_SAFE_NO_PAD))
}

impl Issuer {
    pub fn new(
        config: &IssuerConfig,
        random: Arc<dyn SecureRandom + Sync + Send>,
    ) -> Result<Issuer, IssuerError> {
        let pkcs8;

        if let Some(s) = &config.secret {
            pkcs8 = pem::parse(s.as_bytes())?;
        } else if let Some(secret_path) = &config.secret_path {
            let mut file = File::open(secret_path)?;
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)?;
            pkcs8 = pem::parse(&contents)?;
        } else {
            return Err(IssuerError::ConfigError("No Secret".to_string()));
        }

        if pkcs8.tag != "PRIVATE KEY" {
            return Err(IssuerError::InvalidKey);
        }

        let key_pair = Arc::new(signature::RsaKeyPair::from_pkcs8(&pkcs8.contents)?);
        let jwk = Jwk::new(&config.kid, key_pair.public_key());
        let jwks = serde_json::to_string(&Jwks { keys: vec![jwk] })?;
        let jku = config
            .jku
            .clone()
            .ok_or_else(|| IssuerError::ConfigError("No JKU".to_string()))?;

        let header = b64_encode_obj(&JwtHeader {
            alg: "RS256".to_string(),
            typ: "JWT".to_string(),
            kid: config.kid.clone(),
            jku: jku.clone(),
        })?;

        Ok(Issuer {
            key_pair,
            jku,
            jwks,
            header,
            random,
        })
    }

    pub fn test(random: Arc<dyn SecureRandom + Sync + Send>) -> Result<Issuer, IssuerError> {
        Issuer::new(
            &IssuerConfig {
                secret: Some(include_str!("../test_resources/secret.pem").to_string()),
                secret_path: None,
                jku: Some("http://localhost:8080/.well-known/jwks.json".to_string()),
                kid: "1".to_string(),
            },
            random.clone(),
        )
    }

    pub fn jwks(&self) -> &String {
        &self.jwks
    }

    pub fn new_validator(&self) -> Result<Validator, ValidatorError> {
        Validator::new(&ValidatorConfig {
            jku: Some(self.jku.clone()),
            jwks: Some(self.jwks.clone()),
        })
    }

    pub fn issue<'a, S: AsRef<str> + 'static, T: Iterator<Item = &'a S>>(
        &self,
        subject: Option<String>,
        client_id: String,
        scopes: T,
        ttl: Duration,
    ) -> Result<String, IssuerError> {
        let now = Utc::now();

        let claims = JwtSerializedClaims {
            exp: now + ttl,
            iat: now,
            cid: client_id,
            sub: subject,
            scopes: tag::serialize_space_delimited(scopes),
        };

        let claim_str = b64_encode_obj(&claims)?;
        let message = [self.header.as_ref(), claim_str.as_ref()].join(".");
        let mut sig_bytes = vec![0; self.key_pair.public_modulus_len()];
        self.key_pair.sign(
            &signature::RSA_PKCS1_SHA256,
            self.random.as_ref(),
            message.as_bytes(),
            &mut sig_bytes,
        )?;
        let signature = base64::encode_config(&sig_bytes, base64::URL_SAFE_NO_PAD);
        Ok([message, signature].join("."))
    }
}

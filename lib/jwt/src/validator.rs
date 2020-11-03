use std::collections::HashMap;
use std::convert::TryInto;
use std::hash::Hash;
use std::str::FromStr;

use chrono::Utc;
use ring::signature;
use serde::{Deserialize, Serialize};

use crate::error::ValidatorError;
use crate::model::{Jwks, JwtClaims, JwtHeader, JwtSerializedClaims, PublicKey};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ValidatorConfig {
    // JSON key URL
    pub jku: Option<String>,

    // JSON Web Keys
    pub jwks: Option<String>,
}

impl Default for ValidatorConfig {
    fn default() -> ValidatorConfig {
        ValidatorConfig {
            jku: None,
            jwks: None,
        }
    }
}

#[derive(Clone)]
pub struct Validator {
    jku: String,
    keys: HashMap<String, PublicKey>,
}

impl Validator {
    pub fn new(config: &ValidatorConfig) -> Result<Self, ValidatorError> {
        let jku = config
            .jku
            .as_ref()
            .ok_or_else(|| ValidatorError::ConfigError("Missing JKU".to_string()))?;

        let jwks = config
            .jwks
            .as_ref()
            .ok_or_else(|| ValidatorError::ConfigError("Missing JWKS".to_string()))?;

        let keys: Jwks = serde_json::from_str(&jwks)?;

        Ok(Validator {
            keys: keys.try_into()?,
            jku: jku.clone(),
        })
    }

    pub fn validate<S: Sized + FromStr + Hash + Eq>(
        &self,
        jwt: &str,
    ) -> Result<JwtClaims<S>, ValidatorError> {
        let mut jwt_splitter = jwt.rsplitn(2, '.');
        let raw_signature = jwt_splitter.next().ok_or(ValidatorError::ParseError)?;
        let raw_msg = jwt_splitter.next().ok_or(ValidatorError::ParseError)?;

        let mut msg_splitter = raw_msg.rsplitn(2, '.');
        let raw_claims = msg_splitter.next().ok_or(ValidatorError::ParseError)?;
        let raw_header = msg_splitter.next().ok_or(ValidatorError::ParseError)?;

        let header_bytes = base64::decode_config(raw_header, base64::URL_SAFE_NO_PAD)?;
        let header: JwtHeader = serde_json::from_slice(&header_bytes)?;

        let signature = base64::decode_config(raw_signature, base64::URL_SAFE_NO_PAD)?;

        let claims_bytes = base64::decode_config(raw_claims, base64::URL_SAFE_NO_PAD)?;
        let claims: JwtSerializedClaims = serde_json::from_slice(&claims_bytes)?;

        if header.jku != self.jku {
            return Err(ValidatorError::JwtInvalid);
        }

        let key = self
            .keys
            .get(&header.kid)
            .ok_or(ValidatorError::JwtInvalid)?;

        key.verify(
            &signature::RSA_PKCS1_2048_8192_SHA256,
            raw_msg.as_bytes(),
            &signature,
        )
        .map_err(|_| ValidatorError::JwtInvalid)?;

        let now = Utc::now();
        if claims.exp < now {
            return Err(ValidatorError::JwtExpired);
        }

        Ok(claims
            .try_into()
            .map_err(|_| ValidatorError::DecodeError("Failed to decode claims".to_string()))?)
    }
}

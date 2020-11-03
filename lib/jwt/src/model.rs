use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::hash::Hash;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use ring::signature;
use ring::signature::RsaPublicKeyComponents;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

use crate::tag;

#[derive(Serialize, Deserialize)]
pub struct Jwk {
    pub kty: String,
    pub kid: String,
    pub n: String,
    pub e: String,

    #[serde(rename = "use")]
    pub u: String,
}

impl Jwk {
    pub(crate) fn new(kid: &str, key: &signature::RsaSubjectPublicKey) -> Jwk {
        let n = base64::encode_config(
            key.modulus().big_endian_without_leading_zero(),
            base64::URL_SAFE_NO_PAD,
        );
        let e = base64::encode_config(
            key.exponent().big_endian_without_leading_zero(),
            base64::URL_SAFE_NO_PAD,
        );
        Jwk {
            kty: "RSA".to_string(),
            u: "sig".to_string(),
            kid: kid.to_string(),
            n,
            e,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Jwks {
    pub keys: Vec<Jwk>,
}

pub type PublicKey = RsaPublicKeyComponents<Vec<u8>>;

impl TryInto<HashMap<String, PublicKey>> for Jwks {
    type Error = base64::DecodeError;

    fn try_into(self) -> Result<HashMap<String, PublicKey>, Self::Error> {
        let mut map = HashMap::new();
        for key in self.keys {
            map.insert(
                key.kid,
                PublicKey {
                    n: base64::decode_config(&key.n, base64::URL_SAFE_NO_PAD)?,
                    e: base64::decode_config(&key.e, base64::URL_SAFE_NO_PAD)?,
                },
            );
        }
        Ok(map)
    }
}

#[derive(Serialize, Deserialize)]
pub struct JwtHeader {
    pub alg: String,
    pub typ: String,
    pub kid: String,
    pub jku: String,
}

#[derive(Serialize, Deserialize)]
pub struct JwtSerializedClaims {
    pub exp: DateTime<Utc>,
    pub iat: DateTime<Utc>,
    pub cid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    pub scopes: String,
}

pub struct JwtClaims<S> {
    pub exp: DateTime<Utc>,
    pub iat: DateTime<Utc>,
    pub cid: String,
    pub sub: Option<String>,
    pub scopes: HashSet<S>,
}

impl<S> TryInto<JwtClaims<S>> for JwtSerializedClaims
where
    S: Sized + FromStr + Hash + Eq,
{
    type Error = S::Err;

    fn try_into(self) -> Result<JwtClaims<S>, Self::Error> {
        Ok(JwtClaims {
            exp: self.exp,
            iat: self.iat,
            cid: self.cid,
            sub: self.sub,
            scopes: tag::parse_space_delimited(&self.scopes)?,
        })
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, AsRefStr, EnumString, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Scope {
    Superuser,
    OfflineAccess,
}

#[allow(dead_code)]
pub type DefaultClaims = JwtClaims<Scope>;

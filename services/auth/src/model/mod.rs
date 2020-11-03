use std::str::FromStr;

use derive_more::Display;
use rocket::http::RawStr;
use rocket::request::FromFormValue;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

pub use client::Client;
pub use renewal::RenewalToken;
pub use user::{User, UserCredential};

mod client;
mod renewal;
mod user;

#[derive(Debug, Display)]
pub enum ModelError {
    #[display(fmt = "Primary Key Error")]
    PrimaryKey,
    #[display(fmt = "Missing Attribute Error")]
    MissingAttribute,
    #[display(fmt = "Deserialization Error: {}", _0)]
    DeserializeError(String),
}
impl std::error::Error for ModelError {}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, AsRefStr, EnumString, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum GrantType {
    Password,
    ClientCredentials,
    RefreshToken,
}

impl<'v> FromFormValue<'v> for GrantType {
    type Error = &'v RawStr;

    fn from_form_value(form_value: &'v RawStr) -> Result<GrantType, &'v RawStr> {
        GrantType::from_str(form_value.as_str()).map_err(|_| form_value)
    }
}

pub(crate) type Scope = jwt::Scope;
pub(crate) type JwtClaims = jwt::DefaultClaims;

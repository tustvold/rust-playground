use derive_more::Display;

#[derive(Debug, Display)]
pub enum IssuerError {
    #[display(fmt = "Config Error: {}", _0)]
    ConfigError(String),

    #[display(fmt = "IO Error: {}", _0)]
    IOError(String),

    #[display(fmt = "Only unencrypted pkcs8 keys are supported")]
    InvalidKey,

    #[display(fmt = "Serialization Error: {}", _0)]
    SerializeError(String),

    #[display(fmt = "Signature Error")]
    SignatureError,
}

impl std::error::Error for IssuerError {}

impl From<pem::PemError> for IssuerError {
    fn from(_: pem::PemError) -> Self {
        IssuerError::InvalidKey
    }
}

impl From<std::io::Error> for IssuerError {
    fn from(e: std::io::Error) -> Self {
        Self::IOError(e.to_string())
    }
}

impl From<ring::error::KeyRejected> for IssuerError {
    fn from(_: ring::error::KeyRejected) -> Self {
        Self::InvalidKey
    }
}

impl From<ring::error::Unspecified> for IssuerError {
    fn from(_: ring::error::Unspecified) -> Self {
        Self::SignatureError
    }
}

impl From<serde_json::Error> for IssuerError {
    fn from(e: serde_json::Error) -> Self {
        Self::SerializeError(e.to_string())
    }
}

#[derive(Debug, Display)]
pub enum ValidatorError {
    #[display(fmt = "Config Error: {}", _0)]
    ConfigError(String),

    #[display(fmt = "Error decoding payload: {}", _0)]
    DecodeError(String),

    #[display(fmt = "Error parsing payload")]
    ParseError,

    #[display(fmt = "JWT Missing")]
    JwtMissing,

    #[display(fmt = "JWT Invalid")]
    JwtInvalid,

    #[display(fmt = "JWT Expired")]
    JwtExpired,
}
impl std::error::Error for ValidatorError {}

impl From<base64::DecodeError> for ValidatorError {
    fn from(e: base64::DecodeError) -> Self {
        Self::DecodeError(e.to_string())
    }
}

impl From<serde_json::Error> for ValidatorError {
    fn from(e: serde_json::Error) -> Self {
        Self::DecodeError(e.to_string())
    }
}

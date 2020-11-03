use derive_more::Display;
use rusoto_core::RusotoError;

use crate::model;
use crate::service::token;
use telemetry::IsErr;

#[derive(Debug, Display)]
pub enum DaoError {
    #[display(fmt = "Already Exists")]
    AlreadyExists,

    #[display(fmt = "Not Found")]
    NotFound,

    #[display(fmt = "Invalid Credential")]
    InvalidCredential,

    #[display(fmt = "Expired Credential")]
    ExpiredCredential,

    #[display(fmt = "Internal Error: {}", _0)]
    InternalError(String),
}

impl std::error::Error for DaoError {}

impl IsErr for DaoError {
    fn is_err(&self) -> bool {
        matches!(self, DaoError::InternalError(_))
    }
}

impl<E: std::error::Error + 'static> From<RusotoError<E>> for DaoError {
    fn from(e: RusotoError<E>) -> Self {
        DaoError::InternalError(e.to_string())
    }
}

impl From<model::ModelError> for DaoError {
    fn from(e: model::ModelError) -> Self {
        DaoError::InternalError(e.to_string())
    }
}

impl From<token::TokenError> for DaoError {
    fn from(e: token::TokenError) -> Self {
        DaoError::InternalError(e.to_string())
    }
}

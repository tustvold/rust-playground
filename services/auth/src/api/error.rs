use std::borrow::Cow;

use rocket::http::Status;
use rocket::{response, Request};
use rocket_contrib::json::Json;
use serde::Serialize;

use jwt::IssuerError;
use telemetry::IsErr;

use crate::dao::DaoError;
use crate::policy::PolicyError;
use crate::service::AuthError;

#[derive(Debug)]
pub enum ApiError {
    AlreadyExists,
    NotFound,
    InvalidCredential,
    ExpiredCredential,
    InvalidRequest,
    Forbidden,
    InternalError(String),
}

#[derive(Serialize)]
struct ErrorResponse<'a> {
    message: Cow<'a, str>,
}

impl<'r> response::Responder<'r, 'static> for ApiError {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'static> {
        let (message, status) = match self {
            ApiError::InternalError(e) => {
                error!("InternalServerError: {}", e);
                (
                    Cow::Borrowed("Internal Server Error"),
                    Status::InternalServerError,
                )
            }
            ApiError::AlreadyExists => (Cow::Borrowed("Already Exists"), Status::BadRequest),
            ApiError::NotFound => (Cow::Borrowed("Not Found"), Status::NotFound),
            ApiError::InvalidCredential => {
                (Cow::Borrowed("Invalid Credential"), Status::BadRequest)
            }
            ApiError::ExpiredCredential => {
                (Cow::Borrowed("Expired Credential"), Status::Unauthorized)
            }
            ApiError::InvalidRequest => (Cow::Borrowed("Invalid Request"), Status::BadRequest),
            ApiError::Forbidden => (Cow::Borrowed("Forbidden"), Status::Forbidden),
        };
        response::status::Custom(status, Json(ErrorResponse { message })).respond_to(req)
    }
}

impl From<IssuerError> for ApiError {
    fn from(e: IssuerError) -> Self {
        Self::InternalError(format!("IssuerError: {}", e))
    }
}

impl From<DaoError> for ApiError {
    fn from(e: DaoError) -> Self {
        match e {
            DaoError::AlreadyExists => Self::AlreadyExists,
            DaoError::InvalidCredential => Self::InvalidCredential,
            DaoError::ExpiredCredential => Self::ExpiredCredential,
            DaoError::NotFound => Self::NotFound,
            DaoError::InternalError(e) => Self::InternalError(format!("DaoError: {}", e)),
        }
    }
}

impl From<PolicyError> for ApiError {
    fn from(e: PolicyError) -> Self {
        match e {
            PolicyError::PermissionDenied => Self::Forbidden,
        }
    }
}

impl From<AuthError> for ApiError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::NotFound => Self::InvalidCredential,
            AuthError::NotLoopback => Self::InvalidCredential,
            AuthError::InvalidCredential => Self::InvalidCredential,
            AuthError::IllegalScopes => Self::InvalidRequest,
            AuthError::ExpiredCredential => Self::ExpiredCredential,
            AuthError::AlreadyExists => Self::InvalidRequest,
            AuthError::InternalError(e) => Self::InternalError(format!("AuthError: {}", e)),
        }
    }
}

impl IsErr for ApiError {
    fn is_err(&self) -> bool {
        matches!(self, ApiError::InternalError(_))
    }
}
